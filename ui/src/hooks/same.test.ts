import { describe, expect, it } from 'vitest';
import { unlessSame } from './same';

describe('unlessSame', () => {
  it('keeps the state an equal answer would replace', () => {
    const cur = { state: 'idle', n: [1, 2] };
    expect(unlessSame({ state: 'idle', n: [1, 2] })(cur)).toBe(cur);
    const next = { state: 'idle', n: [1, 3] };
    expect(unlessSame(next)(cur)).toBe(next);
    expect(unlessSame<typeof cur | null>(null)(cur)).toBe(null);
  });
});
