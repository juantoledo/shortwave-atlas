// The data sources the catalog was built from (`CatalogMeta.sources`).

import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { SourceInfo } from '../types/generated/SourceInfo';

/** The published schedule the stations come from (EiBi), if the catalog has one. */
export const scheduleOf = (meta: CatalogMeta | null): SourceInfo | undefined =>
  meta?.sources.find((s) => s.kind === 'schedule');
