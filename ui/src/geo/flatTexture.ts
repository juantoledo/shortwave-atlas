// The flat map's picture: the globe's texture (texture.ts) with day and night painted on, as the
// globe's shader shades it (shade.ts). Painted again as the sun moves (a few ms, every
// `components/FlatMap.tsx` SUN_MS); each frame only draws the result.
//
// The night is worked out on a small grid and smoothed up (it changes slowly across the map):
// one layer of shade and grey-line tint over the base, then the city lights, masked where it is
// lit, added ('lighter').

import { RAD, type Vec3 } from './geo';
import { nightPixel } from './shade';
import type { GlobeTexture } from './texture';

/** The night grid's width (half as tall), and the masked lights' width. */
const NIGHT_W = 720, LIGHTS_W = 2048;

function canvas(w: number, h: number) {
  const c = document.createElement('canvas');
  c.width = w;
  c.height = h;
  return c;
}

export class FlatTexture {
  /** The map as it looks now: equirectangular, -180..180 east, 90..-90 down. */
  readonly canvas: HTMLCanvasElement;
  private readonly shade = canvas(NIGHT_W, NIGHT_W / 2);
  private readonly mask = canvas(NIGHT_W, NIGHT_W / 2);
  private readonly lit: HTMLCanvasElement;
  /** Each grid cell's position (`toVec`'s frame), worked out once. */
  private readonly cells: Float64Array;
  /** The night grid's pixels, painted again each update. */
  private readonly shadeData: ImageData;
  private readonly maskData: ImageData;

  constructor(private readonly tex: GlobeTexture) {
    this.canvas = canvas(tex.base.width, tex.base.height);
    const lw = Math.min(LIGHTS_W, tex.lights.width);
    this.lit = canvas(lw, lw / 2);
    const w = NIGHT_W, h = NIGHT_W / 2;
    this.cells = new Float64Array(w * h * 3);
    this.shadeData = new ImageData(w, h);
    this.maskData = new ImageData(w, h);
    for (let j = 0; j < h; j++) {
      const lat = (90 - ((j + 0.5) * 180) / h) * RAD;
      for (let i = 0; i < w; i++) {
        const lon = (-180 + ((i + 0.5) * 360) / w) * RAD;
        this.cells.set([Math.cos(lat) * Math.cos(lon), Math.cos(lat) * Math.sin(lon), Math.sin(lat)], (j * w + i) * 3);
      }
    }
  }

  /** Paint the map for the sun along `sun` (a unit vector, `sunVec`). */
  update(sun: Vec3): void {
    const w = NIGHT_W, h = NIGHT_W / 2, n = w * h;
    const sg = this.shade.getContext('2d')!, mg = this.mask.getContext('2d')!;
    const shade = this.shadeData, mask = this.maskData, c = this.cells;
    for (let k = 0; k < n; k++) nightPixel(c[k * 3], c[k * 3 + 1], c[k * 3 + 2], sun, shade.data, mask.data, k * 4);
    sg.putImageData(shade, 0, 0);
    mg.putImageData(mask, 0, 0);

    const lg = this.lit.getContext('2d')!, lw = this.lit.width, lh = this.lit.height;
    lg.globalCompositeOperation = 'copy';
    lg.drawImage(this.tex.lights, 0, 0, lw, lh);
    lg.globalCompositeOperation = 'destination-in';
    lg.imageSmoothingQuality = 'high';
    lg.drawImage(this.mask, 0, 0, lw, lh);

    const g = this.canvas.getContext('2d')!, cw = this.canvas.width, ch = this.canvas.height;
    g.globalCompositeOperation = 'copy';
    g.drawImage(this.tex.base, 0, 0);
    g.globalCompositeOperation = 'source-over';
    g.imageSmoothingQuality = 'high';
    g.drawImage(this.shade, 0, 0, cw, ch);
    g.globalCompositeOperation = 'lighter';
    g.drawImage(this.lit, 0, 0, cw, ch);
    g.globalCompositeOperation = 'source-over';
  }
}
