// The stage's world as a flat map (equirectangular, repeating east and west), drawn on a 2D
// canvas. It plays the globe's choreography (stage/useStage.ts) with the same camera, a centre and
// an altitude, turned into a scale (geo/flat.ts), so a flight, a framed route or a zoom shows
// about as much as on the globe. Day and night are painted on the map as the sun moves
// (geo/flatTexture.ts); each frame draws the map, the rings, the route, the sites and the arcs,
// and moves the pins (the globe's DOM pins, so the same CSS).
// Drag to pan, wheel or pinch to zoom; a click picks a site or moves the QTH.

import { useCallback, useEffect, useMemo, useRef, type PointerEvent } from 'react';
import { angleDeg, clamp, greatCircle, sunVec, type LatLon } from '../geo/geo';
import { altFor, clampLat, copies, lerpFlat, project, pxPerDeg, unwrap, wrapLon, type FlatView as View } from '../geo/flat';
import { FlatTexture } from '../geo/flatTexture';
import { GlobeTexture, makeStars } from '../geo/texture';
import { Controls } from './stage/Controls';
import { ARC_IN_MS, MAX_ALT, MIN_ALT, REDUCE, ringColor, ringNow, siteColor, siteRadius, siteTip, START_ALT, TOWER_MS, type Arc, type Ring } from './stage/fx';
import { useStage, type Camera, type Pov, type ViewProps } from './stage/useStage';
import { useStageShape } from './stage/useStageShape';
import type { SiteDot } from '../types/generated/SiteDot';

/** How often day and night are painted again (the sun moves a pixel of the map in ~30 s). */
const SUN_MS = 20_000;
/** The map's width (the globe's texture). */
const TEXTURE_WIDTH = 4096;
/** The deepest zoom, in px per degree: the map's own pixels at most 3× on screen. */
const MAX_PPD = (3 * TEXTURE_WIDTH) / 360;
/** A drag starts this far (px) from where the pointer went down; anything less is a click. */
const DRAG_PX = 4;
/** The wheel's zoom per pixel of scroll. */
const WHEEL_K = 0.0015;
/** The turn (`SPIN` is 0.45): OrbitControls' 6° a second per unit of speed, westward. */
const TURN_DEG_PER_S = 6;
const OCEAN = '#0b1d31';

/** A site's dot in px for its radius in degrees at `ppd`: about as large as on the globe, not
 *  vanishing far out nor swelling close in. */
const dotPx = (rDeg: number, ppd: number) => rDeg * clamp(ppd, 4, 8);

/** The points of an arc from `a` to `b`, `n` steps, lifted off the map in the middle (`lift`
 *  px at its top), as canvas px; longitudes continuous from the copy nearest the centre. */
function arcPx(v: View, a: LatLon, b: LatLon, lift: number): [number, number][] {
  const pts = unwrap(greatCircle(a, b, Math.max(16, Math.round(angleDeg(a, b)))));
  const [x0] = project(v, pts[0]);
  const n = pts.length - 1;
  return pts.map((p, i) => [x0 + (p.lon - pts[0].lon) * v.ppd, v.cy - (p.lat - v.lat) * v.ppd - lift * Math.sin((Math.PI * i) / n)]);
}

/** Stroke `pts` up to `upTo` (0..1 of its points) on every copy of the world that shows. */
function strokeCopies(g: CanvasRenderingContext2D, pts: [number, number][], upTo: number, worldW: number, w: number) {
  const n = Math.ceil(upTo * (pts.length - 1));
  if (n < 1) return;
  let x0 = Infinity, x1 = -Infinity;
  for (const [x] of pts) { x0 = Math.min(x0, x); x1 = Math.max(x1, x); }
  for (const dx of copies(x0, x1, worldW, w)) {
    g.beginPath();
    g.moveTo(pts[0][0] + dx, pts[0][1]);
    for (let i = 1; i <= n; i++) g.lineTo(pts[i][0] + dx, pts[i][1]);
    g.stroke();
  }
}

const pathLength = (pts: [number, number][]) => pts.reduce((s, p, i) => (i ? s + Math.hypot(p[0] - pts[i - 1][0], p[1] - pts[i - 1][1]) : 0), 0);

export function FlatView(props: ViewProps & { failedMsg: string; noGlobe?: boolean }) {
  const { qth, frame: framing, onFrame, paused, sites, activeSite, loggedSites, listenedSites, mode, onMode, noGlobe, failedMsg } = props;
  const stage = useStage(props);
  const { attach, routeProgress, level, hover, userStart } = stage;
  const { stage: el, view, size, shape, offset } = useStageShape();
  const canvas = useRef<HTMLCanvasElement>(null);
  const pinLayer = useRef<HTMLDivElement>(null);
  const tip = useRef<HTMLDivElement>(null);
  const stars = useMemo(() => makeStars(), []);
  const tex = useRef<FlatTexture | null>(null);
  const cam = useRef<Pov>({ lat: qth.lat, lon: qth.lon, alt: START_ALT });
  const off = useRef(offset);
  off.current = offset;
  /** Something changed since the last frame was drawn. */
  const dirty = useRef(true);
  /** Each site's dot radius as drawn (it eases to its size, as a tower rises on the globe). */
  const dots = useRef(new Map<number, number>());
  /** When each ring and arc first showed (rings pulse from then, arcs draw on). */
  const born = useRef(new WeakMap<object, number>());
  /** What the frame loop draws: the latest props and choreography. */
  const scene = useRef({ sites, activeSite, loggedSites, listenedSites, rings: stage.rings, arcs: stage.arcs, pins: stage.pins, path: stage.fx.path, qth });
  scene.current = { sites, activeSite, loggedSites, listenedSites, rings: stage.rings, arcs: stage.arcs, pins: stage.pins, path: stage.fx.path, qth };
  const handlers = useRef({ onQth: props.onQth, onSite: props.onSite, siteLabel: props.siteLabel, onInteract: props.onInteract });
  handlers.current = { onQth: props.onQth, onSite: props.onSite, siteLabel: props.siteLabel, onInteract: props.onInteract };
  useEffect(() => {
    dirty.current = true; // a render: something may have changed
  });

  /** The altitudes the map may show: never past the map's own detail, nor farther than the globe. */
  const range = useCallback((): [number, number] => {
    const s = shape.current;
    return [Math.max(MIN_ALT, altFor(MAX_PPD, s.viewW, s.viewH)), MAX_ALT];
  }, [shape]);

  /** What the canvas shows from `p`. */
  const viewOf = useCallback((p: Pov): View => {
    const s = shape.current;
    return { lat: p.lat, lon: p.lon, ppd: pxPerDeg(p.alt, s.viewW, s.viewH), cx: s.canvasW / 2 + off.current[0], cy: s.canvasH / 2 + off.current[1], w: s.canvasW, h: s.canvasH };
  }, [shape]);

  /** Move the camera, held within the map. */
  const setCam = useCallback((p: Pov) => {
    const [lo, hi] = range();
    const alt = clamp(p.alt, lo, hi), v = viewOf({ ...p, alt });
    const lat = clampLat(p.lat, v.ppd, v.cy, v.h), lon = wrapLon(p.lon);
    const c = cam.current;
    if (c.lat !== lat || c.lon !== lon || c.alt !== alt) {
      cam.current = { lat, lon, alt };
      dirty.current = true;
    }
  }, [range, viewOf]);

  /** Zoom to `alt` keeping the point under canvas px `x, y` where it is. */
  const zoomAt = useCallback((x: number, y: number, alt: number) => {
    const v0 = viewOf(cam.current);
    const lat = v0.lat - (y - v0.cy) / v0.ppd, lon = v0.lon + (x - v0.cx) / v0.ppd;
    const [lo, hi] = range();
    const a = clamp(alt, lo, hi), ppd = viewOf({ ...cam.current, alt: a }).ppd;
    setCam({ lat: lat + (y - v0.cy) / ppd, lon: lon - (x - v0.cx) / ppd, alt: a });
  }, [viewOf, range, setCam]);

  /** The site under canvas px `x, y`, if any (the nearest within its dot and a little more). */
  const siteAt = useCallback((x: number, y: number): SiteDot | null => {
    const v = viewOf(cam.current), worldW = 360 * v.ppd;
    let best: SiteDot | null = null, bestD = Infinity;
    for (const s of scene.current.sites) {
      const [sx, sy] = project(v, s);
      const r = (dots.current.get(s.id) ?? 0) + 4;
      const dx = Math.abs(sx - x) % worldW, d = Math.hypot(Math.min(dx, worldW - dx), sy - y);
      if (d <= r && d < bestD) { best = s; bestD = d; }
    }
    return best;
  }, [viewOf]);

  /** Draw a frame (skipped when nothing moves). */
  const draw = useCallback((now: number, dt: number) => {
    const c = canvas.current;
    if (!c) return;
    const sc = scene.current, s = shape.current;
    const w = s.canvasW, h = s.canvasH, dpr = window.devicePixelRatio || 1;
    const pw = Math.round(w * dpr), ph = Math.round(h * dpr);
    if (c.width !== pw || c.height !== ph) {
      c.width = pw;
      c.height = ph;
      dirty.current = true;
    }
    const v = viewOf(cam.current), ppd = v.ppd, worldW = 360 * ppd, worldH = 180 * ppd;
    const progress = routeProgress(now);

    // the sites' dots ease to their size
    let easing = false;
    const k = REDUCE ? 1 : 1 - Math.exp(-dt / (TOWER_MS / 3));
    const seen = new Set<number>();
    for (const site of sc.sites) {
      const id = site.id;
      seen.add(id);
      const want = dotPx(siteRadius(site, id === sc.activeSite, sc.loggedSites.has(id), sc.listenedSites.get(id)), ppd);
      const was = dots.current.get(id) ?? 0;
      const r = Math.abs(want - was) < 0.02 ? want : was + (want - was) * k;
      if (r !== want) easing = true;
      dots.current.set(id, r);
    }
    if (dots.current.size > seen.size) for (const id of dots.current.keys()) if (!seen.has(id)) dots.current.delete(id);

    const moving = sc.rings.length > 0 || sc.arcs.length > 0 || (sc.path !== null && progress < 1) || easing;
    if (!dirty.current && !moving) return;
    dirty.current = false;

    const g = c.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);

    // the map, as many copies as cover the canvas
    const mx = v.cx + (-180 - v.lon) * ppd, my = v.cy - (90 - v.lat) * ppd;
    g.imageSmoothingEnabled = true;
    g.imageSmoothingQuality = 'high';
    for (const dx of copies(mx, mx + worldW, worldW, w)) {
      if (tex.current) g.drawImage(tex.current.canvas, mx + dx, my, worldW, worldH);
      else { g.fillStyle = OCEAN; g.fillRect(mx + dx, my, worldW, worldH); }
    }

    // rings: each sends a circle out every period, growing to its size and fading
    g.lineWidth = 1.5;
    for (const ring of sc.rings) {
      if (!born.current.has(ring)) born.current.set(ring, now);
      const t0 = born.current.get(ring)!, rn = ringNow(ring as Ring, level.current);
      const life = (rn.maxR / rn.speed) * 1000;
      const [x, y] = project(v, { lat: ring.lat, lon: ring.lng });
      const stretch = 1 / Math.max(0.2, Math.cos((ring.lat * Math.PI) / 180));
      for (let n = Math.floor((now - t0) / rn.period); n >= 0; n--) {
        const age = now - (t0 + n * rn.period);
        if (age > life) break;
        const r = (rn.speed * age) / 1000;
        g.strokeStyle = ringColor(ring as Ring, r / rn.maxR, rn.alpha);
        for (const dx of copies(x - r * ppd * stretch, x + r * ppd * stretch, worldW, w)) {
          g.beginPath();
          g.ellipse(x + dx, y, r * ppd * stretch, r * ppd, 0, 0, 2 * Math.PI);
          g.stroke();
        }
      }
    }

    // the route home on the ground, drawing itself on
    if (sc.path) {
      g.strokeStyle = `rgba(${sc.path.rgb ?? '255,200,130'},0.4)`;
      g.lineWidth = 1.5;
      strokeCopies(g, arcPx(v, sc.path.site, sc.qth, 0), progress, worldW, w);
    }

    // the sites: on-air ones with a glow, the active one on top
    let active: [number, number, number] | null = null;
    for (const site of sc.sites) {
      const id = site.id, r = dots.current.get(id) ?? 0;
      const [x, y] = project(v, site);
      if (y < -r * 3 || y > h + r * 3) continue;
      const isActive = id === sc.activeSite;
      g.fillStyle = siteColor(site, isActive, sc.loggedSites.has(id), sc.listenedSites.get(id));
      for (const dx of copies(x - r * 3, x + r * 3, worldW, w)) {
        if (isActive) { active = [x + dx, y, r]; continue; }
        if (site.on) {
          g.globalAlpha = 0.22;
          g.beginPath(); g.arc(x + dx, y, r * 2.2, 0, 2 * Math.PI); g.fill();
          g.globalAlpha = 1;
        }
        g.beginPath(); g.arc(x + dx, y, r, 0, 2 * Math.PI); g.fill();
      }
    }
    if (active) {
      const [x, y, r] = active;
      g.fillStyle = '#ffffff';
      g.globalAlpha = 0.3;
      g.beginPath(); g.arc(x, y, r * 2.6, 0, 2 * Math.PI); g.fill();
      g.globalAlpha = 1;
      g.beginPath(); g.arc(x, y, r, 0, 2 * Math.PI); g.fill();
    }

    // arcs: raised over the route, drawing on, their dashes running home
    g.lineCap = 'round';
    for (const arc of sc.arcs as Arc[]) {
      if (!born.current.has(arc)) born.current.set(arc, now);
      const a = { lat: arc.sLat, lon: arc.sLng }, b = { lat: arc.eLat, lon: arc.eLng };
      const pts = arcPx(v, a, b, Math.min(0.2 * angleDeg(a, b) * ppd, 0.3 * h));
      const len = pathLength(pts);
      const grow = REDUCE ? 1 : clamp((now - born.current.get(arc)!) / ARC_IN_MS, 0, 1);
      const grad = g.createLinearGradient(pts[0][0], pts[0][1], pts[pts.length - 1][0], pts[pts.length - 1][1]);
      grad.addColorStop(0, arc.colors[0]);
      grad.addColorStop(1, arc.colors[1] ?? arc.colors[0]);
      g.strokeStyle = grad;
      g.lineWidth = Math.max(1, arc.stroke * 5);
      if (arc.gap > 0) {
        const cycle = (arc.dash + arc.gap) * len;
        g.setLineDash([arc.dash * len, arc.gap * len]);
        g.lineDashOffset = arc.anim && !REDUCE ? -(((now / arc.anim) * len) % cycle) : 0;
      }
      strokeCopies(g, pts, grow, worldW, w);
      g.setLineDash([]);
    }

    // the pins, on the copy nearest the centre
    for (const pin of sc.pins) {
      const [x, y] = project(v, { lat: pin.lat, lon: pin.lng });
      pin.el.style.transform = `translate(${x.toFixed(1)}px, ${y.toFixed(1)}px)`;
    }
  }, [viewOf, routeProgress, level, shape]);

  // Hand the choreography the camera once the canvas has a size.
  const sized = size.w > 0;
  useEffect(() => {
    if (!sized) return;
    const camera: Camera = {
      pov: () => cam.current,
      set: setCam,
      turn: (speed, dt) => {
        if (speed) setCam({ ...cam.current, lon: cam.current.lon - (speed * TURN_DEG_PER_S * dt) / 1000 });
      },
      between: lerpFlat,
      fit: (alt) => alt, // the scale already follows the view's shorter side
      range,
      frame: draw,
    };
    attach(camera);
  }, [sized]); // once: the camera reads the shape as it changes

  // a new shape: hold the camera within the map again, and draw
  useEffect(() => {
    setCam(cam.current);
    dirty.current = true;
  }, [size, offset, setCam]);

  // The map and its day and night, painted again as the sun moves.
  useEffect(() => {
    let live = true;
    GlobeTexture.shared(TEXTURE_WIDTH).then((gt) => {
      if (!live) return;
      const ft = new FlatTexture(gt);
      ft.update(sunVec(new Date()));
      tex.current = ft;
      dirty.current = true;
    }).catch((e) => console.warn('flat map texture:', e));
    return () => { live = false; };
  }, []);
  useEffect(() => {
    if (paused) return;
    tex.current?.update(sunVec(new Date()));
    dirty.current = true;
    const id = setInterval(() => {
      tex.current?.update(sunVec(new Date()));
      dirty.current = true;
    }, SUN_MS);
    return () => clearInterval(id);
  }, [paused]);

  // The pins' elements, in their layer while they show.
  useEffect(() => {
    const layer = pinLayer.current!;
    const want = new Set(stage.pins.map((p) => p.el));
    for (const child of Array.from(layer.children)) if (!want.has(child as HTMLElement)) child.remove();
    for (const el of want) {
      if (el.parentElement !== layer) {
        el.classList.add('flat-pin');
        layer.appendChild(el);
      }
    }
    dirty.current = true;
  }, [stage.pins]);

  // Pointers: drag to pan, pinch or wheel to zoom, click to pick a site or move the QTH.
  const pointers = useRef(new Map<number, { x: number; y: number }>());
  const press = useRef({ x: 0, y: 0, dragged: false });
  const hovered = useRef<number | null>(null);
  const local = (e: { clientX: number; clientY: number }) => {
    const r = el.current!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  const showTip = useCallback((s: SiteDot | null, x = 0, y = 0) => {
    const t = tip.current!;
    const id = s?.id ?? null;
    if (id !== hovered.current) {
      hovered.current = id;
      hover(id !== null);
      t.innerHTML = s ? siteTip(s, handlers.current.siteLabel(s)) : '';
      el.current!.style.cursor = s ? 'pointer' : '';
    }
    t.hidden = !s;
    if (s) t.style.transform = `translate(${Math.round(x + 14)}px, ${Math.round(y + 14)}px)`;
  }, [hover, el]);

  const onPointerDown = (e: PointerEvent) => {
    if (!stage.ready) return;
    el.current!.setPointerCapture(e.pointerId);
    const p = local(e);
    pointers.current.set(e.pointerId, p);
    if (pointers.current.size === 1) press.current = { x: p.x, y: p.y, dragged: false };
    else press.current.dragged = true; // a second finger: a pinch, not a click
    userStart();
  };
  const onPointerMove = (e: PointerEvent) => {
    if (!stage.ready) return;
    const p = local(e), ps = pointers.current, prev = ps.get(e.pointerId);
    if (!prev) {
      if (e.pointerType === 'mouse') showTip(siteAt(p.x, p.y), p.x, p.y);
      return;
    }
    if (ps.size === 1) {
      if (!press.current.dragged && Math.hypot(p.x - press.current.x, p.y - press.current.y) < DRAG_PX) return;
      if (!press.current.dragged) showTip(null);
      press.current.dragged = true;
      const ppd = viewOf(cam.current).ppd;
      setCam({ ...cam.current, lat: cam.current.lat + (p.y - prev.y) / ppd, lon: cam.current.lon - (p.x - prev.x) / ppd });
      ps.set(e.pointerId, p);
    } else if (ps.size === 2) {
      const other = [...ps.entries()].find(([id]) => id !== e.pointerId)![1];
      const d0 = Math.hypot(prev.x - other.x, prev.y - other.y), d1 = Math.hypot(p.x - other.x, p.y - other.y);
      ps.set(e.pointerId, p);
      if (d0 < 1 || d1 < 1) return;
      // zoom about the fingers' middle, and follow it
      const m0 = { x: (prev.x + other.x) / 2, y: (prev.y + other.y) / 2 }, m1 = { x: (p.x + other.x) / 2, y: (p.y + other.y) / 2 };
      zoomAt(m0.x, m0.y, cam.current.alt * (d0 / d1));
      const ppd = viewOf(cam.current).ppd;
      setCam({ ...cam.current, lat: cam.current.lat + (m1.y - m0.y) / ppd, lon: cam.current.lon - (m1.x - m0.x) / ppd });
    }
  };
  const onPointerUp = (e: PointerEvent) => {
    const ps = pointers.current;
    if (!ps.delete(e.pointerId)) return;
    if (ps.size > 0 || press.current.dragged || e.type === 'pointercancel') return;
    const p = local(e), site = siteAt(p.x, p.y);
    if (site) {
      handlers.current.onSite(site.id);
      return;
    }
    const at = viewOf(cam.current);
    const lat = at.lat - (p.y - at.cy) / at.ppd;
    if (lat > 90 || lat < -90) return;
    const lon = wrapLon(at.lon + (p.x - at.cx) / at.ppd);
    handlers.current.onInteract();
    handlers.current.onQth({ name: `${lat.toFixed(1)}°, ${lon.toFixed(1)}°`, lat, lon });
  };
  const onPointerLeave = () => {
    if (!pointers.current.size) showTip(null);
  };

  // the wheel zooms about the pointer (a listener of our own: React's is passive)
  useEffect(() => {
    const node = el.current!;
    const onWheel = (e: WheelEvent) => {
      if (!stage.ready) return;
      e.preventDefault();
      userStart();
      const px = e.deltaY * (e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1);
      const p = local(e);
      zoomAt(p.x, p.y, cam.current.alt * Math.exp(px * WHEEL_K));
    };
    node.addEventListener('wheel', onWheel, { passive: false });
    return () => node.removeEventListener('wheel', onWheel);
  }, [stage.ready, userStart, zoomAt]);

  return (
    <>
      <div className="stars" style={{ backgroundImage: `url(${stars})` }} />
      <div className="globe-view" ref={view} />
      <div
        id="globe"
        className="flat"
        ref={el}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onPointerLeave={onPointerLeave}
      >
        <canvas ref={canvas} />
        <div className="flat-pins" ref={pinLayer} />
        <div className="flat-tip" ref={tip} hidden />
      </div>
      {noGlobe && <p className="hud stage-note">{failedMsg}</p>}
      <Controls stage={stage} mode={noGlobe ? 'flat' : mode} onMode={onMode} noGlobe={noGlobe} framing={framing} onFrame={onFrame} />
    </>
  );
}
