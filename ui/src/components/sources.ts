// The data sources the catalog was built from (`CatalogMeta.sources`), and stations by what
// identifies them across catalog versions: ids change whenever a source does.

import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { SourceInfo } from '../types/generated/SourceInfo';
import type { StationKey } from '../types/generated/StationKey';

/** `SourceInfo.id` of the listener's own stations. */
export const MINE = 'mine';

/** The published schedule the stations come from (EiBi), if the catalog has one. */
export const scheduleOf = (meta: CatalogMeta | null): SourceInfo | undefined =>
  meta?.sources.find((s) => s.kind === 'schedule');

/** The ids of the sources whose stations are the listener's own. */
export const userSources = (meta: CatalogMeta | null): ReadonlySet<string> =>
  new Set((meta?.sources ?? []).filter((s) => s.kind === 'user').map((s) => s.id));

/** A station's source and key, as one string to compare or keep. */
export const keyOf = (s: StationKey): string => `${s.source}\u0000${s.key}`;

/** An export's file name: `swatlas-stations-2026-10-11.json` (the local date). */
export function exportName(now: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `swatlas-stations-${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}.json`;
}

/** How many stations an export holds, if it reads as one. */
export function stationsIn(json: string): number | null {
  try {
    const f: unknown = JSON.parse(json);
    const list = f && typeof f === 'object' ? (f as { stations?: unknown }).stations : undefined;
    return Array.isArray(list) ? list.length : null;
  } catch {
    return null;
  }
}
