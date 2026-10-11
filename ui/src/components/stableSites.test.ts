import { describe, expect, it } from 'vitest';
import { stableSites } from './stableSites';
import type { SiteDot } from '../types/generated/SiteDot';

const site = (id: number, on = 0): SiteDot => ({ id, lat: id, lon: -id, name: `S${id}`, itu: 'USA', flag: null, precise: true, on, total: 3, mine: 0 });

describe('stable sites', () => {
  it('keeps each site the same object across refreshes', () => {
    const cache = new Map<number, SiteDot>();
    const a = stableSites(cache, [], [site(1), site(2)]);
    const b = stableSites(cache, a, [site(1), site(2)]);
    expect(b).toBe(a); // nothing changed: the same list
    const c = stableSites(cache, b, [site(1, 2), site(2)]);
    expect(c).not.toBe(b);
    expect(c[0]).toBe(a[0]);
    expect(c[1]).toBe(a[1]);
    expect(c[0].on).toBe(2);
  });

  it("leaves the globe's own fields on the objects", () => {
    const cache = new Map<number, SiteDot>();
    const a = stableSites(cache, [], [site(1)]);
    (a[0] as unknown as Record<string, unknown>).__threeObjPoint = 'mesh';
    const b = stableSites(cache, a, [site(1, 1)]);
    expect((b[0] as unknown as Record<string, unknown>).__threeObjPoint).toBe('mesh');
  });

  it('adds new sites and forgets the ones gone', () => {
    const cache = new Map<number, SiteDot>();
    const a = stableSites(cache, [], [site(1), site(2)]);
    const b = stableSites(cache, a, [site(2), site(3)]);
    expect(b.map((s) => s.id)).toEqual([2, 3]);
    expect(b[0]).toBe(a[1]);
    expect([...cache.keys()].sort()).toEqual([2, 3]);
  });

  it('a new order is a change', () => {
    const cache = new Map<number, SiteDot>();
    const a = stableSites(cache, [], [site(1), site(2)]);
    const b = stableSites(cache, a, [site(2), site(1)]);
    expect(b).not.toBe(a);
    expect(b[0]).toBe(a[1]);
  });
});
