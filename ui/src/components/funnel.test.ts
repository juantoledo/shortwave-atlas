import { describe, expect, it } from 'vitest';
import { FUNNEL_W, funnelFill, funnelLine, funnelOutline, funnelTiers, funnelX, halfHeight } from './funnel';

/** The points of a path of `M`/`L` commands. */
const points = (d: string) => d.replace(/[MLZ]/g, ' ').trim().split(/\s+/).map(Number).reduce<[number, number][]>(
  (acc, v, i, all) => (i % 2 ? acc : [...acc, [v, all[i + 1]]]), []);

describe('the buffer funnel', () => {
  it('gives every doubling the same width, the mouth at 32 s and the outlet at 0.5 s', () => {
    expect(funnelX(32_000)).toBe(0);
    expect(funnelX(500)).toBe(1);
    expect(funnelX(16_000)).toBeCloseTo(1 / 6);
    const tiers = funnelTiers();
    expect(tiers.map((t) => t.ms)).toEqual([1_000, 2_000, 4_000, 8_000, 16_000]);
    const gaps = tiers.slice(1).map((t, i) => tiers[i].pct - t.pct);
    for (const g of gaps) expect(g).toBeCloseTo(100 / 6, 2);
  });

  it('keeps positions inside the funnel', () => {
    expect(funnelX(100_000)).toBe(0);
    expect(funnelX(100)).toBe(1);
    expect(funnelX(0)).toBe(1);
    expect(funnelX(Number.NaN)).toBe(1);
  });

  it('narrows from the mouth to the spout', () => {
    expect(halfHeight(0)).toBe(0.5);
    expect(halfHeight(0.4)).toBeLessThan(halfHeight(0.2));
    expect(halfHeight(funnelX(1_000))).toBe(halfHeight(1));
    expect(points(funnelOutline())).toHaveLength(6);
  });

  it('fills from the outlet with the audio held, inside the outline', () => {
    expect(funnelFill(0)).toBe('');
    expect(funnelFill(400)).toBe('');
    const little = points(funnelFill(800)), lots = points(funnelFill(6_000));
    expect(Math.min(...lots.map(([x]) => x))).toBeLessThan(Math.min(...little.map(([x]) => x)));
    for (const [x, y] of [...little, ...lots]) {
      expect(x).toBeLessThanOrEqual(FUNNEL_W);
      expect(Math.abs(y - 0.5)).toBeLessThanOrEqual(halfHeight(x / FUNNEL_W) + 1e-3);
    }
    expect(lots).toHaveLength(6); // past the throat: it takes the cone's shape
  });

  it('draws a line across where a size sits', () => {
    const l = funnelLine(16_000);
    expect(l.x).toBeCloseTo(FUNNEL_W / 6, 2);
    expect(l.y1).toBeCloseTo(0.5 - halfHeight(1 / 6), 2);
    expect(l.y2).toBeCloseTo(1 - l.y1, 2);
  });
});
