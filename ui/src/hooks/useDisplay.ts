// How this window draws, kept per device (the GPU it costs is this device's): the globe behind
// the whole window with frosted panels, or framed in the stage with solid ones; and whether
// Stats shows its game layer (Collection).

import { useCallback, useState } from 'react';

const FULL_KEY = 'swatlas.fullGlobe';
const COLLECTION_KEY = 'swatlas.collection';

/** On unless this device turned it off (the page works without storage). */
function load(key: string): boolean {
  try { return localStorage.getItem(key) !== '0'; } catch { return true; }
}
function save(key: string, on: boolean) {
  try { localStorage.setItem(key, on ? '1' : '0'); } catch { /* private window: this session only */ }
}

export interface DisplayApi {
  fullGlobe: boolean;
  setFullGlobe: (on: boolean) => void;
  /** Stats shows its Collection tab (passport, badges, bingo). */
  collection: boolean;
  setCollection: (on: boolean) => void;
}

export function useDisplay(): DisplayApi {
  const [fullGlobe, setFull] = useState(() => load(FULL_KEY));
  const [collection, setColl] = useState(() => load(COLLECTION_KEY));
  const setFullGlobe = useCallback((on: boolean) => {
    save(FULL_KEY, on);
    setFull(on);
  }, []);
  const setCollection = useCallback((on: boolean) => {
    save(COLLECTION_KEY, on);
    setColl(on);
  }, []);
  return { fullGlobe, setFullGlobe, collection, setCollection };
}
