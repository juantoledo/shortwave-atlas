// Audio waterfall (port from the original prototype).
//
// Every row the analyser spectrum (0..span Hz) is mapped to W columns (max of the FFT
// bins under each column, so narrow CW carriers aren't lost), coloured relative to a
// slowly tracked noise floor, and pushed in at the top.

export const WF_W = 600, WF_H = 180, SP_H = 70;
const RANGE_DB = 50;

function color(t: number): number[] {
  const PAL: [number, number[]][] = [[0, [8, 10, 40]], [0.25, [20, 50, 160]], [0.5, [0, 170, 200]], [0.75, [240, 220, 60]], [1, [240, 60, 40]]];
  t = Math.min(1, Math.max(0, t));
  for (let i = 1; i < PAL.length; i++) {
    if (t <= PAL[i][0]) {
      const [t0, c0] = PAL[i - 1], [t1, c1] = PAL[i], k = (t - t0) / (t1 - t0);
      return c0.map((v, j) => v + (c1[j] - v) * k);
    }
  }
  return PAL[PAL.length - 1][1];
}
const LUT = Array.from({ length: 256 }, (_, i) => color(i / 255));

/** Max of the FFT bins under each of `w` columns spanning 0..spanHz. Silence (-Infinity) becomes -200. */
export function columns(spec: Float32Array, fftSize: number, sampleRate: number, spanHz: number, w: number): Float32Array {
  const row = new Float32Array(w);
  const binsPerHz = fftSize / sampleRate;
  for (let x = 0; x < w; x++) {
    const b0 = Math.floor((x / w) * spanHz * binsPerHz);
    const b1 = Math.max(b0, Math.ceil(((x + 1) / w) * spanHz * binsPerHz) - 1);
    let m = -200;
    for (let b = b0; b <= b1; b++) if (spec[b] > m) m = spec[b];
    row[x] = m;
  }
  return row;
}

export class Waterfall {
  private floor: number | null = null;
  private spec: Float32Array<ArrayBuffer>;
  private line: ImageData;
  private wf: CanvasRenderingContext2D;
  private sp: CanvasRenderingContext2D;

  constructor(private wfCanvas: HTMLCanvasElement, spCanvas: HTMLCanvasElement, private analyser: AnalyserNode) {
    this.wf = wfCanvas.getContext('2d')!;
    this.sp = spCanvas.getContext('2d')!;
    this.spec = new Float32Array(analyser.frequencyBinCount);
    this.line = this.wf.createImageData(WF_W, 1);
  }

  tick(spanHz: number) {
    this.analyser.getFloatFrequencyData(this.spec);
    const row = columns(this.spec, this.analyser.fftSize, this.analyser.context.sampleRate, spanHz, WF_W);
    const floor = Float32Array.from(row).sort()[Math.floor(WF_W * 0.2)];
    // track the floor slowly, but jump when it rises a lot (first audio after silence)
    this.floor = this.floor === null || floor > this.floor + 20 ? floor : this.floor + (floor - this.floor) * 0.05;
    const lo = this.floor - 3, k = 255 / RANGE_DB;
    // waterfall: scroll down one pixel, new line on top
    this.wf.drawImage(this.wfCanvas, 0, 0, WF_W, WF_H - 1, 0, 1, WF_W, WF_H - 1);
    const d = this.line.data;
    for (let x = 0; x < WF_W; x++) {
      const c = LUT[Math.max(0, Math.min(255, Math.round((row[x] - lo) * k)))];
      d[x * 4] = c[0]; d[x * 4 + 1] = c[1]; d[x * 4 + 2] = c[2]; d[x * 4 + 3] = 255;
    }
    this.wf.putImageData(this.line, 0, 0);
    // spectrum line
    this.sp.clearRect(0, 0, WF_W, SP_H);
    this.sp.beginPath();
    for (let x = 0; x < WF_W; x++) {
      const y = SP_H - 3 - Math.min(1, Math.max(0, (row[x] - lo) / RANGE_DB)) * (SP_H - 6);
      if (x) this.sp.lineTo(x, y); else this.sp.moveTo(x, y);
    }
    this.sp.strokeStyle = '#ffb33f'; this.sp.lineWidth = 1.5; this.sp.stroke();
  }
}

/**
 * VFO change that moves an audio tone at `toneHz` onto the CW pitch, rounded to 10 Hz.
 * Raising the VFO lowers the tones in CW (upper sideband) and raises them in CWR.
 */
export function cwDelta(toneHz: number, pitchHz: number, mode: string): number {
  return Math.round((toneHz - pitchHz) / 10) * 10 * (mode === 'CWR' ? -1 : 1);
}
