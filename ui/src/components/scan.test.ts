import { describe, expect, it } from 'vitest';
import type { Band } from '../types/generated/Band';
import type { ScanSummary } from '../types/generated/ScanSummary';
import { dbY, gridPoints, lastTwo, noiseFloor, profilePath, scanSecs, stepChoices, tally } from './scan';

const b49: Band = { id: '49m', label: '49', kind: 'broadcast', lo_khz: 5800, hi_khz: 6300, scan_step_hz: 5000 };
const ham10: Band = { id: 'ham-10m', label: '10', kind: 'ham', lo_khz: 28000, hi_khz: 29700, scan_step_hz: 1000 };
const pmr: Band = { id: 'pmr446', label: 'PMR', kind: 'utility', lo_khz: 446000, hi_khz: 446200, scan_step_hz: 12500 };

describe('scan steps', () => {
  it('counts grid points and offers the steps that fit', () => {
    expect(gridPoints(b49, 5000)).toBe(101);
    expect(stepChoices(b49)).toEqual([1000, 2500, 5000, 9000, 10000, 12500, 25000, 100000]);
    // 1 kHz on 10 m is 1701 points: allowed; nothing finer is offered
    expect(stepChoices(ham10)[0]).toBe(1000);
    // 200 kHz wide: 100 kHz still gives 3 points
    expect(stepChoices(pmr)).toContain(12500);
    expect(stepChoices(pmr).every((s) => gridPoints(pmr, s) >= 2)).toBe(true);
  });

  it('takes the noise floor as the core does', () => {
    expect(noiseFloor([null, -40, -50, -45, null])).toBe(-45);
    expect(noiseFloor([-40, -50])).toBe(-50);
    expect(noiseFloor([null])).toBeNull();
  });

  it('estimates the time', () => {
    expect(scanSecs(101)).toBe(51);
    expect(scanSecs(10, 1000)).toBe(12);
  });
});

describe('profile', () => {
  it('maps S-meter readings to heights', () => {
    expect(dbY(-54)).toBe(1);
    expect(dbY(40)).toBe(0);
    expect(dbY(-100)).toBe(1);
    expect(dbY(-7)).toBeCloseTo(0.5);
  });

  it('draws a broken line over the window', () => {
    const p = { first_hz: 5_800_000, step_hz: 5000, samples: [-54, -7, null, -54, 40] };
    expect(profilePath(p, 5800, 5820)).toBe('M0 1.000L250 0.500M750 1.000L1000 0.000');
    // points outside the window are left out
    expect(profilePath(p, 5805, 5815)).toBe('M0 0.500M1000 1.000');
    expect(profilePath({ ...p, samples: [] }, 5800, 5820)).toBe('');
    // filled: each piece closed down to the bottom
    expect(profilePath(p, 5800, 5820, true)).toBe('M0 1.000L250 0.500V1H0ZM750 1.000L1000 0.000V1H750Z');
  });
});

describe('scan summaries', () => {
  const ref = (name: string) => ({ source: 'eibi', key: name, freq_hz: 6_070_000, name, itu: 'CHN', flag: 'cn', site_name: 'x', site_itu: 'CHN', lang: 'M', target: 'CHN' });
  const summary = (id: string, band_id: string): ScanSummary => ({
    version: 1, id, band_id, band_label: '49', band_kind: 'broadcast', first_hz: 5_800_000, step_hz: 5000, points: 101, dwell_ms: 300,
    mode: 'AM', started: 0, finished: 1, outcome: { kind: 'complete' }, qth: { name: 'q', lat: 0, lon: 0 }, rig: 'sim', sources: [{ id: 'eibi', version: 'sked-a26.csv' }],
    noise_db: -48, measured: 101,
    peaks: [
      { freq_hz: 6_070_000, db: -6, at: 0, verdict: 'sure', stations: [ref('CRI')] },
      { freq_hz: 6_100_000, db: -10, at: 0, verdict: 'ambiguous', stations: [ref('A'), ref('B')] },
      { freq_hz: 6_200_000, db: -12, at: 0, verdict: 'unidentified', stations: [] },
    ],
    missed: [ref('C')],
  });

  it('tallies logged and expected stations', () => {
    expect(tally(summary('a', '49m'))).toEqual({ logged: 3, expected: 4, signals: 3 });
  });

  it('finds the last two scans of a band', () => {
    const all = [summary('c', '41m'), summary('b', '49m'), summary('a', '49m'), summary('z', '49m')];
    expect(lastTwo(all, '49m').map((s) => s?.id)).toEqual(['b', 'a']);
    expect(lastTwo(all, '41m').map((s) => s?.id)).toEqual(['c', undefined]);
    expect(lastTwo(all, '31m')).toEqual([null, null]);
  });
});
