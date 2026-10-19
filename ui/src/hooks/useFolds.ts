// The folds (folds.ts) at the window's layout size, kept on this device.

import { useCallback, useMemo } from 'react';
import { dockTier, foldedAttr, isFolded, readFolds, sideTier, withFold, type Folds, type Part } from './folds';
import { NARROW, PHONE, SHORT, useMedia, WIDE } from './useMedia';
import { usePersisted } from './usePersisted';

const NONE: Folds = {};

export interface FoldsApi {
  /** For `data-folded`: empty when nothing is. */
  attr: string;
  folded: (p: Part) => boolean;
  toggle: (p: Part) => void;
  /** Unfold, if it is folded. */
  reveal: (p: Part) => void;
}

export function useFolds(): FoldsApi {
  const wide = useMedia(WIDE), narrow = useMedia(NARROW), phone = useMedia(PHONE), short = useMedia(SHORT);
  const [folds, setFolds] = usePersisted<Folds>('swatlas.folds', NONE, readFolds);
  const tiers = useMemo(() => ({ side: sideTier(wide, narrow, phone), dock: dockTier(short, phone) }), [wide, narrow, phone, short]);
  const toggle = useCallback((p: Part) => setFolds((f) => withFold(f, p, tiers, !isFolded(f, p, tiers))), [tiers]);
  const reveal = useCallback((p: Part) => setFolds((f) => withFold(f, p, tiers, false)), [tiers]);
  return {
    attr: foldedAttr(folds, tiers),
    folded: (p) => isFolded(folds, p, tiers),
    toggle,
    reveal,
  };
}
