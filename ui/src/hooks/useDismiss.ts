// Close a popover or sheet on Escape, and (optionally) on a press outside `ref`.

import { useEffect, useRef, type RefObject } from 'react';

export function useDismiss(ref: RefObject<HTMLElement | null>, onClose: () => void, active = true, outside = true) {
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    if (!active) return;
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close.current();
    };
    const down = (e: PointerEvent) => {
      if (outside && ref.current && !ref.current.contains(e.target as Node)) close.current();
    };
    document.addEventListener('keydown', key);
    document.addEventListener('pointerdown', down);
    return () => {
      document.removeEventListener('keydown', key);
      document.removeEventListener('pointerdown', down);
    };
  }, [ref, active, outside]);
}
