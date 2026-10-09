// Global keys. Typing anywhere goes to the station search, so every other shortcut is a key that
// types nothing (arrows, PageUp/Down, F-keys) or a combination. Pure, so it is tested without a DOM.

import type { Mode } from '../types/generated/Mode';

export type Action =
  | { kind: 'search'; text: string }
  | { kind: 'focusSearch' }
  | { kind: 'freq' }
  | { kind: 'nudge'; khz: number }
  | { kind: 'band'; dir: 1 | -1 }
  | { kind: 'station'; dir: 1 | -1 }
  | { kind: 'cand'; dir: 1 | -1 }
  | { kind: 'mode'; mode: Mode }
  | { kind: 'audio' }
  | { kind: 'mute' }
  | { kind: 'volume'; d: number }
  | { kind: 'help' };

/** The parts of a KeyboardEvent the keymap reads. */
export interface KeyLike {
  key: string;
  code: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  isComposing?: boolean;
  /** AltGr on Windows/Linux reports Ctrl+Alt; the character it gives is still text. */
  altGraph?: boolean;
}

export interface KeyCtx {
  /** Focus is in a text field: its keys are its own. */
  typing: boolean;
  /** Focus is on a button or similar, which Space and Enter activate. */
  control: boolean;
  /** Focus is on a slider, checkbox or radio: arrows and Space are its own, letters still search. */
  widget: boolean;
  /** Settings or Help is open. */
  sheet: boolean;
  /** macOS: ⌘ instead of Ctrl. */
  mac: boolean;
}

const MODE_KEYS: Record<string, Mode> = { Digit1: 'AM', Digit2: 'USB', Digit3: 'LSB', Digit4: 'CW' };
export const VOLUME_STEP = 0.1;

const WIDGET_INPUTS = /^(checkbox|radio|range|button|submit|reset|color|file|image)$/;

/** Typing somewhere, so keys are text rather than shortcuts (a select types ahead too). */
export const typing = (el: EventTarget | null) =>
  el instanceof HTMLElement &&
  (el.isContentEditable || /^(TEXTAREA|SELECT)$/.test(el.tagName) || (el instanceof HTMLInputElement && !WIDGET_INPUTS.test(el.type)));

export const widget = (el: EventTarget | null) => el instanceof HTMLInputElement && WIDGET_INPUTS.test(el.type);

/** A control that Space activates. */
export const control = (el: EventTarget | null) =>
  el instanceof Element && el.closest('button, a[href], summary, [role="button"], [role="radio"], [role="option"]') !== null;

export const isMac = (ua: string) => /Mac|iPhone|iPad/.test(ua);

/** The modifier names as the keyboard shows them. */
export const modName = (mac: boolean) => (mac ? '⌘' : 'Ctrl+');
export const altName = (mac: boolean) => (mac ? '⌥' : 'Alt+');

export function keyAction(k: KeyLike, c: KeyCtx): Action | null {
  const a = mapKey(k, c);
  return a && c.widget && !['search', 'focusSearch', 'help', 'freq'].includes(a.kind) ? null : a;
}

function mapKey(k: KeyLike, c: KeyCtx): Action | null {
  if (k.isComposing) return null;
  if (k.key === 'F1') return { kind: 'help' };
  if (c.typing) return null;
  if (k.key === '?' && !k.ctrlKey && !k.metaKey) return { kind: 'help' };
  if (c.sheet) return null;

  const mod = c.mac ? k.metaKey && !k.ctrlKey : k.ctrlKey && !k.metaKey;
  const plain = !k.ctrlKey && !k.metaKey && !k.altKey;

  if (mod && !k.altKey && !k.shiftKey) {
    if (k.code === 'KeyK') return { kind: 'focusSearch' };
    if (k.code === 'KeyG') return { kind: 'freq' };
    if (k.key === 'ArrowUp') return { kind: 'volume', d: VOLUME_STEP };
    if (k.key === 'ArrowDown') return { kind: 'volume', d: -VOLUME_STEP };
    return null;
  }
  // matched by code: on macOS Option changes the character (⌥M is µ)
  if (k.altKey && !k.ctrlKey && !k.metaKey && !k.shiftKey) {
    if (k.code in MODE_KEYS) return { kind: 'mode', mode: MODE_KEYS[k.code] };
    if (k.code === 'KeyM') return { kind: 'mute' };
    if (k.key === 'ArrowDown') return { kind: 'station', dir: 1 };
    if (k.key === 'ArrowUp') return { kind: 'station', dir: -1 };
    return null;
  }

  if (plain || k.altGraph) {
    switch (k.key) {
      case 'ArrowLeft': return { kind: 'nudge', khz: k.shiftKey ? -5 : -1 };
      case 'ArrowRight': return { kind: 'nudge', khz: k.shiftKey ? 5 : 1 };
      case 'ArrowDown': return k.shiftKey ? { kind: 'cand', dir: 1 } : null;
      case 'ArrowUp': return k.shiftKey ? { kind: 'cand', dir: -1 } : null;
      case 'PageUp': return { kind: 'band', dir: 1 };
      case 'PageDown': return { kind: 'band', dir: -1 };
      case 'F2': return { kind: 'freq' };
      case '/': return { kind: 'focusSearch' };
      case ' ': return c.control ? null : { kind: 'audio' };
    }
    if ([...k.key].length === 1 && k.key.trim() !== '') return { kind: 'search', text: k.key };
  }
  return null;
}

/** The row `dir` away from the current one; from no row, the first (or the last going up). */
export function stepIndex(len: number, cur: number, dir: 1 | -1): number | null {
  if (!len) return null;
  if (cur < 0) return dir > 0 ? 0 : len - 1;
  const to = cur + dir;
  return to >= 0 && to < len ? to : null;
}

/** A query that is only a frequency in kHz ("9410", "6070.5"), as Hz. */
export function queryHz(q: string): number | null {
  const t = q.trim().replace(',', '.');
  return /^\d{2,5}(\.\d{1,3})?$/.test(t) ? Math.round(Number(t) * 1000) : null;
}
