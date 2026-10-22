import { describe, expect, it } from 'vitest';
import { CUSHION_STEPS_MS, grown, larger } from './cushion';

const base = { cushionMs: 120, maxAheadMs: 500 };

describe('the listener cushion', () => {
  it('goes up a step after each dropout, keeping the room above it', () => {
    const a = grown(base, base);
    expect(a).toEqual({ cushionMs: 250, maxAheadMs: 630 });
    const b = grown(base, a);
    expect(b).toEqual({ cushionMs: 400, maxAheadMs: 780 });
  });

  it('stops at the last step', () => {
    let b = base;
    for (let i = 0; i < 10; i++) b = grown(base, b);
    expect(b.cushionMs).toBe(CUSHION_STEPS_MS[CUSHION_STEPS_MS.length - 1]);
    expect(grown(base, b)).toEqual(b);
  });

  it('a configured cushion above the steps stays where it is', () => {
    const big = { cushionMs: 1500, maxAheadMs: 2000 };
    expect(grown(big, big)).toEqual(big);
  });

  it('starts from a cushion between the steps', () => {
    expect(grown(base, { cushionMs: 300, maxAheadMs: 700 }).cushionMs).toBe(400);
  });

  it('a new setting never takes away what it grew to', () => {
    expect(larger({ cushionMs: 400, maxAheadMs: 780 }, { cushionMs: 200, maxAheadMs: 900 })).toEqual({ cushionMs: 400, maxAheadMs: 900 });
  });
});
