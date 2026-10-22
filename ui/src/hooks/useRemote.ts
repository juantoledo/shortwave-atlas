// Remote access status, polled while something shows it (Settings → Remote access, the top-bar
// lamp). `null` = don't poll.

import { useCallback, useEffect, useState } from 'react';
import type { Api } from '../api/client';
import type { RemoteStatus } from '../types/generated/RemoteStatus';

export function useRemote(api: Api | null, everyMs: number | null) {
  const [status, setStatus] = useState<RemoteStatus | null>(null);

  const reload = useCallback(() => api?.remoteStatus().then(setStatus, () => {}), [api]);

  useEffect(() => {
    if (!api || everyMs === null) return;
    let alive = true;
    const tick = () => api.remoteStatus().then((s) => { if (alive) setStatus(s); }, () => {});
    tick();
    const id = setInterval(tick, everyMs);
    return () => { alive = false; clearInterval(id); };
  }, [api, everyMs]);

  return { status, setStatus, reload };
}
