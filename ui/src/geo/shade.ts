// Day and night, as both views draw them: the globe's shader (night.ts) builds its GLSL from these
// numbers and the flat map shades its picture with `nightAt` (geo/flatTexture.ts), so the two
// agree. Colours are sRGB values (0..1), composited as the canvas once drew them.

import type { Vec3 } from './geo';

/** Night shading strength and colour. */
export const NIGHT_ALPHA = 0.74;
export const NIGHT_RGB: Vec3 = [2 / 255, 6 / 255, 13 / 255];
/** The grey line: its strength, its half-width (in the cosine of the solar zenith angle) and its
 *  tints at dusk and dawn. */
export const TWILIGHT = 0.07, TWILIGHT_W = 0.035;
export const DUSK: Vec3 = [1.0, 0.62, 0.32], DAWN: Vec3 = [0.5, 0.75, 1.0];
/** Where night falls and where the lights come on (a bit later than dusk), in the cosine of the
 *  solar zenith angle: smoothstep edges. */
export const NIGHT_EDGES = [-0.25, 0.08] as const, LIT_EDGES = [-0.23, 0.02] as const;

export function smoothstep(a: number, b: number, x: number): number {
  const t = Math.min(1, Math.max(0, (x - a) / (b - a)));
  return t * t * (3 - 2 * t);
}

/** The shading at a point whose position is `p` (a unit vector, `toVec`'s frame) with the sun
 *  along `sun`: how dark (`night` 0..1), how lit (`lit` 0..1), the grey line's strength (`twilight`)
 *  and whether the sun has set there (`dusk`) or is about to rise. */
export function shadeAt(p: Vec3, sun: Vec3): { night: number; lit: number; twilight: number; dusk: boolean } {
  const cz = p[0] * sun[0] + p[1] * sun[1] + p[2] * sun[2];
  // east at p: (-sin lon, cos lon, 0), up to cos(lat)
  const east = -p[1] * sun[0] + p[0] * sun[1];
  return {
    night: 1 - smoothstep(NIGHT_EDGES[0], NIGHT_EDGES[1], cz),
    lit: 1 - smoothstep(LIT_EDGES[0], LIT_EDGES[1], cz),
    twilight: TWILIGHT * Math.exp(-(cz * cz) / (TWILIGHT_W * TWILIGHT_W)),
    dusk: east < 0,
  };
}

/** The night layer over the day map as one colour and opacity (sRGB 0..1): the shade at
 *  `NIGHT_ALPHA` and then the grey line's tint, each mixed over what is under it. */
export function nightLayer(night: number, twilight: number, dusk: boolean): { rgb: Vec3; a: number } {
  const a1 = night * NIGHT_ALPHA, a2 = twilight, tint = dusk ? DUSK : DAWN;
  const a = 1 - (1 - a1) * (1 - a2);
  if (a <= 0) return { rgb: [0, 0, 0], a: 0 };
  const rgb = [0, 1, 2].map((i) => (NIGHT_RGB[i] * a1 * (1 - a2) + tint[i] * a2) / a) as Vec3;
  return { rgb, a };
}
