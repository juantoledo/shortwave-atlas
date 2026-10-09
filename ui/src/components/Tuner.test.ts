import { describe, expect, it } from 'vitest';
import { fmtReadout, parseReadout } from './Tuner';

describe('readout', () => {
  it('shows Hz as 000.000.000', () => {
    expect(fmtReadout(9_500_000)).toBe('009.500.000');
    expect(fmtReadout(15_770_000)).toBe('015.770.000');
    expect(fmtReadout(1_800_000)).toBe('001.800.000');
    expect(fmtReadout(531_000)).toBe('000.531.000');
  });

  it('reads its own form as Hz and plain numbers as kHz', () => {
    expect(parseReadout('009.500.000')).toBe(9_500_000);
    expect(parseReadout('9.500.000')).toBe(9_500_000);
    expect(parseReadout('9500')).toBe(9_500_000);
    expect(parseReadout('9500,5')).toBe(9_500_500);
    expect(parseReadout('')).toBeNaN();
    expect(parseReadout('abc')).toBeNaN();
  });
});
