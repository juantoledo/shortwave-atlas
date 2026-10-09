import { describe, expect, it } from 'vitest';
import { flagUrl } from './flags';

describe('flags', () => {
  it('finds flag-icons keys', () => {
    expect(flagUrl('gb')).toMatch(/gb\.svg/);
    expect(flagUrl('sh-ac')).toMatch(/sh-ac\.svg/);
    expect(flagUrl('un')).toMatch(/un\.svg/);
    expect(flagUrl('xx')).toMatch(/xx\.svg/);
  });

  it('has nothing for unknown or missing keys', () => {
    expect(flagUrl('zz-nope')).toBeNull();
    expect(flagUrl(null)).toBeNull();
    expect(flagUrl('')).toBeNull();
  });
});
