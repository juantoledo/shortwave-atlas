// The phone remote's logic that is not drawing: tuning steps, the drag strip, the lists' order and
// the now-playing lines.

import { compass, fmtDur, fmtKm, hhmm, type Messages } from '../i18n';
import type { Names } from '../catalog/names';
import type { Band } from '../types/generated/Band';
import type { Candidate } from '../types/generated/Candidate';
import type { ScanStatus } from '../types/generated/ScanStatus';
import type { StationRow } from '../types/generated/StationRow';

/** The drag strip: this many px per step. */
export const DRAG_PX = 14;

/** One step of the drag strip, in Hz: 100 Hz in the sidebands and CW, else 1 kHz. */
export function fineStepHz(mode: string | null): number {
  return mode && /^(USB|LSB|CW|CWR)$/.test(mode) ? 100 : 1000;
}

/** Whole steps a drag of `dx` px has made (toward zero, so a small wobble tunes nothing). */
export const dragSteps = (dx: number, px = DRAG_PX): number => Math.trunc(dx / px) || 0;

/** The buttons' steps, in kHz. */
export const STEPS_KHZ = [-5, -1, 1, 5] as const;

/** The band whose stations the On air tab lists: a broadcast band (the schedule is broadcast). */
export const listBand = (b: Band | null): string | null => (b?.kind === 'broadcast' ? b.id : null);

/** On the air first, each part in the order it came (frequency). */
export function onAirFirst(rows: readonly StationRow[]): StationRow[] {
  return [...rows.filter((r) => r.on), ...rows.filter((r) => !r.on)];
}

/** The keypad's text after a key (a digit, `.` or `back`): kHz with at most one decimal, which
 *  `parseReadout` reads. */
export function keypadPress(text: string, key: string): string {
  if (key === 'back') return text.slice(0, -1);
  if (key === '.') return text.includes('.') ? text : (text || '0') + '.';
  if (!/^\d$/.test(key)) return text;
  const [whole, frac] = text.split('.');
  if (frac !== undefined ? frac.length >= 1 : whole.length >= 6) return text;
  return text === '0' ? key : text + key;
}

/** What the card says of a candidate: language and site, then distance, bearing and air time. */
export function candLines(t: Messages, names: Names, c: Candidate): { what: string; where: string } {
  const slot = c.slot === null ? null : c.station.slots[c.slot];
  const what = [slot && names.lang(slot.lang), c.site.name, names.country(c.site.itu)].filter(Boolean).join(' · ');
  const deg = Math.round(c.route.bearing);
  const air = c.air.minutes === null ? '' : c.air.on
    ? t.endsIn(fmtDur(t, c.air.minutes), hhmm(c.air.edge))
    : t.backAt(hhmm(c.air.edge), fmtDur(t, c.air.minutes));
  const where = [fmtKm(t, c.route.km), `${deg}° ${compass(t, deg)}`, air].filter(Boolean).join(' · ');
  return { what, where };
}

/** How far a running scan has got, in whole percent (0 when none runs). */
export const scanPct = (s: ScanStatus): number =>
  s.state === 'running' && s.total ? Math.round((s.done / s.total) * 100) : 0;

/** The UTC time now, `14:32`. */
export const utcNow = (d: Date) => hhmm(d.getUTCHours() * 60 + d.getUTCMinutes());
