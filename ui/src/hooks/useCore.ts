// Connection to the core, the live rig state, and station lookups.

import { useCallback, useEffect, useRef, useState } from 'react';
import { Api } from '../api/client';
import { connect, type LinkStatus } from '../api/transport';
import { clampToCoverage } from '../components/dial';
import type { Candidate } from '../types/generated/Candidate';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { Dial } from '../types/generated/Dial';
import type { HeardRow } from '../types/generated/HeardRow';
import type { Info } from '../types/generated/Info';
import type { OffSchedule } from '../types/generated/OffSchedule';
import type { Overview } from '../types/generated/Overview';
import type { Mode } from '../types/generated/Mode';
import type { RigState } from '../types/generated/RigState';
import type { RxRange } from '../types/generated/RxRange';
import type { StationPage } from '../types/generated/StationPage';
import type { StationQuery } from '../types/generated/StationQuery';
import type { StationRow } from '../types/generated/StationRow';

/** Keep showing a frequency we just set for this long while the rig catches up. */
const PENDING_MS = 1500;
/** Wait for the dial to settle before looking stations up (prototype value). */
const LOOKUP_DEBOUNCE_MS = 380;
/** Air status changes by the minute; refresh the globe and the list this often. */
const ON_AIR_REFRESH_MS = 60_000;
/** Wait for typing to pause before searching. */
const SEARCH_DEBOUNCE_MS = 250;
/** Rows per page (the core's maximum is 200). */
export const PAGE_ROWS = 100;

const NO_RIG: RigState = { link: 'down', power: 'unknown', freq_hz: null, mode: null, passband_hz: null, strength_db: null, cw_pitch_hz: null };

export function useCore() {
  const [api, setApi] = useState<Api | null>(null);
  const [info, setInfo] = useState<Info | null>(null);
  const [rig, setRig] = useState<RigState>(NO_RIG);
  const [status, setStatus] = useState<LinkStatus>('connecting');

  useEffect(() => {
    let alive = true;
    const offs: (() => void)[] = [];
    connect().then(async (t) => {
      if (!alive) return t.close();
      const a = new Api(t);
      offs.push(t.onState(setRig), t.onStatus(setStatus), () => t.close());
      setApi(a);
      setInfo(await a.info());
      setRig(await a.state());
    });
    return () => {
      alive = false;
      offs.forEach((f) => f());
    };
  }, []);

  /** Re-read app info (backend, configured) after the settings change. */
  const reloadInfo = useCallback(() => { api?.info().then(setInfo, () => {}); }, [api]);

  return { api, info, reloadInfo, rig, status };
}

/** The displayed frequency: what we just asked for, until the rig reports it (or a timeout). */
/** Tuning, kept within what the rig receives (`coverage`). A station's `mode` goes with its
 *  frequency: the core sets it once the rig is there (a band change may recall another). */
export function useTuning(api: Api | null, rigHz: number | null, coverage: readonly RxRange[], onError: (e: unknown) => void) {
  const [pending, setPending] = useState<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    if (pending !== null && rigHz === pending) setPending(null);
  }, [rigHz, pending]);

  const tune = useCallback((hz: number, mode: Mode | null = null) => {
    if (!api) return;
    hz = clampToCoverage(Math.round(hz / 100) * 100, coverage);
    setPending(hz);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setPending(null), PENDING_MS);
    api.rig(mode ? { cmd: 'tune', hz, mode } : { cmd: 'set_freq', hz }).catch(onError);
  }, [api, coverage, onError]);

  return { freqHz: pending ?? rigHz, tune };
}

/** Stations near `freqHz` (debounced), best first. */
export function useCandidates(api: Api | null, freqHz: number | null, refreshKey: unknown) {
  const [cands, setCands] = useState<Candidate[]>([]);
  const seq = useRef(0);
  useEffect(() => {
    if (!api || freqHz === null) {
      setCands([]);
      return;
    }
    const my = ++seq.current;
    const t = setTimeout(() => {
      api.lookup(freqHz).then((c) => { if (my === seq.current) setCands(c); }, () => {});
    }, LOOKUP_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [api, freqHz, refreshKey]);
  return cands;
}

/** Code tables and facts about the station data, loaded again when the catalog's `generation`
 *  changes (a source changed). */
export function useStationsMeta(api: Api | null, generation: number | null) {
  const [meta, setMeta] = useState<CatalogMeta | null>(null);
  useEffect(() => {
    if (api) api.stationsMeta().then(setMeta, () => {});
  }, [api, generation]);
  return meta;
}

/** The rig's coverage and its dial's bands and spans, asked again when `key` changes (the rig
 *  comes or goes) or on `reload` (the QTH moved). */
export function useDial(api: Api | null, key: string) {
  const [dial, setDial] = useState<Dial | null>(null);
  const reload = useCallback(() => { api?.dial().then(setDial, () => {}); }, [api]);
  useEffect(reload, [reload, key]);
  return { dial, reload };
}

/** Every transmitter site and the frequencies on the air, refreshed each minute, when the window
 *  comes back to the front and on `reload` (after a change to the stations). Its `generation` says
 *  when the catalog changed, so everything holding station ids reloads. */
export function useOverview(api: Api | null) {
  const [overview, setOverview] = useState<Overview | null>(null);
  const reload = useCallback(() => { api?.overview().then(setOverview, () => {}); }, [api]);
  useEffect(() => {
    if (!api) return;
    reload();
    const t = setInterval(reload, ON_AIR_REFRESH_MS);
    const seen = () => { if (document.visibilityState === 'visible') reload(); };
    document.addEventListener('visibilitychange', seen);
    window.addEventListener('focus', seen);
    return () => {
      clearInterval(t);
      document.removeEventListener('visibilitychange', seen);
      window.removeEventListener('focus', seen);
    };
  }, [api, reload]);
  return { overview, reload };
}

export type Filters = Omit<StationQuery, 'offset' | 'limit' | 'facets'>;
export const NO_FILTERS: Filters = { q: '', bands: [], langs: [], countries: [], regions: [], sources: [], site: null, lens: 'all', scan: null, sort: 'freq' };

interface SearchState {
  rows: StationRow[];
  total: number;
  onAir: number;
  facets: StationPage['facets'];
  /** Rows the schedule has none for (signals, favorites), all of them. */
  off: OffSchedule[];
  /** The Heard lens: stations and signals in one list, paged like `rows`. */
  heard: HeardRow[];
  /** Under a scan filter: what it read on each loaded row it heard. */
  scanDb: Record<number, number>;
  loading: boolean;
}

/** The station list for `filters`: first page on change (debounced), more on demand, and
 *  the loaded rows refreshed each minute (on-air dots) and when `refresh` changes. */
export function useStationSearch(api: Api | null, filters: Filters, refresh: unknown = null) {
  const [st, setSt] = useState<SearchState>({ rows: [], total: 0, onAir: 0, facets: null, off: [], heard: [], scanDb: {}, loading: true });
  const seq = useRef(0);
  const shown = useRef(0);
  shown.current = st.rows.length + st.heard.length;
  const key = JSON.stringify(filters);

  /** Load rows [0, n) in pages, replacing what is shown. */
  const load = useCallback(async (n: number) => {
    if (!api) return;
    const my = ++seq.current;
    const first = await api.search({ ...filters, offset: 0, limit: PAGE_ROWS, facets: true });
    const rows = [...first.items];
    const heard = [...first.heard];
    const scanDb = { ...first.scan_db };
    // a page holds either rows or Heard entries
    while (rows.length + heard.length < Math.min(n, first.total)) {
      const p = await api.search({ ...filters, offset: rows.length + heard.length, limit: 200, facets: false });
      if (!p.items.length && !p.heard.length) break;
      rows.push(...p.items);
      heard.push(...p.heard);
      Object.assign(scanDb, p.scan_db);
    }
    if (my === seq.current) {
      setSt({ rows, total: first.total, onAir: first.on_air, facets: first.facets, off: first.off_schedule, heard, scanDb, loading: false });
    }
  }, [api, key, refresh]); // `key` stands for `filters`

  useEffect(() => {
    setSt((s) => ({ ...s, loading: true }));
    const t = setTimeout(() => { load(PAGE_ROWS).catch(() => {}); }, SEARCH_DEBOUNCE_MS);
    const r = setInterval(() => { load(Math.max(PAGE_ROWS, shown.current)).catch(() => {}); }, ON_AIR_REFRESH_MS);
    return () => {
      clearTimeout(t);
      clearInterval(r);
    };
  }, [load]);

  const more = useCallback(() => { load(shown.current + PAGE_ROWS).catch(() => {}); }, [load]);
  return { ...st, more };
}

/** Re-render every `ms` (clocks, "now" markers). */
export function useNow(ms: number) {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return now;
}
