// The timetable's view math: which lanes are in sight, where the hour labels go, and which of
// today's windows hold a minute. The rows and their windows come from the core
// (`Call::Timetable`).

import type { Band } from '../types/generated/Band';
import type { Filters } from '../hooks/useCore';

/** One lane per station, this tall (px; styles.css `.tt-lane`). */
export const LANE_PX = 24;
/** Lanes drawn above and below the ones in sight, so a quick scroll shows no gap. */
const OVERSCAN = 8;

/** The lanes [from, to) to draw for a scroll position. */
export function lanesInSight(scrollTop: number, viewPx: number, n: number, lanePx = LANE_PX): [number, number] {
  const from = Math.min(n, Math.max(0, Math.floor(scrollTop / lanePx) - OVERSCAN));
  const to = Math.min(n, Math.ceil((scrollTop + viewPx) / lanePx) + OVERSCAN);
  return [from, Math.max(from, to)];
}

/** Where to scroll so lane `i` shows, centred; `null` while it is in sight already. */
export function scrollToShow(i: number, scrollTop: number, viewPx: number, lanePx = LANE_PX): number | null {
  const top = i * lanePx;
  if (top >= scrollTop && top + lanePx <= scrollTop + viewPx) return null;
  return Math.max(0, top - (viewPx - lanePx) / 2);
}

/** Stations this close to the tuned frequency share the channel (the core's `LOOKUP_TOLERANCE_HZ`:
 *  the candidates the Station panel offers). */
const CO_CHANNEL_HZ = 1_000;

/** Whether a station on `freqHz` shares the channel the radio is tuned to. */
export const coChannel = (freqHz: number, tunedHz: number | null) => tunedHz !== null && Math.abs(freqHz - tunedHz) <= CO_CHANNEL_HZ;

/** Whether lane `i` starts a frequency: the stations on one share a block, labelled once. */
export const startsFreq = (freqs: (i: number) => number, i: number) => i === 0 || freqs(i - 1) !== freqs(i);

/** Whether `minute` (UTC, 0..1440) falls in one of `windows` (ends exclusive). */
export function onAt(windows: readonly (readonly [number, number])[], minute: number): boolean {
  return windows.some(([s, e]) => minute >= s && minute < e);
}

/** A UTC minute as a share of the day, for `left`/`width` (%). */
export const dayPct = (minute: number) => minute / 14.4;

/** The minute of the day in UTC. */
export const utcMinute = (d: Date) => d.getUTCHours() * 60 + d.getUTCMinutes();

/** Days since 1970-01-01, UTC: the timetable is for this day. */
export const utcDay = (d: Date) => Math.floor(d.getTime() / 86_400_000);

/** The local clock at UTC hour `h`, for the second axis: `HH`, or `HH:MM` off the hour. */
export function localAt(h: number, offsetMin: number): string {
  const m = (((h * 60 + offsetMin) % 1440) + 1440) % 1440;
  const hh = String(Math.floor(m / 60)).padStart(2, '0');
  return m % 60 ? `${hh}:${String(m % 60).padStart(2, '0')}` : hh;
}

/** With the whole schedule in the list: the dial's band only, or every station (up to the cap). */
export type TimetableScope = 'band' | 'all';

/** The list has the whole schedule (no band chosen, no other lens): too many lanes to read at
 *  once, so the timetable's scope switch applies. The other lenses are short already. */
export const wholeSchedule = (f: Filters) => f.lens === 'all' && !f.bands.length;

/** The dial's band if the station filter knows it (broadcast bands only). */
export const scopeBand = (b: Band | null) => (b?.kind === 'broadcast' ? b : null);

/** The query the timetable asks for: the list's filters, narrowed to the dial's band when the
 *  list has the whole schedule and the scope says so. */
export function timetableFilters(f: Filters, dialBand: Band | null, scope: TimetableScope): Filters {
  const band = scopeBand(dialBand);
  if (scope === 'all' || !wholeSchedule(f) || !band) return f;
  return { ...f, bands: [band.id] };
}
