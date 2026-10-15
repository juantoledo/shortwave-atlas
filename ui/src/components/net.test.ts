import { describe, expect, it } from 'vitest';
import { NO_FILTERS, type Filters } from '../hooks/useCore';
import type { NetNow } from '../types/generated/NetNow';
import { netRings, siteColor } from './stage/fx';
import { audioCell, bufferSegs, canListen, dimSites, favFilter, isNet, lensFilters, leaveNet, NET_BAND, NET_DOT_ID, netCountryOpts, netQuery, netSiteDots, playingUuid, stepChannel, streamLabel } from './net';

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

  it('a lens about the schedule leaves NET; All and Favorites keep it', () => {
    expect(lensFilters(onNet, 'all')).toEqual(onNet);
    expect(lensFilters(onNet, 'favorites')).toEqual({ ...onNet, lens: 'favorites' });
    expect(lensFilters(radio, 'favorites')).toEqual({ ...radio, lens: 'favorites' });
    expect(lensFilters(onNet, 'heard').bands).toEqual([]);
    expect(lensFilters(radio, 'listened')).toEqual({ ...radio, lens: 'listened' });
  });

  it('words the stream', () => {
    expect(streamLabel({ codec: 'MP3', bitrate: 128, hls: false })).toBe('MP3 128k');
    expect(streamLabel({ codec: 'AAC+', bitrate: null, hls: false })).toBe('AAC+');
    expect(streamLabel({ codec: null, bitrate: null, hls: true })).toBe('HLS');
    expect(streamLabel({ codec: null, bitrate: null, hls: false })).toBe('');
  });

  it('keeps the audio panel while there is something to hear', () => {
    expect(audioCell(true, false)).toBe('panel');
    expect(audioCell(false, true)).toBe('panel');
    expect(audioCell(false, false)).toBe('off');
    const station = { uuid: 'u1', name: 'La 100', itu: 'ARG', flag: 'ar', langs: [], tags: [], codec: 'MP3', bitrate: 128, hls: false, homepage: null, ok: true, votes: 0, playable: true, pos: null };
    const playing: NetNow = { station, state: { state: 'playing' }, buffer_ms: 1500, place: null, route: null, sun: null, listened_secs: 0 };
    expect(canListen(false, null)).toBe(false);
    expect(canListen(false, playing)).toBe(true);
    expect(canListen(true, null)).toBe(true);
    expect(playingUuid(playing)).toBe('u1');
    expect(playingUuid({ ...playing, state: { state: 'connecting' } })).toBe('u1');
    expect(playingUuid({ ...playing, state: { state: 'failed', message: 'gone' } })).toBeNull();
    expect(playingUuid(null)).toBeNull();
  });

  it('steps memory channels round the ends', () => {
    const ch = [{ uuid: 'a' }, { uuid: 'b' }, { uuid: 'c' }];
    expect(stepChannel(ch, 'a', 1)?.uuid).toBe('b');
    expect(stepChannel(ch, 'c', 1)?.uuid).toBe('a');
    expect(stepChannel(ch, 'a', -1)?.uuid).toBe('c');
    expect(stepChannel(ch, 'b', 4)?.uuid).toBe('c');
    expect(stepChannel(ch, null, 1)?.uuid).toBe('a');
    expect(stepChannel(ch, 'zz', -1)?.uuid).toBe('c');
    expect(stepChannel([], 'a', 1)).toBeNull();
  });

  it('fills the buffer meter over 5 s', () => {
    expect(bufferSegs(0, 11)).toBe(0);
    expect(bufferSegs(2500, 11)).toBe(6);
    expect(bufferSegs(9000, 11)).toBe(11);
  });

  it('finds favorites by name or label', () => {
    const f = (uuid: string, name: string, label = '') => ({ id: uuid, uuid, name, label, itu: null, flag: null, codec: null, added: 0 });
    const list = [f('a', 'Radio Mitre', 'Mitre AM'), f('b', 'La 100')];
    expect(favFilter(list, '').length).toBe(2);
    expect(favFilter(list, 'mitre').map((x) => x.uuid)).toEqual(['a']);
    expect(favFilter(list, ' 100 ').map((x) => x.uuid)).toEqual(['b']);
  });

  it('puts countries on the globe and dims the sites', () => {
    const dots = netSiteDots([{ itu: 'ARG', flag: 'ar', lat: -34, lon: -58, stations: 1240 }, { itu: 'AND', flag: 'ad', lat: 42, lon: 1, stations: 3 }], (c) => `<${c}>`);
    expect(dots.map((d) => [d.id, d.name, d.total])).toEqual([[NET_DOT_ID, '<ARG>', 1240], [NET_DOT_ID + 1, '<AND>', 3]]);
    expect(dots[0].on).toBe(13);
    expect(dots[1].on).toBe(3);
    const sites = [{ ...dots[0], id: 1, on: 4 }, { ...dots[1], id: 2, on: 0 }];
    expect(dimSites(sites).map((s) => s.on)).toEqual([0, 0]);
    expect(dimSites(sites)[1]).toBe(sites[1]);
    // pink on the map, like the tuned station's pin; white when picked
    expect(siteColor(dots[0], false, false)).toBe('rgba(255,122,196,1)');
    expect(siteColor(dots[1], false, false)).toBe('rgba(255,122,196,0.8)');
    expect(siteColor(dots[0], true, false)).toBe('#ffffff');
    expect(siteColor(sites[0], false, false)).not.toContain('255,122,196');
  });

  it('beats while the internet station plays, rests while only tuned', () => {
    const at = { lat: -34.6, lon: -58.4 };
    const tuned = netRings(at, false), playing = netRings(at, true);
    expect(tuned).toHaveLength(1);
    expect(playing).toHaveLength(2);
    expect(Math.max(...playing.map((r) => r.maxR))).toBeGreaterThan(tuned[0].maxR);
    expect(Math.min(...playing.map((r) => r.period))).toBeLessThan(tuned[0].period);
    expect([...tuned, ...playing].every((r) => r.rgb === '255,122,196' && r.lat === at.lat && r.lng === at.lon)).toBe(true);
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
