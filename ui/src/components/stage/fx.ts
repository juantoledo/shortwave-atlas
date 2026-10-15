// What the globe and the flat map draw, and the numbers that time it: pins, rings, arcs, the
// route, how a site looks. The choreography that sets them is `useStage`.

import { receivedRing, skywave, type LatLon } from '../../geo/geo';
import { flagUrl } from '../../catalog/flags';
import type { SiteDot } from '../../types/generated/SiteDot';
import { NET_DOT_ID } from '../net';

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
/** `live`: the "received" ring, sized and paced by the S-meter as it reads; `alt`: off the ground
 *  (globe radii), where a skywave turns at the ionosphere. */
export interface Ring { lat: number; lng: number; maxR: number; speed: number; period: number; rgb: string; alpha?: number; live?: boolean; alt?: number }
export interface Arc { sLat: number; sLng: number; eLat: number; eLng: number; dash: number; gap: number; anim: number; stroke: number; colors: string[] }
/** The great-circle path from a transmitter home (each view draws it on). */
/** The route home on the ground; `rgb`: its colour (a transmitter's amber unless said). */
export interface Path { site: LatLon; rgb?: string }
/** A transmitter's signal home, as `skywave` gives it: hops off the ionosphere. */
export type Sky = ReturnType<typeof skywave>;
/** A strand of the skywave as both views draw it: `[lat, lon, alt]` points (alt in globe radii),
 *  its colour or a gradient (the globe's wide strands take one colour), a width in px (null: a
 *  hairline), and dashes as arcs have them (lengths a share of the whole; `anim`: ms to run it). */
export interface SkyPath { pts: [number, number, number][]; colors: string | string[]; stroke: number | null; dash: number; gap: number; anim: number }
export interface Fx { station: Pin | null; rings: Ring[]; arcs: Arc[]; path: Path | null; sky: Sky | null }
export const NO_FX: Fx = { station: null, rings: [], arcs: [], path: null, sky: null };
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
 *  dim dots elsewhere, so they stand out from the city lights; the countries of the NET band
 *  are pink, like everything internet on the map; the active site is white, sites
 *  listened to (the Listened lens) glow amber, larger with more time (`listened` 0..1), sites
 *  of stations a scan logged are green (a little larger), and sites of the listener's own
 *  stations violet. Radius in degrees. */
export const siteColor = (s: SiteDot, active: boolean, logged: boolean, listened?: number) =>
  active ? '#ffffff'
  : s.id >= NET_DOT_ID ? `rgba(${NET_RGB},${s.total >= 100 ? 1 : 0.8})`
  : listened !== undefined ? `rgba(255,179,63,${(0.55 + 0.45 * listened).toFixed(2)})`
  : logged ? `rgba(79,200,164,${s.on ? 1 : 0.7})`
  : s.mine > 0 ? `rgba(190,160,255,${s.on ? 1 : 0.75})`
  : s.on ? `rgba(255,226,160,${s.precise ? 1 : 0.75})` : `rgba(150,185,220,${s.precise ? 0.6 : 0.35})`;
export const siteRadius = (s: SiteDot, active: boolean, logged: boolean, listened?: number) =>
  (active ? 0.6 : s.on ? 0.45 : 0.32) + (active ? 0 : listened !== undefined ? 0.15 + 0.5 * listened : logged ? 0.1 : 0);
export const siteAltitude = (s: SiteDot, active: boolean) => (active ? 0.05 : s.on ? 0.012 + Math.min(0.03, s.on * 0.002) : 0.003);

/** An internet station's route home: a stream has no radio path, so a plain arc in its pink. */
export function route(site: LatLon, qth: LatLon): Pick<Fx, 'arcs' | 'path' | 'sky'> {
  const [a, b] = [site, qth];
  return {
    arcs: [
      { sLat: a.lat, sLng: a.lon, eLat: b.lat, eLng: b.lon, dash: 1, gap: 0, anim: 0, stroke: 0.22, colors: [`rgba(${NET_RGB},0.4)`, `rgba(${COOL},0.4)`] },
      { sLat: a.lat, sLng: a.lon, eLat: b.lat, eLng: b.lon, dash: 0.16, gap: 1.2, anim: 2600, stroke: 0.55, colors: ['rgba(255,190,225,1)', `rgba(${COOL},1)`] },
    ],
    path: { site, rgb: NET_RGB },
    sky: null,
  };
}

/** A transmitter's route home: the path on the ground, and the signal hopping over it. */
export const skyRoute = (site: LatLon, qth: LatLon): Pick<Fx, 'arcs' | 'path' | 'sky'> => ({ arcs: [], path: { site }, sky: skywave(site, qth) });

/** The skywave's look by `skyTier` (0: no reading, 1 weak, 2 fair, 3 strong): the pulse's
 *  opacity, width in px, and time per hop. */
const SKY_LOOKS = [
  { alpha: 0.9, stroke: 2, hopMs: 1100 },
  { alpha: 0.65, stroke: 1.5, hopMs: 1500 },
  { alpha: 0.9, stroke: 2, hopMs: 1100 },
  { alpha: 1, stroke: 2.8, hopMs: 750 },
];
/** A pulse's length, in hops, and how many hops apart they run (fewer on a short path). */
const PULSE_HOPS = 0.3, PULSE_EVERY = 2;

/** The skywave's strands, to be given a look (`skyLook`): a hairline over the hops, amber to the
 *  QTH's blue, and the pulses running them home (not with reduced motion). */
export function skyStrands(): SkyPath[] {
  const base: SkyPath = { pts: [], colors: [`rgba(${AMBER},0.45)`, `rgba(${COOL},0.45)`], stroke: null, dash: 1, gap: 0, anim: 0 };
  return REDUCE ? [base] : [base, { pts: [], colors: '', stroke: 0, dash: 1, gap: 0, anim: 0 }];
}

/** The pulse for `tier` over `sky`: its look, and its dashes (one pulse every few hops). */
function pulseOf(sky: Sky, tier: number) {
  const look = SKY_LOOKS[tier] ?? SKY_LOOKS[0];
  const dash = PULSE_HOPS / sky.hops, cycle = Math.min(PULSE_EVERY, sky.hops) / sky.hops;
  return { look, dash, gap: cycle - dash, anim: sky.hops * look.hopMs };
}

/** Lay `skyStrands` over `sky`, the pulse looking as `tier` says, in place. */
export function skyLook(strands: readonly SkyPath[], sky: Sky, tier: number): void {
  for (const s of strands) s.pts = sky.pts;
  const pulse = strands[1];
  if (!pulse) return;
  const { look, dash, gap, anim } = pulseOf(sky, tier);
  Object.assign(pulse, { dash, gap, anim, colors: `rgba(255,228,175,${look.alpha})`, stroke: look.stroke });
}

/** A splash's size off the ground and off the ionosphere (degrees), and how long one lasts (ms). */
const SPLASH_R = 2.4, FLASH_R = 1.6, SPLASH_MS = 700;

/** Where the skywave bounces, a splash each time a pulse gets there: a ring per bounce (off the
 *  ionosphere, smaller, up where it turns), sent as it is added and every pulse after, and when to
 *  add it (ms after the pulse strand showed with the look of `tier`; not before `after`, while the
 *  strand is still growing in). */
export function splashRings(sky: Sky, tier: number, after: number): { ring: Ring; at: number }[] {
  const { look, dash, gap, anim } = pulseOf(sky, tier), period = (dash + gap) * anim;
  return sky.bounces.map((b) => {
    // a pulse's head is at `dash` when the strand shows, and runs its length in `anim`
    let at = ((((b.along - dash) * anim) % period) + period) % period;
    while (at < after) at += period;
    const r = b.alt > 0 ? FLASH_R : SPLASH_R;
    return { ring: { lat: b.lat, lng: b.lon, alt: b.alt, maxR: r, speed: r / (SPLASH_MS / 1000), period, rgb: '255,228,175', alpha: look.alpha }, at };
  });
}

/** The station's point: while listened to, a beat of quick wide rings with a tighter one inside,
 *  in its colour (`rgb`); else `calm` (a slow ring, or none). */
export function beatRings(p: LatLon, rgb: string, listening: boolean, calm: Ring[]): Ring[] {
  if (!listening) return calm;
  const at = { lat: p.lat, lng: p.lon, rgb };
  return [
    { ...at, maxR: 9, speed: 6, period: 1100 },
    { ...at, maxR: 4, speed: 3, period: 550, alpha: 0.8 },
  ];
}

/** A transmitter on the air: its steady ring, or the beat while listened to. */
export const siteRings = (p: LatLon, listening: boolean): Ring[] =>
  beatRings(p, AMBER, listening, [{ lat: p.lat, lng: p.lon, maxR: 7, speed: 3.2, period: 1400, rgb: AMBER }]);

/** An internet station's place: one slow ring while it is only tuned; while it plays, a beat of
 *  quick wide rings with a tighter one inside, its audio going out. */
export function netRings(p: LatLon, playing: boolean): Ring[] {
  return beatRings(p, NET_RGB, playing, [{ lat: p.lat, lng: p.lon, rgb: NET_RGB, maxR: 4, speed: 1.5, period: 2600, alpha: 0.6 }]);
}

/** What arrives at the QTH from an internet station while it plays (no S-meter to follow). */
export const netQthRing = (q: LatLon): Ring => ({ lat: q.lat, lng: q.lon, maxR: 4.5, speed: 2, period: 2200, rgb: COOL });

export const qthRing = (q: LatLon): Ring => ({ lat: q.lat, lng: q.lon, maxR: 4.5, speed: 2, period: 2200, rgb: COOL, live: true });
export const sonarRing = (q: LatLon): Ring => ({ lat: q.lat, lng: q.lon, maxR: 25, speed: 6, period: 2500, rgb: COOL, alpha: 0.6 });

/** A ring's size and pace now: the received ring's follow the S-meter (`level`, smoothed dB). */
export function ringNow(r: Ring, level: number | null): { maxR: number; speed: number; period: number; alpha: number } {
  return r.live ? receivedRing(level) : { maxR: r.maxR, speed: r.speed, period: r.period, alpha: r.alpha ?? 1 };
}

/** A ring's colour `t` of the way out (0..1), at opacity `alpha`. */
export const ringColor = (r: Ring, t: number, alpha: number) => `rgba(${r.rgb},${(Math.pow(1 - t, 1.6) * 0.9 * alpha).toFixed(3)})`;
