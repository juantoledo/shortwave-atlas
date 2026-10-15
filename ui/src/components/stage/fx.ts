// What the globe and the flat map draw, and the numbers that time it: pins, rings, arcs, the
// route, how a site looks. The choreography that sets them is `useStage`.

import { receivedRing, type LatLon } from '../../geo/geo';
import { flagUrl } from '../../catalog/flags';
import type { SiteDot } from '../../types/generated/SiteDot';

export const REDUCE = typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
/** Camera altitudes, in globe radii, for a square view. */
export const START_ALT = 2.6, LOCK_ALT = 1.5, HOME_ALT = 2.4, MIN_ALT = 0.3, MAX_ALT = 4;
/** One press of + or −. */
export const ZOOM_STEP = 0.72;
/** Scans: how long a ping's ring and a signal's arc last. */
export const PING_MS = 1600, ARC_MS = 2400;
/** How long the route takes to draw itself home; a tower's rise; an arc's. */
export const ROUTE_MS = 1300, TOWER_MS = 700, ARC_IN_MS = 1400;
/** The turn's speed (OrbitControls' `autoRotateSpeed`: 1 is a turn a minute). */
export const SPIN = 0.45;
export const AMBER = '255,179,63', COOL = '127,208,255';
/** An internet station (the NET band): neither a transmitter's amber nor the QTH's blue. */
export const NET_RGB = '255,122,196';
/** A scan ping's size on screen per degree of `pingRadius`. */
export const PING_PX_PER_DEG = 8;

export interface Pin { lat: number; lng: number; el: HTMLElement }
/** `live`: the "received" ring, sized and paced by the S-meter as it reads. */
export interface Ring { lat: number; lng: number; maxR: number; speed: number; period: number; rgb: string; alpha?: number; live?: boolean }
export interface Arc { sLat: number; sLng: number; eLat: number; eLng: number; dash: number; gap: number; anim: number; stroke: number; colors: string[] }
/** The great-circle path from a transmitter home (each view draws it on). */
export interface Path { site: LatLon }
export interface Fx { station: Pin | null; rings: Ring[]; arcs: Arc[]; path: Path | null }
export const NO_FX: Fx = { station: null, rings: [], arcs: [], path: null };
/** A scan's pings (glowing markers that pulse once) and arcs, each until a time (performance.now()). */
export interface ScanFx { marks: (Pin & { until: number })[]; arcs: (Arc & { until: number })[] }
export const NO_SCAN_FX: ScanFx = { marks: [], arcs: [] };

export function mkPin(cls: string, title: string, sub: string, flag: string | null = null): HTMLElement {
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
export function mkPulse(cls: string, sizePx: number, alpha: number): HTMLElement {
  const el = document.createElement('div');
  el.className = 'pulse ' + cls;
  el.style.setProperty('--s', `${Math.round(sizePx)}px`);
  el.style.setProperty('--a', alpha.toFixed(2));
  return el;
}

const esc = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

/** A site's tooltip (html): its name and flag, country and "n of m on the air". */
export function siteTip(s: SiteDot, label: { country: string; onAir: string }): string {
  const url = flagUrl(s.flag);
  const flag = url ? `<img class="flag" src="${esc(url)}" alt="">` : '';
  return `<div class="site-tip"><b>${flag}${esc(s.name)}</b><span>${esc(label.country)}</span><span>${esc(label.onAir)}</span></div>`;
}

/** Transmitter sites: short towers where something is on the air (taller with more), flat
 *  dim dots elsewhere, so they stand out from the city lights; the active site is white, sites
 *  listened to (the Listened lens) glow amber, larger with more time (`listened` 0..1), sites
 *  of stations a scan logged are green (a little larger), and sites of the listener's own
 *  stations violet. Radius in degrees. */
export const siteColor = (s: SiteDot, active: boolean, logged: boolean, listened?: number) =>
  active ? '#ffffff'
  : listened !== undefined ? `rgba(255,179,63,${(0.55 + 0.45 * listened).toFixed(2)})`
  : logged ? `rgba(79,200,164,${s.on ? 1 : 0.7})`
  : s.mine > 0 ? `rgba(190,160,255,${s.on ? 1 : 0.75})`
  : s.on ? `rgba(255,226,160,${s.precise ? 1 : 0.75})` : `rgba(150,185,220,${s.precise ? 0.6 : 0.35})`;
export const siteRadius = (s: SiteDot, active: boolean, logged: boolean, listened?: number) =>
  (active ? 0.6 : s.on ? 0.45 : 0.32) + (active ? 0 : listened !== undefined ? 0.15 + 0.5 * listened : logged ? 0.1 : 0);
export const siteAltitude = (s: SiteDot, active: boolean) => (active ? 0.05 : s.on ? 0.012 + Math.min(0.03, s.on * 0.002) : 0.003);

export function route(site: LatLon, qth: LatLon): Pick<Fx, 'arcs' | 'path'> {
  const [a, b] = [site, qth];
  return {
    arcs: [
      { sLat: a.lat, sLng: a.lon, eLat: b.lat, eLng: b.lon, dash: 1, gap: 0, anim: 0, stroke: 0.22, colors: [`rgba(${AMBER},0.4)`, `rgba(${COOL},0.4)`] },
      { sLat: a.lat, sLng: a.lon, eLat: b.lat, eLng: b.lon, dash: 0.16, gap: 1.2, anim: 2600, stroke: 0.55, colors: ['rgba(255,228,175,1)', `rgba(${COOL},1)`] },
    ],
    path: { site },
  };
}

export const qthRing = (q: LatLon): Ring => ({ lat: q.lat, lng: q.lon, maxR: 4.5, speed: 2, period: 2200, rgb: COOL, live: true });
export const sonarRing = (q: LatLon): Ring => ({ lat: q.lat, lng: q.lon, maxR: 25, speed: 6, period: 2500, rgb: COOL, alpha: 0.6 });

/** A ring's size and pace now: the received ring's follow the S-meter (`level`, smoothed dB). */
export function ringNow(r: Ring, level: number | null): { maxR: number; speed: number; period: number; alpha: number } {
  return r.live ? receivedRing(level) : { maxR: r.maxR, speed: r.speed, period: r.period, alpha: r.alpha ?? 1 };
}

/** A ring's colour `t` of the way out (0..1), at opacity `alpha`. */
export const ringColor = (r: Ring, t: number, alpha: number) => `rgba(${r.rgb},${(Math.pow(1 - t, 1.6) * 0.9 * alpha).toFixed(3)})`;
