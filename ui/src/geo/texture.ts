// The globe texture: the "receiver at night" look, in detail.
//
// Two static layers, loaded once and uploaded once:
//   base:   ocean, graticule, coastal glow, land, shaded relief, borders and coastlines, drawn
//           ahead of time from Natural Earth 50m (baseLayer.ts, ui/scripts/make_globe_base.ts);
//   lights: city lights (NASA Black Marble) tinted amber, here.
// The night shading, the lights it reveals and the grey line are drawn by the globe's shader
// from the sun's direction (night.ts), so the terminator moves continuously at no cost; the flat
// map paints them on a copy (flatTexture.ts). Both views share one load (`shared`).

import baseUrl from '../assets/globe/base.webp';
import lightsUrl from '../assets/globe/lights.jpg';

/** The textures' width for a screen whose shorter side is `screenMin` CSS px: 4096, or 2048 on
 *  a phone (either way up; PHONE's breakpoint), where the globe is a few hundred px across and the
 *  full size would hold ~85 MB of GPU memory. */
export function textureWidthFor(screenMin: number): number {
  return screenMin < 600 ? 2048 : 4096;
}

/** `textureWidthFor` this device: one width for both views, so they share one load. */
export const textureWidth = (): number =>
  typeof screen === 'undefined' ? 4096 : textureWidthFor(Math.min(screen.width, screen.height));

/** The warm tint of the city lights. */
const LIGHT_TINT = 'rgb(255,196,120)';

function canvas(w: number, h: number) {
  const c = document.createElement('canvas');
  c.width = w;
  c.height = h;
  return c;
}

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error('could not load ' + url));
    img.src = url;
  });
}

/** The drawn base layer at w×h: the image itself at its own size (no copy to hold), else
 *  scaled onto a canvas. */
function scaled(img: HTMLImageElement, w: number, h: number): HTMLImageElement | HTMLCanvasElement {
  if (img.naturalWidth === w && img.naturalHeight === h) return img;
  const c = canvas(w, h);
  const g = c.getContext('2d')!;
  g.imageSmoothingQuality = 'high';
  g.drawImage(img, 0, 0, w, h);
  return c;
}

/** City lights tinted amber, on transparent black (added with 'lighter'). */
function buildLights(w: number, h: number, lights: HTMLImageElement): HTMLCanvasElement {
  const c = canvas(w, h);
  const g = c.getContext('2d')!;
  g.drawImage(lights, 0, 0, w, h);
  g.globalCompositeOperation = 'multiply';
  g.fillStyle = LIGHT_TINT; g.fillRect(0, 0, w, h);
  return c;
}

const built = new Map<number, Promise<GlobeTexture>>();

/** The texture's static layers, as canvases for `CanvasTexture`s. */
export class GlobeTexture {
  private constructor(readonly base: HTMLImageElement | HTMLCanvasElement, readonly lights: HTMLCanvasElement) {}

  /** `create`, once per width: switching between the globe and the flat map builds nothing again. */
  static shared(width: number): Promise<GlobeTexture> {
    let p = built.get(width);
    if (!p) {
      p = GlobeTexture.create(width);
      p.catch(() => built.delete(width));
      built.set(width, p);
    }
    return p;
  }

  /** Load the two rasters (lazily, ~0.8 MB) and make the layers. */
  static async create(width: number): Promise<GlobeTexture> {
    const [base, lights] = await Promise.all([loadImage(baseUrl), loadImage(lightsUrl)]);
    const w = width, h = width / 2;
    return new GlobeTexture(scaled(base, w, h), buildLights(w, h, lights));
  }
}

/** Tileable star field for the stage background, as a data URL. */
export function makeStars(): string {
  const c = canvas(512, 512);
  const g = c.getContext('2d')!;
  let s = 1234567;
  const rnd = () => (s = (s * 1664525 + 1013904223) >>> 0) / 4294967296;
  for (let i = 0; i < 90; i++) {
    const r = rnd() < 0.12 ? 1.3 : 0.7;
    g.fillStyle = 'rgba(190,215,255,' + (0.25 + rnd() * 0.5).toFixed(2) + ')';
    g.beginPath(); g.arc(rnd() * 512, rnd() * 512, r, 0, 6.2832); g.fill();
  }
  return c.toDataURL('image/png');
}
