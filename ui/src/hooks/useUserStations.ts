// The listener's own stations: the core's list with each one's row in the running catalog,
// reloaded when the catalog's `generation` changes. A change rebuilds the catalog, so after one
// `onChanged` reloads what holds station ids (the overview, whose generation moves the rest).

import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Api } from '../api/client';
import type { UserStationDraft } from '../types/generated/UserStationDraft';
import type { UserStationView } from '../types/generated/UserStationView';

export function useUserStations(api: Api | null, onError: (e: unknown) => void, generation: number | null, onChanged: () => void) {
  const [views, setViews] = useState<UserStationView[]>([]);

  const reload = useCallback(() => { api?.userStations().then(setViews, () => {}); }, [api]);
  useEffect(reload, [reload, generation]);

  /** Run a change; `null` (and the error told) if it fails. */
  const run = useCallback(async <T,>(f: (api: Api) => Promise<T>): Promise<T | null> => {
    if (!api) return null;
    try {
      const out = await f(api);
      onChanged();
      return out;
    } catch (e) {
      onError(e);
      return null;
    } finally {
      reload();
    }
  }, [api, onError, onChanged, reload]);

  const add = useCallback((d: UserStationDraft) => run((a) => a.addUserStation(d)), [run]);
  const update = useCallback((id: string, d: UserStationDraft) => run((a) => a.updateUserStation(id, d)), [run]);
  const remove = useCallback((id: string) => run((a) => a.deleteUserStation(id)), [run]);
  const importJson = useCallback((json: string, replace: boolean) => run((a) => a.importUserStations(json, replace)), [run]);

  /** The stations by their id (`Station.key` in the catalog). */
  const byId = useMemo(() => new Map(views.map((v) => [v.station.id, v])), [views]);

  return { views, byId, add, update, remove, importJson };
}

export type UserStationsApi = ReturnType<typeof useUserStations>;
