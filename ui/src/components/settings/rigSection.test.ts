import { describe, expect, it } from 'vitest';
import { hamlibVersion, modelLabel, parseModel } from './RigSection';

describe('rig model field', () => {
  it('reads the id from a picked label or a typed number', () => {
    const ftdx10 = { id: 1042, mfg: 'Yaesu', model: 'FTDX-10', status: 'Stable' };
    expect(modelLabel(ftdx10)).toBe('1042 · Yaesu FTDX-10');
    expect(parseModel(modelLabel(ftdx10))).toBe(1042);
    expect(parseModel(' 3073')).toBe(3073);
    expect(parseModel('Yaesu')).toBeNull();
  });

  it('shortens the Hamlib version line', () => {
    expect(hamlibVersion('rigctl Hamlib 4.5.5 Apr 05 11:43:08Z 2023 SHA=6eecd3')).toBe('4.5.5');
    expect(hamlibVersion('rigctld Hamlib 4.6~git')).toBe('4.6~git');
  });
});
