// Connection to the core, the live rig state, and station lookups.

import { useCallback, useEffect, useRef, useState } from 'react';
import { Api } from '../api/client';
import { connect, type LinkStatus } from '../api/transport';
import type { Candidate } from '../types/generated/Candidate';
import type { Info } from '../types/generated/Info';
import type { RigState } from '../types/generated/RigState';

export const FREQ_MIN_HZ = 30_000;
export const FREQ_MAX_HZ = 56_000_000;
/** Keep showing a frequency we just set for this long while the rig catches up. */
const PENDING_MS = 1500;
/** Wait for the dial to settle before looking stations up (prototype value). */
const LOOKUP_DEBOUNCE_MS = 380;
/** Air status changes by the minute; refresh the list this often. */
const LIST_REFRESH_MS = 20_000;

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
export function useTuning(api: Api | null, rigHz: number | null, onError: (e: unknown) => void) {
  const [pending, setPending] = useState<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    if (pending !== null && rigHz === pending) setPending(null);
  }, [rigHz, pending]);

  const tune = useCallback((hz: number) => {
    if (!api) return;
    hz = Math.round(Math.min(FREQ_MAX_HZ, Math.max(FREQ_MIN_HZ, hz)) / 100) * 100;
    setPending(hz);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setPending(null), PENDING_MS);
    api.rig({ cmd: 'set_freq', hz }).catch(onError);
  }, [api, onError]);

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

/** Every station (list and dial marks), refreshed periodically and when `refreshKey` changes. */
export function useStationList(api: Api | null, refreshKey: unknown) {
  const [list, setList] = useState<Candidate[]>([]);
  useEffect(() => {
    if (!api) return;
    const load = () => api.list().then(setList, () => {});
    load();
    const t = setInterval(load, LIST_REFRESH_MS);
    return () => clearInterval(t);
  }, [api, refreshKey]);
  return list;
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
