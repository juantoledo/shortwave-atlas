import { describe, expect, it } from 'vitest';
import { dockTier, foldedAttr, isFolded, keyOf, readFolds, sideTier, withFold, type Tiers } from './folds';

const WIDE: Tiers = { side: 'wide', dock: 'tall' };
const MID: Tiers = { side: 'mid', dock: 'tall' };
const NARROW: Tiers = { side: 'narrow', dock: 'short' };
const PHONE: Tiers = { side: 'phone', dock: 'short' };

describe('tiers', () => {
  it('follows the layout breakpoints', () => {
    expect(sideTier(true, false, false)).toBe('wide');
    expect(sideTier(false, false, false)).toBe('mid');
    expect(sideTier(false, true, false)).toBe('narrow');
    expect(sideTier(false, true, true)).toBe('phone');
    expect(dockTier(false, false)).toBe('tall');
    expect(dockTier(true, false)).toBe('short');
    expect(dockTier(false, true)).toBe('short');
  });

  it('folds both panels as one column below three columns, and never on phones', () => {
    expect(keyOf('browse', WIDE)).toBe('browse.wide');
    expect(keyOf('station', WIDE)).toBe('station.wide');
    expect(keyOf('browse', MID)).toBe('side.mid');
    expect(keyOf('station', MID)).toBe('side.mid');
    expect(keyOf('station', PHONE)).toBeNull();
    expect(keyOf('rx', PHONE)).toBe('rx.short');
  });
});

describe('folds', () => {
  it('starts small layouts folded and the rest open', () => {
    expect(foldedAttr({}, WIDE)).toBe('');
    expect(foldedAttr({}, MID)).toBe('');
    expect(foldedAttr({}, NARROW)).toBe('side rx audio');
    expect(foldedAttr({}, PHONE)).toBe('rx audio');
  });

  it('keeps a choice per layout size', () => {
    let f = withFold({}, 'browse', WIDE, true);
    expect(f).toEqual({ 'browse.wide': true });
    expect(foldedAttr(f, WIDE)).toBe('browse');
    expect(isFolded(f, 'browse', MID)).toBe(false);
    f = withFold(f, 'station', NARROW, false);
    expect(foldedAttr(f, NARROW)).toBe('rx audio');
    f = withFold(f, 'rx', NARROW, false);
    expect(foldedAttr(f, NARROW)).toBe('audio');
    expect(foldedAttr(f, PHONE)).toBe('audio');
  });

  it('changes nothing it does not have to', () => {
    const f = {};
    expect(withFold(f, 'browse', WIDE, false)).toBe(f);
    expect(withFold(f, 'browse', PHONE, true)).toBe(f);
  });

  it('reads back only the choices that hold', () => {
    expect(readFolds({ 'browse.wide': true, 'side.narrow': false, 'rx.short': false })).toEqual({ 'browse.wide': true, 'side.narrow': false, 'rx.short': false });
    expect(readFolds({ 'browse.mid': true, 'side.wide': true, 'rx.tall': 'yes', moon: true })).toEqual({});
    expect(readFolds(null)).toBeNull();
    expect(readFolds([true])).toBeNull();
    expect(readFolds('x')).toBeNull();
  });
});
