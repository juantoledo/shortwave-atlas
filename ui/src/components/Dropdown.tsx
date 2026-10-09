// Dropdowns: a button that opens a listbox (Dropdown), and a text box that filters one
// (Combobox). Options can carry a dim hint (a band's kHz range) and a row count. Native
// <select>s and <datalist>s can't lay out columns or be styled alike on every webview, hence
// these. Keys as in a select: arrows, Home/End, PgUp/PgDn, Enter (or Space) to pick, Escape or
// Tab to close; in the Dropdown, letters jump to an option.

import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from 'react';
import { createPortal } from 'react-dom';
import { keepInView, placeStyle, usePopup, type Place } from '../hooks/usePopup';
import { useT } from '../i18n';

export interface DropOpt {
  value: string;
  label: string;
  /** Dim text after the label (a band's range). */
  hint?: string;
  /** Rows it would give, shown in a pill. */
  n?: number;
  disabled?: boolean;
}

const PAGE = 8;

/** The next usable option from `i` going `dir`, or `i` itself if none. */
function step(items: DropOpt[], usable: (o: DropOpt) => boolean, i: number, dir: 1 | -1) {
  for (let j = i + dir; j >= 0 && j < items.length; j += dir) if (usable(items[j])) return j;
  return i;
}
const jump = (items: DropOpt[], usable: (o: DropOpt) => boolean, i: number, dir: 1 | -1) =>
  usable(items[i]) ? i : step(items, usable, i, dir);

/** Arrows, Home/End, PgUp/PgDn as a new active index, or null for other keys. */
function moveKey(key: string, items: DropOpt[], usable: (o: DropOpt) => boolean, i: number): number | null {
  const last = items.length - 1;
  switch (key) {
    case 'ArrowDown': return step(items, usable, i, 1);
    case 'ArrowUp': return step(items, usable, i, -1);
    case 'Home': return jump(items, usable, 0, 1);
    case 'End': return jump(items, usable, last, -1);
    case 'PageDown': return jump(items, usable, Math.min(last, i + PAGE), -1);
    case 'PageUp': return jump(items, usable, Math.max(0, i - PAGE), 1);
    default: return null;
  }
}

interface ListProps {
  id: string;
  list: RefObject<HTMLUListElement | null>;
  place: Place | null;
  items: DropOpt[];
  active: number;
  selected: string;
  usable: (o: DropOpt) => boolean;
  labelledBy?: string;
  /** The listbox takes focus (Dropdown) rather than leaving it in a text box (Combobox). */
  focusable: boolean;
  onActive: (i: number) => void;
  onPick: (o: DropOpt) => void;
  onKeyDown?: (e: KeyboardEvent) => void;
  empty?: ReactNode;
}

function OptionList(p: ListProps) {
  const t = useT();
  useEffect(() => {
    if (p.place) keepInView(p.list.current, p.active);
  }, [p.place, p.active, p.list]);
  return createPortal(
    <ul
      ref={p.list}
      id={`${p.id}-list`}
      className="drop-list"
      role="listbox"
      tabIndex={p.focusable ? -1 : undefined}
      aria-labelledby={p.labelledBy}
      aria-activedescendant={p.focusable && p.items.length ? `${p.id}-o${p.active}` : undefined}
      style={placeStyle(p.place)}
      onKeyDown={p.onKeyDown}
      // a text box keeps its focus while the list is pressed
      onPointerDown={p.focusable ? undefined : (e) => e.preventDefault()}
    >
      {p.items.map((o, i) => (
        <li
          key={o.value || '*'}
          id={`${p.id}-o${i}`}
          role="option"
          className={'drop-o' + (i === p.active ? ' act' : '') + (o.value === '' ? ' all' : '')}
          aria-selected={o.value === p.selected}
          aria-disabled={!p.usable(o) || undefined}
          onPointerMove={() => p.usable(o) && i !== p.active && p.onActive(i)}
          onClick={() => p.onPick(o)}
        >
          <svg className="drop-tick" viewBox="0 0 12 10" aria-hidden="true"><path d="M1 5l3.5 3.5L11 1" /></svg>
          <span className="drop-l">{o.label}</span>
          {o.hint && <span className="drop-h">{o.hint}</span>}
          {o.n !== undefined && <span className="drop-n">{o.n.toLocaleString(t.locale)}</span>}
        </li>
      ))}
      {p.items.length === 0 && p.empty && <li className="drop-empty" role="presentation">{p.empty}</li>}
    </ul>,
    document.body,
  );
}

const Chevron = () => <svg className="drop-chev" viewBox="0 0 10 6" aria-hidden="true"><path d="M1 1l4 4 4-4" /></svg>;

interface DropdownProps {
  /** Id of the visible label. */
  labelledBy: string;
  value: string;
  opts: DropOpt[];
  onChange: (v: string) => void;
  /** A first "any" option with value ''. */
  all?: string;
  /** Light the button up (a filter that is set). */
  lit?: boolean;
}

export function Dropdown({ labelledBy, value, opts, onChange, all, lit }: DropdownProps) {
  const id = useId();
  const box = useRef<HTMLDivElement>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const typed = useRef({ text: '', at: 0 });
  const items: DropOpt[] = all === undefined ? opts : [{ value: '', label: all }, ...opts];
  const usable = (o: DropOpt) => !o.disabled || o.value === value;
  const cur = items.find((o) => o.value === value);

  const close = (focus = true) => {
    setOpen(false);
    if (focus) btn.current?.focus();
  };
  const place = usePopup(open, btn, box, list, () => close(false));
  useEffect(() => {
    if (place) list.current?.focus({ preventScroll: true });
  }, [place]);

  const show = () => {
    setActive(Math.max(0, items.findIndex((o) => o.value === value)));
    setOpen(true);
  };
  const pick = (o: DropOpt) => {
    if (!usable(o)) return;
    if (o.value !== value) onChange(o.value);
    close();
  };

  const onListKey = (e: KeyboardEvent) => {
    const to = moveKey(e.key, items, usable, active);
    if (to !== null) setActive(to);
    else if (e.key === 'Enter' || e.key === ' ') pick(items[active]);
    else if (e.key === 'Escape') close();
    else if (e.key === 'Tab') return close();
    else {
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
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div className="drop-box" ref={box}>
      <button
        ref={btn}
        type="button"
        className={'drop' + (lit ? ' lit' : '')}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? `${id}-list` : undefined}
        aria-labelledby={`${labelledBy} ${id}-v`}
        onClick={() => (open ? close() : show())}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault();
            show();
          }
        }}
      >
        <span className="drop-v" id={`${id}-v`}>
          <span>{cur?.label ?? value}</span>
          {cur?.hint && <small>{cur.hint}</small>}
        </span>
        <Chevron />
      </button>
      {open && (
        <OptionList
          id={id} list={list} place={place} items={items} active={active} selected={value} usable={usable}
          labelledBy={labelledBy} focusable onActive={setActive} onPick={pick} onKeyDown={onListKey}
        />
      )}
    </div>
  );
}

interface ComboProps {
  labelledBy: string;
  /** The text in the box. */
  text: string;
  onText: (text: string) => void;
  opts: DropOpt[];
  /** The value picked, ticked in the list. */
  selected: string;
  onPick: (o: DropOpt) => void;
  placeholder?: string;
  /** Shown when nothing matches. */
  empty?: ReactNode;
  /** Most options listed at once. */
  max?: number;
}

/** Options whose value, label and hint hold every word typed. */
export function matching(opts: DropOpt[], text: string): DropOpt[] {
  const words = text.toLocaleLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return opts;
  return opts.filter((o) => {
    const hay = `${o.value} ${o.label} ${o.hint ?? ''}`.toLocaleLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

export function Combobox({ labelledBy, text, onText, opts, selected, onPick, placeholder, empty, max = 200 }: ComboProps) {
  const id = useId();
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  // the whole list until something is typed: the box usually holds the current pick
  const [typed, setTyped] = useState(false);
  const items = (typed ? matching(opts, text) : opts).slice(0, max);
  const usable = (o: DropOpt) => !o.disabled;
  const place = usePopup(open, input, box, list, () => setOpen(false));

  const show = (filtering: boolean) => {
    const all = filtering ? matching(opts, text) : opts;
    setTyped(filtering);
    setActive(filtering ? 0 : Math.max(0, all.findIndex((o) => o.value === selected)));
    setOpen(true);
  };
  const pick = (o: DropOpt) => {
    if (!usable(o)) return;
    onPick(o);
    setOpen(false);
  };

  const onKey = (e: KeyboardEvent) => {
    if (!open) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        show(false);
      }
      return;
    }
    const to = e.key === 'Home' || e.key === 'End' ? null : moveKey(e.key, items, usable, active);
    if (to !== null) setActive(to);
    else if (e.key === 'Enter' && items[active]) pick(items[active]);
    else if (e.key === 'Escape') setOpen(false);
    else {
      if (e.key === 'Tab') setOpen(false);
      return;
    }
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div className="drop-box combo" ref={box}>
      <input
        ref={input}
        role="combobox"
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={open ? `${id}-list` : undefined}
        aria-activedescendant={open && items.length ? `${id}-o${active}` : undefined}
        aria-labelledby={labelledBy}
        autoComplete="off"
        spellCheck={false}
        value={text}
        placeholder={placeholder}
        onChange={(e) => {
          onText(e.currentTarget.value);
          setTyped(true);
          setActive(0);
          setOpen(true);
        }}
        onClick={() => (open ? setOpen(false) : show(false))}
        onKeyDown={onKey}
      />
      <Chevron />
      {open && (
        <OptionList
          id={id} list={list} place={place} items={items} active={active} selected={selected} usable={usable}
          labelledBy={labelledBy} focusable={false} onActive={setActive} onPick={pick} empty={empty}
        />
      )}
    </div>
  );
}
