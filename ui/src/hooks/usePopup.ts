// Place a floating list against its anchor (a button or a text box): under it, or over it when
// there's more room above; as wide as its options need, at least the anchor, never off screen.
// The list lives in a portal on <body> (a sheet's transform would trap `position: fixed`), so
// "outside" means outside both `box` and the list. Closes when pressed outside or when the page
// scrolls or resizes under it.

import { useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from 'react';

export interface Place { left: number; minWidth: number; maxWidth: number; top?: number; bottom?: number; maxHeight: number }

const GAP = 6, MARGIN = 10, MIN_W = 220, MAX_W = 440, MAX_H = 340;

export function usePopup(
  open: boolean,
  anchor: RefObject<HTMLElement | null>,
  box: RefObject<HTMLElement | null>,
  list: RefObject<HTMLElement | null>,
  onClose: () => void,
): Place | null {
  const [place, setPlace] = useState<Place | null>(null);
  const close = useRef(onClose);
  close.current = onClose;

  useLayoutEffect(() => {
    if (!open) {
      setPlace(null);
      return;
    }
    const r = anchor.current!.getBoundingClientRect();
    const vw = window.innerWidth, vh = window.innerHeight;
    const maxWidth = Math.min(MAX_W, vw - 2 * MARGIN);
    const minWidth = Math.min(Math.max(r.width, MIN_W), maxWidth);
    const left = Math.min(Math.max(MARGIN, r.left), vw - MARGIN - minWidth);
    const below = vh - r.bottom - GAP - MARGIN, above = r.top - GAP - MARGIN;
    setPlace(below >= Math.min(MAX_H, 200) || below >= above
      ? { left, minWidth, maxWidth, top: r.bottom + GAP, maxHeight: Math.min(MAX_H, below) }
      : { left, minWidth, maxWidth, bottom: vh - r.top + GAP, maxHeight: Math.min(MAX_H, above) });

    const inside = (t: EventTarget | null) => t instanceof Node && (!!box.current?.contains(t) || !!list.current?.contains(t));
    const moved = (e: Event) => {
      if (!inside(e.target)) close.current();
    };
    window.addEventListener('resize', moved);
    window.addEventListener('scroll', moved, true);
    document.addEventListener('pointerdown', moved);
    return () => {
      window.removeEventListener('resize', moved);
      window.removeEventListener('scroll', moved, true);
      document.removeEventListener('pointerdown', moved);
    };
  }, [open, anchor, box, list]);

  // wider than the anchor: pull it back inside the window
  useLayoutEffect(() => {
    const l = list.current;
    if (!place || !l) return;
    const over = l.getBoundingClientRect().right - (window.innerWidth - MARGIN);
    l.style.left = `${Math.max(MARGIN, place.left - Math.max(0, over))}px`;
  });

  return place;
}

/** The list's style: hidden until placed. */
export const placeStyle = (p: Place | null): CSSProperties => p ?? { visibility: 'hidden' };

/** Scroll the list (only the list: moving the page would close it) to show item `i`. */
export function keepInView(list: HTMLElement | null, i: number) {
  const el = list?.children[i] as HTMLElement | undefined;
  if (!list || !el) return;
  if (el.offsetTop < list.scrollTop) list.scrollTop = el.offsetTop - 4;
  else if (el.offsetTop + el.offsetHeight > list.scrollTop + list.clientHeight) list.scrollTop = el.offsetTop + el.offsetHeight - list.clientHeight + 4;
}
