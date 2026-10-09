// Dial geometry: frequency <-> position, and the station marks as two SVG paths
// (hundreds of frequencies; one <path> each for off-air and on-air marks).
// The band dial zooms on a window: the band around the frequency, or a fixed slice outside them.

import type { Band } from '../types/generated/Band';

export const DIAL_MIN_KHZ = 1700, DIAL_MAX_KHZ = 30000;
/** Width of the marks' SVG viewBox. */
export const MARKS_W = 1000;

/** Percent along the dial for a frequency in kHz. */
export const dialPct = (khz: number) => ((khz - DIAL_MIN_KHZ) / (DIAL_MAX_KHZ - DIAL_MIN_KHZ)) * 100;

/** Whole-MHz ticks on the dial. */
export const dialTicks = () => Array.from({ length: Math.floor(DIAL_MAX_KHZ / 1000) - 1 }, (_, k) => k + 2);

/** Vertical marks (`M x 0 V 1`) for each distinct x in [lo, hi] kHz, rounded to half a viewBox unit. */
function path(hz: Iterable<number>, lo: number, hi: number): string {
  const xs = new Set<number>();
  for (const h of hz) {
    const khz = h / 1000;
    if (khz < lo || khz > hi) continue;
    xs.add(Math.round(((khz - lo) / (hi - lo)) * MARKS_W * 2) / 2);
  }
  return [...xs].sort((a, b) => a - b).map((x) => `M${x} 0V1`).join('');
}

/** Paths for frequencies off the air and on the air (an on-air mark hides the off one),
 *  over the whole dial or over [lo, hi] kHz. */
export function markPaths(allHz: readonly number[], onHz: readonly number[], lo = DIAL_MIN_KHZ, hi = DIAL_MAX_KHZ): { off: string; on: string } {
  const on = new Set(onHz);
  return { off: path(allHz.filter((h) => !on.has(h)), lo, hi), on: path(onHz, lo, hi) };
}

/** What the band dial shows, in kHz. */
export interface DialWindow { lo: number; hi: number; band: Band | null }

/** Width of the slice shown outside the bands, and the step it moves by. */
const SLICE_KHZ = 200, SLICE_STEP_KHZ = 100;

export const bandAt = (khz: number, bands: readonly Band[]) => bands.find((b) => khz >= b.lo_khz && khz <= b.hi_khz) ?? null;

/** The band around `khz` with a margin, or a 200 kHz slice aligned to 100 kHz that keeps
 *  `khz` in its middle half (so the window does not slide while you drag). */
export function bandWindow(khz: number, bands: readonly Band[]): DialWindow {
  const band = bandAt(khz, bands);
  if (band) {
    const pad = Math.round(Math.max(10, (band.hi_khz - band.lo_khz) * 0.08));
    return { lo: Math.max(0, band.lo_khz - pad), hi: band.hi_khz + pad, band };
  }
  const lo = Math.max(0, Math.floor(khz / SLICE_STEP_KHZ) * SLICE_STEP_KHZ - (SLICE_KHZ - SLICE_STEP_KHZ) / 2);
  return { lo, hi: lo + SLICE_KHZ, band: null };
}

/** Percent along the band dial. */
export const windowPct = (khz: number, w: DialWindow) => ((khz - w.lo) / (w.hi - w.lo)) * 100;

const TICK_STEPS = [1, 2, 5, 10, 25, 50, 100, 250, 500];

/** Labelled ticks (at most ~12 over the window) and unlabelled ones between them, in kHz. */
export function windowTicks(w: DialWindow): { major: number[]; minor: number[] } {
  const span = w.hi - w.lo;
  const step = TICK_STEPS.find((s) => span / s <= 12) ?? 1000;
  const sub = step >= 5 ? step / 5 : step;
  const major: number[] = [], minor: number[] = [];
  for (let k = Math.ceil(w.lo / sub) * sub; k <= w.hi; k += sub) (k % step === 0 ? major : minor).push(k);
  return { major, minor };
}

/** The next band up (`dir` 1) or down (-1) from `khz`, skipping the band it is in. */
export function adjacentBand(khz: number, bands: readonly Band[], dir: 1 | -1): Band | null {
  const cur = bandAt(khz, bands);
  const sorted = [...bands].sort((a, b) => a.lo_khz - b.lo_khz);
  if (dir > 0) return sorted.find((b) => b.lo_khz > (cur ? cur.hi_khz : khz)) ?? null;
  return sorted.reverse().find((b) => b.hi_khz < (cur ? cur.lo_khz : khz)) ?? null;
}

/** Where a band jump lands, in Hz: the lowest frequency on the air in it, else its lower edge. */
export function bandEntryHz(b: Band, onHz: readonly number[]): number {
  let best = Infinity;
  for (const h of onHz) if (h >= b.lo_khz * 1000 && h <= b.hi_khz * 1000 && h < best) best = h;
  return Number.isFinite(best) ? best : b.lo_khz * 1000;
}

/** The bands as bars on the whole dial (percent), labelled by metres without the "m". */
export function bandBars(bands: readonly Band[]): { id: string; label: string; left: number; width: number }[] {
  return bands
    .filter((b) => b.hi_khz > DIAL_MIN_KHZ && b.lo_khz < DIAL_MAX_KHZ)
    .map((b) => ({ id: b.id, label: b.id.replace(/m$/, ''), left: dialPct(b.lo_khz), width: dialPct(b.hi_khz) - dialPct(b.lo_khz) }));
}
