// The guided tour (tour/tour.ts): open or not, at which step, and whether this device has been
// through it (or skipped it), which the welcome splash asks before offering it again.

import { useCallback, useState } from 'react';

const DONE_KEY = 'swatlas.tourDone';

function load(): boolean {
  try { return localStorage.getItem(DONE_KEY) === '1'; } catch { return false; }
}
function save() {
  try { localStorage.setItem(DONE_KEY, '1'); } catch { /* private window: offered again next time */ }
}

export interface TourApi {
  open: boolean;
  index: number;
  /** Taken or skipped on this device. */
  done: boolean;
  start: () => void;
  go: (i: number) => void;
  finish: () => void;
}

export function useTour(): TourApi {
  const [open, setOpen] = useState(false);
  const [index, setIndex] = useState(0);
  const [done, setDone] = useState(load);
  const start = useCallback(() => { setIndex(0); setOpen(true); }, []);
  const finish = useCallback(() => { save(); setDone(true); setOpen(false); }, []);
  return { open, index, done, start, go: setIndex, finish };
}
