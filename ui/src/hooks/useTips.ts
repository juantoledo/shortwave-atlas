// The one-time tips (components/Tip.tsx): which this device has seen, and none at all while
// something covers the window (the welcome, the tour, a sheet), so a first run shows one thing
// at a time. Given to the components below App through `TipsProvider`.

import { createContext, useCallback, useContext, useMemo } from 'react';
import { readSeen, type TipId } from '../tour/tour';
import { usePersisted } from './usePersisted';

export interface TipsApi {
  shown: (id: TipId) => boolean;
  dismiss: (id: TipId) => void;
  /** Show them all again. */
  reset: () => void;
}

export function useTipsState(quiet: boolean): TipsApi {
  const [seen, setSeen] = usePersisted<TipId[]>('swatlas.tipsSeen', [], readSeen);
  const dismiss = useCallback((id: TipId) => setSeen((s) => (s.includes(id) ? s : [...s, id])), []);
  const reset = useCallback(() => setSeen([]), []);
  return useMemo(() => ({ shown: (id) => !quiet && !seen.includes(id), dismiss, reset }), [quiet, seen]);
}

const NONE: TipsApi = { shown: () => false, dismiss: () => {}, reset: () => {} };
const TipsContext = createContext<TipsApi>(NONE);
export const TipsProvider = TipsContext.Provider;
export const useTips = (): TipsApi => useContext(TipsContext);
