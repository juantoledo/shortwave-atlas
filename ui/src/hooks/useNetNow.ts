// The internet station the receiver plays instead of the rig (the NET band), for every window:
// asked every couple of seconds while internet radio is on, so a station started, stopped or
// failed elsewhere (another window, tuning the rig) shows here too; more often while its stream
// connects or fills its buffer, so the faceplate follows it.

import { useCallback, useEffect, useRef, useState } from 'react';
import type { Api } from '../api/client';
import type { NetNow } from '../types/generated/NetNow';
import { unlessSame } from './same';

const POLL_MS = 2000;
const FILLING_POLL_MS = 500;

export function useNetNow(api: Api | null, enabled: boolean) {
  const [now, setNow] = useState<NetNow | null>(null);
  // bumped by `setNetNow`: an answer asked for before it is stale and dropped
  const gen = useRef(0);
  const load = useCallback(() => {
    if (!api) return;
    const g = gen.current;
    api.netNow().then((n) => { if (g === gen.current) setNow(unlessSame(n)); }, () => {});
  }, [api]);
  const setNetNow = useCallback((n: NetNow | null) => {
    gen.current++;
    setNow(n);
  }, []);
  const filling = now?.state.state === 'connecting' || now?.state.state === 'buffering';
  useEffect(() => {
    if (!api || !enabled) {
      setNow(null);
      return;
    }
    load();
    const id = setInterval(load, filling ? FILLING_POLL_MS : POLL_MS);
    return () => clearInterval(id);
  }, [api, enabled, load, filling]);
  return { netNow: now, setNetNow, reloadNetNow: load };
}
