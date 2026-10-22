// Which page a browser gets: the full atlas (index.html) or the phone remote (remote.html).
// A phone opening the atlas is sent to the remote, unless its listener chose the full atlas there
// (kept on that device). The desktop window is always the atlas.

import type { TransportKind } from './api/transport';
import { COARSE, PHONE } from './hooks/useMedia';

/** Kept as '1' once the listener opens the full atlas from the remote. */
export const FULL_KEY = 'swatlas.fullAtlas';
/** The remote's page, next to index.html. */
export const REMOTE_PAGE = 'remote.html';

export interface PageFacts {
  transport: TransportKind;
  /** Phone width (`PHONE`) and a touch screen (`COARSE`). */
  phone: boolean;
  coarse: boolean;
  /** The listener chose the full atlas on this device. */
  keptFull: boolean;
}

export const openPage = (f: PageFacts): 'atlas' | 'remote' =>
  f.transport === 'ws' && f.phone && f.coarse && !f.keptFull ? 'remote' : 'atlas';

function stored(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/** This device's facts, read once at start. */
export function pageFacts(transport: TransportKind): PageFacts {
  const mq = (q: string) => !!window.matchMedia?.(q).matches;
  let keptFull = false;
  try {
    keptFull = stored()?.getItem(FULL_KEY) === '1';
  } catch { /* storage refused: nothing kept */ }
  return { transport, phone: mq(PHONE), coarse: mq(COARSE), keptFull };
}

/** Remember the choice (the full atlas, or the remote again) and go there. */
export function goTo(page: 'atlas' | 'remote') {
  try {
    if (page === 'atlas') stored()?.setItem(FULL_KEY, '1');
    else stored()?.removeItem(FULL_KEY);
  } catch { /* not kept: the next visit asks again */ }
  location.assign(page === 'atlas' ? './' : REMOTE_PAGE);
}
