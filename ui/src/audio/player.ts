// Low-latency rig audio: raw PCM from the transport (HTTP in the browser, a Tauri channel
// on the desktop) played through Web Audio (port of the player in reference/ftdx10_web.py).
//
// Blocks are scheduled back to back on the AudioContext clock. The queue ahead of the
// playhead is kept between CUSHION (rebuilt after an underrun) and MAX_AHEAD (blocks
// beyond that are dropped), so latency cannot grow over time.
// Graph: sources -> bus -> gain -> speakers
//                       \-> analyser (waterfall, independent of the volume)

import type { OpenAudio } from '../api/transport';

const CUSHION_S = 0.12, MAX_AHEAD_S = 0.5, RETRY_MS = 1000;
export const FFT_SIZE = 4096;

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

  private rate = 0;

  constructor(private open: OpenAudio, volume: number, private onStatus: (s: PlayerStatus) => void) {
    this.ctx = new AudioContext();
    void this.ctx.resume();
    this.bus = this.ctx.createGain();
    this.gain = this.ctx.createGain();
    this.analyser = this.ctx.createAnalyser();
    this.gain.gain.value = volume;
    this.gain.connect(this.ctx.destination);
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

  stop() {
    this.running = false;
    this.close?.();
    this.cancel?.();
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
    if (this.t < now) this.t = now + CUSHION_S; // underrun (or first block): rebuild a small cushion
    else if (this.t - now > MAX_AHEAD_S) return; // too far behind live: drop this block
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
