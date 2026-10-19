// What is folded away: the side panels (to a rail) and the faceplate's two cells (to their first
// line). Each layout size keeps its own choice, so folding on a small window leaves the big one
// as it was; until the listener picks, small layouts start folded (`FOLDED`).

import type { Reader } from './usePersisted';

/** The side column's layout (styles.css): three columns, one tabbed column (wide or narrow), phone. */
export type SideTier = 'wide' | 'mid' | 'narrow' | 'phone';
/** The faceplate's: roomy, or short windows and phones. */
export type DockTier = 'tall' | 'short';
/** What folds: `side` is the one tabbed column that holds both panels below three columns. */
export type Section = 'browse' | 'station' | 'side' | 'rx' | 'audio';
/** What the window asks about: a panel, or a faceplate cell. */
export type Part = 'browse' | 'station' | 'rx' | 'audio';
/** The listener's choices, keyed `section.tier`; what isn't there is the default. */
export type Folds = Record<string, boolean>;

export const sideTier = (wide: boolean, narrow: boolean, phone: boolean): SideTier =>
  phone ? 'phone' : wide ? 'wide' : narrow ? 'narrow' : 'mid';
export const dockTier = (short: boolean, phone: boolean): DockTier => (short || phone ? 'short' : 'tall');

export interface Tiers { side: SideTier; dock: DockTier }

/** Folded until the listener opens it. */
const FOLDED = new Set(['side.narrow', 'rx.short', 'audio.short']);

/** The section a part folds with, and the key of its choice; null where it never folds. */
export function keyOf(part: Part, tiers: Tiers): string | null {
  if (part === 'rx' || part === 'audio') return `${part}.${tiers.dock}`;
  switch (tiers.side) {
    case 'phone': return null;
    case 'wide': return `${part}.wide`;
    default: return `side.${tiers.side}`;
  }
}

export function isFolded(folds: Folds, part: Part, tiers: Tiers): boolean {
  const k = keyOf(part, tiers);
  return k !== null && (folds[k] ?? FOLDED.has(k));
}

/** `folds` with `part` folded (or not) at this layout. */
export function withFold(folds: Folds, part: Part, tiers: Tiers, folded: boolean): Folds {
  const k = keyOf(part, tiers);
  return k === null || isFolded(folds, part, tiers) === folded ? folds : { ...folds, [k]: folded };
}

/** What is folded now, for `data-folded` (styles.css matches it with `~=`). */
export function foldedAttr(folds: Folds, tiers: Tiers): string {
  const out: Section[] = [];
  if (tiers.side === 'wide') {
    if (isFolded(folds, 'browse', tiers)) out.push('browse');
    if (isFolded(folds, 'station', tiers)) out.push('station');
  } else if (isFolded(folds, 'browse', tiers)) {
    out.push('side');
  }
  if (isFolded(folds, 'rx', tiers)) out.push('rx');
  if (isFolded(folds, 'audio', tiers)) out.push('audio');
  return out.join(' ');
}

const KEY = /^(browse|station)\.wide$|^side\.(mid|narrow)$|^(rx|audio)\.(tall|short)$/;

/** The choices that are still ones: known keys with a yes or a no. */
export const readFolds: Reader<Folds> = (raw) => {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return null;
  const out: Folds = {};
  for (const [k, v] of Object.entries(raw)) if (KEY.test(k) && typeof v === 'boolean') out[k] = v;
  return out;
};
