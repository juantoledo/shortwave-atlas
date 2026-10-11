import { describe, expect, it } from 'vitest';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { SourceInfo } from '../types/generated/SourceInfo';
import { scheduleOf } from './sources';

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
});
