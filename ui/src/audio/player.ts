// Low-latency rig audio: raw PCM from the transport (HTTP in the browser, a Tauri channel
// on the desktop) played through Web Audio (port of the original prototype's player).
//
// Blocks are scheduled back to back on the AudioContext clock. The queue ahead of the
// playhead is kept between the cushion (rebuilt after an underrun) and the maximum (blocks
// beyond that are dropped), so latency cannot grow over time. Both come from Settings →
// Audio → Advanced; each underrun while playing raises them a step (cushion.ts).
// Graph: sources -> bus -> gain -> speakers (or, `background`, a media element)
//                       \-> analyser (waterfall, independent of the volume)

import type { OpenAudio } from '../api/transport';
import { grown, larger } from './cushion';

const RETRY_MS = 1000;
export const FFT_SIZE = 4096;

/** The listener's buffer, in milliseconds (`AudioTuning.cushion_ms`, `max_ahead_ms`). */
export interface PlayerBuffer {
  cushionMs: number;
  maxAheadMs: number;
}

/** How the phone remote plays: `latencyHint` for the AudioContext ('playback' buffers more), and
 *  `background` through a media element, which phones keep playing with the screen off. */
export interface PlayerOptions {
  latencyHint?: AudioContextLatencyCategory;
  background?: boolean;
}

export type PlayerStatus =
  | { kind: 'connecting' }
  | { kind: 'playing'; bufferMs: number }
  | { kind: 'ended' }
  | { kind: 'error'; message: string };

export class AudioPlayer {
  readonly ctx: AudioContext;
  readonly analyser: AnalyserNode;
  private bus: GainNode;
  private gain: GainNode;
  private close: (() => void) | null = null;
  private cancel: (() => void) | null = null;
  private running = true;
  private t = 0;
  private block: Float32Array;
  private n = 0;
  private odd: number | null = null;
  private shown = 0;
  /** The buffer in use: the configured one (`buffer`), raised after underruns. */
  private live: PlayerBuffer;
  private element: HTMLAudioElement | null = null;
  private wake = () => {
    if (document.visibilityState !== 'visible') return;
    if (this.ctx.state !== 'running') void this.ctx.resume();
    if (this.element?.paused) void this.element.play().catch(() => {});
  };

  private rate = 0;

  /** Make it from the gesture that asks for sound (a tap on Listen): phones only allow it then. */
  constructor(private open: OpenAudio, volume: number, private buffer: PlayerBuffer, private onStatus: (s: PlayerStatus) => void, opts: PlayerOptions = {}) {
    this.ctx = new AudioContext(opts.latencyHint ? { latencyHint: opts.latencyHint } : undefined);
    void this.ctx.resume();
    this.live = buffer;
    this.bus = this.ctx.createGain();
    this.gain = this.ctx.createGain();
    this.analyser = this.ctx.createAnalyser();
    this.gain.gain.value = volume;
    if (opts.background && typeof this.ctx.createMediaStreamDestination === 'function') {
      const out = this.ctx.createMediaStreamDestination();
      this.gain.connect(out);
      this.element = new Audio();
      this.element.srcObject = out.stream;
      void this.element.play().catch(() => {});
    } else {
      this.gain.connect(this.ctx.destination);
    }
    // back in front: a phone may have suspended it
    document.addEventListener('visibilitychange', this.wake);
    this.analyser.fftSize = FFT_SIZE;
    this.analyser.smoothingTimeConstant = 0.2;
    this.bus.connect(this.gain);
    this.bus.connect(this.analyser);
    this.block = new Float32Array(0);
    void this.loop();
  }

  setVolume(v: number) {
    this.gain.gain.value = v;
  }

  /** A new setting: the buffer grown so far stays if it is larger. */
  setBuffer(b: PlayerBuffer) {
    this.buffer = b;
    this.live = larger(this.live, b);
  }

  stop() {
    this.running = false;
    this.close?.();
    this.cancel?.();
    document.removeEventListener('visibilitychange', this.wake);
    if (this.element) {
      this.element.pause();
      this.element.srcObject = null;
    }
    void this.ctx.close();
  }

  private async loop() {
    while (this.running) {
      this.onStatus({ kind: 'connecting' });
      const err = await new Promise<string | undefined>((resolve) => {
        this.close = this.open({
          // each stream says its rate: it can change while we listen (Settings → Audio)
          onStart: (rate) => {
            this.rate = rate;
            this.block = new Float32Array(Math.round(rate * 0.04));
            this.n = 0;
          },
          onData: (b) => { if (this.running && this.rate) this.feed(b); },
          onEnd: resolve,
        });
        this.cancel = () => resolve(undefined);
      });
      this.close = this.cancel = null;
      if (!this.running) return;
      this.onStatus(err === undefined ? { kind: 'ended' } : { kind: 'error', message: err });
      this.t = 0; this.n = 0; this.odd = null;
      await new Promise((res) => setTimeout(res, RETRY_MS));
    }
  }

  /** s16le bytes; a chunk may split a sample, so carry the odd byte over. */
  private feed(bytes: Uint8Array) {
    let b = bytes;
    if (this.odd !== null) {
      const m = new Uint8Array(b.length + 1);
      m[0] = this.odd; m.set(b, 1); b = m; this.odd = null;
    }
    if (b.length & 1) { this.odd = b[b.length - 1]; b = b.subarray(0, b.length - 1); }
    const dv = new DataView(b.buffer, b.byteOffset, b.length);
    for (let i = 0; i < b.length; i += 2) {
      this.block[this.n++] = dv.getInt16(i, true) / 32768;
      if (this.n === this.block.length) { this.play(); this.n = 0; }
    }
  }

  private play() {
    const now = this.ctx.currentTime;
    if (this.t < now) {
      // underrun (or first block): rebuild the cushion, a step bigger if it ran dry while playing
      if (this.t > 0) this.live = grown(this.buffer, this.live);
      this.t = now + this.live.cushionMs / 1000;
    } else if (this.t - now > this.live.maxAheadMs / 1000) return; // too far behind live: drop this block
    const ab = this.ctx.createBuffer(1, this.block.length, this.rate);
    ab.getChannelData(0).set(this.block);
    const src = this.ctx.createBufferSource();
    src.buffer = ab; src.connect(this.bus); src.start(this.t);
    this.t += this.block.length / this.rate;
    if (now - this.shown > 0.5) {
      this.shown = now;
      this.onStatus({ kind: 'playing', bufferMs: Math.round((this.t - now) * 1000) });
    }
  }
}
