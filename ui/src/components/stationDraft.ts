// The station form's pure parts (components/StationForm.tsx): drafts to start from (what is tuned, a station shown, a row off
// the schedule, a station kept), times and days, and what to fix before saving. The core checks
// everything again (atlas_core::user_stations::check).

import type { Candidate } from '../types/generated/Candidate';
import type { Days } from '../types/generated/Days';
import type { Mode } from '../types/generated/Mode';
import type { OffSchedule } from '../types/generated/OffSchedule';
import type { Slot } from '../types/generated/Slot';
import type { UserSite } from '../types/generated/UserSite';
import type { UserSlot } from '../types/generated/UserSlot';
import type { UserStation } from '../types/generated/UserStation';
import type { UserStationDraft } from '../types/generated/UserStationDraft';

/** The core's limits (user_stations.rs). */
export const LIMITS = { name: 80, siteName: 60, notes: 2000, url: 300, slots: 24, langs: 4, minKhz: 10, maxKhz: 2_000_000 };
const ALL_DAYS = 0x7f;

/** The UTC hour now: 23:20 -> 23:00–24:00. */
export function hourSlot(now: Date): Pick<UserSlot, 'start' | 'end'> {
  const h = now.getUTCHours();
  return { start: h * 60, end: (h + 1) * 60 };
}

/** `HH:MM`, `HHMM`, `H` or `HH` as minutes of the day (`24:00` = 1440); null if it is not one. */
export function parseHhmm(text: string): number | null {
  const m = /^\s*(\d{1,2})(?::?(\d{2}))?\s*$/.exec(text);
  if (!m) return null;
  const h = Number(m[1]), min = Number(m[2] ?? 0);
  if (min > 59 || h > 24 || (h === 24 && min > 0)) return null;
  return h * 60 + min;
}

export const fmtHhmm = (m: number) => `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;

/** The weekdays of `d` as a mask, Monday = bit 0 (every day for anything but a weekly pattern). */
export const daysMask = (d: Days) => (d.t === 'weekly' ? d.mask : ALL_DAYS);

/** `d` with weekday `wd` switched; every day is Daily. */
export function toggleDay(d: Days, wd: number): Days {
  const mask = daysMask(d) ^ (1 << wd);
  return mask === ALL_DAYS ? { t: 'daily' } : { t: 'weekly', mask };
}

/** A user slot from a schedule slot: its times, language and target; days other than daily or
 *  weekly (which a station of one's own does not keep) become daily. */
function slotFrom(s: Pick<Slot, 'start' | 'end' | 'days' | 'lang' | 'target'>): UserSlot {
  const days: Days = s.days.t === 'weekly' ? s.days : { t: 'daily' };
  return { start: s.start, end: s.end, days, lang: s.lang, target: s.target };
}

/** A station on the air now, this hour. */
const thisHour = (now: Date, lang = '', target = ''): UserSlot => ({ ...hourSlot(now), days: { t: 'daily' }, lang, target });

const blank = (freqHz: number, mode: Mode, itu: string, site: UserSite, slot: UserSlot): UserStationDraft => ({
  freq_hz: freqHz, mode, name: '', itu, site, slots: [slot], notes: '', url: null,
});

/** What is tuned: its frequency and mode, on the air this hour; the country is for the listener
 *  to say. */
export const draftHere = (freqHz: number, mode: Mode, now: Date): UserStationDraft =>
  blank(freqHz, mode, '', { kind: 'country', itu: '' }, thisHour(now));

/** Another station where `c` is: its frequency, mode, country, site and the language and target
 *  of its slot now (or next), on the air this hour. */
export function draftFrom(c: Candidate, now: Date): UserStationDraft {
  const { station: st, site } = c;
  const slot = c.slot === null ? null : st.slots[c.slot];
  const where: UserSite = site.precise
    ? { kind: 'known', itu: site.itu, code: site.code, name: site.name, lat: site.lat, lon: site.lon }
    : { kind: 'country', itu: site.itu };
  return blank(st.freq_hz, st.mode, st.itu, where, thisHour(now, slot?.lang, slot?.target));
}

/** A row off the schedule (a favorite, a signal the scans heard): its frequency and mode, and
 *  the name and country of the station saved with the favorite, if any. */
export function draftFromOff(o: OffSchedule, rigMode: Mode, now: Date): UserStationDraft {
  const st = o.station;
  const d = blank(o.freq_hz, o.mode ?? rigMode, st?.itu ?? '', { kind: 'country', itu: st?.site_itu ?? st?.itu ?? '' }, thisHour(now, st?.lang, st?.target));
  return { ...d, name: o.label || st?.name || '' };
}

/** A kept station's fields, to edit. */
export function draftOf(u: UserStation): UserStationDraft {
  const { freq_hz, mode, name, itu, site, slots, notes, url } = u;
  return { freq_hz, mode, name, itu, site, slots: slots.map(slotFrom), notes, url };
}

/** A slot to add after `last`: the hour after it, the same days, language and target. */
export function nextSlot(last: UserSlot | undefined, now: Date): UserSlot {
  if (!last) return thisHour(now);
  const start = last.end % 1440;
  return { ...last, start, end: Math.min(start + 60, 1440) };
}

/** A latitude or longitude typed in (a comma for the decimal point too); null if it is not one
 *  within `max`. */
export function parseDeg(text: string, max: number): number | null {
  const t = text.trim().replace(',', '.');
  if (!/^[-+]?\d+(\.\d+)?$/.test(t)) return null;
  const v = Number(t);
  return Math.abs(v) <= max ? v : null;
}

/** kHz typed in as Hz; null if it is not a frequency the core keeps. */
export function parseKhz(text: string): number | null {
  const t = text.trim().replace(',', '.');
  if (!/^\d+(\.\d{1,3})?$/.test(t)) return null;
  const hz = Math.round(Number(t) * 1000);
  return hz >= LIMITS.minKhz * 1000 && hz <= LIMITS.maxKhz * 1000 ? hz : null;
}

export type Problem = 'freq' | 'name' | 'country' | 'time' | 'days' | 'site' | 'siteName' | 'position' | 'url';

const okPos = (lat: number, lon: number) => Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 90 && Math.abs(lon) <= 180;

/** What to fix before the draft can be saved (`freqOk`: the frequency field reads). */
export function problems(d: UserStationDraft, freqOk: boolean): Problem[] {
  const out: Problem[] = [];
  if (!freqOk) out.push('freq');
  if (!d.name.trim()) out.push('name');
  if (!d.itu) out.push('country');
  if (!d.slots.length || d.slots.some((s) => s.start === s.end || s.start >= 1440 || s.end > 1440)) out.push('time');
  if (d.slots.some((s) => daysMask(s.days) === 0)) out.push('days');
  const s = d.site;
  if (!s.itu) out.push('site');
  if (s.kind !== 'country' && !s.name.trim()) out.push(s.kind === 'known' ? 'site' : 'siteName');
  if (s.kind !== 'country' && !okPos(s.lat, s.lon)) out.push('position');
  const url = d.url?.trim();
  if (url && !/^https?:\/\/\S+$/i.test(url)) out.push('url');
  return out;
}

/** The draft as sent: trimmed, an empty link as none. */
export function cleaned(d: UserStationDraft): UserStationDraft {
  const url = d.url?.trim() || null;
  return { ...d, name: d.name.trim(), notes: d.notes.trim(), url };
}
