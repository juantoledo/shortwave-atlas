import { describe, expect, it } from 'vitest';
import { NO_FILTERS } from '../hooks/useCore';
import type { Band } from '../types/generated/Band';
import { coChannel, lanesInSight, localAt, scrollToShow, startsFreq, onAt, timetableFilters, utcDay, utcMinute, wholeSchedule } from './timetableView';

const band = (id: string, kind: Band['kind']) => ({ id, kind, label: id, lo_khz: 0, hi_khz: 1 }) as Band;

describe('timetable', () => {
  it('draws the lanes in sight and a few around them', () => {
    expect(lanesInSight(0, 240, 1000, 24)).toEqual([0, 18]);
    expect(lanesInSight(2400, 240, 1000, 24)).toEqual([92, 118]);
    expect(lanesInSight(2400, 240, 50, 24)).toEqual([50, 50]);
    expect(lanesInSight(0, 240, 0, 24)).toEqual([0, 0]);
  });

  it('scrolls a lane out of sight to the middle, and leaves one in sight', () => {
    expect(scrollToShow(5, 0, 240, 24)).toBeNull();
    expect(scrollToShow(9, 0, 240, 24)).toBeNull();
    expect(scrollToShow(10, 0, 240, 24)).toBe(132);
    expect(scrollToShow(0, 600, 240, 24)).toBe(0);
  });

  it('groups lanes by frequency and finds those sharing the tuned channel', () => {
    const hz = [5_900_000, 5_900_000, 5_905_000];
    expect([0, 1, 2].map((i) => startsFreq((k) => hz[k], i))).toEqual([true, false, true]);
    expect(coChannel(5_900_000, 5_900_500)).toBe(true);
    expect(coChannel(5_905_000, 5_900_000)).toBe(false);
    expect(coChannel(5_900_000, null)).toBe(false);
  });

  it('holds a minute inside a window, its end left out', () => {
    const w = [[0, 60], [1380, 1440]] as const;
    expect(onAt(w, 0)).toBe(true);
    expect(onAt(w, 60)).toBe(false);
    expect(onAt(w, 1439)).toBe(true);
    expect(onAt([], 600)).toBe(false);
  });

  it('labels the local clock at each UTC hour', () => {
    expect(localAt(0, -180)).toBe('21');
    expect(localAt(23, 120)).toBe('01');
    expect(localAt(10, 330)).toBe('15:30');
    expect(localAt(0, 0)).toBe('00');
  });

  it('reads the UTC day and minute', () => {
    const d = new Date(Date.UTC(2026, 9, 8, 17, 5));
    expect(utcMinute(d)).toBe(17 * 60 + 5);
    expect(utcDay(d)).toBe(Math.floor(Date.UTC(2026, 9, 8) / 86_400_000));
  });

  it("narrows the whole schedule to the dial's broadcast band, unless asked for all", () => {
    const b31 = band('31m', 'broadcast');
    expect(timetableFilters(NO_FILTERS, b31, 'band').bands).toEqual(['31m']);
    expect(timetableFilters(NO_FILTERS, b31, 'all')).toBe(NO_FILTERS);
    // a band chosen, another lens, or a dial off the broadcast bands: as the list has it
    const chosen = { ...NO_FILTERS, bands: ['49m'] };
    expect(timetableFilters(chosen, b31, 'band')).toBe(chosen);
    const favs = { ...NO_FILTERS, lens: 'favorites' as const };
    expect(timetableFilters(favs, b31, 'band')).toBe(favs);
    expect(timetableFilters(NO_FILTERS, band('ham-40m', 'ham'), 'band')).toBe(NO_FILTERS);
    expect(timetableFilters(NO_FILTERS, null, 'band')).toBe(NO_FILTERS);
    expect([wholeSchedule(NO_FILTERS), wholeSchedule(chosen), wholeSchedule(favs)]).toEqual([true, false, false]);
  });
});
