// Draw the globe texture's base layer (src/geo/baseLayer.ts) into src/assets/globe/base.webp,
// with a Node canvas (Skia, as Chromium draws it). Run again after changing baseLayer.ts,
// relief.jpg (assets/make_globe_rasters.py) or the world-atlas version:
//
//   npm --prefix ui run globe-base

import { createCanvas, loadImage } from '@napi-rs/canvas';
import { writeFileSync } from 'node:fs';
import world from 'world-atlas/countries-50m.json' with { type: 'json' };
import { drawBase, type World } from '../src/geo/baseLayer.ts';

const W = 4096, H = 2048, QUALITY = 90;
const dir = new URL('../src/assets/globe/', import.meta.url);

const relief = await loadImage(new URL('relief.jpg', dir));
const c = createCanvas(W, H);
drawBase(c.getContext('2d') as unknown as CanvasRenderingContext2D, W, H, world as unknown as World, relief as unknown as CanvasImageSource);
const out = await c.encode('webp', QUALITY);
writeFileSync(new URL('base.webp', dir), out);
console.log(`base.webp: ${W}×${H}, ${Math.round(out.length / 1024)} KB`);
