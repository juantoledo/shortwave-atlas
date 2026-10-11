// How this window draws, kept per device (the GPU it costs is this device's): the globe behind
// the whole window with frosted panels, or framed in the stage with solid ones.

import { useCallback, useState } from 'react';

const FULL_KEY = 'swatlas.fullGlobe';

/** On unless this device turned it off (the page works without storage). */
function loadFull(): boolean {
  try { return localStorage.getItem(FULL_KEY) !== '0'; } catch { return true; }
}
function saveFull(on: boolean) {
  try { localStorage.setItem(FULL_KEY, on ? '1' : '0'); } catch { /* private window: this session only */ }
}

export interface DisplayApi {
  fullGlobe: boolean;
  setFullGlobe: (on: boolean) => void;
}

export function useDisplay(): DisplayApi {
  const [fullGlobe, setFull] = useState(loadFull);
  const setFullGlobe = useCallback((on: boolean) => {
    saveFull(on);
    setFull(on);
  }, []);
  return { fullGlobe, setFullGlobe };
}
