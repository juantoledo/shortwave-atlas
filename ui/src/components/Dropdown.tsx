// A filter dropdown: a button that opens a listbox of options, each with an optional hint
// (a band's kHz range) and a row count. Native <select>s can't lay out columns or be styled
// alike on every webview, hence this one. Keys as in a select: arrows, Home/End, PgUp/PgDn,
// Enter or Space to pick, Escape or Tab to close, letters to jump.

import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react';
import { useDismiss } from '../hooks/useDismiss';
import { useT } from '../i18n';

export interface DropOpt {
  value: string;
  label: string;
  /** Dim text after the label (a band's range). */
  hint?: string;
  n: number;
}

interface Props {
  label: string;
  /** The "no filter" option, value ''. */
  all: string;
  value: string;
  opts: DropOpt[];
  onChange: (v: string) => void;
}

/** Where the list opens: under the button, or over it when there's more room above. */
interface Place { left: number; minWidth: number; maxWidth: number; top?: number; bottom?: number; maxHeight: number }

const GAP = 6, MARGIN = 10, MIN_W = 220, MAX_W = 440, MAX_H = 340;

export function Dropdown({ label, all, value, opts, onChange }: Props) {
  const t = useT();
  const id = useId();
  const box = useRef<HTMLDivElement>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const [open, setOpen] = useState(false);
  const [place, setPlace] = useState<Place | null>(null);
  const items: DropOpt[] = [{ value: '', label: all, n: -1 }, ...opts];
  const usable = (o: DropOpt) => o.n !== 0 || o.value === value;
  const [active, setActive] = useState(0);
  const typed = useRef({ text: '', at: 0 });
  const cur = items.find((o) => o.value === value) ?? items[0];

  const close = (focus = true) => {
    setOpen(false);
    if (focus) btn.current?.focus();
  };
  useDismiss(box, () => close(false), open);

  const show = () => {
    setActive(Math.max(0, items.findIndex((o) => o.value === value)));
    setOpen(true);
  };
  const pick = (o: DropOpt) => {
    if (!usable(o)) return;
    if (o.value !== value) onChange(o.value);
    close();
  };

  // place the list against the button; close it if the page moves under it
  useLayoutEffect(() => {
    if (!open) return;
    const at = () => {
      const r = btn.current!.getBoundingClientRect();
      const vw = window.innerWidth, vh = window.innerHeight;
      // as wide as the options need (names are never cut short of MAX_W), at least the button
      const maxWidth = Math.min(MAX_W, vw - 2 * MARGIN);
      const minWidth = Math.min(Math.max(r.width, MIN_W), maxWidth);
      const left = Math.min(Math.max(MARGIN, r.left), vw - MARGIN - minWidth);
      const below = vh - r.bottom - GAP - MARGIN, above = r.top - GAP - MARGIN;
      setPlace(below >= Math.min(MAX_H, 200) || below >= above
        ? { left, minWidth, maxWidth, top: r.bottom + GAP, maxHeight: Math.min(MAX_H, below) }
        : { left, minWidth, maxWidth, bottom: vh - r.top + GAP, maxHeight: Math.min(MAX_H, above) });
    };
    at();
    const moved = (e: Event) => {
      if (e.target instanceof Node && list.current?.contains(e.target)) return;
      close(false);
    };
    window.addEventListener('resize', moved);
    window.addEventListener('scroll', moved, true);
    return () => {
      window.removeEventListener('resize', moved);
      window.removeEventListener('scroll', moved, true);
    };
  }, [open]);

  // wider than the button: keep it inside the window
  useLayoutEffect(() => {
    const l = list.current;
    if (!open || !place || !l) return;
    const over = l.getBoundingClientRect().right - (window.innerWidth - MARGIN);
    l.style.left = `${Math.max(MARGIN, place.left - Math.max(0, over))}px`;
  }, [open, place]);
  useEffect(() => {
    if (open && place) list.current?.focus({ preventScroll: true });
  }, [open, place]);
  // keep the active option in view
  useEffect(() => {
    const l = list.current, el = l?.children[active] as HTMLElement | undefined;
    if (!open || !place || !l || !el) return;
    // scroll the list only (scrollIntoView could move the page, which closes the list)
    if (el.offsetTop < l.scrollTop) l.scrollTop = el.offsetTop - 4;
    else if (el.offsetTop + el.offsetHeight > l.scrollTop + l.clientHeight) l.scrollTop = el.offsetTop + el.offsetHeight - l.clientHeight + 4;
  }, [open, active, place]);

  /** The next usable option from `i` going `dir`, or `i` itself if none. */
  const step = (i: number, dir: 1 | -1) => {
    for (let j = i + dir; j >= 0 && j < items.length; j += dir) if (usable(items[j])) return j;
    return i;
  };
  const jump = (i: number, dir: 1 | -1) => (usable(items[i]) ? i : step(i, dir));

  const onBtnKey = (e: KeyboardEvent) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      show();
    }
  };
  const onListKey = (e: KeyboardEvent) => {
    const page = 8;
    switch (e.key) {
      case 'ArrowDown': setActive((i) => step(i, 1)); break;
      case 'ArrowUp': setActive((i) => step(i, -1)); break;
      case 'Home': setActive(jump(0, 1)); break;
      case 'End': setActive(jump(items.length - 1, -1)); break;
      case 'PageDown': setActive((i) => jump(Math.min(items.length - 1, i + page), -1)); break;
      case 'PageUp': setActive((i) => jump(Math.max(0, i - page), 1)); break;
      case 'Enter':
      case ' ': pick(items[active]); break;
      case 'Escape': close(); break;
      case 'Tab': close(false); return;
      default: {
        if (e.key.length !== 1 || e.ctrlKey || e.metaKey || e.altKey) return;
        const now = Date.now(), ty = typed.current;
        ty.text = (now - ty.at > 700 ? '' : ty.text) + e.key.toLocaleLowerCase();
        ty.at = now;
        const from = ty.text.length === 1 ? active + 1 : active;
        for (let k = 0; k < items.length; k++) {
          const j = (from + k) % items.length;
          if (usable(items[j]) && items[j].label.toLocaleLowerCase().startsWith(ty.text)) {
            setActive(j);
            break;
          }
        }
      }
    }
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div className="flt" ref={box}>
      <span className="flt-lab" id={`${id}-l`}>{label}</span>
      <button
        ref={btn}
        type="button"
        className={'drop' + (value ? ' set' : '')}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? `${id}-list` : undefined}
        aria-labelledby={`${id}-l ${id}-v`}
        onClick={() => (open ? close() : show())}
        onKeyDown={onBtnKey}
      >
        <span className="drop-v" id={`${id}-v`}>
          {cur.label}
          {cur.hint && <small>{cur.hint}</small>}
        </span>
        <svg className="drop-chev" viewBox="0 0 10 6" aria-hidden="true"><path d="M1 1l4 4 4-4" /></svg>
      </button>
      {open && (
        <ul
          ref={list}
          id={`${id}-list`}
          className="drop-list"
          role="listbox"
          tabIndex={-1}
          aria-labelledby={`${id}-l`}
          aria-activedescendant={`${id}-o${active}`}
          style={place ?? { visibility: 'hidden' }}
          onKeyDown={onListKey}
        >
          {items.map((o, i) => (
            <li
              key={o.value || '*'}
              id={`${id}-o${i}`}
              role="option"
              className={'drop-o' + (i === active ? ' act' : '') + (i === 0 ? ' all' : '')}
              aria-selected={o.value === value}
              aria-disabled={!usable(o) || undefined}
              onPointerMove={() => usable(o) && i !== active && setActive(i)}
              onClick={() => pick(o)}
            >
              <svg className="drop-tick" viewBox="0 0 12 10" aria-hidden="true"><path d="M1 5l3.5 3.5L11 1" /></svg>
              <span className="drop-l">{o.label}</span>
              {o.hint && <span className="drop-h">{o.hint}</span>}
              {o.n >= 0 && <span className="drop-n">{o.n.toLocaleString(t.locale)}</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
