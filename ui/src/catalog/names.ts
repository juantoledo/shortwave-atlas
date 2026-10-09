// Display names for EiBi codes in the UI language: languages, countries, target areas,
// regions and bands. The core sends English and (mostly) Spanish names; languages it has
// no Spanish name for are localised through their ISO 639-3 code when the browser knows it.

import { createContext, useContext } from 'react';
import type { Messages } from '../i18n';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { Region } from '../types/generated/Region';

export interface Names {
  /** `S,Q` -> `Spanish / Quechua` */
  lang(codes: string): string;
  country(itu: string): string;
  target(code: string): string;
  region(id: Region): string;
  band(id: string): string;
}

function displayNames(locale: string): ((iso: string) => string | null) {
  try {
    const dn = new Intl.DisplayNames([locale], { type: 'language', fallback: 'none' });
    return (iso) => {
      try {
        const n = dn.of(iso);
        return n && n.toLowerCase() !== iso ? n.charAt(0).toLocaleUpperCase(locale) + n.slice(1) : null;
      } catch {
        return null;
      }
    };
  } catch {
    return () => null;
  }
}

export function makeNames(meta: CatalogMeta | null, t: Messages): Names {
  const es = t.locale === 'es';
  const intl = displayNames(t.locale);
  const langs = new Map<string, string>();
  for (const l of meta?.langs ?? []) {
    langs.set(l.code, es ? (l.es ?? (l.iso && intl(l.iso)) ?? l.en) : l.en);
  }
  const countries = new Map((meta?.countries ?? []).map((c) => [c.itu, es ? c.es : c.en]));
  const targets = new Map((meta?.targets ?? []).map((x) => [x.code, es ? x.es : x.en]));
  return {
    lang: (codes) => codes.split(',').filter(Boolean).map((c) => langs.get(c) ?? c).join(' / '),
    country: (itu) => countries.get(itu) ?? targets.get(itu) ?? itu,
    target: (code) => targets.get(code) ?? countries.get(code) ?? code,
    region: (id) => t.regions[id],
    band: (id) => (id === 'oob' ? t.bandOob : id),
  };
}

const NamesContext = createContext<Names | null>(null);
export const NamesProvider = NamesContext.Provider;
export function useNames(): Names {
  const n = useContext(NamesContext);
  if (!n) throw new Error('useNames outside NamesProvider');
  return n;
}
