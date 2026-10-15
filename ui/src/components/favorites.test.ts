import { describe, expect, it } from 'vitest';
import type { Favorite } from '../types/generated/Favorite';
import type { FavoriteView } from '../types/generated/FavoriteView';
import type { StationRow } from '../types/generated/StationRow';
import { asMode, favAt, favNote, favTuning } from './favorites';

const view = (id: string, freq_hz: number, station_id: number | null): FavoriteView => ({
  favorite: { id, freq_hz, mode: 'AM', label: id, station: null, added: 0 },
  station_id,
});

const row: StationRow = {
  id: 7, freq_hz: 5905000, name: 'Radio X', itu: 'USA', flag: null, mode: 'AM', site: 1, site_name: 'Site',
  site_itu: 'USA', site_flag: null, precise: true, on: true, lang: 'E', target: 'NAm', source: 'eibi', key: 'k',
};
const fav = (label: string, mode: Favorite['mode']): Favorite => ({ id: label, freq_hz: 5905000, mode, label, station: null, added: 0 });

describe('favorites', () => {
  it('says what a favorite adds to its station', () => {
    expect(favNote(fav('Radio X', 'AM'), row)).toBe('');
    expect(favNote(fav('Radio X', 'USB'), row)).toBe('USB');
    expect(favNote(fav('Evening', 'AM'), row)).toBe('Evening');
    expect(favNote(fav('Evening', 'LSB'), row)).toBe('Evening · LSB');
  });

  it('tunes a station favorite in its own mode', () => {
    expect(favTuning({ kind: 'station', row, favorite: fav('Radio X', 'AM') })).toBe(row);
    expect(favTuning({ kind: 'station', row, favorite: fav('Radio X', 'USB') })).toEqual({ ...row, mode: 'USB' });
  });

  it('finds the favorite on a frequency, the selected station first', () => {
    const vs = [view('a', 5905000, null), view('b', 5905000, 7), view('c', 6070000, 9)];
    expect(favAt(vs, 5905000, 7)?.favorite.id).toBe('b');
    expect(favAt(vs, 5905000, null)?.favorite.id).toBe('a');
    expect(favAt(vs, 5905000, 3)?.favorite.id).toBe('a');
    expect(favAt(vs, 5906000, 7)).toBeNull();
    expect(favAt(vs, null, 7)).toBeNull();
  });

  it('keeps only the modes a favorite can have', () => {
    expect(asMode('USB')).toBe('USB');
    expect(asMode('PKTUSB')).toBe('AM');
    expect(asMode(null)).toBe('AM');
  });
});
