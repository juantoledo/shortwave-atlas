import { describe, expect, it } from 'vitest';
import { changed, choiceOf, draftOf } from './RemoteSection';
import type { RemoteStatus } from '../../types/generated/RemoteStatus';

const status = (patch: Partial<RemoteStatus> = {}): RemoteStatus => ({
  headless: false,
  running: false,
  choice: { enabled: false, bind: '127.0.0.1', port: 8080 },
  scope: 'loopback',
  urls: [],
  clients: 0,
  failure: null,
  error: null,
  locked: [],
  config_path: '',
  ...patch,
});

describe('remote access settings', () => {
  it('turns the scope into the bind address and keeps a custom one', () => {
    const d = draftOf(status());
    expect(d).toEqual({ scope: 'loopback', bind: '127.0.0.1', port: 8080 });
    expect(choiceOf({ ...d, scope: 'network' }, true)).toEqual({ enabled: true, bind: '0.0.0.0', port: 8080 });
    const custom = draftOf(status({ scope: 'custom', choice: { enabled: true, bind: '100.64.1.2', port: 9000 } }));
    expect(choiceOf(custom, false)).toEqual({ enabled: false, bind: '100.64.1.2', port: 9000 });
  });

  it('offers a restart only when the served choice would change', () => {
    const served = status({ running: true, scope: 'network', choice: { enabled: true, bind: '0.0.0.0', port: 8080 } });
    const d = draftOf(served);
    expect(changed(d, served.choice)).toBe(false);
    expect(changed({ ...d, port: 8081 }, served.choice)).toBe(true);
    expect(changed({ ...d, scope: 'loopback' }, served.choice)).toBe(true);
  });
});
