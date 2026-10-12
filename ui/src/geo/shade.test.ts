import { describe, expect, it } from 'vitest';
import { toVec } from './geo';
import { DAWN, DUSK, NIGHT_ALPHA, nightLayer, shadeAt, TWILIGHT } from './shade';

const SUN = toVec(0, 0);

describe('day and night', () => {
  it('is day under the sun and night at the antipode', () => {
    const day = shadeAt(toVec(0, 0), SUN), night = shadeAt(toVec(0, 180), SUN);
    expect(day.night).toBe(0);
    expect(day.lit).toBe(0);
    expect(night.night).toBe(1);
    expect(night.lit).toBe(1);
  });

  it('lights the cities a little after night falls', () => {
    // just past the terminator: darkening, lights not yet full
    const s = shadeAt(toVec(0, 95), SUN);
    expect(s.night).toBeGreaterThan(s.lit);
  });

  it('draws the grey line on the terminator, dusk to the east of the sun', () => {
    const east = shadeAt(toVec(0, 90), SUN), west = shadeAt(toVec(0, -90), SUN);
    expect(east.twilight).toBeCloseTo(TWILIGHT, 9);
    expect(east.dusk).toBe(true);
    expect(west.dusk).toBe(false);
    expect(shadeAt(toVec(0, 0), SUN).twilight).toBeLessThan(1e-6);
  });

  it('composes the shade and the tint as the shader mixes them', () => {
    const full = nightLayer(1, 0, true);
    expect(full.a).toBeCloseTo(NIGHT_ALPHA, 9);
    // only the tint: its colour at its strength
    const line = nightLayer(0, TWILIGHT, true);
    expect(line.a).toBeCloseTo(TWILIGHT, 9);
    line.rgb.forEach((c, i) => expect(c).toBeCloseTo(DUSK[i], 9));
    expect(nightLayer(0, TWILIGHT, false).rgb[2]).toBeCloseTo(DAWN[2], 9);
    expect(nightLayer(0, 0, true).a).toBe(0);
    // one layer over a colour gives what two mixes give
    const base = 0.6, a1 = 0.5 * NIGHT_ALPHA, a2 = 0.05;
    const two = (base * (1 - a1) + (2 / 255) * a1) * (1 - a2) + DUSK[0] * a2;
    const one = nightLayer(0.5, a2, true);
    expect(base * (1 - one.a) + one.rgb[0] * one.a).toBeCloseTo(two, 9);
  });
});
