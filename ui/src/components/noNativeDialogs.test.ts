import { describe, expect, it } from 'vitest';

// The desktop app's Linux (WebKitGTK) and macOS (WKWebView) webviews never show `confirm`,
// `alert` or `prompt`: they return at once, so a `confirm` reads as "no". Ask in the app instead
// (components/Confirm.tsx).
const SOURCES = import.meta.glob(['../**/*.{ts,tsx}', '!../**/*.test.ts'], { eager: true, query: '?raw', import: 'default' }) as Record<string, string>;
const NATIVE = /\b(?:window\.)?(?:confirm|alert|prompt)\s*\(/;

describe('native dialogs', () => {
  it('the UI never calls confirm, alert or prompt', () => {
    expect(Object.keys(SOURCES).length).toBeGreaterThan(10);
    const used = Object.entries(SOURCES).filter(([, src]) => NATIVE.test(src)).map(([path]) => path);
    expect(used).toEqual([]);
  });
});
