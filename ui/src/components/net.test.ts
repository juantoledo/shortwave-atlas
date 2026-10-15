import { describe, expect, it } from 'vitest';
import { NO_FILTERS, type Filters } from '../hooks/useCore';
import { isNet, lensFilters, leaveNet, NET_BAND, netCountryOpts, netQuery, streamLabel } from './net';

const radio: Filters = { ...NO_FILTERS, q: 'radio', langs: ['S'], regions: ['am'], countries: ['ARG'], sources: ['eibi'], site: 7, scan: 's1', lens: 'all' };
const onNet: Filters = { ...radio, bands: [NET_BAND] };

describe('NET band', () => {
  it('is on only when picked and internet radio is on', () => {
    expect(isNet(onNet, true)).toBe(true);
    expect(isNet(onNet, false)).toBe(false);
    expect(isNet(radio, true)).toBe(false);
    expect(isNet({ ...radio, bands: ['49m'] }, true)).toBe(false);
  });

  it('leaving it brings every other filter back as it was', () => {
    expect(leaveNet(onNet)).toEqual(radio);
    expect(leaveNet(radio)).toBe(radio);
    const banded = { ...radio, bands: ['49m'] };
    expect(leaveNet(banded)).toBe(banded);
  });

  it('searches by text and country only', () => {
    expect(netQuery({ ...onNet, q: '  la 100 ' }, 50, 50)).toEqual({ q: 'la 100', country: 'ARG', offset: 50, limit: 50 });
    expect(netQuery({ ...onNet, countries: [] }, 0, 50).country).toBeNull();
  });

  it('a lens about the schedule leaves NET; All keeps it', () => {
    expect(lensFilters(onNet, 'all')).toEqual(onNet);
    expect(lensFilters(onNet, 'favorites')).toEqual({ ...radio, lens: 'favorites' });
    expect(lensFilters(onNet, 'heard').bands).toEqual([]);
    expect(lensFilters(radio, 'listened')).toEqual({ ...radio, lens: 'listened' });
  });

  it('words the stream', () => {
    expect(streamLabel({ codec: 'MP3', bitrate: 128, hls: false })).toBe('MP3 128k');
    expect(streamLabel({ codec: 'AAC+', bitrate: null, hls: false })).toBe('AAC+');
    expect(streamLabel({ codec: null, bitrate: null, hls: true })).toBe('HLS');
    expect(streamLabel({ codec: null, bitrate: null, hls: false })).toBe('');
  });

  it('offers countries by name in the UI language', () => {
    const list = [
      { itu: 'D', flag: 'de', en: 'Germany', es: 'Alemania' },
      { itu: 'ARG', flag: 'ar', en: 'Argentina', es: 'Argentina' },
      { itu: 'E', flag: 'es', en: 'Spain', es: 'España' },
    ];
    expect(netCountryOpts(list, 'en').map((o) => o.value)).toEqual(['ARG', 'D', 'E']);
    expect(netCountryOpts(list, 'es').map((o) => o.label)).toEqual(['Alemania', 'Argentina', 'España']);
  });
});
