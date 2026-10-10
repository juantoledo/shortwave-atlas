import { describe, expect, it } from 'vitest';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import { makeNames } from './names';

const meta = {
  langs: [
    { code: 'S', en: 'Spanish', es: 'Español', iso: 'spa', rows: 1 },
    { code: 'Q', en: 'Quechua', es: 'Quechua', iso: 'que', rows: 1 },
    { code: 'AMD', en: 'Tibetan Amdo', es: null, iso: 'adx', rows: 1 },
    { code: 'GZ', en: "Ge'ez", es: null, iso: 'gez', rows: 1 },
    { code: 'Vn', en: 'Vernacular', es: null, iso: null, rows: 1 },
  ],
  countries: [{ itu: 'G', flag: 'gb', en: 'United Kingdom', es: 'Reino Unido', region: 'eu', rows: 2 }],
  targets: [
    { code: 'CAm', en: 'Central America', es: 'Centroamérica', region: 'am' },
    { code: 'KRE', en: 'North Korea', es: 'Corea del Norte', region: 'as' },
  ],
} as unknown as CatalogMeta;

describe('names', () => {
  it('uses English names in English', () => {
    const n = makeNames(meta, en);
    expect(n.lang('S,Q')).toBe('Spanish / Quechua');
    expect(n.lang('Vn')).toBe('Vernacular');
    expect(n.country('G')).toBe('United Kingdom');
    expect(n.target('CAm')).toBe('Central America');
    expect(n.region('oc')).toBe('Oceania');
    expect(n.band('49m')).toBe('49m');
    expect(n.band('oob')).toBe(en.bandOob);
    expect(n.dialBand({ id: '49m', label: '49', kind: 'broadcast', lo_khz: 5800, hi_khz: 6300 })).toBe('49m');
    expect(n.dialBand({ id: 'ham-40m', label: '40', kind: 'ham', lo_khz: 7000, hi_khz: 7300 })).toBe('Ham 40m');
    expect(n.dialBand({ id: 'ham-70cm', label: '70cm', kind: 'ham', lo_khz: 430000, hi_khz: 440000 })).toBe('Ham 70cm');
    expect(n.dialBand({ id: 'mw', label: 'MW', kind: 'broadcast', lo_khz: 525, hi_khz: 1705 })).toBe('Mediumwave');
  });

  it('uses Spanish names, then the browser, then English', () => {
    const n = makeNames(meta, es);
    expect(n.lang('S,Q')).toBe('Español / Quechua');
    expect(n.lang('GZ')).toBe('Geez'); // CLDR: "geez"
    expect(n.lang('Vn')).toBe('Vernacular');
    expect(n.country('G')).toBe('Reino Unido');
    expect(n.target('KRE')).toBe('Corea del Norte');
    expect(n.region('am')).toBe('América');
  });

  it('falls back to the code', () => {
    const n = makeNames(null, en);
    expect(n.lang('XYZ')).toBe('XYZ');
    expect(n.country('ZZZ')).toBe('ZZZ');
  });
});
