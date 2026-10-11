// Day and night on the globe, in its shader: the night side darkens, the city lights come on and
// a faint grey line marks the terminator (warm at dusk, cool at dawn), all from the sun's
// direction. Moving the sun costs one uniform; the texture is never redrawn.
//
// The base texture (`material.map`, sRGB) is taken back to sRGB values to be composited, so the
// result matches what the canvas once drew: the shade at `NIGHT_ALPHA` over the base, then the
// lights added ('lighter').

import { Vector3, type MeshPhongMaterial, type Texture } from 'three';
import type { Vec3 } from './geo';

/** Night shading strength and colour (sRGB). */
const NIGHT_ALPHA = 0.74;
const NIGHT_RGB = 'vec3(2.0, 6.0, 13.0) / 255.0';
/** The grey line: its strength, its half-width (in the cosine of the solar zenith angle) and its
 *  tints at dusk and dawn. */
const TWILIGHT = 0.07, TWILIGHT_W = 0.035;
const DUSK = 'vec3(1.0, 0.62, 0.32)', DAWN = 'vec3(0.5, 0.75, 1.0)';

const PARS = /* glsl */ `
uniform vec3 uSun;
uniform sampler2D uLights;
uniform float uTwilight;
`;

// After <map_fragment>: the map is equirectangular (u: -180..180 east, v: -90..90 north).
const BODY = /* glsl */ `
#ifdef USE_MAP
{
  float lat = (vMapUv.y - 0.5) * PI, lon = vMapUv.x * PI2 - PI;
  vec3 p = vec3(cos(lat) * cos(lon), cos(lat) * sin(lon), sin(lat));
  float cz = dot(p, uSun); // cosine of the solar zenith angle
  float night = 1.0 - smoothstep(-0.25, 0.08, cz);
  // lights come on a bit later than dusk falls
  float lit = 1.0 - smoothstep(-0.23, 0.02, cz);
  vec3 c = sRGBTransferOETF(vec4(diffuseColor.rgb, 1.0)).rgb;
  c = mix(c, ${NIGHT_RGB}, night * ${NIGHT_ALPHA.toFixed(2)});
  // the sun to the west: it has set here
  vec3 east = vec3(-sin(lon), cos(lon), 0.0);
  c = mix(c, dot(uSun, east) < 0.0 ? ${DUSK} : ${DAWN}, uTwilight * exp(-(cz * cz) / ${(TWILIGHT_W ** 2).toFixed(6)}));
  c = min(c + texture2D(uLights, vMapUv).rgb * lit, 1.0);
  diffuseColor.rgb = sRGBTransferEOTF(vec4(c, 1.0)).rgb;
}
#endif
`;

export interface Night {
  /** Point the sun along `v` (a unit vector, `toVec`'s frame). */
  setSun(v: Vec3): void;
  dispose(): void;
}

/** Shade `material` (the globe's, with its base `map`) for day and night; `lights` is the city
 *  lights layer, sampled as is (no colour space). */
export function applyNight(material: MeshPhongMaterial, lights: Texture): Night {
  const uniforms = {
    uSun: { value: new Vector3(1, 0, 0) },
    uLights: { value: lights },
    uTwilight: { value: TWILIGHT },
  };
  material.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, uniforms);
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', `#include <common>\n${PARS}`)
      .replace('#include <map_fragment>', `#include <map_fragment>\n${BODY}`);
  };
  material.customProgramCacheKey = () => 'atlas-night';
  material.needsUpdate = true;
  return {
    setSun: (v) => uniforms.uSun.value.set(v[0], v[1], v[2]),
    dispose: () => lights.dispose(),
  };
}
