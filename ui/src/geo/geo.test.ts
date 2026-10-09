import { describe, expect, it } from 'vitest';
import { angleDeg, fitAltitude, greatCircle, midpoint, slerp, sunPos, toVec } from './geo';

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
    expect(fitAltitude(2.6, 1)).toBe(2.6);
    expect(fitAltitude(2.6, 1.8)).toBe(2.6);
    // the globe's apparent radius (tan of its half-angle) shrinks with the width
    const share = (alt: number) => Math.tan(Math.asin(1 / (1 + alt)));
    for (const aspect of [0.9, 0.6, 0.45]) {
      const alt = fitAltitude(2.6, aspect);
      expect(alt).toBeGreaterThan(2.6);
      expect(share(alt) / aspect).toBeCloseTo(share(2.6), 9);
    }
    expect(fitAltitude(1.5, 0.5)).toBeLessThan(fitAltitude(2.6, 0.5));
  });
});
