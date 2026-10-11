// The globe's transmitter sites keep one object each across overview refreshes. three-globe
// joins its data by object identity: new objects every minute would rebuild every tower (a
// hitch), and towers only rise or sink (`pointsTransitionDuration`) when their object stays.

import type { SiteDot } from '../types/generated/SiteDot';

/**
 * `next` as the objects `cache` already holds for those ids, with the new fields copied onto
 * them (three-globe's own fields on them stay). Ids no longer there leave the cache. Returns
 * `prev` itself when nothing changed, so the globe has nothing to do.
 */
export function stableSites(cache: Map<number, SiteDot>, prev: readonly SiteDot[], next: readonly SiteDot[]): readonly SiteDot[] {
  let changed = next.length !== prev.length;
  const list = next.map((n, i) => {
    let c = cache.get(n.id);
    if (!c) {
      c = { ...n };
      cache.set(n.id, c);
      changed = true;
    } else if ((Object.keys(n) as (keyof SiteDot)[]).some((k) => c![k] !== n[k])) {
      Object.assign(c, n);
      changed = true;
    }
    if (prev[i] !== c) changed = true;
    return c;
  });
  if (cache.size > next.length) {
    const ids = new Set(next.map((n) => n.id));
    for (const id of cache.keys()) if (!ids.has(id)) cache.delete(id);
  }
  return changed ? list : prev;
}
