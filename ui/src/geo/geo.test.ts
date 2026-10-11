import { describe, expect, it } from 'vitest';
import { angleDeg, fitAltitude, frame, frameAltitude, frameWithHome, greatCircle, midpoint, pingAlpha, pingRadius, slerp, sunPos, toVec } from './geo';

const SANTIAGO = { lat: -33.45, lon: -70.67 };
const GREENVILLE = { lat: 35.47, lon: -77.19 };

describe('geo (mirrors atlas-core)', () => {
  it('Santiago to Greenville matches the Rust distance', () => {
    // atlas-core: 7693.5 km on a 6371 km sphere
    expect((angleDeg(SANTIAGO, GREENVILLE) * Math.PI / 180) * 6371).toBeCloseTo(7693.5, 0);
  });

  it('antipodes are 180 degrees apart', () => {
    expect(angleDeg({ lat: 0, lon: 0 }, { lat: 0, lon: 180 })).toBeCloseTo(180, 9);
  });

  it('great circle starts and ends at the endpoints', () => {
    const pts = greatCircle(SANTIAGO, GREENVILLE, 10);
    expect(pts).toHaveLength(11);
    expect(pts[0].lat).toBeCloseTo(SANTIAGO.lat, 9);
    expect(pts[10].lon).toBeCloseTo(GREENVILLE.lon, 9);
  });

  it('midpoint is equidistant', () => {
    const m = midpoint(SANTIAGO, GREENVILLE);
    expect(angleDeg(SANTIAGO, m)).toBeCloseTo(angleDeg(m, GREENVILLE), 9);
  });

  it('slerp of identical vectors is that vector', () => {
    const v = toVec(10, 20);
    expect(slerp(v, v, 0.5)).toEqual(v);
  });

  it('sun is over the equator near the March equinox and the tropic in June', () => {
    expect(Math.abs(sunPos(new Date(Date.UTC(2026, 2, 20, 12))).lat)).toBeLessThan(1);
    expect(sunPos(new Date(Date.UTC(2026, 5, 21, 12))).lat).toBeCloseTo(23.44, 0);
  });

  it('sun is near the Greenwich meridian at 12:00 UTC', () => {
    // equation of time keeps it within ~4 degrees
    expect(Math.abs(sunPos(new Date(Date.UTC(2026, 3, 15, 12))).lon)).toBeLessThan(4.5);
  });

  it('fits camera altitudes to tall views and leaves wide ones alone', () => {
    expect(fitAltitude(2.6, 600, 600)).toBe(2.6);
    expect(fitAltitude(2.6, 1080, 600)).toBe(2.6);
    // the globe's apparent radius (tan of its half-angle) shrinks with the width
    const share = (alt: number) => Math.tan(Math.asin(1 / (1 + alt)));
    for (const aspect of [0.9, 0.6, 0.45]) {
      const alt = fitAltitude(2.6, 1000 * aspect, 1000);
      expect(alt).toBeGreaterThan(2.6);
      expect(share(alt) / aspect).toBeCloseTo(share(2.6), 9);
    }
    expect(fitAltitude(1.5, 500, 1000)).toBeLessThan(fitAltitude(2.6, 500, 1000));
    expect(fitAltitude(2.6, 0, 0)).toBe(2.6);
  });

  it('fits a view inside a taller canvas to the view, not the canvas', () => {
    const share = (alt: number) => Math.tan(Math.asin(1 / (1 + alt)));
    // a wide 1200×700 stage on a 1000 px tall window: the globe keeps its share of 700 px
    const alt = fitAltitude(2.6, 1200, 700, 1000);
    expect(alt).toBeGreaterThan(2.6);
    expect(share(alt) * 1000).toBeCloseTo(share(2.6) * 700, 9);
    // a tall stage fits its width
    expect(share(fitAltitude(2.6, 400, 700, 1000)) * 1000).toBeCloseTo(share(2.6) * 400, 9);
    // the same view on a canvas of its own height is plain fitting
    expect(fitAltitude(2.6, 1200, 700, 700)).toBe(2.6);
  });

  it('sizes scan pings by the S-meter', () => {
    expect(pingRadius(null)).toBe(2.5);
    expect(pingRadius(-54)).toBe(2.5);
    expect(pingRadius(0)).toBe(6);
    expect(pingRadius(30)).toBe(9);
    expect(pingRadius(60)).toBe(9);
    expect(pingRadius(-24)).toBeCloseTo(4.44, 2);
    expect(pingAlpha(null)).toBe(0.7);
    expect(pingAlpha(30)).toBe(1);
    expect(pingAlpha(0)).toBeGreaterThan(pingAlpha(-30));
  });

  it('frames a set of points', () => {
    const one = frame([SANTIAGO]);
    expect(one.center.lat).toBeCloseTo(SANTIAGO.lat, 9);
    expect(one.spreadDeg).toBeCloseTo(0, 6);
    const two = frame([SANTIAGO, GREENVILLE]);
    const mid = midpoint(SANTIAGO, GREENVILLE);
    expect(two.center.lat).toBeCloseTo(mid.lat, 6);
    expect(two.center.lon).toBeCloseTo(mid.lon, 6);
    expect(two.spreadDeg).toBeCloseTo(angleDeg(SANTIAGO, GREENVILLE) / 2, 6);
    // antipodes have no mean direction: frame from the first
    const anti = frame([{ lat: 0, lon: 0 }, { lat: 0, lon: 180 }]);
    expect(anti.center).toEqual({ lat: 0, lon: 0 });
    expect(anti.spreadDeg).toBeCloseTo(180, 6);
  });

  it('frames home with the points only when they fit in one view', () => {
    const near = frameWithHome([GREENVILLE], SANTIAGO);
    expect(near.spreadDeg).toBeCloseTo(angleDeg(SANTIAGO, GREENVILLE) / 2, 6);
    // Tokyo and Beijing seen from Santiago: home would be beyond the horizon
    const asia = [{ lat: 35.7, lon: 139.7 }, { lat: 39.9, lon: 116.4 }];
    const far = frameWithHome(asia, SANTIAGO);
    expect(far).toEqual(frame(asia));
    expect(frameWithHome([], SANTIAGO).center.lat).toBeCloseTo(SANTIAGO.lat, 9);
    expect(frameAltitude(0)).toBe(1.5);
    expect(frameAltitude(90)).toBe(3.4);
  });
});
