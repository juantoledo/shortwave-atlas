// "Update available": the core's update status, polled (fast while downloading), and what
// the banner shows for it.

import { useCallback, useEffect, useState } from 'react';
import type { Api } from '../api/client';
import type { TransportKind } from '../api/transport';
import type { Channel } from '../types/generated/Channel';
import type { ManualReason } from '../types/generated/ManualReason';
import type { UpdateStatus } from '../types/generated/UpdateStatus';

/** The core checks by itself; this only picks its result up. */
const IDLE_MS = 60_000;
const BUSY_MS = 500;
const SKIP_KEY = 'swatlas.skipUpdate';

export type UpdateAction =
  /** This window can install it (desktop app). */
  | { kind: 'install' }
  /** Installed by hand: a link to the release. */
  | { kind: 'download'; reason: ManualReason }
  /** A remote browser of a desktop app: install it there. */
  | { kind: 'host' };

export interface UpdateView {
  version: string;
  notes: string;
  url: string;
  phase: 'offer' | 'downloading' | 'installing' | 'failed';
  /** Download progress 0-100, `null` when the size is unknown. */
  percent: number | null;
  error: string | null;
  action: UpdateAction;
}

/** What the banner shows, or `null` for nothing. A skipped version is only hidden while offered. */
export function updateView(s: UpdateStatus | null, transport: TransportKind, skipped: string | null): UpdateView | null {
  if (!s) return null;
  const st = s.state;
  if (!('release' in st) || !st.release) return null;
  const release = st.release;
  if (st.state === 'available' && skipped === release.version) return null;
  const action: UpdateAction =
    s.install.kind === 'manual' ? { kind: 'download', reason: s.install.reason }
    : transport === 'tauri' ? { kind: 'install' }
    : { kind: 'host' };
  return {
    version: release.version,
    notes: release.notes,
    url: release.url,
    phase: st.state === 'available' ? 'offer' : st.state,
    percent: st.state === 'downloading' && st.total ? Math.min(100, Math.round((st.done / st.total) * 100)) : null,
    error: st.state === 'failed' ? st.message : null,
    action,
  };
}

/** Per-viewer "skip this version" (the page works without storage). */
function loadSkipped(): string | null {
  try { return localStorage.getItem(SKIP_KEY); } catch { return null; }
}
function saveSkipped(v: string) {
  try { localStorage.setItem(SKIP_KEY, v); } catch { /* private window: skipped for this session only */ }
}

export function useUpdate(api: Api | null) {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [skipped, setSkipped] = useState<string | null>(loadSkipped);

  const refresh = useCallback(async () => { if (api) setStatus(await api.updateStatus()); }, [api]);
  const busy = status?.state.state === 'downloading' || status?.state.state === 'installing';

  useEffect(() => {
    if (!api) return;
    const tick = () => void refresh().catch(() => {});
    tick();
    const id = setInterval(tick, busy ? BUSY_MS : IDLE_MS);
    return () => clearInterval(id);
  }, [api, busy, refresh]);

  const check = useCallback(async () => { if (api) setStatus(await api.checkUpdate()); }, [api]);
  const install = useCallback(async () => {
    if (!api) return;
    await api.installUpdate();
    await refresh();
  }, [api, refresh]);
  const setPrefs = useCallback(async (check: boolean, channel: Channel) => {
    if (!api) return;
    await api.setUpdatePrefs({ check, channel });
    await refresh();
  }, [api, refresh]);
  const skip = useCallback((v: string) => { saveSkipped(v); setSkipped(v); }, []);

  return { status, skipped, check, install, setPrefs, skip };
}

export type UpdateApi = ReturnType<typeof useUpdate>;
