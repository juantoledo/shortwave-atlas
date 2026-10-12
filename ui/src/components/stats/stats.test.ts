import { describe, expect, it } from 'vitest';
import { heatLevel, labelEvery, newestEarned, shares, sparkPath, split, trend, yearly } from './stats';

describe('stats math', () => {
  it('scales bars to the largest', () => {
    expect(shares([1, 2, 4])).toEqual([0.25, 0.5, 1]);
    expect(shares([0, 0])).toEqual([0, 0]);
    expect(shares([])).toEqual([]);
  });

  it('keeps a little heat visible', () => {
    expect(heatLevel(0, 100)).toBe(0);
    expect(heatLevel(100, 100)).toBe(1);
    expect(heatLevel(25, 100)).toBe(0.5);
    expect(heatLevel(0.001, 100)).toBe(0.08);
    expect(heatLevel(5, 0)).toBe(0);
  });

  it('compares with the period before', () => {
    expect(trend(150, 100)).toEqual({ dir: 'up', pct: 50 });
    expect(trend(50, 100)).toEqual({ dir: 'down', pct: -50 });
    expect(trend(5, 0)).toEqual({ dir: 'up', pct: null });
    expect(trend(3, 3)).toEqual({ dir: 'flat', pct: 0 });
    expect(trend(1001, 1000)).toEqual({ dir: 'flat', pct: 0 });
    expect(trend(3, null)).toBeNull();
  });

  it('breaks the sparkline at empty weeks', () => {
    const p = (db: number | null) => ({ start: 0, db, sessions: db === null ? 0 : 1 });
    expect(sparkPath([p(-60), p(20)], 100, 10)).toBe('M0 10L100 0');
    expect(sparkPath([p(-20), p(null), p(-20)], 100, 10)).toBe('M0 5M100 5');
    expect(sparkPath([p(0)], 100, 10)).toBe('');
  });

  it('splits, labels and finds the newest badge', () => {
    expect(split(1, 1, 2)).toEqual([0.25, 0.25, 0.5]);
    expect(split(0, 0, 0)).toEqual([0, 0, 0]);
    expect(labelEvery(30, 10)).toBe(3);
    expect(labelEvery(7, 10)).toBe(1);
    expect(yearly([{ start: 0, seconds: 0 }, { start: 365 * 86_400, seconds: 0 }])).toBe(true);
    expect(yearly([{ start: 0, seconds: 0 }, { start: 31 * 86_400, seconds: 0 }])).toBe(false);
    expect(newestEarned([null, 5, 3])).toBe(5);
    expect(newestEarned([])).toBe(0);
  });
});
