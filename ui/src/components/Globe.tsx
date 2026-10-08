// The globe and the prototype's "lock on" sequence:
//  1. zoom out and swing to the transmitter (slerp + ease, duration by distance)
//  2. land, show the label and pulsing rings
//  3. after 450 ms draw the arc and the great-circle path to the QTH
//  4. after 1.5 s a blue "received" ring at the QTH
//  5. optionally reframe the whole route
// Off-air stations get the flight and the label only.

import { Component, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import GlobeGL, { type GlobeMethods } from 'react-globe.gl';
import { CanvasTexture, MeshPhongMaterial, SRGBColorSpace } from 'three';
import { angleDeg, clamp, easeInOut, fromVec, greatCircle, midpoint, slerp, toVec, type LatLon } from '../geo/geo';
import { GlobeTexture, makeStars } from '../geo/texture';
import type { Candidate } from '../types/generated/Candidate';
import type { Qth } from '../types/generated/Qth';

const REDUCE = typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
const TEXTURE_REFRESH_MS = 120_000;
/** Texture width; capped by the GPU's limit. */
const TEXTURE_WIDTH = 4096;

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
const sleep = (ms: number) => new Promise((r) => setTimeout(r, REDUCE ? 0 : ms));

interface Pin { lat: number; lng: number; el: HTMLElement }
interface Ring { lat: number; lng: number; maxR: number; speed: number; period: number; rgb: string }
interface Arc { sLat: number; sLng: number; eLat: number; eLng: number; dash: number; gap: number; anim: number; stroke: number; colors: string[] }
interface Path { pts: LatLon[] }
interface Fx { station: Pin | null; rings: Ring[]; arcs: Arc[]; paths: Path[] }
const NO_FX: Fx = { station: null, rings: [], arcs: [], paths: [] };

function mkPin(cls: string, title: string, sub: string): HTMLElement {
  const el = document.createElement('div');
  el.className = 'pin ' + cls;
  el.innerHTML = '<span class="pin-dot"></span><span class="pin-tag"><b></b><i></i></span>';
  el.querySelector('b')!.textContent = title;
  el.querySelector('i')!.textContent = sub;
  return el;
}

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

const qthRing = (q: LatLon): Ring => ({ lat: q.lat, lng: q.lon, maxR: 4.5, speed: 2, period: 2200, rgb: COOL });

interface Props {
  sel: Candidate | null;
  qth: Qth;
  frame: boolean;
  onQth: (q: Qth) => void;
  failedMsg: string;
}

function GlobeView({ sel, qth, frame, onQth }: Props) {
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
  const frameRef = useRef(frame);
  frameRef.current = frame;

  useEffect(() => {
    const el = stage.current!;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

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
    g.pointOfView({ lat: -18, lng: -60, altitude: 2.7 }, 0);
    const ctl = g.controls();
    ctl.autoRotate = !REDUCE;
    ctl.autoRotateSpeed = 0.45;
    ctl.enablePan = false;
    ctl.minDistance = 125;
    ctl.maxDistance = 520;
    ctl.addEventListener('start', () => {
      ctl.autoRotate = false;
      flight.current++;
      grabbed.current = seq.current;
    });
    setReady(true);
    loadTexture(g).catch((e) => console.warn('globe texture:', e));
  }, [loadTexture]);

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

  // Lock on when the selected station (or its on-air status) changes.
  const lockKey = sel ? `${sel.station.id}:${sel.air.on}` : null;
  useEffect(() => {
    if (!ready) return;
    const my = ++seq.current;
    if (!sel) {
      flight.current++;
      setFx(NO_FX);
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
      await fly(site.lat, site.lon, 1.5, Math.round(1500 + 1700 * Math.min(1, mv / 150)), 0.5 + 1.4 * Math.min(1, mv / 120));
      if (my !== seq.current) return;
      const station = { lat: site.lat, lng: site.lon, el: mkPin(on ? '' : 'off', sel.station.name, sel.site.short) };
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
      fly(mid.lat, mid.lon, clamp(0.55 + angleDeg(qthRef.current, site) / 42, 1.5, 3.4), 2200, 0.15);
    })();
  }, [lockKey, ready, fly]);

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

  const qthPin = useMemo<Pin>(() => ({ lat: qth.lat, lng: qth.lon, el: mkPin('qth', 'QTH', qth.name) }), [qth]);
  const pins = useMemo(() => (fx.station ? [qthPin, fx.station] : [qthPin]), [qthPin, fx.station]);
  const stars = useMemo(() => makeStars(), []);

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
            onGlobeClick={(c) => onQth({ name: `${c.lat.toFixed(1)}°, ${c.lng.toFixed(1)}°`, lat: c.lat, lon: c.lng })}
            htmlElementsData={pins}
            htmlLat="lat"
            htmlLng="lng"
            htmlAltitude={0.004}
            htmlElement={(d) => (d as Pin).el}
            htmlTransitionDuration={0}
            ringsData={fx.rings}
            ringLat="lat"
            ringLng="lng"
            ringAltitude={0.003}
            ringColor={(d: object) => (t: number) => `rgba(${(d as Ring).rgb},${(Math.pow(1 - t, 1.6) * 0.9).toFixed(3)})`}
            ringMaxRadius="maxR"
            ringPropagationSpeed="speed"
            ringRepeatPeriod="period"
            arcsData={fx.arcs}
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
            pathPointLat={(p) => (p as LatLon).lat}
            pathPointLng={(p) => (p as LatLon).lon}
            pathPointAlt={0.004}
            pathColor={() => 'rgba(255,200,130,0.4)'}
            pathStroke={0.2}
            pathTransitionDuration={0}
          />
        )}
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
