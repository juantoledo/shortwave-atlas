// The globe and the prototype's "lock on" sequence:
//  1. zoom out and swing to the transmitter (slerp + ease, duration by distance)
//  2. land, show the label and pulsing rings
//  3. after 450 ms draw the arc and the great-circle path to the QTH
//  4. after 1.5 s a blue "received" ring at the QTH
//  5. optionally reframe the whole route
// Off-air stations get the flight and the label only. With nothing tuned the camera goes back
// to the QTH and, left alone, the globe turns again.
// While a band scan runs, lock-on rests: the camera holds over the QTH, a sonar pulse spreads
// from it, and at each step the sites on the air on that channel flash, sized by the reading
// (a signal also sends an arc home). When the scan ends, the sites it logged pulse green, framed
// together with the QTH, and lock-on resumes. Every altitude is fitted to the view's
// shape (`fitAltitude`), so a tall phone view shows as much as a wide window.

import { Component, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import GlobeGL, { type GlobeMethods } from 'react-globe.gl';
import { CanvasTexture, MeshPhongMaterial, SRGBColorSpace } from 'three';
import { angleDeg, clamp, easeInOut, fitAltitude, frameAltitude, frameWithHome, fromVec, greatCircle, midpoint, pingAlpha, pingRadius, slerp, toVec, type LatLon } from '../geo/geo';
import { GlobeTexture, makeStars } from '../geo/texture';
import { flagUrl } from '../catalog/flags';
import { useT } from '../i18n';
import { SIGNAL_DB } from './scan';
import type { Candidate } from '../types/generated/Candidate';
import type { Qth } from '../types/generated/Qth';
import type { ScanPing } from '../types/generated/ScanPing';
import type { SiteDot } from '../types/generated/SiteDot';

const REDUCE = typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
const TEXTURE_REFRESH_MS = 120_000;
/** Texture width; capped by the GPU's limit. */
const TEXTURE_WIDTH = 4096;
/** Camera altitudes, in globe radii, for a square view. */
const START_ALT = 2.6, LOCK_ALT = 1.5, HOME_ALT = 2.4, MIN_ALT = 0.3, MAX_ALT = 4;
/** One press of + or −. */
const ZOOM_STEP = 0.72;
/** Turn again after this long without a station or a touch. */
const IDLE_MS = 30_000;
/** Scans: the camera's altitude over the QTH; how long a ping's ring, a signal's arc and the
 *  finale last; how many of each at most; and how recently a touch keeps the camera still. */
const SCAN_ALT = 2.8, PING_MS = 1600, ARC_MS = 2400, FINALE_MS = 4000, MAX_PINGS = 30, MAX_ARCS = 8, HANDS_OFF_MS = 10_000;
/** While scanning the camera eases this often towards the sites pinged in the last `FOLLOW_SPAN_MS`
 *  (most transmitters may face away from the QTH), unless it is nearly there already. */
const FOLLOW_MS = 6000, FOLLOW_SPAN_MS = 15_000, FOLLOW_MIN_DEG = 10;

/** A 1×1 ocean-coloured image: three-globe only reports "ready" after loading an image. */
function placeholder(): string {
  const c = document.createElement('canvas');
  c.width = c.height = 1;
  const g = c.getContext('2d')!;
  g.fillStyle = '#0b1d31';
  g.fillRect(0, 0, 1, 1);
  return c.toDataURL('image/png');
}
const AMBER = '255,179,63', COOL = '127,208,255';
/** A scan ping's size on screen per degree of `pingRadius`. */
const PING_PX_PER_DEG = 8;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, REDUCE ? 0 : ms));

interface Pin { lat: number; lng: number; el: HTMLElement }
interface Ring { lat: number; lng: number; maxR: number; speed: number; period: number; rgb: string; alpha?: number }
interface Arc { sLat: number; sLng: number; eLat: number; eLng: number; dash: number; gap: number; anim: number; stroke: number; colors: string[] }
interface Path { pts: LatLon[] }
interface Fx { station: Pin | null; rings: Ring[]; arcs: Arc[]; paths: Path[] }
const NO_FX: Fx = { station: null, rings: [], arcs: [], paths: [] };
/** A scan's pings (glowing markers that pulse once) and arcs, each until a time (performance.now()). */
interface ScanFx { marks: (Pin & { until: number })[]; arcs: (Arc & { until: number })[] }
const NO_SCAN_FX: ScanFx = { marks: [], arcs: [] };

function mkPin(cls: string, title: string, sub: string, flag: string | null = null): HTMLElement {
  const el = document.createElement('div');
  el.className = 'pin ' + cls;
  el.innerHTML = '<span class="pin-dot"></span><span class="pin-tag"><b></b><i></i></span>';
  el.querySelector('b')!.textContent = title;
  el.querySelector('i')!.textContent = sub;
  const url = flagUrl(flag);
  if (url) {
    const img = document.createElement('img');
    img.className = 'flag';
    img.src = url;
    img.alt = '';
    el.querySelector('b')!.prepend(img);
  }
  return el;
}

/** A pulse at a site: `ping` (a scan step; `sig` for a signal) or `finale` (logged, pulses
 *  three times). Size in px and opacity as CSS variables; styles.css animates it. */
function mkPulse(cls: string, sizePx: number, alpha: number): HTMLElement {
  const el = document.createElement('div');
  el.className = 'pulse ' + cls;
  el.style.setProperty('--s', `${Math.round(sizePx)}px`);
  el.style.setProperty('--a', alpha.toFixed(2));
  return el;
}

const esc = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

/** Transmitter sites: short towers where something is on the air (taller with more), flat
 *  dim dots elsewhere, so they stand out from the city lights; the active site is white, and
 *  sites of stations a scan logged are green (a little larger). */
const siteColor = (s: SiteDot, active: boolean, logged: boolean) =>
  active ? '#ffffff'
  : logged ? `rgba(79,200,164,${s.on ? 1 : 0.7})`
  : s.on ? `rgba(255,226,160,${s.precise ? 1 : 0.75})` : `rgba(150,185,220,${s.precise ? 0.6 : 0.35})`;
const siteRadius = (s: SiteDot, active: boolean, logged: boolean) => (active ? 0.6 : s.on ? 0.45 : 0.32) + (logged && !active ? 0.1 : 0);
const siteAltitude = (s: SiteDot, active: boolean) => (active ? 0.05 : s.on ? 0.012 + Math.min(0.03, s.on * 0.002) : 0.003);

function route(site: LatLon, qth: LatLon): Pick<Fx, 'arcs' | 'paths'> {
  const [a, b] = [site, qth];
  return {
    arcs: [
      { sLat: a.lat, sLng: a.lon, eLat: b.lat, eLng: b.lon, dash: 1, gap: 0, anim: 0, stroke: 0.22, colors: [`rgba(${AMBER},0.4)`, `rgba(${COOL},0.4)`] },
      { sLat: a.lat, sLng: a.lon, eLat: b.lat, eLng: b.lon, dash: 0.16, gap: 1.2, anim: 2600, stroke: 0.55, colors: ['rgba(255,228,175,1)', `rgba(${COOL},1)`] },
    ],
    paths: [{ pts: greatCircle(a, b, Math.max(8, Math.round(angleDeg(a, b) / 2))) }],
  };
}

// Accessors live out here (or in useCallback) so each render hands the globe the same functions:
// a new `htmlElement` makes three-globe rebuild every pin, which replays their fade-in (a blink),
// and any other new accessor re-applies its whole layer.
const pinEl = (d: object) => (d as Pin).el;
const ringColor = (d: object) => (t: number) => `rgba(${(d as Ring).rgb},${(Math.pow(1 - t, 1.6) * 0.9 * ((d as Ring).alpha ?? 1)).toFixed(3)})`;
const pathLat = (p: object) => (p as LatLon).lat;
const pathLng = (p: object) => (p as LatLon).lon;
const pathColor = () => 'rgba(255,200,130,0.4)';

const qthRing = (q: LatLon): Ring =>({ lat: q.lat, lng: q.lon, maxR: 4.5, speed: 2, period: 2200, rgb: COOL });
const sonarRing = (q: LatLon): Ring => ({ lat: q.lat, lng: q.lon, maxR: 25, speed: 6, period: 2500, rgb: COOL, alpha: 0.6 });

interface Props {
  sel: Candidate | null;
  qth: Qth;
  frame: boolean;
  onFrame: () => void;
  /** Hidden behind another view (phones): stop rendering. */
  paused: boolean;
  /** The user dragged, clicked or zoomed the globe. */
  onInteract: () => void;
  onQth: (q: Qth) => void;
  failedMsg: string;
  /** Every transmitter site with its on-air count. */
  sites: SiteDot[];
  activeSite: number | null;
  /** Sites of stations a band scan logged. */
  loggedSites: ReadonlySet<number>;
  /** While a band scan runs: its last steps measured, and the noise floor so far. */
  scan: { pings: readonly ScanPing[]; floorDb: number | null } | null;
  /** A scan that just ended (`key`: its id) and the sites of the stations it logged. */
  finale: { key: string; sites: readonly number[] } | null;
  onSite: (id: number) => void;
  /** Tooltip text for a site: country name and "n of m on the air". */
  siteLabel: (s: SiteDot) => { country: string; onAir: string };
}

function GlobeView({ sel, qth, frame: framing, onFrame, paused, onInteract, onQth, sites, activeSite, loggedSites, scan, finale, onSite, siteLabel }: Props) {
  const t = useT();
  const stage = useRef<HTMLDivElement>(null);
  const globe = useRef<GlobeMethods>(undefined);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const placeholderUrl = useMemo(placeholder, []);
  // our own material (three-globe's default), so the drawn texture can be swapped in
  const material = useMemo(() => new MeshPhongMaterial({ color: 0x000000 }), []);
  const texture = useRef<{ tex: CanvasTexture; timer: ReturnType<typeof setInterval> } | null>(null);
  const [fx, setFx] = useState<Fx>(NO_FX);
  const [ready, setReady] = useState(false);
  const seq = useRef(0), flight = useRef(0), grabbed = useRef(-1);
  const qthRef = useRef(qth);
  qthRef.current = qth;
  const frameRef = useRef(framing);
  frameRef.current = framing;
  const [scanFx, setScanFx] = useState<ScanFx>(NO_SCAN_FX);
  const scanning = scan !== null;
  /** The last scan step that pinged, and the sites pinged lately (for the camera to follow). */
  const pinged = useRef(-1);
  const recent = useRef<{ at: number; site: LatLon }[]>([]);
  /** The finale already played (lock-on rests until then). */
  const [finaleDone, setFinaleDone] = useState<string | null>(null);
  const finaleOn = !!finale && finale.sites.length > 0 && finale.key !== finaleDone;
  const siteIx = useMemo(() => new Map(sites.map((s) => [s.id, s])), [sites]);
  const handsOff = () => performance.now() - touched.current > HANDS_OFF_MS;
  const selRef = useRef(sel);
  selRef.current = sel;
  const interact = useRef(onInteract);
  interact.current = onInteract;
  /** When the user last touched the globe (performance.now()). */
  const touched = useRef(-Infinity);
  /** The pointer is on a transmitter site. */
  const hovering = useRef(false);
  /** Nothing tuned, nothing hovered, untouched for a while: the globe may turn. */
  const idle = useCallback(
    () => !REDUCE && !selRef.current && !hovering.current && performance.now() - touched.current > IDLE_MS,
    [],
  );
  const aspect = useRef(1);
  const fit = useCallback((alt: number) => fitAltitude(alt, aspect.current), []);

  useEffect(() => {
    const el = stage.current!;
    const ro = new ResizeObserver(() => {
      if (!el.clientWidth || !el.clientHeight) return;
      aspect.current = el.clientWidth / el.clientHeight;
      setSize({ w: el.clientWidth, h: el.clientHeight });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // a taller view may pull back further
  useEffect(() => {
    if (ready) globe.current!.controls().maxDistance = 100 * (1 + fit(MAX_ALT));
  }, [ready, size, fit]);

  useEffect(() => {
    if (!ready) return;
    if (paused) globe.current!.pauseAnimation();
    else globe.current!.resumeAnimation();
  }, [ready, paused]);

  // left alone with nothing tuned, turn again
  useEffect(() => {
    if (!ready || REDUCE) return;
    const id = setInterval(() => {
      if (idle()) globe.current!.controls().autoRotate = true;
    }, 5000);
    return () => clearInterval(id);
  }, [ready, idle]);

  useEffect(() => () => {
    if (texture.current) {
      clearInterval(texture.current.timer);
      texture.current.tex.dispose();
      texture.current = null;
    }
  }, []);

  /** Swap the placeholder for the drawn texture, then redraw it in place every 2 minutes. */
  const loadTexture = useCallback(async (g: GlobeMethods) => {
    const max = g.renderer().capabilities.maxTextureSize;
    const gt = await GlobeTexture.create(Math.min(TEXTURE_WIDTH, max));
    if (texture.current || !globe.current) return;
    gt.render(new Date());
    const tex = new CanvasTexture(gt.canvas);
    tex.colorSpace = SRGBColorSpace;
    tex.anisotropy = g.renderer().capabilities.getMaxAnisotropy();
    material.map?.dispose(); // the placeholder
    material.map = tex;
    material.needsUpdate = true;
    const timer = setInterval(() => {
      gt.render(new Date());
      tex.needsUpdate = true;
    }, TEXTURE_REFRESH_MS);
    texture.current = { tex, timer };
  }, [material]);

  const onReady = useCallback(() => {
    const g = globe.current!;
    g.pointOfView({ lat: qthRef.current.lat, lng: qthRef.current.lon, altitude: fit(START_ALT) }, 0);
    const ctl = g.controls();
    ctl.autoRotate = !REDUCE;
    ctl.autoRotateSpeed = 0.45;
    ctl.enablePan = false;
    ctl.minDistance = 100 * (1 + MIN_ALT);
    ctl.maxDistance = 100 * (1 + fit(MAX_ALT));
    ctl.addEventListener('start', () => {
      ctl.autoRotate = false;
      flight.current++;
      grabbed.current = seq.current;
      touched.current = performance.now();
      interact.current();
    });
    setReady(true);
    loadTexture(g).catch((e) => console.warn('globe texture:', e));
  }, [loadTexture, fit]);

  const fly = useCallback((lat: number, lon: number, alt: number, ms: number, bump: number) => {
    const g = globe.current!;
    const id = ++flight.current;
    return new Promise<boolean>((resolve) => {
      const p0 = g.pointOfView();
      if (REDUCE || ms <= 0) {
        g.pointOfView({ lat, lng: lon, altitude: alt }, 0);
        resolve(true);
        return;
      }
      const a = toVec(p0.lat, p0.lng), b = toVec(lat, lon), t0 = performance.now();
      const step = (now: number) => {
        if (id !== flight.current) return resolve(false);
        const k = clamp((now - t0) / ms, 0, 1), e = easeInOut(k);
        const v = fromVec(slerp(a, b, e));
        g.pointOfView({ lat: v.lat, lng: v.lon, altitude: p0.altitude + (alt - p0.altitude) * e + bump * Math.sin(Math.PI * e) }, 0);
        if (k < 1) requestAnimationFrame(step);
        else resolve(true);
      };
      requestAnimationFrame(step);
    });
  }, []);

  /** The user took the camera: no automatic framing for this lock-on, no turning for a while. */
  const grab = useCallback(() => {
    globe.current!.controls().autoRotate = false;
    grabbed.current = seq.current;
    touched.current = performance.now();
    interact.current();
  }, []);

  /** Fly back over the QTH. */
  const home = useCallback(() => {
    const g = globe.current!, q = qthRef.current;
    const p0 = g.pointOfView();
    const mv = angleDeg({ lat: p0.lat, lon: p0.lng }, q);
    return fly(q.lat, q.lon, fit(HOME_ALT), Math.round(1200 + 1200 * Math.min(1, mv / 150)), 0.3 * Math.min(1, mv / 90));
  }, [fly, fit]);

  const zoom = useCallback((k: number) => {
    grab();
    const p = globe.current!.pointOfView();
    fly(p.lat, p.lng, clamp(p.altitude * k, MIN_ALT, fit(MAX_ALT)), 450, 0);
  }, [grab, fly, fit]);

  // Lock on when the selected station (or its on-air status) changes; not during a scan or its
  // finale (they own the globe), and again when they end.
  const lockKey = sel ? `${sel.station.id}:${sel.air.on}` : null;
  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    if (!ready) return;
    if (scanning || finaleOn) {
      lastKey.current = null;
      return;
    }
    const my = ++seq.current;
    const was = lastKey.current;
    lastKey.current = lockKey;
    if (!sel) {
      flight.current++;
      setFx(NO_FX);
      // the station went away (rig off, tuned off it): back home unless the user is steering
      if (was !== null) {
        sleep(800).then(() => {
          if (my === seq.current && performance.now() - touched.current > 10_000) home();
        });
      }
      return;
    }
    const site = { lat: sel.site.lat, lon: sel.site.lon };
    const on = sel.air.on;
    (async () => {
      const g = globe.current!;
      g.controls().autoRotate = false;
      setFx(NO_FX);
      const p0 = g.pointOfView();
      const mv = angleDeg({ lat: p0.lat, lon: p0.lng }, site);
      await fly(site.lat, site.lon, fit(LOCK_ALT), Math.round(1500 + 1700 * Math.min(1, mv / 150)), 0.5 + 1.4 * Math.min(1, mv / 120));
      if (my !== seq.current) return;
      const station = { lat: site.lat, lng: site.lon, el: mkPin(on ? '' : 'off', sel.station.name, sel.site.name, sel.station.flag) };
      const siteRing: Ring = { lat: site.lat, lng: site.lon, maxR: 7, speed: 3.2, period: 1400, rgb: AMBER };
      setFx({ ...NO_FX, station, rings: on ? [siteRing] : [] });
      if (!on) return;
      await sleep(450); if (my !== seq.current) return;
      setFx((f) => ({ ...f, ...route(site, qthRef.current) }));
      await sleep(1500); if (my !== seq.current) return;
      setFx((f) => ({ ...f, rings: [...f.rings, qthRing(qthRef.current)] }));
      if (!frameRef.current || grabbed.current === my) return;
      await sleep(1100); if (my !== seq.current || grabbed.current === my) return;
      const mid = midpoint(qthRef.current, site);
      fly(mid.lat, mid.lon, fit(clamp(0.55 + angleDeg(qthRef.current, site) / 42, 1.5, 3.4)), 2200, 0.15);
    })();
  }, [lockKey, ready, fly, fit, home, scanning, finaleOn]);

  // A scan starts: drop the station and its route, hold over the QTH. It ends: clear its layers.
  useEffect(() => {
    if (!ready) return;
    if (!scanning) {
      setScanFx(NO_SCAN_FX);
      return;
    }
    seq.current++;
    flight.current++;
    pinged.current = -1;
    recent.current = [];
    setFx(NO_FX);
    setScanFx(NO_SCAN_FX);
    globe.current!.controls().autoRotate = false;
    if (handsOff()) {
      const q = qthRef.current;
      fly(q.lat, q.lon, fit(SCAN_ALT), 1600, 0.2);
    }
  }, [scanning, ready, fly, fit]);

  // Each new step measured: its sites flash; a signal also sends an arc home.
  useEffect(() => {
    if (!scan || !ready) return;
    const fresh = scan.pings.filter((p) => p.index > pinged.current);
    if (!fresh.length) return;
    pinged.current = fresh[fresh.length - 1].index;
    const now = performance.now(), q = qthRef.current;
    const marks: ScanFx['marks'] = [], arcs: ScanFx['arcs'] = [];
    for (const p of fresh) {
      const signal = p.db !== null && scan.floorDb !== null && p.db >= scan.floorDb + SIGNAL_DB;
      for (const id of p.sites) {
        const s = siteIx.get(id);
        if (!s) continue;
        recent.current.push({ at: now, site: { lat: s.lat, lon: s.lon } });
        const el = mkPulse(signal ? 'ping sig' : 'ping', pingRadius(p.db) * PING_PX_PER_DEG, pingAlpha(p.db));
        marks.push({ lat: s.lat, lng: s.lon, el, until: now + PING_MS });
        if (signal && !REDUCE) {
          arcs.push({
            sLat: s.lat, sLng: s.lon, eLat: q.lat, eLng: q.lon, dash: 0.25, gap: 1.2, anim: 1100, stroke: 0.45,
            colors: ['rgba(255,228,175,0.95)', `rgba(${COOL},0.9)`], until: now + ARC_MS,
          });
        }
      }
    }
    if (marks.length) setScanFx((f) => ({ marks: [...f.marks, ...marks].slice(-MAX_PINGS), arcs: [...f.arcs, ...arcs].slice(-MAX_ARCS) }));
  }, [scan, ready, siteIx]);

  // The scan ended: the sites it logged pulse green, framed with the QTH; then lock-on resumes.
  const finaleKey = finaleOn ? finale!.key : null;
  useEffect(() => {
    if (!ready || !finaleKey || !finale) return;
    const pts = finale.sites.map((id) => siteIx.get(id)).filter((s): s is SiteDot => !!s);
    const now = performance.now();
    seq.current++;
    setFx(NO_FX);
    setScanFx({ marks: pts.map((s) => ({ lat: s.lat, lng: s.lon, el: mkPulse('finale', 40, 1), until: now + FINALE_MS })), arcs: [] });
    if (pts.length && handsOff()) {
      const f = frameWithHome(pts.map((s) => ({ lat: s.lat, lon: s.lon })), qthRef.current);
      fly(f.center.lat, f.center.lon, fit(frameAltitude(f.spreadDeg)), 2000, 0.2);
    }
    const id = setTimeout(() => {
      setScanFx(NO_SCAN_FX);
      setFinaleDone(finaleKey);
    }, FINALE_MS);
    return () => clearTimeout(id);
  }, [finaleKey, ready]); // the finale plays once per scan, whatever else changes

  // While scanning, ease towards where the pings are (and home, when it fits in the view).
  useEffect(() => {
    if (!scanning || !ready || REDUCE) return;
    const id = setInterval(() => {
      const now = performance.now();
      recent.current = recent.current.filter((r) => now - r.at < FOLLOW_SPAN_MS);
      if (!recent.current.length || !handsOff()) return;
      const f = frameWithHome(recent.current.map((r) => r.site), qthRef.current);
      const p = globe.current!.pointOfView();
      if (angleDeg({ lat: p.lat, lon: p.lng }, f.center) < FOLLOW_MIN_DEG) return;
      fly(f.center.lat, f.center.lon, fit(frameAltitude(f.spreadDeg)), 3000, 0.1);
    }, FOLLOW_MS);
    return () => clearInterval(id);
  }, [scanning, ready, fly, fit]);

  // Pings and arcs go when their time is up.
  useEffect(() => {
    if (!scanning) return;
    const id = setInterval(() => {
      const now = performance.now();
      setScanFx((f) => {
        const marks = f.marks.filter((m) => m.until > now), arcs = f.arcs.filter((a) => a.until > now);
        return marks.length === f.marks.length && arcs.length === f.arcs.length ? f : { marks, arcs };
      });
    }, 250);
    return () => clearInterval(id);
  }, [scanning]);

  const sonar = useMemo(() => sonarRing(qth), [qth]);
  const rings = useMemo(() => (scanning && !REDUCE ? [...fx.rings, sonar] : fx.rings), [fx.rings, scanning, sonar]);
  const arcs = useMemo(() => [...fx.arcs, ...scanFx.arcs], [fx.arcs, scanFx.arcs]);

  // Moving the QTH redraws the route in place.
  useEffect(() => {
    setFx((f) => {
      if (!sel || !f.arcs.length) return f;
      return {
        ...f,
        ...route({ lat: sel.site.lat, lon: sel.site.lon }, qth),
        rings: f.rings.filter((r) => r.rgb !== COOL).concat([qthRing(qth)]),
      };
    });
  }, [qth.lat, qth.lon]);

  /** Hold the turn while the pointer is on a site, or the dot slides out from under it and the
   *  tooltip blinks. */
  const onPointHover = useCallback((d: object | null) => {
    const ctl = globe.current?.controls();
    if (!ctl) return;
    hovering.current = !!d;
    if (d) ctl.autoRotate = false;
    else if (idle()) ctl.autoRotate = true;
  }, [idle]);

  const qthPin = useMemo<Pin>(() => ({ lat: qth.lat, lng: qth.lon, el: mkPin('qth', 'QTH', qth.name) }), [qth]);
  const pins = useMemo(() => [qthPin, ...(fx.station ? [fx.station] : []), ...scanFx.marks], [qthPin, fx.station, scanFx.marks]);
  const stars = useMemo(() => makeStars(), []);
  const pointAltitude = useCallback((d: object) => siteAltitude(d as SiteDot, (d as SiteDot).id === activeSite), [activeSite]);
  const pointRadius = useCallback((d: object) => siteRadius(d as SiteDot, (d as SiteDot).id === activeSite, loggedSites.has((d as SiteDot).id)), [activeSite, loggedSites]);
  const pointColor = useCallback((d: object) => siteColor(d as SiteDot, (d as SiteDot).id === activeSite, loggedSites.has((d as SiteDot).id)), [activeSite, loggedSites]);
  const siteTip = useCallback((d: object) => {
    const s = d as SiteDot;
    const l = siteLabel(s);
    const url = flagUrl(s.flag);
    const flag = url ? `<img class="flag" src="${esc(url)}" alt="">` : '';
    return `<div class="site-tip"><b>${flag}${esc(s.name)}</b><span>${esc(l.country)}</span><span>${esc(l.onAir)}</span></div>`;
  }, [siteLabel]);

  return (
    <>
      <div className="stars" style={{ backgroundImage: `url(${stars})` }} />
      <div id="globe" ref={stage}>
        {size.w > 0 && (
          <GlobeGL
            ref={globe}
            width={size.w}
            height={size.h}
            animateIn={false}
            backgroundColor="rgba(0,0,0,0)"
            globeImageUrl={placeholderUrl}
            globeMaterial={material}
            atmosphereColor="#4d8fd6"
            atmosphereAltitude={0.17}
            showGraticules={false}
            onGlobeReady={onReady}
            onGlobeClick={(c) => {
              interact.current();
              onQth({ name: `${c.lat.toFixed(1)}°, ${c.lng.toFixed(1)}°`, lat: c.lat, lon: c.lng });
            }}
            pointsData={sites}
            pointLat="lat"
            pointLng="lon"
            pointAltitude={pointAltitude}
            pointRadius={pointRadius}
            pointColor={pointColor}
            pointResolution={10}
            pointsMerge={false}
            pointsTransitionDuration={0}
            pointLabel={siteTip}
            onPointHover={onPointHover}
            onPointClick={(d) => onSite((d as SiteDot).id)}
            htmlElementsData={pins}
            htmlLat="lat"
            htmlLng="lng"
            htmlAltitude={0.004}
            htmlElement={pinEl}
            htmlTransitionDuration={0}
            ringsData={rings}
            ringLat="lat"
            ringLng="lng"
            ringAltitude={0.003}
            ringColor={ringColor}
            ringMaxRadius="maxR"
            ringPropagationSpeed="speed"
            ringRepeatPeriod="period"
            arcsData={arcs}
            arcStartLat="sLat"
            arcStartLng="sLng"
            arcEndLat="eLat"
            arcEndLng="eLng"
            arcColor="colors"
            arcStroke="stroke"
            arcDashLength="dash"
            arcDashGap="gap"
            arcDashAnimateTime="anim"
            arcAltitudeAutoScale={0.42}
            arcsTransitionDuration={1400}
            pathsData={fx.paths}
            pathPoints="pts"
            pathPointLat={pathLat}
            pathPointLng={pathLng}
            pathPointAlt={0.004}
            pathColor={pathColor}
            pathStroke={0.2}
            pathTransitionDuration={0}
          />
        )}
      </div>
      <div className="globe-ctl" role="group" aria-label={t.globeControls}>
        <div className="zoom">
          <button className="icon-btn" type="button" disabled={!ready} aria-label={t.zoomIn} title={t.zoomIn} onClick={() => zoom(ZOOM_STEP)}>+</button>
          <button className="icon-btn" type="button" disabled={!ready} aria-label={t.zoomOut} title={t.zoomOut} onClick={() => zoom(1 / ZOOM_STEP)}>−</button>
          <button className="icon-btn" type="button" disabled={!ready} aria-label={t.centerQth} title={t.centerQth} onClick={() => { grab(); home(); }}>⌖</button>
        </div>
        <button className="toggle" type="button" aria-pressed={framing} onClick={onFrame}>{t.frameRoute}</button>
      </div>
    </>
  );
}

/** WebGL can be missing (old GPU, remote desktop): keep the rest of the app usable. */
class GlobeBoundary extends Component<{ msg: string; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    return this.state.failed ? <div className="stage-msg">{this.props.msg}</div> : this.props.children;
  }
}

export function Globe(props: Props) {
  return (
    <GlobeBoundary msg={props.failedMsg}>
      <GlobeView {...props} />
    </GlobeBoundary>
  );
}
