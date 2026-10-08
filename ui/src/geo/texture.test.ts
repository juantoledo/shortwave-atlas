import { describe, expect, it } from 'vitest';
import { textureProjection } from './texture';

describe('globe texture projection', () => {
  // three-globe maps lon -180..180 to u 0..1 and lat 90..-90 to v 0..1
  const p = textureProjection(4096, 2048);
  const near = (a: [number, number] | null, b: [number, number]) => {
    expect(a![0]).toBeCloseTo(b[0], 6);
    expect(a![1]).toBeCloseTo(b[1], 6);
  };

  it('puts the corners and the centre where three-globe expects them', () => {
    near(p([0, 0]), [2048, 1024]);
    near(p([-180, 90]), [0, 0]);
    near(p([180, -90]), [4096, 2048]);
  });

  it('is linear in longitude and latitude', () => {
    near(p([-70.67, -33.45]), [((-70.67 + 180) / 360) * 4096, ((90 + 33.45) / 180) * 2048]);
  });
});
