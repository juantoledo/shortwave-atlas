import { describe, expect, it } from 'vitest';
import type { Release } from '../types/generated/Release';
import type { UpdateStatus } from '../types/generated/UpdateStatus';
import { updateView } from './useUpdate';

const release: Release = { version: '0.2.0', notes: '* banner', pub_date: null, url: 'https://github.com/o/r/releases/tag/v0.2.0' };
const status = (patch: Partial<UpdateStatus>): UpdateStatus => ({
  current: '0.1.0',
  check: true,
  channel: 'stable',
  install: { kind: 'auto' },
  state: { state: 'available', release },
  checked_at: 0,
  locked: [],
  ...patch,
});

describe('updateView', () => {
  it('shows nothing without an offer', () => {
    expect(updateView(null, 'tauri', null)).toBeNull();
    for (const state of ['idle', 'checking', 'up_to_date', 'disabled'] as const) {
      expect(updateView(status({ state: { state } }), 'tauri', null)).toBeNull();
    }
    expect(updateView(status({ state: { state: 'failed', message: 'offline', release: null } }), 'tauri', null)).toBeNull();
  });

  it('picks the action from the install mode and the transport', () => {
    expect(updateView(status({}), 'tauri', null)?.action).toEqual({ kind: 'install' });
    expect(updateView(status({}), 'ws', null)?.action).toEqual({ kind: 'host' });
    const deb = status({ install: { kind: 'manual', reason: 'linux_package' } });
    expect(updateView(deb, 'tauri', null)?.action).toEqual({ kind: 'download', reason: 'linux_package' });
    const server = status({ install: { kind: 'manual', reason: 'server' } });
    expect(updateView(server, 'ws', null)?.action).toEqual({ kind: 'download', reason: 'server' });
  });

  it('hides a skipped version only while it is offered', () => {
    expect(updateView(status({}), 'tauri', '0.2.0')).toBeNull();
    expect(updateView(status({}), 'tauri', '0.1.5')?.phase).toBe('offer');
    const dl = status({ state: { state: 'downloading', release, done: 0, total: null } });
    expect(updateView(dl, 'tauri', '0.2.0')?.phase).toBe('downloading');
  });

  it('reports progress and failures', () => {
    const dl = (done: number, total: number | null) =>
      updateView(status({ state: { state: 'downloading', release, done, total } }), 'tauri', null);
    expect(dl(512, 2048)?.percent).toBe(25);
    expect(dl(512, null)?.percent).toBeNull();
    expect(dl(5000, 2048)?.percent).toBe(100);
    const failed = updateView(status({ state: { state: 'failed', message: 'bad signature', release } }), 'tauri', null);
    expect([failed?.phase, failed?.error, failed?.version]).toEqual(['failed', 'bad signature', '0.2.0']);
    expect(updateView(status({ state: { state: 'installing', release } }), 'tauri', null)?.phase).toBe('installing');
  });
});
