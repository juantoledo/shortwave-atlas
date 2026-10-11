import { describe, expect, it } from 'vitest';
import type { Candidate } from '../types/generated/Candidate';
import type { OffSchedule } from '../types/generated/OffSchedule';
import type { UserStation } from '../types/generated/UserStation';
import {
  cleaned, daysMask, draftFrom, draftFromOff, draftHere, draftOf, fmtHhmm, hourSlot, nextSlot, parseDeg, parseHhmm, parseKhz,
  problems, toggleDay,
} from './stationDraft';

const at = (h: number, m = 0) => new Date(Date.UTC(2026, 9, 8, h, m));

const cand = (precise: boolean): Candidate => ({
  station: {
    id: 3, freq_hz: 6_070_000, name: 'CNR', itu: 'CHN', flag: 'cn', mode: 'AM', site: 1, source: 'eibi', key: 'k',
    slots: [{ start: 0, end: 600, days: { t: 'nth', n: 1, wd: 0 }, lang: 'M', target: 'CHN', season: 'all', from: null, to: null, heard: null }],
  },
  site: { id: 1, itu: 'CHN', flag: 'cn', code: precise ? 'ka' : null, name: precise ? 'Kashi' : 'China', lat: 39.35, lon: 75.75, precise },
  route: { km: 1, bearing: 0 } as Candidate['route'],
  air: { on: true, minutes: null, edge: 0 } as Candidate['air'],
  slot: 0,
  today: [],
  sun: {} as Candidate['sun'],
});

describe('station form', () => {
  it('reads and writes times', () => {
    expect([parseHhmm('7'), parseHhmm('07:30'), parseHhmm('0730'), parseHhmm('24:00'), parseHhmm(' 23:59 ')]).toEqual([420, 450, 450, 1440, 1439]);
    for (const bad of ['', '24:01', '25', '7:60', 'ab', '7:5']) expect(parseHhmm(bad)).toBeNull();
    expect([fmtHhmm(0), fmtHhmm(450), fmtHhmm(1440)]).toEqual(['00:00', '07:30', '24:00']);
    expect(hourSlot(at(23, 20))).toEqual({ start: 1380, end: 1440 });
  });

  it('switches weekdays', () => {
    const monOff = toggleDay({ t: 'daily' }, 0);
    expect(monOff).toEqual({ t: 'weekly', mask: 0x7e });
    expect(toggleDay(monOff, 0)).toEqual({ t: 'daily' });
    expect(daysMask(toggleDay({ t: 'weekly', mask: 1 }, 0))).toBe(0);
  });

  it('starts from what is tuned', () => {
    const d = draftHere(5_905_000, 'USB', at(14, 5));
    expect(d).toMatchObject({ freq_hz: 5_905_000, mode: 'USB', name: '', itu: '', site: { kind: 'country', itu: '' } });
    expect(d.slots).toEqual([{ start: 840, end: 900, days: { t: 'daily' }, lang: '', target: '' }]);
  });

  it('starts from a station shown: its country, site, language and target', () => {
    const d = draftFrom(cand(true), at(1));
    expect(d).toMatchObject({ freq_hz: 6_070_000, mode: 'AM', name: '', itu: 'CHN' });
    expect(d.site).toEqual({ kind: 'known', itu: 'CHN', code: 'ka', name: 'Kashi', lat: 39.35, lon: 75.75 });
    expect(d.slots[0]).toEqual({ start: 60, end: 120, days: { t: 'daily' }, lang: 'M', target: 'CHN' });
    // a site EiBi does not know: its country
    expect(draftFrom(cand(false), at(1)).site).toEqual({ kind: 'country', itu: 'CHN' });
  });

  it('starts from a row off the schedule', () => {
    const o: OffSchedule = {
      freq_hz: 6_200_000, mode: null, label: 'Mystery', favorite: 'f', heard: null,
      station: { source: 'eibi', key: 'k', freq_hz: 6_200_000, name: 'Old', itu: 'G', flag: 'gb', site_name: 'x', site_itu: 'AFS', lang: 'E', target: 'Eu' },
    };
    const d = draftFromOff(o, 'LSB', at(2));
    expect(d).toMatchObject({ freq_hz: 6_200_000, mode: 'LSB', name: 'Mystery', itu: 'G', site: { kind: 'country', itu: 'AFS' } });
    expect(d.slots[0]).toMatchObject({ lang: 'E', target: 'Eu' });
    expect(draftFromOff({ ...o, label: '', station: null, mode: 'AM' }, 'LSB', at(2))).toMatchObject({ name: '', itu: '', mode: 'AM' });
  });

  it('edits a kept station', () => {
    const u: UserStation = {
      id: '1-2', added: 1, updated: 2, freq_hz: 1, mode: 'AM', name: 'x', itu: 'CHL', site: { kind: 'country', itu: 'CHL' },
      slots: [{ start: 0, end: 60, days: { t: 'daily' }, lang: '', target: '' }], notes: 'n', url: null,
    };
    const d = draftOf(u);
    expect(d).not.toHaveProperty('id');
    expect(d).toMatchObject({ name: 'x', notes: 'n' });
  });

  it('adds the next hour', () => {
    expect(nextSlot({ start: 1380, end: 1440, days: { t: 'weekly', mask: 3 }, lang: 'S', target: '' }, at(0))).toEqual({
      start: 0, end: 60, days: { t: 'weekly', mask: 3 }, lang: 'S', target: '',
    });
    expect(nextSlot(undefined, at(5)).start).toBe(300);
  });

  it('reads positions and frequencies', () => {
    expect([parseDeg('-33.45', 90), parseDeg('70,67', 180), parseDeg('91', 90), parseDeg('x', 90)]).toEqual([-33.45, 70.67, null, null]);
    expect([parseKhz('6070'), parseKhz('6070.5'), parseKhz('9'), parseKhz('6070.1234'), parseKhz('')]).toEqual([6_070_000, 6_070_500, null, null, null]);
  });

  it('says what to fix', () => {
    const d = draftHere(5_905_000, 'AM', at(14));
    expect(problems(d, true)).toEqual(['name', 'country', 'site']);
    const ok = { ...d, name: 'Radio', itu: 'CHL', site: { kind: 'country', itu: 'CHL' } as const };
    expect(problems(ok, true)).toEqual([]);
    expect(problems(ok, false)).toEqual(['freq']);
    expect(problems({ ...ok, slots: [{ ...ok.slots[0], end: ok.slots[0].start }] }, true)).toEqual(['time']);
    expect(problems({ ...ok, slots: [{ ...ok.slots[0], days: { t: 'weekly', mask: 0 } }] }, true)).toEqual(['days']);
    expect(problems({ ...ok, site: { kind: 'custom', itu: 'CHL', name: ' ', lat: 95, lon: 0 } }, true)).toEqual(['siteName', 'position']);
    expect(problems({ ...ok, url: 'javascript:alert(1)' }, true)).toEqual(['url']);
    expect(cleaned({ ...ok, name: ' Radio ', url: '  ' })).toMatchObject({ name: 'Radio', url: null });
  });
});
