import { describe, expect, it } from 'vitest';
import { altFor, clampLat, copies, invert, lerpFlat, project, pxPerDeg, spanDeg, unwrap, wrapLon, type FlatView } from './flat';
import { greatCircle } from './geo';

const view: FlatView = { lat: 10, lon: 170, ppd: 4, cx: 400, cy: 300, w: 800, h: 600 };

describe('flat map', () => {
  it('wraps longitudes into -180..180', () => {
    expect(wrapLon(190)).toBeCloseTo(-170, 9);
    expect(wrapLon(-190)).toBeCloseTo(170, 9);
    expect(wrapLon(540)).toBeCloseTo(-180, 9);
    expect(wrapLon(-70.67)).toBeCloseTo(-70.67, 9);
  });

  it('projects and inverts back', () => {
    const p = { lat: -33.45, lon: -170.67 };
    const [x, y] = project(view, p);
    const q = invert(view, x, y)!;
    expect(q.lat).toBeCloseTo(p.lat, 9);
    expect(q.lon).toBeCloseTo(p.lon, 9);
  });

  it('puts a point on the world copy nearest the centre', () => {
    // 170°E at the centre: 170°W is 20° east, not 340° west
    const [x] = project(view, { lat: 0, lon: -170 });
    expect(x).toBeCloseTo(400 + 20 * 4, 9);
  });

  it('has nothing past the poles', () => {
    expect(invert(view, 400, 300 - (81 * 4))).toBeNull();
    expect(invert(view, 400, 300 + (101 * 4))).toBeNull();
  });

  it('keeps the poles from coming into view', () => {
    // 600 px tall at 4 px/° shows 150°: the centre stays within ±15°
    expect(clampLat(80, 4, 300, 600)).toBeCloseTo(15, 9);
    expect(clampLat(-80, 4, 300, 600)).toBeCloseTo(-15, 9);
    expect(clampLat(5, 4, 300, 600)).toBe(5);
  });

  it('centres a world shorter than the canvas', () => {
    // 2 px/° is 360 px of world in 600: the equator at the canvas's middle, whatever the centre
    expect(clampLat(40, 2, 300, 600)).toBeCloseTo(0, 9);
    // the view's centre 100 px above the canvas's: the equator still at y 300
    const lat = clampLat(40, 2, 200, 600);
    expect(200 + lat * 2).toBeCloseTo(300, 9);
  });

  it('shows more with altitude, and the whole world height from the farthest', () => {
    expect(spanDeg(1.5)).toBeLessThan(spanDeg(2.4));
    expect(spanDeg(4)).toBeGreaterThanOrEqual(180);
    expect(altFor(pxPerDeg(1.5, 1200, 700), 1200, 700)).toBeCloseTo(1.5, 9);
    // the shorter side sets the scale
    expect(pxPerDeg(2, 400, 900)).toBeCloseTo(pxPerDeg(2, 900, 400), 9);
  });

  it('pans across the 180th meridian the short way', () => {
    const a = { lat: 0, lon: 170 }, b = { lat: 10, lon: -170 };
    const m = lerpFlat(a, b, 0.5);
    expect(m.lat).toBeCloseTo(5, 9);
    expect(Math.abs(m.lon)).toBeCloseTo(180, 9);
    expect(lerpFlat(a, b, 1).lon).toBeCloseTo(-170, 9);
  });

  it('draws a Pacific route in one piece', () => {
    // Tokyo to San Francisco crosses the 180th meridian
    const pts = unwrap(greatCircle({ lat: 35.7, lon: 139.7 }, { lat: 37.8, lon: -122.4 }, 60));
    for (let i = 1; i < pts.length; i++) expect(Math.abs(pts[i].lon - pts[i - 1].lon)).toBeLessThan(10);
    expect(pts[pts.length - 1].lon).toBeCloseTo(-122.4 + 360, 6);
  });

  it('repeats sideways to cover the canvas', () => {
    // a world 300 px wide from x -50 on an 800 px canvas: copies at 0, 300, 600
    expect(copies(-50, 250, 300, 800)).toEqual([0, 300, 600]);
    // something at x 790..810 also shows 300 px to the left, and 600
    expect(copies(790, 810, 300, 800)).toEqual([-600, -300, 0]);
  });
});
