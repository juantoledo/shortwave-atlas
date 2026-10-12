// Math for the flat map (equirectangular, repeating east and west). Its camera is the globe's:
// a centre and an altitude in globe radii, so flights, framing and zoom are shared
// (components/stage/useStage.ts). An altitude shows the scale the globe shows at the centre of a
// square view, across the view's shorter side.

import { RAD, clamp, type LatLon } from './geo';

/** The globe camera's vertical field of view (three-globe's default), in degrees. */
const GLOBE_FOV = 50;
/** Degrees of the map across the view's shorter side per unit of altitude: what the globe shows
 *  at its centre from that height (a small arc at distance `alt` from the camera). */
export const DEG_PER_ALT = (2 * Math.tan((GLOBE_FOV / 2) * RAD)) / RAD;

/** Degrees across the view's shorter side at altitude `alt`. */
export const spanDeg = (alt: number) => alt * DEG_PER_ALT;

/** The map's scale (px per degree) at altitude `alt` in a `w`×`h` view. */
export const pxPerDeg = (alt: number, w: number, h: number) => Math.max(1, Math.min(w, h)) / spanDeg(alt);

/** The altitude with `ppd` px per degree in a `w`×`h` view (`pxPerDeg`'s inverse). */
export const altFor = (ppd: number, w: number, h: number) => Math.max(1, Math.min(w, h)) / (ppd * DEG_PER_ALT);

/** Longitude in -180..180. */
export const wrapLon = (lon: number) => ((((lon + 180) % 360) + 360) % 360) - 180;

/**
 * What the canvas shows: the point `lat, lon` sits at `cx, cy` (canvas px), at `ppd` px per
 * degree. The canvas is `w`×`h`.
 */
export interface FlatView { lat: number; lon: number; ppd: number; cx: number; cy: number; w: number; h: number }

/** Where `p` falls on the canvas, on the world copy nearest the centre. */
export function project(v: FlatView, p: LatLon): [number, number] {
  return [v.cx + wrapLon(p.lon - v.lon) * v.ppd, v.cy - (p.lat - v.lat) * v.ppd];
}

/** The point under canvas px `x, y`, or null off the top or bottom of the world. */
export function invert(v: FlatView, x: number, y: number): LatLon | null {
  const lat = v.lat - (y - v.cy) / v.ppd;
  if (lat > 90 || lat < -90) return null;
  return { lat, lon: wrapLon(v.lon + (x - v.cx) / v.ppd) };
}

/**
 * The centre latitude `lat` held so the canvas never shows past a pole: the world's top edge at
 * or above the canvas's, its bottom at or below. A world shorter than the canvas is centred on it.
 */
export function clampLat(lat: number, ppd: number, cy: number, h: number): number {
  const hi = 90 - cy / ppd, lo = -90 + (h - cy) / ppd;
  return lo > hi ? (h / 2 - cy) / ppd : clamp(lat, lo, hi);
}

/** `e` of the way from `a` to `b` as the flat map pans: straight on the map, the short way round. */
export function lerpFlat(a: LatLon, b: LatLon, e: number): LatLon {
  return { lat: a.lat + (b.lat - a.lat) * e, lon: wrapLon(a.lon + wrapLon(b.lon - a.lon) * e) };
}

/** `pts` with longitudes made continuous (no jump of more than 180° between neighbours), so a
 *  line across the 180th meridian draws in one piece; its copies 360° apart draw the rest. */
export function unwrap(pts: readonly LatLon[]): LatLon[] {
  const out: LatLon[] = [];
  for (const p of pts) {
    const prev = out[out.length - 1];
    out.push(prev ? { lat: p.lat, lon: prev.lon + wrapLon(p.lon - prev.lon) } : { ...p });
  }
  return out;
}

/** The x offsets (multiples of the world's width `worldW`) at which something spanning `x0..x1`
 *  shows on a canvas `w` wide. */
export function copies(x0: number, x1: number, worldW: number, w: number): number[] {
  if (!(worldW > 0)) return [0];
  const out: number[] = [];
  for (let k = Math.ceil(-x1 / worldW) || 0; x0 + k * worldW <= w; k++) out.push(k * worldW);
  return out;
}
