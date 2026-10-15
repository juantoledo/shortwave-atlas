// The NET band's list: internet stations for the text and country filters, first page on change
// (debounced), more on demand. Unlike the schedule's list there is no refresh each minute: nothing
// here goes on or off the air, and the directory is not ours to poll.

import { useCallback, useEffect, useRef, useState } from 'react';
import type { Api } from '../api/client';
import { netQuery } from '../components/net';
import type { NetCountry } from '../types/generated/NetCountry';
import type { NetStation } from '../types/generated/NetStation';
import type { Filters } from './useCore';

const DEBOUNCE_MS = 350;
const PAGE = 50;

export interface NetSearch {
  items: NetStation[];
  more: boolean;
  loading: boolean;
  /** The directory could not be reached. */
  failed: boolean;
  loadMore: () => void;
}

/** `active`: the list is on NET; nothing is asked otherwise. */
export function useNetSearch(api: Api | null, filters: Filters, active: boolean): NetSearch {
  const [st, setSt] = useState({ items: [] as NetStation[], more: false, loading: false, failed: false });
  const seq = useRef(0);
  const q = netQuery(filters, 0, PAGE);
  const key = `${q.q}|${q.country ?? ''}`;

  useEffect(() => {
    seq.current++;
    if (!api || !active) {
      setSt({ items: [], more: false, loading: false, failed: false });
      return;
    }
    const my = seq.current;
    setSt((s) => ({ ...s, loading: true, failed: false }));
    const t = setTimeout(() => {
      api.netSearch(q).then(
        (p) => { if (my === seq.current) setSt({ items: p.items, more: p.more, loading: false, failed: false }); },
        () => { if (my === seq.current) setSt({ items: [], more: false, loading: false, failed: true }); },
      );
    }, DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [api, active, key]); // `key` stands for `q`

  const loadMore = useCallback(() => {
    if (!api || !active || st.loading || !st.more) return;
    const my = seq.current;
    setSt((s) => ({ ...s, loading: true }));
    api.netSearch(netQuery(filters, st.items.length, PAGE)).then(
      (p) => {
        if (my !== seq.current) return;
        // a station the directory moved between pages shows once
        setSt((s) => {
          const seen = new Set(s.items.map((i) => i.uuid));
          return { items: [...s.items, ...p.items.filter((i) => !seen.has(i.uuid))], more: p.more, loading: false, failed: false };
        });
      },
      () => { if (my === seq.current) setSt((s) => ({ ...s, loading: false, failed: true })); },
    );
  }, [api, active, key, st.loading, st.more, st.items.length]);

  return { ...st, loadMore };
}

/** The countries NET can filter by, read once the list first goes to NET. */
export function useNetCountries(api: Api | null, active: boolean): NetCountry[] {
  const [list, setList] = useState<NetCountry[]>([]);
  const asked = useRef(false);
  useEffect(() => {
    if (!api || !active || asked.current) return;
    asked.current = true;
    api.netCountries().then(setList, () => { asked.current = false; });
  }, [api, active]);
  return list;
}
