// Favorites: the core's list with each one's station in the running schedule, reloaded after a
// change and when the window comes back to the front (another window may have changed them).

import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Api } from '../api/client';
import type { Favorite } from '../types/generated/Favorite';
import type { FavoriteRequest } from '../types/generated/FavoriteRequest';
import type { FavoriteView } from '../types/generated/FavoriteView';

export function useFavorites(api: Api | null, onError: (e: unknown) => void) {
  const [views, setViews] = useState<FavoriteView[]>([]);

  const reload = useCallback(() => { api?.favorites().then(setViews, () => {}); }, [api]);
  useEffect(() => {
    reload();
    const seen = () => { if (document.visibilityState === 'visible') reload(); };
    document.addEventListener('visibilitychange', seen);
    window.addEventListener('focus', seen);
    return () => {
      document.removeEventListener('visibilitychange', seen);
      window.removeEventListener('focus', seen);
    };
  }, [reload]);

  const run = useCallback(async <T,>(f: (api: Api) => Promise<T>): Promise<T | null> => {
    if (!api) return null;
    try {
      return await f(api);
    } catch (e) {
      onError(e);
      return null;
    } finally {
      reload();
    }
  }, [api, onError, reload]);

  const add = useCallback((req: FavoriteRequest) => run((a) => a.addFavorite(req)), [run]);
  const rename = useCallback((id: string, label: string) => run((a) => a.renameFavorite(id, label)), [run]);
  const remove = useCallback((id: string) => run((a) => a.deleteFavorite(id)), [run]);

  /** Favorites by the station id they have in the running schedule. */
  const byStation = useMemo(() => {
    const m = new Map<number, Favorite>();
    for (const v of views) if (v.station_id !== null && !m.has(v.station_id)) m.set(v.station_id, v.favorite);
    return m;
  }, [views]);

  return { views, byStation, add, rename, remove };
}

export type FavoritesApi = ReturnType<typeof useFavorites>;
