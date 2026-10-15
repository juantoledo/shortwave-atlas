// Polled answers that haven't changed keep the state they'd replace: React then skips the
// render (a new but equal object would re-render everything that reads it).

/** A state update to `next`, unless the state already equals it (as JSON). */
export function unlessSame<T>(next: T): (cur: T) => T {
  return (cur) => (cur === next || JSON.stringify(cur) === JSON.stringify(next) ? cur : next);
}
