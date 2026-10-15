import { describe, expect, it } from 'vitest';
import { findSite, loadValue, oneOf, readFilters, readPov, readSiteRef, readString } from './usePersisted';
import { NO_FILTERS } from './useCore';

const store = (items: Record<string, string>) => ({ getItem: (k: string) => items[k] ?? null });

describe('loadValue', () => {
  it('reads what the reader takes, else the initial value', () => {
    expect(loadValue(store({ k: '"browse"' }), 'k', 'globe', oneOf('globe', 'browse'))).toBe('browse');
    expect(loadValue(store({ k: '"moon"' }), 'k', 'globe', oneOf('globe', 'browse'))).toBe('globe');
    expect(loadValue(store({}), 'k', 'globe', oneOf('globe', 'browse'))).toBe('globe');
    expect(loadValue(store({ k: '{not json' }), 'k', null, readString)).toBeNull();
    expect(loadValue(null, 'k', 'x', readString)).toBe('x');
  });

  it('works without storage', () => {
    const broken = { getItem: () => { throw new Error('SecurityError'); } };
    expect(loadValue(broken, 'k', 'x', readString)).toBe('x');
  });
});

describe('readFilters', () => {
  it('keeps the filters that hold', () => {
    const f = { ...NO_FILTERS, q: 'bbc', bands: ['49m'], langs: ['E'], countries: ['G'], regions: ['eu' as const], sources: ['eibi'], lens: 'heard' as const, scan: 's1', sort: 'strongest' as const };
    expect(readFilters(JSON.parse(JSON.stringify(f)))).toEqual(f);
  });

  it('drops junk and a site id (it changes with the catalog)', () => {
    expect(readFilters({ q: 3, bands: 'x', regions: ['mars'], lens: 'nope', sort: null, scan: 7, site: 12 })).toEqual(NO_FILTERS);
    expect(readFilters(null)).toBeNull();
    expect(readFilters('x')).toBeNull();
  });
});

describe('sites and the camera', () => {
  it('finds a site again by name and place', () => {
    const sites = [{ id: 4, name: 'Woofferton', lat: 52.31, lon: -2.72 }, { id: 9, name: 'Nauen', lat: 52.65, lon: 12.91 }];
    expect(findSite(sites, { name: 'Nauen', lat: 52.65, lon: 12.91 })?.id).toBe(9);
    expect(findSite(sites, { name: 'Nauen', lat: 1, lon: 2 })).toBeNull();
    expect(readSiteRef({ name: 'Nauen', lat: 52.65, lon: 12.91 })).toEqual({ name: 'Nauen', lat: 52.65, lon: 12.91 });
    expect(readSiteRef({ name: 'Nauen', lat: '52' })).toBeNull();
  });

  it('reads a point of view', () => {
    expect(readPov({ lat: 48, lon: 10, alt: 1.2 })).toEqual({ lat: 48, lon: 10, alt: 1.2 });
    expect(readPov({ lat: 120, lon: 10, alt: 1.2 })).toBeNull();
    expect(readPov({ lat: 48, lon: 10, alt: 0 })).toBeNull();
    expect(readPov({ lat: 48, lon: NaN, alt: 1 })).toBeNull();
  });
});
