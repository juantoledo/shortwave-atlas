import { describe, expect, it } from 'vitest';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { SourceInfo } from '../types/generated/SourceInfo';
import { exportName, keyOf, scheduleOf, stationsIn, userSources } from './sources';

const source = (id: string, kind: SourceInfo['kind']): SourceInfo => ({
  id, name: id, kind, editable: kind === 'user', url: null, version: '1', season: null, from: null, to: null,
  current: true, rows: 1, report: null,
});
const meta = (sources: SourceInfo[]) => ({ sources }) as CatalogMeta;

describe('sources', () => {
  it('finds the schedule among the sources', () => {
    expect(scheduleOf(meta([source('mine', 'user'), source('eibi', 'schedule')]))?.id).toBe('eibi');
    expect(scheduleOf(meta([source('mine', 'user')]))).toBeUndefined();
    expect(scheduleOf(null)).toBeUndefined();
  });

  it('knows which sources are the listener\'s', () => {
    expect([...userSources(meta([source('eibi', 'schedule'), source('mine', 'user')]))]).toEqual(['mine']);
    expect(userSources(null).size).toBe(0);
  });

  it('keys a station by its source and key', () => {
    expect(keyOf({ source: 'mine', key: '1-2' })).not.toBe(keyOf({ source: 'eibi', key: '1-2' }));
    expect(keyOf({ source: 'mine', key: '1-2' })).toBe(keyOf({ source: 'mine', key: '1-2' }));
  });

  it('names an export by the date and counts what one holds', () => {
    expect(exportName(new Date(2026, 9, 1, 23, 59))).toBe('swatlas-stations-2026-10-01.json');
    expect(stationsIn('{"version":1,"stations":[{},{}]}')).toBe(2);
    expect([stationsIn('{'), stationsIn('[]'), stationsIn('{"stations":3}'), stationsIn('null')]).toEqual([null, null, null, null]);
  });
});
