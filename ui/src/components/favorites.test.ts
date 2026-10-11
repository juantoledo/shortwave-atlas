import { describe, expect, it } from 'vitest';
import type { FavoriteView } from '../types/generated/FavoriteView';
import { asMode, favAt } from './favorites';

const view = (id: string, freq_hz: number, station_id: number | null): FavoriteView => ({
  favorite: { id, freq_hz, mode: 'AM', label: id, station: null, added: 0 },
  station_id,
});

describe('favorites', () => {
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
