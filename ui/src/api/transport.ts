// How the UI reaches the core. Both transports carry the same `Call`s (see
// `crates/atlas-core/src/api.rs`), so the desktop and remote clients behave the same.

import type { ApiError } from '../types/generated/ApiError';
import type { Call } from '../types/generated/Call';
import type { ClientMsg } from '../types/generated/ClientMsg';
import type { RigState } from '../types/generated/RigState';
import type { ServerMsg } from '../types/generated/ServerMsg';

export type TransportKind = 'tauri' | 'ws';
export type LinkStatus = 'connecting' | 'open' | 'lost';

export interface Transport {
  readonly kind: TransportKind;
  call(call: Call): Promise<unknown>;
  /** Rig state pushes; returns an unsubscribe function. */
  onState(cb: (s: RigState) => void): () => void;
  /** Connection to the core (always `open` in the desktop app). */
  onStatus(cb: (s: LinkStatus) => void): () => void;
  /** Rig audio: `onStart` with the sample rate, then raw s16le mono PCM chunks until the
   *  stream ends (`onEnd`, with the reason if it failed). Returns a function that closes it. */
  openAudio: OpenAudio;
  close(): void;
}

export interface AudioSink {
  onStart(rate: number): void;
  onData(b: Uint8Array): void;
  onEnd(err?: string): void;
}
export type OpenAudio = (sink: AudioSink) => () => void;

/** An error from the core, with its kind (invalid, conflict, rig, internal). */
export class ApiFailure extends Error {
  constructor(readonly kind: ApiError['kind'] | 'connection', message: string) {
    super(message);
  }
}

const asFailure = (e: unknown) =>
  e && typeof e === 'object' && 'kind' in e && 'message' in e
    ? new ApiFailure((e as ApiError).kind, (e as ApiError).message)
    : new ApiFailure('internal', String(e));

export function detectKind(w: object): TransportKind {
  return '__TAURI_INTERNALS__' in w ? 'tauri' : 'ws';
}

export async function connect(): Promise<Transport> {
  return detectKind(window) === 'tauri'
    ? tauriTransport()
    : new WsTransport(wsUrl(window.location), new URL('api/audio', window.location.href).toString());
}

/** `/api/ws` next to the page, so the UI also works behind a reverse proxy subpath. */
export function wsUrl(loc: { href: string; protocol: string }): string {
  const u = new URL('api/ws', loc.href);
  u.protocol = loc.protocol === 'https:' ? 'wss:' : 'ws:';
  return u.toString();
}

/* ---------------- Desktop: Tauri invoke + events ---------------- */

async function tauriTransport(): Promise<Transport> {
  const { Channel, invoke } = await import('@tauri-apps/api/core');
  const { listen } = await import('@tauri-apps/api/event');
  return {
    kind: 'tauri',
    call: (call) => invoke('call', { call }).catch((e) => Promise.reject(asFailure(e))),
    onState(cb) {
      const un = listen<RigState>('rig://state', (e) => cb(e.payload));
      return () => void un.then((f) => f());
    },
    onStatus(cb) {
      cb('open');
      return () => {};
    },
    // the counterpart of GET /api/audio: a binary channel, an empty message marks the end
    openAudio(sink) {
      let id: number | null = null, done = false;
      const finish = (err?: string) => {
        if (done) return;
        done = true;
        if (id !== null) void invoke('audio_close', { id });
        sink.onEnd(err);
      };
      const ch = new Channel<ArrayBuffer>((buf) => {
        if (done) return;
        if (buf.byteLength === 0) finish();
        else sink.onData(new Uint8Array(buf));
      });
      invoke<{ id: number; rate: number }>('audio_open', { onBlock: ch }).then(
        (s) => {
          id = s.id;
          if (done) void invoke('audio_close', { id });
          else sink.onStart(s.rate);
        },
        (e) => finish(asFailure(e).message),
      );
      return () => {
        if (done) return;
        done = true;
        if (id !== null) void invoke('audio_close', { id });
      };
    },
    close() {},
  };
}

/* ---------------- Remote browser: WebSocket ---------------- */

const RETRY_MS = 1500;

class WsTransport implements Transport {
  readonly kind = 'ws' as const;
  private ws: WebSocket | null = null;
  private nextId = 1;
  private pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();
  private stateSubs = new Set<(s: RigState) => void>();
  private statusSubs = new Set<(s: LinkStatus) => void>();
  private status: LinkStatus = 'connecting';
  private opened: Promise<void>;
  private markOpen!: () => void;

  constructor(private url: string, private audioUrl: string) {
    this.opened = new Promise((r) => (this.markOpen = r));
    this.open();
  }

  private setStatus(s: LinkStatus) {
    this.status = s;
    this.statusSubs.forEach((cb) => cb(s));
  }

  close() {
    const ws = this.ws;
    this.ws = null; // stops the reconnect in onclose
    ws?.close();
  }

  private open() {
    const ws = new WebSocket(this.url);
    this.ws = ws;
    ws.onopen = () => {
      this.setStatus('open');
      this.markOpen();
    };
    ws.onmessage = (ev) => {
      const msg = JSON.parse(ev.data as string) as ServerMsg;
      if (msg.type === 'state') {
        this.stateSubs.forEach((cb) => cb(msg.state));
        return;
      }
      const p = this.pending.get(msg.id);
      if (!p) return;
      this.pending.delete(msg.id);
      if (msg.err) p.reject(asFailure(msg.err));
      else p.resolve(msg.ok);
    };
    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.pending.forEach((p) => p.reject(new ApiFailure('connection', 'connection lost')));
      this.pending.clear();
      this.opened = new Promise((r) => (this.markOpen = r));
      this.setStatus('lost');
      setTimeout(() => this.open(), RETRY_MS);
    };
  }

  async call(call: Call): Promise<unknown> {
    await this.opened;
    const id = this.nextId++;
    const msg: ClientMsg = { id, call };
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws!.send(JSON.stringify(msg));
    });
  }

  openAudio(sink: AudioSink) {
    const ctl = new AbortController();
    void (async () => {
      try {
        const r = await fetch(this.audioUrl, { signal: ctl.signal, cache: 'no-store' });
        if (!r.ok) throw new Error((await r.text()) || r.statusText);
        sink.onStart(Number(r.headers.get('x-audio-rate')));
        const rd = r.body!.getReader();
        for (;;) {
          const { value, done } = await rd.read();
          if (done) break;
          sink.onData(value);
        }
        sink.onEnd();
      } catch (e) {
        if (!ctl.signal.aborted) sink.onEnd(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => ctl.abort();
  }

  onState(cb: (s: RigState) => void) {
    this.stateSubs.add(cb);
    return () => void this.stateSubs.delete(cb);
  }

  onStatus(cb: (s: LinkStatus) => void) {
    this.statusSubs.add(cb);
    cb(this.status);
    return () => void this.statusSubs.delete(cb);
  }
}
