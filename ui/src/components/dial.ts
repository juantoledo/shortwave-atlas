// Dial geometry: frequency <-> position, and the station marks as two SVG paths
// (hundreds of frequencies; one <path> each for off-air and on-air marks).
// The whole dial shows one span (LW/MW, HF, VHF, UHF) with the bands on two lanes: broadcast
// above, ham/CB/utility below. The band dial zooms on a window: the band around the
// frequency, or a fixed slice outside them.

import type { Band } from '../types/generated/Band';
import type { RxRange } from '../types/generated/RxRange';
import type { Span } from '../types/generated/Span';

/** What the dial shows until the core says what the rig receives. */
export const HF_SPAN: Span = { id: 'hf', lo_khz: 1700, hi_khz: 30000 };
export const DEFAULT_COVERAGE: RxRange[] = [{ lo_khz: 30, hi_khz: 56000 }];
/** Width of the marks' SVG viewBox. */
export const MARKS_W = 1000;

/** Percent along the whole dial for a frequency in kHz. */
export const dialPct = (khz: number, s: Span) => ((khz - s.lo_khz) / (s.hi_khz - s.lo_khz)) * 100;

/** Tick and labelled-tick steps per span, in kHz. */
const SPAN_STEPS: Record<string, [number, number]> = { mf: [100, 500], hf: [1000, 5000], vhf: [10000, 50000], uhf: [50000, 100000] };

export interface DialTick { khz: number; major: boolean; label: string | null; end: boolean }

/** Ticks on the whole dial; the first label carries the unit (MHz, or kHz on LW/MW). */
export function dialTicks(s: Span): DialTick[] {
  const [step, maj] = SPAN_STEPS[s.id] ?? [(s.hi_khz - s.lo_khz) / 20, (s.hi_khz - s.lo_khz) / 4];
  const mhz = s.id !== 'mf';
  const out: DialTick[] = [];
  let first = true;
  for (let k = Math.floor(s.lo_khz / step) * step + step; k <= s.hi_khz; k += step) {
    const major = k % maj === 0;
    let label: string | null = null;
    if (major) {
      const n = mhz ? k / 1000 : k;
      label = first ? `${n} ${mhz ? 'MHz' : 'kHz'}` : String(n);
      first = false;
    }
    out.push({ khz: k, major, label, end: k === s.hi_khz });
  }
  return out;
}

/** The span a frequency is in (an edge belongs to the span above it); the nearest one outside them all. */
export function spanAt(khz: number, spans: readonly Span[]): Span {
  if (!spans.length) return HF_SPAN;
  const inside = spans.find((s) => khz >= s.lo_khz && khz < s.hi_khz) ?? spans.find((s) => khz === s.hi_khz);
  if (inside) return inside;
  const gap = (s: Span) => Math.min(Math.abs(khz - s.lo_khz), Math.abs(khz - s.hi_khz));
  return spans.reduce((a, b) => (gap(b) < gap(a) ? b : a));
}

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

/** Paths for frequencies off the air and on the air (an on-air mark hides the off one) over [lo, hi] kHz. */
export function markPaths(allHz: readonly number[], onHz: readonly number[], lo: number, hi: number): { off: string; on: string } {
  const on = new Set(onHz);
  return { off: path(allHz.filter((h) => !on.has(h)), lo, hi), on: path(onHz, lo, hi) };
}

/** What the band dial shows, in kHz. */
export interface DialWindow { lo: number; hi: number; band: Band | null }

/** Width of the slice shown outside the bands, and the step it moves by. */
const SLICE_KHZ = 200, SLICE_STEP_KHZ = 100;

/** The band at `khz`; where bands overlap (41 m and 40 m), a broadcast one. */
export function bandAt(khz: number, bands: readonly Band[]): Band | null {
  const hits = bands.filter((b) => khz >= b.lo_khz && khz <= b.hi_khz);
  return hits.find((b) => b.kind === 'broadcast') ?? hits[0] ?? null;
}

/** A band with a margin on each side. */
export function windowOf(band: Band): DialWindow {
  const pad = Math.round(Math.max(10, (band.hi_khz - band.lo_khz) * 0.08));
  return { lo: Math.max(0, band.lo_khz - pad), hi: band.hi_khz + pad, band };
}

/** The band around `khz` with a margin, or a 200 kHz slice aligned to 100 kHz that keeps
 *  `khz` in its middle half (so the window does not slide while you drag). */
export function bandWindow(khz: number, bands: readonly Band[]): DialWindow {
  const band = bandAt(khz, bands);
  if (band) return windowOf(band);
  const lo = Math.max(0, Math.floor(khz / SLICE_STEP_KHZ) * SLICE_STEP_KHZ - (SLICE_KHZ - SLICE_STEP_KHZ) / 2);
  return { lo, hi: lo + SLICE_KHZ, band: null };
}

/** Percent along the band dial. */
export const windowPct = (khz: number, w: DialWindow) => ((khz - w.lo) / (w.hi - w.lo)) * 100;

const TICK_STEPS = [1, 2, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000];

/** Labelled ticks (at most ~12 over the window) and unlabelled ones between them, in kHz. */
export function windowTicks(w: DialWindow): { major: number[]; minor: number[] } {
  const span = w.hi - w.lo;
  const step = TICK_STEPS.find((s) => span / s <= 12) ?? 25000;
  const sub = step >= 5 ? step / 5 : step;
  const major: number[] = [], minor: number[] = [];
  for (let k = Math.ceil(w.lo / sub) * sub; k <= w.hi; k += sub) (k % step === 0 ? major : minor).push(k);
  return { major, minor };
}

/** Above shortwave, the band dial counts in MHz. */
export const tickLabel = (khz: number) => (khz >= 30000 ? String(khz / 1000) : String(khz));

/** A band's edges for its name: `9300–10000`, or `144–148 MHz` above shortwave. */
export const rangeLabel = (b: Band) =>
  b.lo_khz >= 30000 ? `${b.lo_khz / 1000}–${b.hi_khz / 1000} MHz` : `${b.lo_khz}–${b.hi_khz}`;

/** The next band up (`dir` 1) or down (-1) from `khz` by lower edge, skipping `cur` (the band
 *  the band dial shows), so overlapping bands such as 40 m and 41 m each get their turn. */
export function adjacentBand(khz: number, bands: readonly Band[], dir: 1 | -1, cur: Band | null = bandAt(khz, bands)): Band | null {
  const from = cur ? cur.lo_khz : khz;
  const sorted = [...bands].sort((a, b) => a.lo_khz - b.lo_khz || a.hi_khz - b.hi_khz);
  const other = (b: Band) => b.id !== cur?.id;
  if (dir > 0) return sorted.find((b) => other(b) && b.lo_khz > from) ?? null;
  return sorted.reverse().find((b) => other(b) && b.lo_khz < from) ?? null;
}

/** Where a band jump lands, in Hz: the lowest frequency on the air in it, else its lower edge. */
export function bandEntryHz(b: Band, onHz: readonly number[]): number {
  let best = Infinity;
  for (const h of onHz) if (h >= b.lo_khz * 1000 && h <= b.hi_khz * 1000 && h < best) best = h;
  return Number.isFinite(best) ? best : b.lo_khz * 1000;
}

export interface BandBar { id: string; label: string; kind: Band['kind']; lane: 0 | 1; left: number; width: number }

/** The bands in a span as bars (percent, cut at the span's ends): broadcast on lane 0, the rest
 *  on lane 1. A band shows on the span that holds most of it (MW's top 5 kHz stays off HF). */
export function bandBars(bands: readonly Band[], s: Span): BandBar[] {
  const inside = (b: Band) => Math.min(b.hi_khz, s.hi_khz) - Math.max(b.lo_khz, s.lo_khz);
  return bands
    .filter((b) => inside(b) >= (b.hi_khz - b.lo_khz) / 2)
    .map((b) => {
      const left = Math.max(0, dialPct(b.lo_khz, s)), right = Math.min(100, dialPct(b.hi_khz, s));
      return { id: b.id, label: b.label, kind: b.kind, lane: b.kind === 'broadcast' ? 0 : 1, left, width: right - left };
    });
}

/** Ids of the bar labels that fit on a dial `widthPx` wide, left to right per lane, each
 *  centred on its bar and `charPx` per character plus a gap. */
export function fitLabels(bars: readonly BandBar[], widthPx: number, charPx: number, gapPx = 6): Set<string> {
  const shown = new Set<string>();
  const edge: [number, number] = [-Infinity, -Infinity];
  for (const b of [...bars].sort((x, y) => x.left + x.width / 2 - (y.left + y.width / 2))) {
    const mid = ((b.left + b.width / 2) / 100) * widthPx, half = (b.label.length * charPx) / 2;
    if (mid - half < edge[b.lane] + gapPx) continue;
    shown.add(b.id);
    edge[b.lane] = mid + half;
  }
  return shown;
}

/** The parts of a span the rig does not receive, as percent stretches. */
export function uncovered(coverage: readonly RxRange[], s: Span): { left: number; width: number }[] {
  const out: { left: number; width: number }[] = [];
  let at = s.lo_khz;
  for (const r of [...coverage].sort((a, b) => a.lo_khz - b.lo_khz)) {
    if (r.hi_khz <= at) continue;
    if (r.lo_khz >= s.hi_khz) break;
    if (r.lo_khz > at) out.push({ left: dialPct(at, s), width: dialPct(r.lo_khz, s) - dialPct(at, s) });
    at = Math.max(at, r.hi_khz);
  }
  if (at < s.hi_khz) out.push({ left: dialPct(at, s), width: 100 - dialPct(at, s) });
  return out;
}

/** `hz` moved into the coverage: kept if the rig receives it, else the nearest edge. */
export function clampToCoverage(hz: number, coverage: readonly RxRange[]): number {
  let best = hz, dist = Infinity;
  for (const r of coverage) {
    const lo = r.lo_khz * 1000, hi = r.hi_khz * 1000;
    if (hz >= lo && hz <= hi) return hz;
    const edge = hz < lo ? lo : hi;
    if (Math.abs(hz - edge) < dist) { dist = Math.abs(hz - edge); best = edge; }
  }
  return best;
}

/** Narrowest heat cell, in the marks' viewBox units. */
const HEAT_MIN_W = 2;

/** The listened channels over [lo, hi] kHz as cells of the marks' viewBox (`MARKS_W` wide): one
 *  channel (`stepKhz`) wide around each, at least `HEAT_MIN_W`, cut at the window's edges. */
export function heatCells(heat: readonly { khz: number; level: number }[], lo: number, hi: number, stepKhz: number): { x: number; w: number; level: number }[] {
  const at = (khz: number) => ((khz - lo) / (hi - lo)) * MARKS_W;
  const out: { x: number; w: number; level: number }[] = [];
  for (const h of heat) {
    if (h.khz < lo || h.khz > hi) continue;
    const w = Math.max(HEAT_MIN_W, at(h.khz + stepKhz / 2) - at(h.khz - stepKhz / 2));
    const x = Math.max(0, Math.min(MARKS_W - w, at(h.khz) - w / 2));
    out.push({ x: Math.round(x * 10) / 10, w: Math.round(w * 10) / 10, level: h.level });
  }
  return out;
}
