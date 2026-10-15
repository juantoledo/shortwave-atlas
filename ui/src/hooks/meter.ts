// The S-meter, apart from the rest of the rig state: it changes with every poll (5 a second),
// and only the meter and the globe's received ring show it. Held in this store, a reading
// re-renders the meter alone; the stage reads it in its frame loop.

import { useSyncExternalStore } from 'react';

export class Meter {
  private db: number | null = null;
  private subs = new Set<() => void>();

  get = (): number | null => this.db;

  set(db: number | null) {
    if (db === this.db) return;
    this.db = db;
    this.subs.forEach((f) => f());
  }

  subscribe = (f: () => void): (() => void) => {
    this.subs.add(f);
    return () => { this.subs.delete(f); };
  };
}

/** The reading, re-rendering the caller as it changes. */
export function useMeter(m: Meter): number | null {
  return useSyncExternalStore(m.subscribe, m.get);
}
