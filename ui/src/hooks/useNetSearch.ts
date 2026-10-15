// The NET band's list: internet stations for the text and country filters, first page on change
// (debounced), more on demand. Unlike the schedule's list there is no refresh each minute: nothing
// here goes on or off the air, and the directory is not ours to poll.

import { useCallback, useEffect, useRef, useState } from 'react';
import type { Api } from '../api/client';
import { netQuery } from '../components/net';
import type { NetCountry } from '../types/generated/NetCountry';
import type { NetDot } from '../types/generated/NetDot';
import type { NetPage } from '../types/generated/NetPage';
import type { NetStation } from '../types/generated/NetStation';
import type { Filters } from './useCore';

const DEBOUNCE_MS = 350;
const PAGE = 50;
/** Directory pages read at most to find stations that can play, before showing none. */
const TRIES = 4;

/** A page of stations that can play from `offset`, reading on while the directory's pages hold
 *  none (they may all be HLS) and it has more. */
async function somePlayable(api: Api, filters: Filters, offset: number): Promise<NetPage> {
  let p = await api.netSearch(netQuery(filters, offset, PAGE));
  for (let i = 1; i < TRIES && p.items.length === 0 && p.more; i++) {
    p = await api.netSearch(netQuery(filters, p.next_offset, PAGE));
  }
  return p;
}

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
  const [st, setSt] = useState({ items: [] as NetStation[], more: false, next: 0, loading: false, failed: false });
  const seq = useRef(0);
  const q = netQuery(filters, 0, PAGE);
  const key = `${q.q}|${q.country ?? ''}`; // what the search uses of `filters`

  useEffect(() => {
    seq.current++;
    if (!api || !active) {
      setSt({ items: [], more: false, next: 0, loading: false, failed: false });
      return;
    }
    const my = seq.current;
    setSt((s) => ({ ...s, loading: true, failed: false }));
    const t = setTimeout(() => {
      somePlayable(api, filters, 0).then(
        (p) => { if (my === seq.current) setSt({ items: p.items, more: p.more, next: p.next_offset, loading: false, failed: false }); },
        () => { if (my === seq.current) setSt({ items: [], more: false, next: 0, loading: false, failed: true }); },
      );
    }, DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [api, active, key]); // `key` stands for `filters`

  const loadMore = useCallback(() => {
    if (!api || !active || st.loading || !st.more) return;
    const my = seq.current;
    setSt((s) => ({ ...s, loading: true }));
    // from where the directory left off: stations that can't play were left out of the pages
    somePlayable(api, filters, st.next).then(
      (p) => {
        if (my !== seq.current) return;
        // a station the directory moved between pages shows once
        setSt((s) => {
          const seen = new Set(s.items.map((i) => i.uuid));
          return { items: [...s.items, ...p.items.filter((i) => !seen.has(i.uuid))], more: p.more, next: p.next_offset, loading: false, failed: false };
        });
      },
      () => { if (my === seq.current) setSt((s) => ({ ...s, loading: false, failed: true })); },
    );
  }, [api, active, key, st.loading, st.more, st.next]);

  return { ...st, loadMore };
}

/** Countries with internet stations, for the map, read once the list first goes to NET. */
export function useNetOverview(api: Api | null, active: boolean): NetDot[] {
  const [dots, setDots] = useState<NetDot[]>([]);
  const asked = useRef(false);
  useEffect(() => {
    if (!api || !active || asked.current) return;
    asked.current = true;
    api.netOverview().then(setDots, () => { asked.current = false; });
  }, [api, active]);
  return dots;
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
