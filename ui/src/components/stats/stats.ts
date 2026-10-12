// The Stats view's arithmetic: bar heights, heat levels, trends against the period before,
// timeline labels and the sparkline path. No React, so it is tested on its own.

import type { Bucket } from '../../types/generated/Bucket';
import type { Period } from '../../types/generated/Period';
import type { TrendPoint } from '../../types/generated/TrendPoint';

/** Each value as a share (0..1) of the largest; all zero stays zero. */
export function shares(values: readonly number[]): number[] {
  const max = Math.max(0, ...values);
  return values.map((v) => (max > 0 ? v / max : 0));
}

/** A heat cell's level 0..1. Square root, so a few minutes still show next to hours. */
export function heatLevel(v: number, max: number): number {
  return max > 0 && v > 0 ? Math.max(0.08, Math.sqrt(v / max)) : 0;
}

export type Trend = { dir: 'up' | 'down' | 'flat'; pct: number | null };

/** `now` against `before`: the direction and the change in percent (none from zero). */
export function trend(now: number, before: number | null | undefined): Trend | null {
  if (before === null || before === undefined) return null;
  if (now === before) return { dir: 'flat', pct: 0 };
  const pct = before > 0 ? Math.round(((now - before) / before) * 100) : null;
  // under half a percent either way reads as no change
  if (pct === 0) return { dir: 'flat', pct: 0 };
  return { dir: now > before ? 'up' : 'down', pct };
}

/** The label under a timeline bucket, in the listener's own time. */
export function bucketLabel(locale: string, period: Period, b: Bucket, spanYears = false): string {
  const d = new Date(b.start * 1000);
  const f = (o: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat(locale, o).format(d);
  switch (period) {
    case 'day': return f({ hour: '2-digit', hourCycle: 'h23' });
    case 'week': return f({ weekday: 'short' });
    case 'month': return f({ day: 'numeric' });
    case 'year': return f({ month: 'short' });
    case 'all': return spanYears ? f({ year: 'numeric' }) : f({ month: 'short', year: '2-digit' });
  }
}

/** Buckets of All a year apart (the core switches to years after three). */
export const yearly = (bs: readonly Bucket[]) => bs.length > 1 && bs[1].start - bs[0].start > 40 * 86_400;

/** Which bucket labels to print so they don't collide: every `n`th. */
export const labelEvery = (count: number, room: number) => Math.max(1, Math.ceil(count / Math.max(1, room)));

/** The sparkline of a station's weekly signal in a `w`×`h` box, dB mapped from `lo` to `hi`;
 *  weeks without sessions break the line. */
export function sparkPath(points: readonly TrendPoint[], w: number, h: number, lo = -60, hi = 20): string {
  if (points.length < 2) return '';
  const x = (i: number) => Math.round((i / (points.length - 1)) * w * 10) / 10;
  const y = (db: number) => Math.round((1 - (Math.min(hi, Math.max(lo, db)) - lo) / (hi - lo)) * h * 10) / 10;
  let d = '', pen = false;
  points.forEach((p, i) => {
    if (p.db === null) { pen = false; return; }
    d += `${pen ? 'L' : 'M'}${x(i)} ${y(p.db)}`;
    pen = true;
  });
  return d;
}

/** Day, twilight and night of a band as shares of its time. */
export function split(day: number, twilight: number, night: number): [number, number, number] {
  const all = day + twilight + night;
  return all > 0 ? [day / all, twilight / all, night / all] : [0, 0, 0];
}

/** The newest badge earned (Unix seconds), for the "new" mark on the Collection tab. */
export const newestEarned = (earned: readonly (number | null)[]) => Math.max(0, ...earned.map((e) => e ?? 0));
