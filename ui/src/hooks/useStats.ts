// The listening history as the core works it out: the session being recorded (for the top
// bar), the stats of a period while the Stats view is open, the nudges of the Hunt tab, and the
// channels listened to (the band dial's heat strip).
// The core records; these only ask, every few seconds, so the view stays live.

import { useCallback, useEffect, useState } from 'react';
import type { Api } from '../api/client';
import type { FreqHeat } from '../types/generated/FreqHeat';
import type { Hunt } from '../types/generated/Hunt';
import type { ListeningNow } from '../types/generated/ListeningNow';
import type { NetStats } from '../types/generated/NetStats';
import type { Period } from '../types/generated/Period';
import type { Stats } from '../types/generated/Stats';
import { unlessSame } from './same';

/** How often the top bar and the open view ask again. */
const POLL_MS = 10_000;

/** Minutes east of UTC here (the core's `tz_offset_min`). */
export const tzOffsetMin = () => -new Date().getTimezoneOffset();

/** Run `f` now and every `ms` while `on`. */
function usePoll(on: boolean, ms: number, f: () => void) {
  useEffect(() => {
    if (!on) return;
    f();
    const id = setInterval(f, ms);
    return () => clearInterval(id);
  }, [on, ms, f]);
}

/** The session under way, or the usual station at this hour. */
export function useListening(api: Api | null) {
  const [now, setNow] = useState<ListeningNow | null>(null);
  const load = useCallback(() => { api?.listening().then((n) => setNow(unlessSame<ListeningNow | null>(n)), () => {}); }, [api]);
  usePoll(api !== null, POLL_MS, load);
  return { listening: now, reload: load };
}

/** The stats of `period`, live while `on`. */
export function useStats(api: Api | null, period: Period, on: boolean) {
  const [stats, setStats] = useState<Stats | null>(null);
  const load = useCallback(() => {
    api?.stats({ period, tz_offset_min: tzOffsetMin() }).then((s) => setStats(unlessSame<Stats | null>(s)), () => {});
  }, [api, period]);
  usePoll(on && api !== null, POLL_MS, load);
  return { stats, reload: load };
}

/** Internet listening in `period` (the NET band, kept apart from the rig's stats), live while `on`. */
export function useNetStats(api: Api | null, period: Period, on: boolean) {
  const [stats, setStats] = useState<NetStats | null>(null);
  const load = useCallback(() => {
    api?.netStats({ period, tz_offset_min: tzOffsetMin() }).then(setStats, () => {});
  }, [api, period]);
  usePoll(on && api !== null, POLL_MS, load);
  return { netStats: stats, reloadNet: load };
}

/** What to hunt now; asked again with `key` (the app's clock). */
export function useHunt(api: Api | null, on: boolean, key: unknown) {
  const [hunt, setHunt] = useState<Hunt | null>(null);
  useEffect(() => {
    if (!on || !api) return;
    let alive = true;
    api.hunt().then((h) => { if (alive) setHunt(h); }, () => {});
    return () => { alive = false; };
  }, [api, on, key]);
  return hunt;
}

/** The channels listened to with a valid signal; asked again with `key` (the session under way,
 *  so a session that starts or ends shows). */
export function useFreqHeat(api: Api | null, key: unknown) {
  const [heat, setHeat] = useState<FreqHeat[]>([]);
  useEffect(() => {
    if (!api) return;
    let alive = true;
    api.listenedFreqs().then((h) => { if (alive) setHeat(h); }, () => {});
    return () => { alive = false; };
  }, [api, key]);
  return heat;
}
