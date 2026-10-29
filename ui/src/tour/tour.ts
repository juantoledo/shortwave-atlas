// The guided tour: the parts of the window it shows, one at a time, and where its card goes
// beside each (components/Tour.tsx draws it). And the one-time tips on deeper features
// (components/Tip.tsx), with which of them this device has seen.

import type { View } from '../components/NavBar';
import type { Part } from '../hooks/folds';
import type { Reader } from '../hooks/usePersisted';

/** The tour's steps, in order; each is also the `data-tour` of the element it shows. */
export const STEP_IDS = ['globe', 'browse', 'station', 'dial', 'audio', 'top', 'power', 'help'] as const;
export type StepId = (typeof STEP_IDS)[number];

export interface TourStep {
  id: StepId;
  /** The view to switch to first (it is all a phone shows, a side column's tab elsewhere). */
  view?: View;
  /** The panel to unfold first. */
  reveal?: Part;
}

export const TOUR_STEPS: readonly TourStep[] = [
  { id: 'globe', view: 'globe' },
  { id: 'browse', view: 'browse', reveal: 'browse' },
  { id: 'station', view: 'station', reveal: 'station' },
  { id: 'dial' },
  { id: 'audio' },
  { id: 'top' },
  { id: 'power' },
  { id: 'help' },
];

/** The step `dir` from `i` of `n`: none past the last (the tour is over), never before the first. */
export const stepTo = (i: number, n: number, dir: 1 | -1): number | null =>
  i + dir >= n ? null : Math.max(0, i + dir);

export interface Rect { left: number; top: number; width: number; height: number }
export interface Size { width: number; height: number }
export type Side = 'below' | 'above' | 'right' | 'left' | 'inside';

const GAP = 12, MARGIN = 12, PAD = 6;

/** The spotlight over `r`: a little larger, kept inside the window. */
export function holeOf(r: Rect, vw: number, vh: number): Rect {
  const left = Math.max(0, r.left - PAD), top = Math.max(0, r.top - PAD);
  const right = Math.min(vw, r.left + r.width + PAD), bottom = Math.min(vh, r.top + r.height + PAD);
  return { left, top, width: Math.max(0, right - left), height: Math.max(0, bottom - top) };
}

/** Where the card goes beside `r`: under it, over it, right or left of it, whichever has room
 *  first; over the bottom of the window when nothing does (a target that fills it). Never off
 *  screen. */
export function placeCard(r: Rect, c: Size, vw: number, vh: number): { left: number; top: number; side: Side } {
  const x = (v: number) => Math.min(Math.max(MARGIN, v), Math.max(MARGIN, vw - MARGIN - c.width));
  const y = (v: number) => Math.min(Math.max(MARGIN, v), Math.max(MARGIN, vh - MARGIN - c.height));
  const right = r.left + r.width, bottom = r.top + r.height;
  const midX = x(r.left + r.width / 2 - c.width / 2), midY = y(r.top + r.height / 2 - c.height / 2);
  if (vh - bottom - GAP - MARGIN >= c.height) return { left: midX, top: bottom + GAP, side: 'below' };
  if (r.top - GAP - MARGIN >= c.height) return { left: midX, top: r.top - GAP - c.height, side: 'above' };
  if (vw - right - GAP - MARGIN >= c.width) return { left: right + GAP, top: midY, side: 'right' };
  if (r.left - GAP - MARGIN >= c.width) return { left: r.left - GAP - c.width, top: midY, side: 'left' };
  return { left: x((vw - c.width) / 2), top: y(vh - MARGIN - c.height), side: 'inside' };
}

/** The features with a tip of their own. */
export const TIP_IDS = ['net', 'stats', 'scans', 'favorites'] as const;
export type TipId = (typeof TIP_IDS)[number];

/** The tips this device has seen: the known ones of what is stored. */
export const readSeen: Reader<TipId[]> = (raw) =>
  Array.isArray(raw) ? TIP_IDS.filter((id) => raw.includes(id)) : null;
