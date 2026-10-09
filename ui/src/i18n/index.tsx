// Tiny typed i18n: `const t = useT(); t.onAir; t.endsIn(dur, at)`.

import { createContext, useContext } from 'react';
import type { AudioHint } from '../types/generated/AudioHint';
import type { Days } from '../types/generated/Days';
import type { Hint } from '../types/generated/Hint';
import type { Os } from '../types/generated/Os';
import { en, type Messages } from './en';
import { es } from './es';

export type { Messages };
export const CATALOGS: Record<string, Messages> = { en, es };

/** Pick a catalog: explicit override, else the browser/OS language, else English. */
export function pickMessages(override: string | null | undefined, navLangs: readonly string[]): Messages {
  for (const l of [override, ...navLangs]) {
    const base = l?.toLowerCase().split('-')[0];
    if (base && CATALOGS[base]) return CATALOGS[base];
  }
  return en;
}

const I18n = createContext<Messages>(en);
export const I18nProvider = I18n.Provider;
export const useT = () => useContext(I18n);

/** A hint's title and text for `os`: its own wording if it has one, else the shared one. */
export const hintText = (t: Messages, os: Os, h: Hint) => t.hintsByOs[os]?.[h] ?? t.hints[h];
export const audioHintText = (t: Messages, os: Os, h: AudioHint) => t.audioHintsByOs[os]?.[h] ?? t.audioHints[h];

/* ---------------- Locale-aware formatting ---------------- */

export const hhmm = (m: number) => String(Math.floor(m / 60)).padStart(2, '0') + ':' + String(m % 60).padStart(2, '0');

export function fmtDur(t: Messages, m: number): string {
  if (m < 60) return `${m} ${t.min}`;
  if (m >= 1440) {
    const d = Math.floor(m / 1440), h = Math.floor((m % 1440) / 60);
    return h ? `${d} ${t.d} ${h} ${t.h}` : `${d} ${t.d}`;
  }
  const h = Math.floor(m / 60), r = m % 60;
  return r ? `${h} ${t.h} ${r} ${t.min}` : `${h} ${t.h}`;
}

export const compass = (t: Messages, bearing: number) => t.compass[Math.round(bearing / 45) % 8];

export const fmtKm = (t: Messages, km: number) => Math.round(km).toLocaleString(t.locale) + ' km';

/** kHz with one decimal, e.g. 13570.0 */
export const fmtKhz = (hz: number) => (hz / 1000).toFixed(1);

/** Short weekday name, Monday = 0. */
export function weekdayName(t: Messages, wd: number, style: 'short' | 'long' = 'short'): string {
  // 2024-01-01 was a Monday
  return new Intl.DateTimeFormat(t.locale, { weekday: style, timeZone: 'UTC' }).format(new Date(Date.UTC(2024, 0, 1 + wd)));
}

/** A day number (days since 1970-01-01) or a day and month, as a short date. */
export function fmtDate(t: Messages, day: number): string {
  return new Intl.DateTimeFormat(t.locale, { day: 'numeric', month: 'short', timeZone: 'UTC' }).format(new Date(day * 864e5));
}

/** `YYYY-MM` as a month and year. */
export function fmtMonth(t: Messages, ym: string): string {
  const [y, m] = ym.split('-').map(Number);
  return new Intl.DateTimeFormat(t.locale, { month: 'short', year: 'numeric', timeZone: 'UTC' }).format(new Date(Date.UTC(y, m - 1, 1)));
}

/** EiBi day patterns: "Daily", "Mon–Fri", "Tue, Fri", "1st Sat of the month", "15 Sep". */
export function fmtDays(t: Messages, d: Days): string {
  switch (d.t) {
    case 'daily': return t.daily;
    case 'irregular': return t.irregular;
    case 'nth': return t.nth(d.n, weekdayName(t, d.wd, 'long'));
    case 'last': return t.last(weekdayName(t, d.wd, 'long'));
    case 'date':
      return new Intl.DateTimeFormat(t.locale, { day: 'numeric', month: 'short', timeZone: 'UTC' }).format(new Date(Date.UTC(2024, d.m - 1, d.d)));
    case 'weekly': {
      if (d.mask === 0x7f) return t.daily;
      // runs of consecutive days; three or more read as a range
      const parts: string[] = [];
      for (let i = 0; i < 7; ) {
        if (!(d.mask & (1 << i))) { i++; continue; }
        let j = i;
        while (j + 1 < 7 && d.mask & (1 << (j + 1))) j++;
        if (j - i >= 2) parts.push(`${weekdayName(t, i)}–${weekdayName(t, j)}`);
        else for (let k = i; k <= j; k++) parts.push(weekdayName(t, k));
        i = j + 1;
      }
      return parts.join(', ');
    }
  }
}

/** S-meter label from dB relative to S9 (6 dB per S-unit). */
export function sLabel(db: number | null): string {
  if (db === null) return '--';
  if (db <= 0) return 'S' + Math.max(0, Math.round(9 + db / 6));
  return 'S9+' + Math.round(db);
}
