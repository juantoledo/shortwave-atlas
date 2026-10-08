// Globe texture drawn at runtime: the "receiver at night" look, in detail.
//
// Static base (built once):  ocean gradient, graticule, coastal glow, land, shaded relief,
//                            country borders, coastlines (Natural Earth 50m via world-atlas).
// Per refresh (every 2 min): night shading from the real sun position, and city lights
//                            (NASA Black Marble) glowing amber only on the night side.
//
// The result is one canvas used directly as a THREE.CanvasTexture: no PNG round trip.

import { geoEquirectangular, geoGraticule10, geoPath, type GeoProjection } from 'd3-geo';
import { feature, mesh } from 'topojson-client';
import type { GeometryCollection, Topology } from 'topojson-specification';
import lightsUrl from '../assets/globe/lights.jpg';
import reliefUrl from '../assets/globe/relief.jpg';
import { clamp, RAD, sunPos } from './geo';

/** Equirectangular projection onto a w×h texture, matching three-globe's UV mapping. */
export function textureProjection(w: number, h: number): GeoProjection {
  return geoEquirectangular().scale(w / (2 * Math.PI)).translate([w / 2, h / 2]).precision(0.1);
}

/** Night mask resolution (upscaled smoothly onto the texture). */
const MW = 720, MH = 360;
/** Night shading strength and the warm tint of the city lights. */
const NIGHT_ALPHA = 0.74;
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

type World = Topology<{ countries: GeometryCollection; land: GeometryCollection }>;

function buildBase(w: number, h: number, world: World, relief: HTMLImageElement): HTMLCanvasElement {
  const c = canvas(w, h);
  const g = c.getContext('2d')!;
  const k = w / 4096; // line widths are tuned for a 4096 px wide texture
  const path = geoPath(textureProjection(w, h), g);
  const land = feature(world, world.objects.land);
  const borders = mesh(world, world.objects.countries, (a, b) => a !== b);

  // ocean: a little lighter toward the equator
  const og = g.createLinearGradient(0, 0, 0, h);
  og.addColorStop(0, '#08131f'); og.addColorStop(0.5, '#0b1d31'); og.addColorStop(1, '#08131f');
  g.fillStyle = og; g.fillRect(0, 0, w, h);

  g.beginPath(); path(geoGraticule10());
  g.lineWidth = 0.8 * k; g.strokeStyle = 'rgba(130,180,230,0.07)'; g.stroke();

  // coastal glow: a wide blurred halo around the land, like a lit shelf
  g.save();
  g.beginPath(); path(land);
  if (typeof g.filter === 'string') {
    g.filter = `blur(${Math.round(6 * k)}px)`;
    g.lineWidth = 14 * k; g.strokeStyle = 'rgba(90,160,230,0.16)'; g.stroke();
  } else {
    // no canvas filters (older WebKit): stack soft strokes instead
    for (const [lw, a] of [[22, 0.03], [15, 0.04], [9, 0.05], [5, 0.06]]) {
      g.lineWidth = lw * k; g.strokeStyle = `rgba(90,160,230,${a})`; g.stroke();
    }
  }
  g.restore();

  g.beginPath(); path(land);
  g.fillStyle = '#183350'; g.fill();

  // shaded relief, clipped to land. Under 'overlay' a dark base is scaled by 2×relief,
  // so 128 grey (flat) leaves it as is, lit slopes brighten and shadows darken.
  g.save();
  g.beginPath(); path(land); g.clip();
  g.globalCompositeOperation = 'overlay';
  g.drawImage(relief, 0, 0, w, h);
  g.restore();

  g.beginPath(); path(borders);
  g.lineWidth = 1.1 * k; g.lineJoin = 'round'; g.strokeStyle = 'rgba(150,200,250,0.22)'; g.stroke();

  g.beginPath(); path(land);
  g.lineWidth = 2 * k; g.strokeStyle = 'rgba(150,205,255,0.62)'; g.stroke();
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

export class GlobeTexture {
  readonly canvas: HTMLCanvasElement;
  private base: HTMLCanvasElement;
  private lights: HTMLCanvasElement;
  private lightsNow: HTMLCanvasElement;
  private shade = canvas(MW, MH);
  private reveal = canvas(MW, MH);

  private constructor(w: number, h: number, world: World, relief: HTMLImageElement, lights: HTMLImageElement) {
    this.canvas = canvas(w, h);
    this.base = buildBase(w, h, world, relief);
    this.lights = buildLights(w, h, lights);
    this.lightsNow = canvas(w, h);
  }

  /** Load the map data and rasters (lazily, they are ~2 MB) and build the static layers. */
  static async create(width: number): Promise<GlobeTexture> {
    const [world, relief, lights] = await Promise.all([
      import('world-atlas/countries-50m.json').then((m) => m.default as unknown as World),
      loadImage(reliefUrl),
      loadImage(lightsUrl),
    ]);
    return new GlobeTexture(width, width / 2, world, relief, lights);
  }

  /** Redraw for time `d`: night shading and the city lights it reveals. */
  render(d: Date) {
    const sun = sunPos(d), sd = Math.sin(sun.lat * RAD), cd = Math.cos(sun.lat * RAD);
    const shadeImg = this.shade.getContext('2d')!.createImageData(MW, MH);
    const revealImg = this.reveal.getContext('2d')!.createImageData(MW, MH);
    for (let j = 0; j < MH; j++) {
      const lat = (90 - (j + 0.5) * (180 / MH)) * RAD, sl = Math.sin(lat) * sd, cl = Math.cos(lat) * cd;
      for (let i = 0; i < MW; i++) {
        const lon = (i + 0.5) * (360 / MW) - 180;
        const cz = sl + cl * Math.cos((lon - sun.lon) * RAD); // cosine of the solar zenith angle
        const t = clamp((0.08 - cz) / 0.33, 0, 1), night = t * t * (3 - 2 * t);
        // lights come on a bit later than dusk falls
        const l = clamp((0.02 - cz) / 0.25, 0, 1), lit = l * l * (3 - 2 * l);
        const q = (j * MW + i) * 4;
        shadeImg.data[q] = 2; shadeImg.data[q + 1] = 6; shadeImg.data[q + 2] = 13;
        shadeImg.data[q + 3] = Math.round(night * NIGHT_ALPHA * 255);
        revealImg.data[q + 3] = Math.round(lit * 255);
      }
    }
    this.shade.getContext('2d')!.putImageData(shadeImg, 0, 0);
    this.reveal.getContext('2d')!.putImageData(revealImg, 0, 0);

    const { width: w, height: h } = this.canvas;
    const ln = this.lightsNow.getContext('2d')!;
    ln.globalCompositeOperation = 'copy';
    ln.drawImage(this.lights, 0, 0);
    ln.globalCompositeOperation = 'destination-in';
    ln.imageSmoothingQuality = 'high';
    ln.drawImage(this.reveal, 0, 0, w, h);

    const g = this.canvas.getContext('2d')!;
    g.globalCompositeOperation = 'copy';
    g.drawImage(this.base, 0, 0);
    g.globalCompositeOperation = 'source-over';
    g.imageSmoothingQuality = 'high';
    g.drawImage(this.shade, 0, 0, w, h);
    g.globalCompositeOperation = 'lighter';
    g.drawImage(this.lightsNow, 0, 0);
    g.globalCompositeOperation = 'source-over';
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
