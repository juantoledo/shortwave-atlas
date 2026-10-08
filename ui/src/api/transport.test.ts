import { describe, expect, it } from 'vitest';
import { detectKind, wsUrl } from './transport';

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
