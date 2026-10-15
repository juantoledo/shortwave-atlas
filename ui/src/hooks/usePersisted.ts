// View state kept per device between runs (the list's filters, the picked station, the tab, the
// camera): JSON in localStorage, read back through a check of its own, since what is stored may
// be from an older version, or junk. The page works without storage: then for this session only.

import { useEffect, useState, type Dispatch, type SetStateAction } from 'react';
import type { Lens } from '../types/generated/Lens';
import type { Region } from '../types/generated/Region';
import type { Sort } from '../types/generated/Sort';
import { NO_FILTERS, type Filters } from './useCore';

/** What a reader makes of a stored value: null when it is not one. */
export type Reader<T> = (raw: unknown) => T | null;

/** The stored value at `key`, read by `read`; `initial` when there is none, or it isn't one. */
export function loadValue<T>(storage: Pick<Storage, 'getItem'> | null, key: string, initial: T, read: Reader<T>): T {
  try {
    const s = storage?.getItem(key);
    return s == null ? initial : read(JSON.parse(s)) ?? initial;
  } catch {
    return initial;
  }
}

function storage(): Storage | null {
  try { return localStorage; } catch { return null; }
}

/** What this device kept at `key` (as `loadValue`). */
export const loadKept = <T>(key: string, initial: T, read: Reader<T>): T => loadValue(storage(), key, initial, read);

/** Keep `value` at `key` on this device. */
export function keep(key: string, value: unknown) {
  try { storage()?.setItem(key, JSON.stringify(value)); } catch { /* private window: this session only */ }
}

/** `useState` that this device keeps between runs. */
export function usePersisted<T>(key: string, initial: T, read: Reader<T>): [T, Dispatch<SetStateAction<T>>] {
  const [value, setValue] = useState(() => loadKept(key, initial, read));
  useEffect(() => keep(key, value), [key, value]);
  return [value, setValue];
}

// The readers.

const isStr = (x: unknown): x is string => typeof x === 'string';
const strs = (x: unknown): string[] | null => (Array.isArray(x) && x.every(isStr) ? x : null);

/** One of `values`. */
export const oneOf = <T extends string>(...values: T[]): Reader<T> => (raw) => (values.includes(raw as T) ? raw as T : null);
export const readString: Reader<string> = (raw) => (isStr(raw) ? raw : null);
export const readBool: Reader<boolean> = (raw) => (typeof raw === 'boolean' ? raw : null);

const readLens = oneOf<Lens>('all', 'heard', 'favorites', 'listened');
const readSort = oneOf<Sort>('freq', 'strongest', 'recent');
const REGIONS: Region[] = ['am', 'eu', 'af', 'me', 'as', 'oc', 'other'];

/** The list's filters: each one that holds, the rest as `NO_FILTERS` has them. The transmitter
 *  site is not kept here (its id changes with the catalog: see `SiteRef`). */
export const readFilters: Reader<Filters> = (raw) => {
  if (typeof raw !== 'object' || raw === null) return null;
  const r = raw as Record<string, unknown>;
  const regions = strs(r.regions);
  return {
    ...NO_FILTERS,
    q: isStr(r.q) ? r.q : NO_FILTERS.q,
    bands: strs(r.bands) ?? NO_FILTERS.bands,
    langs: strs(r.langs) ?? NO_FILTERS.langs,
    countries: strs(r.countries) ?? NO_FILTERS.countries,
    regions: regions && regions.every((x) => REGIONS.includes(x as Region)) ? regions as Region[] : NO_FILTERS.regions,
    sources: strs(r.sources) ?? NO_FILTERS.sources,
    lens: readLens(r.lens) ?? NO_FILTERS.lens,
    scan: isStr(r.scan) ? r.scan : NO_FILTERS.scan,
    sort: readSort(r.sort) ?? NO_FILTERS.sort,
  };
};

/** A transmitter site by what it is, since its id changes when the catalog is built again. */
export interface SiteRef { name: string; lat: number; lon: number }

const isNum = (x: unknown): x is number => typeof x === 'number' && Number.isFinite(x);

export const readSiteRef: Reader<SiteRef> = (raw) => {
  if (typeof raw !== 'object' || raw === null) return null;
  const { name, lat, lon } = raw as Record<string, unknown>;
  return isStr(name) && isNum(lat) && isNum(lon) ? { name, lat, lon } : null;
};

/** The site of `sites` that `ref` names: by name and place (a site's place is how the catalog
 *  tells sites apart). */
export function findSite<S extends SiteRef & { id: number }>(sites: readonly S[], ref: SiteRef): S | null {
  const near = (a: number, b: number) => Math.abs(a - b) < 1e-4;
  return sites.find((s) => s.name === ref.name && near(s.lat, ref.lat) && near(s.lon, ref.lon)) ?? null;
}

/** Where the camera looks from. */
export interface StoredPov { lat: number; lon: number; alt: number }

export const readPov: Reader<StoredPov> = (raw) => {
  if (typeof raw !== 'object' || raw === null) return null;
  const { lat, lon, alt } = raw as Record<string, unknown>;
  return isNum(lat) && isNum(lon) && isNum(alt) && Math.abs(lat) <= 90 && alt > 0 ? { lat, lon, alt } : null;
};
