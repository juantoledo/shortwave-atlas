import { describe, expect, it } from 'vitest';
import type { Band } from '../types/generated/Band';
import type { Span } from '../types/generated/Span';
import {
  adjacentBand, bandAt, bandBars, bandEntryHz, bandWindow, clampToCoverage, dialPct, dialTicks, fitLabels, HF_SPAN, markPaths, rangeLabel,
  spanAt, tickLabel, uncovered, windowPct, windowTicks,
} from './dial';

const bc = (id: string, lo_khz: number, hi_khz: number): Band => ({ id, label: id.replace(/m$/, ''), kind: 'broadcast', lo_khz, hi_khz, scan_step_hz: 5000 });
const ham = (len: string, lo_khz: number, hi_khz: number): Band => ({ id: `ham-${len}m`, label: len, kind: 'ham', lo_khz, hi_khz, scan_step_hz: 1000 });

const BANDS: Band[] = [bc('49m', 5800, 6300), bc('31m', 9300, 9950), bc('25m', 11500, 12200)];
/** Region 2 around 40 m: the ham band runs into the 41 m broadcast band. */
const R2: Band[] = [bc('49m', 5800, 6300), ham('40', 7000, 7300), bc('41m', 7200, 7600), bc('31m', 9300, 10000), ham('30', 10100, 10150)];
const VHF: Span = { id: 'vhf', lo_khz: 30000, hi_khz: 300000 };
const MF: Span = { id: 'mf', lo_khz: 100, hi_khz: 1700 };

describe('dial', () => {
  it('maps the span edges to the dial ends', () => {
    expect(dialPct(1700, HF_SPAN)).toBe(0);
    expect(dialPct(30000, HF_SPAN)).toBe(100);
    expect(dialPct(165000, VHF)).toBe(50);
  });

  it('ticks every MHz on HF, labelled every 5 with the unit once', () => {
    const t = dialTicks(HF_SPAN);
    expect(t[0].khz).toBe(2000);
    expect(t.at(-1)).toMatchObject({ khz: 30000, label: '30', end: true });
    expect(t.filter((k) => k.label).map((k) => k.label)).toEqual(['5 MHz', '10', '15', '20', '25', '30']);
  });

  it('ticks other spans in their own steps and units', () => {
    expect(dialTicks(MF).filter((k) => k.label).map((k) => k.label)).toEqual(['500 kHz', '1000', '1500']);
    expect(dialTicks(VHF).filter((k) => k.label).map((k) => k.label)).toEqual(['50 MHz', '100', '150', '200', '250', '300']);
  });

  it('finds the span of a frequency, the nearest one outside them', () => {
    const spans = [MF, HF_SPAN, VHF];
    expect(spanAt(1000, spans).id).toBe('mf');
    expect(spanAt(1700, spans).id).toBe('hf');
    expect(spanAt(145000, spans).id).toBe('vhf');
    expect(spanAt(300000, spans).id).toBe('vhf');
    expect(spanAt(400000, spans).id).toBe('vhf');
    expect(spanAt(9000, []).id).toBe('hf');
  });

  it('draws one mark per position, on-air ones apart', () => {
    const p = markPaths([1_700_000, 6_070_000, 6_070_000, 9_410_000, 31_000_000], [9_410_000], 1700, 30000);
    expect(p.off).toBe('M0 0V1M154.5 0V1');
    expect(p.on).toBe('M272.5 0V1');
    expect(markPaths([], [], 1700, 30000).off).toBe('');
  });

  it('draws marks over a window only', () => {
    const p = markPaths([9_300_000, 9_400_000, 9_500_000, 12_000_000], [9_500_000], 9300, 9500);
    expect(p.off).toBe('M0 0V1M500 0V1');
    expect(p.on).toBe('M1000 0V1');
  });

  it('puts broadcast bands on the upper lane and cuts bars at the span ends', () => {
    const bars = bandBars([...R2, bc('mw', 525, 1705)], HF_SPAN);
    expect(bars.find((b) => b.id === '41m')).toMatchObject({ lane: 0, label: '41' });
    expect(bars.find((b) => b.id === 'ham-40m')).toMatchObject({ lane: 1, kind: 'ham' });
    // MW's top 5 kHz reach into HF, but it belongs on LW/MW, cut at the span's end
    expect(bars.find((b) => b.id === 'mw')).toBeUndefined();
    const mw = bandBars([bc('mw', 525, 1705)], MF)[0];
    expect(mw.left).toBeCloseTo(dialPct(525, MF));
    expect(mw.left + mw.width).toBe(100);
    expect(bandBars(R2, VHF)).toEqual([]);
  });

  it('labels as many bars as fit, per lane', () => {
    const bars = bandBars(R2, HF_SPAN);
    // wide: every label
    expect(fitLabels(bars, 2000, 7).size).toBe(bars.length);
    // narrow: 41 is too close to 49 and drops out; the ham lane keeps both of its labels
    const narrow = fitLabels(bars, 300, 7);
    expect([...narrow].sort()).toEqual(['31m', '49m', 'ham-30m', 'ham-40m']);
    // the two lanes do not crowd each other: 40 and 41 overlap but sit on different lanes
    const tiny = fitLabels(bandBars([ham('40', 7000, 7300), bc('41m', 7200, 7600)], HF_SPAN), 300, 7);
    expect(tiny.size).toBe(2);
    expect(fitLabels(bandBars([bc('41m', 7200, 7600), bc('42m', 7300, 7700)], HF_SPAN), 300, 7).size).toBe(1);
  });

  it('hatches what the rig does not receive', () => {
    expect(uncovered([{ lo_khz: 30, hi_khz: 56000 }], HF_SPAN)).toEqual([]);
    const g = uncovered([{ lo_khz: 30, hi_khz: 56000 }], VHF);
    expect(g).toHaveLength(1);
    expect(g[0].left).toBeCloseTo(dialPct(56000, VHF));
    expect(g[0].left + g[0].width).toBeCloseTo(100);
    const two = uncovered([{ lo_khz: 144000, hi_khz: 146000 }, { lo_khz: 30, hi_khz: 60000 }], VHF);
    expect(two.map((x) => Math.round(x.left))).toEqual([Math.round(dialPct(60000, VHF)), Math.round(dialPct(146000, VHF))]);
  });

  it('keeps tuning within the coverage', () => {
    const cov = [{ lo_khz: 30, hi_khz: 56000 }, { lo_khz: 144000, hi_khz: 146000 }];
    expect(clampToCoverage(7_100_000, cov)).toBe(7_100_000);
    expect(clampToCoverage(10_000, cov)).toBe(30_000);
    expect(clampToCoverage(60_000_000, cov)).toBe(56_000_000);
    expect(clampToCoverage(140_000_000, cov)).toBe(144_000_000);
    expect(clampToCoverage(500_000_000, cov)).toBe(146_000_000);
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

  it('prefers the broadcast band where bands overlap', () => {
    expect(bandAt(7250, R2)?.id).toBe('41m');
    expect(bandAt(7100, R2)?.id).toBe('ham-40m');
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
    expect(windowTicks({ lo: 417600, hi: 452400, band: null }).major).toEqual([420000, 425000, 430000, 435000, 440000, 445000, 450000]);
  });

  it('counts in MHz above shortwave', () => {
    expect(tickLabel(9400)).toBe('9400');
    expect(tickLabel(145500)).toBe('145.5');
    expect(rangeLabel(bc('31m', 9300, 10000))).toBe('9300–10000');
    expect(rangeLabel(ham('2', 144000, 148000))).toBe('144–148 MHz');
  });

  it('steps to the neighbour bands', () => {
    expect(adjacentBand(9420, BANDS, 1)?.id).toBe('25m');
    expect(adjacentBand(9420, BANDS, -1)?.id).toBe('49m');
    expect(adjacentBand(7300, BANDS, 1)?.id).toBe('31m');
    expect(adjacentBand(7300, BANDS, -1)?.id).toBe('49m');
    expect(adjacentBand(12000, BANDS, 1)).toBeNull();
    expect(adjacentBand(5900, BANDS, -1)).toBeNull();
  });

  it('steps through overlapping bands one by one', () => {
    const forty = R2[1], fortyOne = R2[2];
    // in 40 m: up to 41 m, which starts inside it
    expect(adjacentBand(7050, R2, 1, forty)?.id).toBe('41m');
    expect(adjacentBand(7250, R2, 1, fortyOne)?.id).toBe('31m');
    expect(adjacentBand(9400, R2, 1)?.id).toBe('ham-30m');
    // and back down
    expect(adjacentBand(7250, R2, -1, fortyOne)?.id).toBe('ham-40m');
    expect(adjacentBand(7050, R2, -1, forty)?.id).toBe('49m');
  });

  it('enters a band on its lowest on-air frequency, else at its edge', () => {
    expect(bandEntryHz(BANDS[1], [9_900_000, 9_420_000, 6_000_000])).toBe(9_420_000);
    expect(bandEntryHz(BANDS[1], [6_000_000])).toBe(9_300_000);
  });
});
