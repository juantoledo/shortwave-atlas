import { describe, expect, it } from 'vitest';
import { columns, cwDelta } from './waterfall';

describe('waterfall', () => {
  it('keeps a narrow carrier visible when many bins share a column', () => {
    const spec = new Float32Array(2048).fill(-100);
    spec[300] = -20; // one bin
    // 16 kHz context, 4096 FFT -> 3.9 Hz per bin; 4 kHz span over 100 columns
    const row = columns(spec, 4096, 16000, 4000, 100);
    expect(Math.max(...row)).toBe(-20);
    expect(row.filter((v) => v === -20)).toHaveLength(1);
  });

  it('treats silence as -200 dB', () => {
    const row = columns(new Float32Array(2048).fill(-Infinity), 4096, 16000, 3000, 10);
    expect([...row]).toEqual(new Array(10).fill(-200));
  });

  it('CW click-to-tune moves the VFO by the tone offset (inverted in CWR)', () => {
    expect(cwDelta(800, 600, 'CW')).toBe(200);
    expect(cwDelta(800, 600, 'CWR')).toBe(-200);
    expect(cwDelta(604, 600, 'CW')).toBe(0);
  });
});
