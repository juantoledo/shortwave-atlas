// The timetable's rows for a query, while the stage shows it: asked again when the query, the UTC
// day or `refresh` (the catalog, and what the lenses draw on) changes. The windows are today's;
// what is on the air now the view works out from the clock.

import { useEffect, useState } from 'react';
import type { Api } from '../api/client';
import type { Timetable } from '../types/generated/Timetable';
import type { Filters } from './useCore';

/** Wait for typing in the list's search to pause. */
const DEBOUNCE_MS = 250;

export interface TimetableState {
  table: Timetable | null;
  loading: boolean;
}

export function useTimetable(api: Api | null, filters: Filters, active: boolean, refresh: unknown, day: number): TimetableState {
  const [st, setSt] = useState<TimetableState>({ table: null, loading: true });
  const key = JSON.stringify(filters);
  useEffect(() => {
    if (!api || !active) return;
    let live = true;
    setSt((s) => ({ ...s, loading: true }));
    const t = setTimeout(() => {
      api.timetable({ ...filters, offset: 0, limit: 0, facets: false }).then(
        (table) => { if (live) setSt({ table, loading: false }); },
        () => { if (live) setSt((s) => ({ ...s, loading: false })); },
      );
    }, DEBOUNCE_MS);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [api, active, key, refresh, day]); // `key` stands for `filters`
  return st;
}
