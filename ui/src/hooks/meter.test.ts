import { describe, expect, it } from 'vitest';
import { sameRig } from './useCore';
import { Meter } from './meter';
import type { RigState } from '../types/generated/RigState';

describe('meter', () => {
  it('tells its subscribers of a new reading only', () => {
    const m = new Meter();
    let calls = 0;
    const off = m.subscribe(() => { calls++; });
    m.set(-12);
    m.set(-12);
    m.set(null);
    expect([m.get(), calls]).toEqual([null, 2]);
    off();
    m.set(3);
    expect(calls).toBe(2);
  });

  it('leaves the S-meter out of the rig state the app re-renders for', () => {
    const a: RigState = { link: 'ok', power: 'on', freq_hz: 9_500_000, mode: 'AM', passband_hz: 6000, strength_db: -20, cw_pitch_hz: null };
    expect(sameRig(a, { ...a, strength_db: 5 })).toBe(true);
    expect(sameRig(a, { ...a, freq_hz: 9_505_000 })).toBe(false);
    expect(sameRig(a, { ...a, power: 'off' })).toBe(false);
  });
});
