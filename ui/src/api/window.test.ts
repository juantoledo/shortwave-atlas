import { describe, expect, it } from 'vitest';
import { chromeFor } from './window';

describe('chromeFor', () => {
  it('draws its own buttons on Windows and Linux', () => {
    expect(chromeFor('tauri', 'windows')).toBe('own');
    expect(chromeFor('tauri', 'linux')).toBe('own');
  });
  it('leaves the traffic lights to macOS', () => {
    expect(chromeFor('tauri', 'macos')).toBe('mac');
  });
  it('has no window chrome in a browser, or before the OS is known', () => {
    expect(chromeFor('ws', 'windows')).toBeNull();
    expect(chromeFor('ws', 'macos')).toBeNull();
    expect(chromeFor('tauri', undefined)).toBeNull();
  });
});
