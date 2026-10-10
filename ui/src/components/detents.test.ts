import { describe, expect, it } from 'vitest';
import { angleDelta, detents, pointerDeg, stepHz, wheelDetents, WHEEL_PX } from './detents';

describe('knob detents', () => {
  it('measures the pointer clockwise from 12 o\'clock', () => {
    expect(pointerDeg(0, 0, 0, -10)).toBe(0);
    expect(pointerDeg(0, 0, 10, 0)).toBe(90);
    expect(pointerDeg(0, 0, 0, 10)).toBe(180);
    expect(pointerDeg(0, 0, -10, 0)).toBe(270);
  });

  it('takes the short way round across 12 o\'clock', () => {
    expect(angleDelta(355, 5)).toBe(10);
    expect(angleDelta(5, 355)).toBe(-10);
    expect(angleDelta(90, 100)).toBe(10);
    expect(angleDelta(0, 180)).toBe(180);
  });

  it('adds slow turns up to a detent', () => {
    let acc = 0, clicks = 0;
    for (const d of [3, 3, 5]) {
      const r = detents(acc, d);
      acc = r.rest;
      clicks += r.clicks;
    }
    expect(clicks).toBe(1);
    expect(acc).toBeCloseTo(1);
  });

  it('counts every detent of a fast turn, either way', () => {
    expect(detents(0, 25)).toEqual({ clicks: 2, rest: 5 });
    expect(detents(0, -25)).toEqual({ clicks: -2, rest: -5 });
    expect(detents(5, -3)).toEqual({ clicks: 0, rest: 2 });
  });

  it('steps along the grid, lands on it from odd hertz', () => {
    expect(stepHz(13_616_000, 1, 1000)).toBe(13_617_000);
    expect(stepHz(13_616_000, -2, 1000)).toBe(13_614_000);
    expect(stepHz(13_616_370, 1, 1000)).toBe(13_617_000);
    expect(stepHz(13_616_370, -1, 1000)).toBe(13_616_000);
    expect(stepHz(13_616_370, 3, 1000)).toBe(13_619_000);
    expect(stepHz(13_616_370, 1, 5000)).toBe(13_620_000);
    expect(stepHz(13_616_370, -1, 5000)).toBe(13_615_000);
    expect(stepHz(13_616_370, 0, 1000)).toBe(13_616_370);
  });

  it('turns one detent per wheel notch, up is up', () => {
    expect(wheelDetents(0, -100, 0)).toEqual({ clicks: 1, rest: 0 });
    expect(wheelDetents(0, 100, 0)).toEqual({ clicks: -1, rest: 0 });
    expect(wheelDetents(0, 3, 1)).toEqual({ clicks: -1, rest: 0 });
    expect(wheelDetents(7, 0, 0)).toEqual({ clicks: 0, rest: 7 });
  });

  it('adds up trackpad pixels', () => {
    let acc = 0, clicks = 0;
    for (let k = 0; k < 10; k++) {
      const r = wheelDetents(acc, -WHEEL_PX / 4, 0);
      acc = r.rest;
      clicks += r.clicks;
    }
    expect(clicks).toBe(2);
    expect(acc).toBe(WHEEL_PX / 2);
  });
});
