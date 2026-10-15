// The globe texture's base layer: ocean gradient, graticule, coastal glow, land, shaded relief,
// country borders and coastlines (Natural Earth 50m via world-atlas), in the "receiver at night"
// look. Drawn once, ahead of time, by `ui/scripts/make_globe_base.ts` (a Node canvas running this
// code) into `assets/globe/base.webp`, which the app loads (texture.ts): drawing it in the page
// took the map data (0.75 MB) and a long main-thread stall. No DOM here, so Node can run it.

import { geoEquirectangular, geoGraticule10, geoPath, type GeoProjection } from 'd3-geo';
import { feature, mesh } from 'topojson-client';
import type { GeometryCollection, Topology } from 'topojson-specification';

/** Equirectangular projection onto a w×h texture, matching three-globe's UV mapping. */
export function textureProjection(w: number, h: number): GeoProjection {
  return geoEquirectangular().scale(w / (2 * Math.PI)).translate([w / 2, h / 2]).precision(0.1);
}

export type World = Topology<{ countries: GeometryCollection; land: GeometryCollection }>;

/** Draw the base layer onto `g`, a w×h canvas; `relief` is `relief.jpg`. */
export function drawBase(g: CanvasRenderingContext2D, w: number, h: number, world: World, relief: CanvasImageSource): void {
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
  g.filter = `blur(${Math.round(6 * k)}px)`;
  g.lineWidth = 14 * k; g.strokeStyle = 'rgba(90,160,230,0.16)'; g.stroke();
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
}
