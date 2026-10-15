import { describe, expect, it } from 'vitest';
import { detectKind, retryDelay, wsUrl } from './transport';

describe('transport selection', () => {
  it('uses Tauri inside the desktop shell and the WebSocket elsewhere', () => {
    expect(detectKind({ __TAURI_INTERNALS__: {} })).toBe('tauri');
    expect(detectKind({})).toBe('ws');
  });

  it('builds the WebSocket URL next to the page', () => {
    expect(wsUrl({ href: 'http://10.0.0.5:8080/', protocol: 'http:' })).toBe('ws://10.0.0.5:8080/api/ws');
    expect(wsUrl({ href: 'https://box.ts.net/atlas/index.html', protocol: 'https:' })).toBe('wss://box.ts.net/atlas/api/ws');
  });
});

describe('reconnect delay', () => {
  it('grows from 1.5 s to 15 s at most', () => {
    expect(retryDelay(0, 0.5)).toBeCloseTo(1500);
    expect(retryDelay(1, 0.5)).toBeCloseTo(2250);
    expect(retryDelay(20, 0.5)).toBeCloseTo(15_000);
  });

  it('spreads a fifth either way', () => {
    expect(retryDelay(0, 0)).toBeCloseTo(1200);
    expect(retryDelay(0, 1)).toBeCloseTo(1800);
  });
});
