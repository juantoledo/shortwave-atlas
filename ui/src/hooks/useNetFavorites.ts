// The NET band's favorites: internet stations kept, which are also its memory channels (in the
// order they were added). Read at start and after every change; a change elsewhere (another
// window) shows when this one comes back into view.

import { useCallback, useEffect, useState } from 'react';
import type { Api } from '../api/client';
import type { NetFavorite } from '../types/generated/NetFavorite';

export interface NetFavoritesApi {
  list: NetFavorite[];
  /** By station uuid. */
  byUuid: ReadonlyMap<string, NetFavorite>;
  add: (uuid: string) => Promise<void>;
  rename: (id: string, label: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
}

export function useNetFavorites(api: Api | null, enabled: boolean, onError: (e: unknown) => void): NetFavoritesApi {
  const [list, setList] = useState<NetFavorite[]>([]);
  const load = useCallback(() => { api?.netFavorites().then(setList, () => {}); }, [api]);
  useEffect(() => {
    if (!enabled) return;
    load();
    const again = () => { if (document.visibilityState === 'visible') load(); };
    window.addEventListener('focus', again);
    document.addEventListener('visibilitychange', again);
    return () => {
      window.removeEventListener('focus', again);
      document.removeEventListener('visibilitychange', again);
    };
  }, [enabled, load]);
  const run = useCallback(async (f: () => Promise<unknown>) => {
    try {
      await f();
    } catch (e) {
      onError(e);
    }
    load();
  }, [load, onError]);
  const [byUuid, setByUuid] = useState<ReadonlyMap<string, NetFavorite>>(new Map());
  useEffect(() => setByUuid(new Map(list.map((f) => [f.uuid, f]))), [list]);
  return {
    list,
    byUuid,
    add: (uuid) => run(() => api!.addNetFavorite(uuid, null)),
    rename: (id, label) => run(() => api!.renameNetFavorite(id, label)),
    remove: (id) => run(() => api!.deleteNetFavorite(id)),
  };
}
