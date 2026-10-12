import { describe, expect, it } from 'vitest';
import { shouldShow } from './useWelcome';

describe('welcome', () => {
  it('shows unless this device asked not to', () => {
    expect(shouldShow(null)).toBe(true);
    expect(shouldShow('0')).toBe(true);
    expect(shouldShow('1')).toBe(false);
  });
});
