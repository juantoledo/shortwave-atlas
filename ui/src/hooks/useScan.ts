// Band scans: the core's scan status, polled (fast while one runs), the saved scans and the
// logbook, reloaded when a scan ends, as a running one hears something (`heard_rev`), and when
// the catalog's `generation` changes (the logbook's station ids with it); and the saved scans
// the band dial draws.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { Api } from '../api/client';
import { lastTwo } from '../components/scan';
import type { Logged } from '../types/generated/Logged';
import type { Scan } from '../types/generated/Scan';
import type { ScanSpeed } from '../types/generated/ScanSpeed';
import type { ScanStatus } from '../types/generated/ScanStatus';
import type { ScanSummary } from '../types/generated/ScanSummary';
import { unlessSame } from './same';

const BUSY_MS = 300;
/** A scan started from another window shows up within this. */
const IDLE_MS = 5000;

export function useScan(api: Api | null, onError: (e: unknown) => void, generation: number | null) {
  const [status, setStatus] = useState<ScanStatus>({ state: 'idle' });
  const [scans, setScans] = useState<ScanSummary[]>([]);
  /** The saved scans have been read once (until then, `scans` being empty says nothing). */
  const [loaded, setLoaded] = useState(false);
  const [logbook, setLogbook] = useState<Logged[]>([]);
  const cache = useRef(new Map<string, Promise<Scan>>());

  const reload = useCallback(() => {
    if (!api) return;
    api.scans().then((s) => {
      setScans(s);
      setLoaded(true);
    }, () => {});
    api.logbook().then(setLogbook, () => {});
  }, [api]);
  useEffect(reload, [reload, generation]);

  const running = status.state === 'running';
  // a scan seen running and then done, in the same render as the change (the globe's finale);
  // cleared when the next one starts
  const [prev, setPrev] = useState(status);
  const [finished, setFinished] = useState<{ id: string; sites: number[] } | null>(null);
  if (status !== prev) {
    setPrev(status);
    if (prev.state === 'running' && status.state === 'done') setFinished({ id: status.id, sites: status.sites });
    else if (running) setFinished(null);
  }

  useEffect(() => {
    if (!api) return;
    const tick = () => void api.scanStatus().then((s) => setStatus(unlessSame(s)), () => {});
    tick();
    const id = setInterval(tick, running ? BUSY_MS : IDLE_MS);
    return () => clearInterval(id);
  }, [api, running]);

  // a scan ended: its result is in the list and the logbook now; a failure seen ending is told once
  const ended = status.state === 'done' ? status.id : status.state === 'failed' ? status.message : null;
  const wasRunning = useRef(false);
  useEffect(() => {
    if (ended !== null) reload();
    if (wasRunning.current && status.state === 'failed') onError(status.message);
    wasRunning.current = running;
  }, [ended, running]); // on a change of state only, not of `reload` or `onError`

  // a running scan heard something new: it is in the logbook already (provisionally)
  const heardRev = status.state === 'running' ? status.heard_rev : 0;
  useEffect(() => {
    if (api && heardRev > 0) api.logbook().then(setLogbook, () => {});
  }, [api, heardRev]);

  const start = useCallback(async (bandId: string, stepHz: number, speed: ScanSpeed) => {
    if (!api) return;
    try {
      setStatus(await api.startScan({ band_id: bandId, step_hz: stepHz, dwell_ms: null, speed }));
    } catch (e) {
      onError(e);
    }
  }, [api, onError]);
  const stop = useCallback(() => { api?.stopScan().catch(onError); }, [api, onError]);
  const remove = useCallback(async (id: string) => {
    if (!api) return;
    try {
      await api.deleteScan(id);
      cache.current.delete(id);
      reload();
    } catch (e) {
      onError(e);
    }
  }, [api, onError, reload]);
  const removeAll = useCallback(async () => {
    if (!api) return;
    try {
      await api.deleteScans();
      cache.current.clear();
      reload();
    } catch (e) {
      onError(e);
    }
  }, [api, onError, reload]);
  /** A saved scan with its samples (asked once). */
  const load = useCallback((id: string): Promise<Scan> => {
    if (!api) return Promise.reject(new Error('not connected'));
    let p = cache.current.get(id);
    if (!p) {
      p = api.scan(id);
      p.catch(() => cache.current.delete(id));
      cache.current.set(id, p);
    }
    return p;
  }, [api]);

  const logged = useMemo(() => new Map(logbook.map((l) => [l.station_id, l])), [logbook]);
  const loggedSites = useMemo(() => new Set(logbook.map((l) => l.site_id)), [logbook]);
  return { status, finished, scans, loaded, logged, loggedSites, start, stop, remove, removeAll, load };
}

export type ScanApi = ReturnType<typeof useScan>;

/** The band's newest saved scan and the one before it (the ghost), with their samples. */
export function useBandScans(scan: ScanApi, bandId: string | null) {
  const [summary, before] = bandId ? lastTwo(scan.scans, bandId) : [null, null];
  const [full, setFull] = useState<{ latest: Scan | null; ghost: Scan | null }>({ latest: null, ghost: null });
  const { load } = scan;
  const a = summary?.id ?? null, b = before?.id ?? null;
  useEffect(() => {
    let alive = true;
    Promise.all([a ? load(a) : null, b ? load(b) : null]).then(
      ([latest, ghost]) => { if (alive) setFull({ latest, ghost }); },
      () => { if (alive) setFull({ latest: null, ghost: null }); },
    );
    return () => { alive = false; };
  }, [a, b, load]);
  return { summary, ...full };
}
