import { describe, expect, it } from 'vitest';
import { isMac, keyAction, queryHz, stepIndex, type KeyCtx, type KeyLike } from './keymap';

const CTX: KeyCtx = { typing: false, control: false, widget: false, sheet: false, mac: false };
const codeOf = (key: string) => (/^[a-z]$/i.test(key) ? 'Key' + key.toUpperCase() : /^\d$/.test(key) ? 'Digit' + key : key);
const k = (key: string, mods: Partial<KeyLike> = {}): KeyLike =>
  ({ key, code: codeOf(key), shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, ...mods });
const act = (key: string, mods: Partial<KeyLike> = {}, ctx: Partial<KeyCtx> = {}) => keyAction(k(key, mods), { ...CTX, ...ctx });

describe('keymap', () => {
  it('sends printable keys to the search', () => {
    expect(act('b')).toEqual({ kind: 'search', text: 'b' });
    expect(act('B', { shiftKey: true })).toEqual({ kind: 'search', text: 'B' });
    expect(act('9')).toEqual({ kind: 'search', text: '9' });
    expect(act('ñ')).toEqual({ kind: 'search', text: 'ñ' });
    // AltGr reports Ctrl+Alt but still types
    expect(act('@', { ctrlKey: true, altKey: true, altGraph: true })).toEqual({ kind: 'search', text: '@' });
    // a control still lets letters through: only Space is its own
    expect(act('x', {}, { control: true })).toEqual({ kind: 'search', text: 'x' });
    expect(act('x', {}, { widget: true })).toEqual({ kind: 'search', text: 'x' });
  });

  it('leaves text fields, open sheets and composition alone', () => {
    expect(act('b', {}, { typing: true })).toBeNull();
    expect(act('ArrowLeft', {}, { typing: true })).toBeNull();
    expect(act('b', {}, { sheet: true })).toBeNull();
    expect(act(' ', {}, { sheet: true })).toBeNull();
    expect(keyAction({ ...k('a'), isComposing: true }, CTX)).toBeNull();
    expect(act('Dead')).toBeNull();
    expect(act('Shift', { shiftKey: true })).toBeNull();
    expect(act('c', { ctrlKey: true })).toBeNull();
    expect(act('v', { metaKey: true }, { mac: true })).toBeNull();
    expect(act('x', { altKey: true })).toBeNull();
  });

  it('keeps Help on F1 and ?', () => {
    expect(act('F1', {}, { typing: true })).toEqual({ kind: 'help' });
    expect(act('?', { shiftKey: true })).toEqual({ kind: 'help' });
    expect(act('?', { shiftKey: true }, { typing: true })).toBeNull();
  });

  it('focuses the search and the readout', () => {
    expect(act('/')).toEqual({ kind: 'focusSearch' });
    expect(act('k', { ctrlKey: true })).toEqual({ kind: 'focusSearch' });
    expect(act('k', { metaKey: true }, { mac: true })).toEqual({ kind: 'focusSearch' });
    expect(act('k', { ctrlKey: true }, { mac: true })).toBeNull();
    expect(act('F2')).toEqual({ kind: 'freq' });
    expect(act('g', { ctrlKey: true })).toEqual({ kind: 'freq' });
  });

  it('tunes with arrows and page keys', () => {
    expect(act('ArrowRight')).toEqual({ kind: 'nudge', khz: 1 });
    expect(act('ArrowLeft', { shiftKey: true })).toEqual({ kind: 'nudge', khz: -5 });
    expect(act('PageUp')).toEqual({ kind: 'band', dir: 1 });
    expect(act('PageDown')).toEqual({ kind: 'band', dir: -1 });
    expect(act('ArrowDown')).toBeNull();
    expect(act('ArrowDown', { shiftKey: true })).toEqual({ kind: 'cand', dir: 1 });
    expect(act('ArrowUp', { altKey: true })).toEqual({ kind: 'station', dir: -1 });
    expect(act('ArrowDown', { altKey: true })).toEqual({ kind: 'station', dir: 1 });
  });

  it('matches Alt combinations by key position', () => {
    expect(act('1', { altKey: true })).toEqual({ kind: 'mode', mode: 'AM' });
    expect(act('4', { altKey: true })).toEqual({ kind: 'mode', mode: 'CW' });
    // macOS: ⌥M types µ, the code is still KeyM
    expect(keyAction({ ...k('µ', { altKey: true }), code: 'KeyM' }, { ...CTX, mac: true })).toEqual({ kind: 'mute' });
  });

  it('plays audio on Space unless a button has focus', () => {
    expect(act(' ')).toEqual({ kind: 'audio' });
    expect(act(' ', {}, { control: true })).toBeNull();
    // a slider or checkbox keeps its arrows and Space
    expect(act(' ', {}, { widget: true })).toBeNull();
    expect(act('ArrowRight', {}, { widget: true })).toBeNull();
    expect(act('PageUp', {}, { widget: true })).toBeNull();
    expect(act('ArrowUp', { ctrlKey: true })).toEqual({ kind: 'volume', d: 0.1 });
    expect(act('ArrowDown', { metaKey: true }, { mac: true })).toEqual({ kind: 'volume', d: -0.1 });
  });

  it('steps through rows', () => {
    expect(stepIndex(0, -1, 1)).toBeNull();
    expect(stepIndex(3, -1, 1)).toBe(0);
    expect(stepIndex(3, -1, -1)).toBe(2);
    expect(stepIndex(3, 1, 1)).toBe(2);
    expect(stepIndex(3, 2, 1)).toBeNull();
    expect(stepIndex(3, 0, -1)).toBeNull();
  });

  it('reads a frequency query', () => {
    expect(queryHz('9410')).toBe(9_410_000);
    expect(queryHz(' 6070.5 ')).toBe(6_070_500);
    expect(queryHz('6070,5')).toBe(6_070_500);
    expect(queryHz('bbc')).toBeNull();
    expect(queryHz('9')).toBeNull();
    expect(queryHz('')).toBeNull();
  });

  it('knows a Mac', () => {
    expect(isMac('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)')).toBe(true);
    expect(isMac('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')).toBe(false);
  });
});
