// Tiny typed i18n: `const t = useT(); t.onAir; t.endsIn(dur, at)`.

import { createContext, useContext } from 'react';
import type { AudioHint } from '../types/generated/AudioHint';
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
  const h = Math.floor(m / 60), r = m % 60;
  return r ? `${h} ${t.h} ${r} ${t.min}` : `${h} ${t.h}`;
}

export const compass = (t: Messages, bearing: number) => t.compass[Math.round(bearing / 45) % 8];

export const fmtKm = (t: Messages, km: number) => Math.round(km).toLocaleString(t.locale) + ' km';

/** kHz with one decimal, e.g. 13570.0 */
export const fmtKhz = (hz: number) => (hz / 1000).toFixed(1);

export function localTime(t: Messages, tz: string, d: Date): string {
  try {
    return new Intl.DateTimeFormat(t.locale, { timeZone: tz, weekday: 'short', hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(d);
  } catch {
    return hhmm(d.getUTCHours() * 60 + d.getUTCMinutes()) + ' UTC';
  }
}

/** S-meter label from dB relative to S9 (6 dB per S-unit). */
export function sLabel(db: number | null): string {
  if (db === null) return '--';
  if (db <= 0) return 'S' + Math.max(0, Math.round(9 + db / 6));
  return 'S9+' + Math.round(db);
}
