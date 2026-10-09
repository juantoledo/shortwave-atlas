// Animation math for the globe (camera flights, path drawing, terminator).
// Station distances and bearings come from the core; these only drive visuals.
// Mirrors `crates/atlas-core/src/geo.rs` where the two overlap.

export const RAD = Math.PI / 180;
export const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));

export interface LatLon { lat: number; lon: number }
export type Vec3 = [number, number, number];

/** Central angle in degrees between two points (haversine). */
export function angleDeg(a: LatLon, b: LatLon): number {
  const p1 = a.lat * RAD, p2 = b.lat * RAD, dl = (b.lon - a.lon) * RAD;
  const s = Math.sin((p2 - p1) / 2) ** 2 + Math.cos(p1) * Math.cos(p2) * Math.sin(dl / 2) ** 2;
  return (2 * Math.asin(Math.min(1, Math.sqrt(s)))) / RAD;
}

export function toVec(lat: number, lon: number): Vec3 {
  const p = lat * RAD, l = lon * RAD;
  return [Math.cos(p) * Math.cos(l), Math.cos(p) * Math.sin(l), Math.sin(p)];
}

export function fromVec(v: Vec3): LatLon {
  return { lat: Math.asin(clamp(v[2], -1, 1)) / RAD, lon: Math.atan2(v[1], v[0]) / RAD };
}

/** Spherical interpolation between unit vectors. */
export function slerp(a: Vec3, b: Vec3, t: number): Vec3 {
  const d = clamp(a[0] * b[0] + a[1] * b[1] + a[2] * b[2], -1, 1);
  const w = Math.acos(d);
  const s = Math.sin(w);
  if (w < 1e-6 || s < 1e-9) return [a[0], a[1], a[2]];
  const k1 = Math.sin((1 - t) * w) / s, k2 = Math.sin(t * w) / s;
  return [a[0] * k1 + b[0] * k2, a[1] * k1 + b[1] * k2, a[2] * k1 + b[2] * k2];
}

/** n+1 points along the great circle from a to b. */
export function greatCircle(a: LatLon, b: LatLon, n: number): LatLon[] {
  const va = toVec(a.lat, a.lon), vb = toVec(b.lat, b.lon);
  return Array.from({ length: n + 1 }, (_, i) => fromVec(slerp(va, vb, i / n)));
}

/** Midpoint on the great circle. */
export function midpoint(a: LatLon, b: LatLon): LatLon {
  return fromVec(slerp(toVec(a.lat, a.lon), toVec(b.lat, b.lon), 0.5));
}

/** Sub-solar point (low-precision: good to a fraction of a degree, enough for the terminator). */
export function sunPos(d: Date): LatLon {
  const n = (d.getTime() - Date.UTC(d.getUTCFullYear(), 0, 0)) / 864e5;
  const B = (2 * Math.PI * (n - 81)) / 364;
  const eot = 9.87 * Math.sin(2 * B) - 7.53 * Math.cos(B) - 1.5 * Math.sin(B);
  const decl = -23.44 * Math.cos((2 * Math.PI * (n + 10)) / 365);
  const h = d.getUTCHours() + d.getUTCMinutes() / 60 + d.getUTCSeconds() / 3600;
  const lon = (((-15 * (h - 12 + eot / 60)) + 540) % 360) - 180;
  return { lat: decl, lon };
}

/**
 * Camera altitude (in globe radii) that looks the same in a view of `aspect` (width / height)
 * as `alt` does in a square one. The camera's field of view is vertical, so wide views change
 * nothing; tall views pull back until the globe takes the same share of the width.
 */
export function fitAltitude(alt: number, aspect: number): number {
  if (!(aspect > 0) || aspect >= 1) return alt;
  const th = Math.asin(1 / (1 + alt));
  const fitted = Math.atan(aspect * Math.tan(th));
  return 1 / Math.sin(fitted) - 1;
}

/** Ease in-out cubic, for camera flights. */
export const easeInOut = (k: number) => (k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2);
