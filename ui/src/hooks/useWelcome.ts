// The welcome splash at start, until this device ticks "Don't show this again" (the page works
// without storage: then it shows every time).

import { useCallback, useState } from 'react';

const HIDE_KEY = 'swatlas.hideWelcome';

/** Shown unless this device asked not to see it again. */
export const shouldShow = (stored: string | null): boolean => stored !== '1';

function load(): boolean {
  try { return shouldShow(localStorage.getItem(HIDE_KEY)); } catch { return true; }
}
function save() {
  try { localStorage.setItem(HIDE_KEY, '1'); } catch { /* private window: shown again next time */ }
}

export interface WelcomeApi {
  open: boolean;
  close: (dontShow: boolean) => void;
}

export function useWelcome(): WelcomeApi {
  const [open, setOpen] = useState(load);
  const close = useCallback((dontShow: boolean) => {
    if (dontShow) save();
    setOpen(false);
  }, []);
  return { open, close };
}
