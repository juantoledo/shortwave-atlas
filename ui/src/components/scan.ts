// Band scans on the dial: the strength profile as an SVG path over the band dial's window, the
// steps the picker offers, how long a scan takes, and what a band's last scan says (its chip).

import type { Band } from '../types/generated/Band';
import type { ScanSummary } from '../types/generated/ScanSummary';
import { MARKS_W } from './dial';

/** Steps the picker offers, in Hz (the core accepts 0.5 kHz to 1 MHz). */
export const STEP_CHOICES_HZ = [1000, 2500, 5000, 9000, 10000, 12500, 25000, 100000];
/** The core's limits (`scan::MAX_STEPS`, `DEFAULT_DWELL_MS`). */
export const MAX_STEPS = 2000;
export const DEFAULT_DWELL_MS = 300;
/** Per step on top of the dwell: three meter reads 60 ms apart and the CAT round trips. */
const STEP_OVERHEAD_MS = 200;
/** The profile's scale: S0 at the bottom, S9+40 dB at the top. */
const FLOOR_DB = -54, TOP_DB = 40;

/** Grid points of a scan of `b` at `stepHz`. */
export const gridPoints = (b: Band, stepHz: number) => Math.floor(((b.hi_khz - b.lo_khz) * 1000) / stepHz) + 1;

/** The steps that fit a band (at most `MAX_STEPS` points, and at least two); the band's own always. */
export function stepChoices(b: Band): number[] {
  const ok = STEP_CHOICES_HZ.filter((s) => s === b.scan_step_hz || (gridPoints(b, s) <= MAX_STEPS && gridPoints(b, s) >= 2));
  return ok.includes(b.scan_step_hz) ? ok : [...ok, b.scan_step_hz].sort((x, y) => x - y);
}

/** Seconds a scan of `points` takes. */
export const scanSecs = (points: number, dwellMs = DEFAULT_DWELL_MS) => Math.round((points * (dwellMs + STEP_OVERHEAD_MS)) / 1000);

/** `5`, `2.5`, `12.5`: a step in kHz. */
export const stepKhz = (hz: number) => String(hz / 1000);

/** What a profile is drawn from: a running scan or a saved one. */
export interface Profile { first_hz: number; step_hz: number; samples: readonly (number | null)[] }

/** dB relative to S9 as a height: 0 at the top, 1 at the bottom. */
export const dbY = (db: number) => 1 - Math.min(1, Math.max(0, (db - FLOOR_DB) / (TOP_DB - FLOOR_DB)));

/** The profile over [lo, hi] kHz in the marks' viewBox (`MARKS_W` × 1): a line broken where
 *  nothing was measured (points outside the window are left out), or with `area` each piece
 *  closed down to the bottom, to fill. */
export function profilePath(p: Profile, lo: number, hi: number, area = false): string {
  const runs: [number, number][][] = [];
  let run: [number, number][] | null = null;
  p.samples.forEach((db, i) => {
    const khz = (p.first_hz + i * p.step_hz) / 1000;
    if (db === null || khz < lo || khz > hi) {
      run = null;
      return;
    }
    if (!run) runs.push((run = []));
    run.push([Math.round(((khz - lo) / (hi - lo)) * MARKS_W * 10) / 10, dbY(db)]);
  });
  return runs
    .map((r) => {
      const line = r.map(([x, y], i) => `${i ? 'L' : 'M'}${x} ${y.toFixed(3)}`).join('');
      return area ? `${line}V1H${r[0][0]}Z` : line;
    })
    .join('');
}

/** The lower median of the readings so far, as the core takes the noise floor; `null` before any. */
export function noiseFloor(samples: readonly (number | null)[]): number | null {
  const v = samples.filter((x): x is number => x !== null).sort((a, b) => a - b);
  return v.length ? v[Math.floor((v.length - 1) / 2)] : null;
}

/** dB over the noise floor that make a signal (the core's `SIGNAL_DB`). */
export const SIGNAL_DB = 6;

/** A scan's band, for `Names.dialBand`. */
export const bandOf = (s: ScanSummary) => ({ id: s.band_id, label: s.band_label, kind: s.band_kind });

/** Stations a scan logged (signals with a match), and those it expected (logged + missed). */
export function tally(s: ScanSummary): { logged: number; expected: number; signals: number } {
  const logged = s.peaks.reduce((n, p) => n + (p.verdict === 'unidentified' ? 0 : p.stations.length), 0);
  return { logged, expected: logged + s.missed.length, signals: s.peaks.length };
}

/** The newest scan of a band and the one before it (scans come newest first). */
export function lastTwo(scans: readonly ScanSummary[], bandId: string): [ScanSummary | null, ScanSummary | null] {
  const of = scans.filter((s) => s.band_id === bandId);
  return [of[0] ?? null, of[1] ?? null];
}
