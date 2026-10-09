// A CSS media query as React state (layout itself stays in CSS; this is for behaviour).

import { useEffect, useState } from 'react';

const match = (q: string) => typeof window !== 'undefined' && !!window.matchMedia?.(q).matches;

export function useMedia(query: string): boolean {
  const [on, setOn] = useState(() => match(query));
  useEffect(() => {
    const mq = window.matchMedia?.(query);
    if (!mq) return;
    const f = () => setOn(mq.matches);
    f();
    mq.addEventListener('change', f);
    return () => mq.removeEventListener('change', f);
  }, [query]);
  return on;
}

/** Fingers rather than a mouse: hints say "tap" and targets grow. */
export const COARSE = '(pointer: coarse)';
/** The phone layout (bottom tabs): keep in step with styles.css. */
export const PHONE = '(max-width: 599px)';
