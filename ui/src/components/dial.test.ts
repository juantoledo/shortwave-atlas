import { describe, expect, it } from 'vitest';
import type { Band } from '../types/generated/Band';
import { adjacentBand, bandBars, bandEntryHz, bandWindow, DIAL_MAX_KHZ, DIAL_MIN_KHZ, dialPct, dialTicks, markPaths, windowPct, windowTicks } from './dial';

const BANDS: Band[] = [
  { id: '49m', lo_khz: 5800, hi_khz: 6300 },
  { id: '31m', lo_khz: 9300, hi_khz: 9950 },
  { id: '25m', lo_khz: 11500, hi_khz: 12200 },
];

describe('dial', () => {
  it('maps the band edges to the dial ends', () => {
    expect(dialPct(DIAL_MIN_KHZ)).toBe(0);
    expect(dialPct(DIAL_MAX_KHZ)).toBe(100);
  });

  it('ticks every MHz from 2 to 30', () => {
    const t = dialTicks();
    expect(t[0]).toBe(2);
    expect(t.at(-1)).toBe(30);
  });

  it('draws one mark per position, on-air ones apart', () => {
    const p = markPaths([1_700_000, 6_070_000, 6_070_000, 9_410_000, 31_000_000], [9_410_000]);
    expect(p.off).toBe('M0 0V1M154.5 0V1');
    expect(p.on).toBe('M272.5 0V1');
    expect(markPaths([], []).off).toBe('');
  });

  it('draws marks over a window only', () => {
    const p = markPaths([9_300_000, 9_400_000, 9_500_000, 12_000_000], [9_500_000], 9300, 9500);
    expect(p.off).toBe('M0 0V1M500 0V1');
    expect(p.on).toBe('M1000 0V1');
  });
});

describe('band dial', () => {
  it('zooms on the band with a margin', () => {
    const w = bandWindow(9420, BANDS);
    expect(w.band?.id).toBe('31m');
    expect(w).toMatchObject({ lo: 9300 - 52, hi: 9950 + 52 });
    expect(windowPct(9420, w)).toBeGreaterThan(0);
    expect(windowPct(9420, w)).toBeLessThan(100);
  });

  it('keeps a still slice outside the bands', () => {
    const a = bandWindow(7310, BANDS), b = bandWindow(7390, BANDS);
    expect(a).toEqual({ lo: 7250, hi: 7450, band: null });
    expect(b).toEqual(a);
    expect(bandWindow(20, BANDS).lo).toBe(0);
  });

  it('ticks with a round step, about a dozen labels', () => {
    const t = windowTicks({ lo: 9248, hi: 10002, band: null });
    expect(t.major).toEqual([9300, 9400, 9500, 9600, 9700, 9800, 9900, 10000]);
    expect(t.minor.length).toBeGreaterThan(t.major.length);
    expect(windowTicks({ lo: 7250, hi: 7450, band: null }).major).toEqual([7250, 7275, 7300, 7325, 7350, 7375, 7400, 7425, 7450]);
  });

  it('steps to the neighbour bands', () => {
    expect(adjacentBand(9420, BANDS, 1)?.id).toBe('25m');
    expect(adjacentBand(9420, BANDS, -1)?.id).toBe('49m');
    expect(adjacentBand(7300, BANDS, 1)?.id).toBe('31m');
    expect(adjacentBand(7300, BANDS, -1)?.id).toBe('49m');
    expect(adjacentBand(12000, BANDS, 1)).toBeNull();
    expect(adjacentBand(5900, BANDS, -1)).toBeNull();
  });

  it('enters a band on its lowest on-air frequency, else at its edge', () => {
    expect(bandEntryHz(BANDS[1], [9_900_000, 9_420_000, 6_000_000])).toBe(9_420_000);
    expect(bandEntryHz(BANDS[1], [6_000_000])).toBe(9_300_000);
  });

  it('places band bars on the whole dial', () => {
    const [b] = bandBars(BANDS);
    expect(b).toMatchObject({ id: '49m', label: '49' });
    expect(b.left).toBeCloseTo(dialPct(5800));
    expect(b.width).toBeCloseTo(dialPct(6300) - dialPct(5800));
  });
});
