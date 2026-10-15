// The stage's world, as a 3D globe or a flat map (the switch over it; Settings keeps the choice).
// Both play the same choreography (stage/useStage.ts: lock-on, scans, flights); this file is the
// globe's renderer and the switch between the two.
//
// The globe: night falls in its shader (geo/night.ts), the terminator moving as it does, and
// towers rise and sink as their sites come on and go off the air (`stableSites`). The great-circle
// path home is a line of our own in its scene, drawn on as the route lands; a transmitter's
// signal hops over it as three-globe paths (a fat line for the pulse), growing in with it.
// The canvas may cover the whole window, behind the panels (styles.css, `data-globe="full"`):
// the globe is then centred on the stage (`globeOffset`) and sized for it, the rest is overflow.

import { Component, memo, useCallback, useEffect, useMemo, useRef, type ReactNode } from 'react';
import GlobeGL, { type GlobeMethods } from 'react-globe.gl';
import { BufferAttribute, BufferGeometry, CanvasTexture, Line, LineBasicMaterial, MeshPhongMaterial, SRGBColorSpace, Texture } from 'three';
import { angleDeg, fitAltitude, greatCircle, sunVec, type LatLon } from '../geo/geo';
import { applyNight, type Night } from '../geo/night';
import { GlobeTexture, makeStars, textureWidth } from '../geo/texture';
import { FlatView } from './FlatMap';
import { Controls } from './stage/Controls';
import { MAX_ALT, MIN_ALT, REDUCE, ringColor, ringNow, siteAltitude, siteColor, siteRadius, siteTip, TOWER_MS, ARC_IN_MS, ROUTE_MS, type Pin, type Ring } from './stage/fx';
import { useStage, type Camera, type ViewProps } from './stage/useStage';
import { useStageShape } from './stage/useStageShape';
import { stableSites } from './stableSites';
import type { SiteDot } from '../types/generated/SiteDot';

/** How often the sun moves (a uniform: this costs nothing). */
const SUN_MS = 1000;

/** A 1×1 ocean-coloured image: three-globe only reports "ready" after loading an image. */
function placeholder(): string {
  const c = document.createElement('canvas');
  c.width = c.height = 1;
  const g = c.getContext('2d')!;
  g.fillStyle = '#0b1d31';
  g.fillRect(0, 0, 1, 1);
  return c.toDataURL('image/png');
}

/** The great-circle path from `site` to `qth` as a line just above the ground (`coords`: the
 *  globe's), a vertex per degree, to be drawn on with its draw range. */
function routeLine(site: LatLon, qth: LatLon, coords: GlobeMethods['getCoords'], rgb = '255,200,130'): Line {
  const pts = greatCircle(site, qth, Math.max(16, Math.round(angleDeg(site, qth))));
  const pos = new Float32Array(pts.length * 3);
  pts.forEach((p, i) => {
    const c = coords(p.lat, p.lon, 0.004);
    pos.set([c.x, c.y, c.z], i * 3);
  });
  const geom = new BufferGeometry();
  geom.setAttribute('position', new BufferAttribute(pos, 3));
  const [r, g, b] = rgb.split(',').map(Number);
  return new Line(geom, new LineBasicMaterial({ color: (r << 16) | (g << 8) | b, transparent: true, opacity: 0.4, depthWrite: false }));
}

// Accessors live out here (or in useCallback) so each render hands the globe the same functions:
// a new `htmlElement` makes three-globe rebuild every pin, which replays their fade-in (a blink),
// and any other new accessor re-applies its whole layer.
const pinEl = (d: object) => (d as Pin).el;
/** A skywave point's altitude (three-globe's own default is a constant, flat on the ground). */
const hopAlt = (p: object) => (p as [number, number, number])[2];

function GlobeView(props: ViewProps) {
  const { qth, frame: framing, onFrame, paused, sites, activeSite, loggedSites, listenedSites, mode, onMode } = props;
  const stage = useStage(props);
  const { attach, routeProgress, level, hover } = stage;
  const { stage: el, view, size, shape, offset } = useStageShape();
  const globe = useRef<GlobeMethods>(undefined);
  const placeholderUrl = useMemo(placeholder, []);
  // our own material (three-globe's default), so the drawn texture can be swapped in
  const material = useMemo(() => new MeshPhongMaterial({ color: 0x000000 }), []);
  const texture = useRef<{ tex: Texture; night: Night } | null>(null);
  /** The route's line, drawn on as `routeProgress` goes. */
  const routing = useRef<Line | null>(null);
  const handlers = useRef({ onQth: props.onQth, onSite: props.onSite, siteLabel: props.siteLabel });
  handlers.current = { onQth: props.onQth, onSite: props.onSite, siteLabel: props.siteLabel };
  const fit = useCallback((alt: number) => fitAltitude(alt, shape.current.viewW, shape.current.viewH, shape.current.canvasH), [shape]);

  // a taller view may pull back further
  useEffect(() => {
    if (stage.ready) globe.current!.controls().maxDistance = 100 * (1 + fit(MAX_ALT));
  }, [stage.ready, size, fit]);

  useEffect(() => {
    if (!stage.ready) return;
    if (paused) globe.current!.pauseAnimation();
    else globe.current!.resumeAnimation();
  }, [stage.ready, paused]);

  useEffect(() => () => {
    if (texture.current) {
      texture.current.tex.dispose();
      texture.current.night.dispose();
      texture.current = null;
    }
  }, []);

  /** Swap the placeholder for the drawn texture, shaded for day and night by the sun. */
  const loadTexture = useCallback(async (g: GlobeMethods) => {
    const caps = g.renderer().capabilities;
    const gt = await GlobeTexture.shared(Math.min(textureWidth(), caps.maxTextureSize));
    if (texture.current || !globe.current) return;
    // the base layer is the loaded image itself (or a canvas, scaled down for a small GPU)
    const tex = new Texture(gt.base);
    tex.needsUpdate = true;
    tex.colorSpace = SRGBColorSpace;
    tex.anisotropy = caps.getMaxAnisotropy();
    const lights = new CanvasTexture(gt.lights);
    lights.anisotropy = tex.anisotropy;
    material.map?.dispose(); // the placeholder
    material.map = tex;
    const night = applyNight(material, lights);
    night.setSun(sunVec(new Date()));
    texture.current = { tex, night };
  }, [material]);

  const onReady = useCallback(() => {
    const g = globe.current!;
    const ctl = g.controls();
    // left on: the frame loop eases its speed in and out
    ctl.autoRotate = !REDUCE;
    ctl.autoRotateSpeed = 0;
    ctl.enablePan = false;
    ctl.minDistance = 100 * (1 + MIN_ALT);
    ctl.maxDistance = 100 * (1 + fit(MAX_ALT));
    let sunAt = 0;
    const camera: Camera = {
      pov: () => {
        const p = g.pointOfView();
        return { lat: p.lat, lon: p.lng, alt: p.altitude };
      },
      set: (p) => g.pointOfView({ lat: p.lat, lng: p.lon, altitude: p.alt }, 0),
      turn: (speed) => { ctl.autoRotateSpeed = speed; },
      fit,
      range: () => [MIN_ALT, fit(MAX_ALT)],
      frame: (now) => {
        const line = routing.current;
        if (line) line.geometry.setDrawRange(0, Math.ceil(routeProgress(now) * line.geometry.getAttribute('position').count));
        if (now - sunAt > SUN_MS) {
          sunAt = now;
          texture.current?.night.setSun(sunVec(new Date()));
        }
      },
    };
    ctl.addEventListener('start', stage.userStart);
    attach(camera);
    loadTexture(g).catch((e) => console.warn('globe texture:', e));
  }, [loadTexture, fit, attach, routeProgress, stage.userStart]);

  // The route's line, in the scene while there is a route; moving the QTH redraws it.
  const path = stage.fx.path;
  useEffect(() => {
    const g = globe.current;
    if (!stage.ready || !g || !path) return;
    const line = routeLine(path.site, qth, g.getCoords, path.rgb);
    line.geometry.setDrawRange(0, Math.ceil(routeProgress(performance.now()) * line.geometry.getAttribute('position').count));
    g.scene().add(line);
    routing.current = line;
    return () => {
      g.scene().remove(line);
      line.geometry.dispose();
      (line.material as LineBasicMaterial).dispose();
      routing.current = null;
    };
  }, [stage.ready, path, qth, routeProgress]);

  // the received ring follows the S-meter ring by ring (three-globe reads these as it sends one)
  const ringMaxR = useCallback((d: object) => ringNow(d as Ring, level.current).maxR, [level]);
  const ringSpeed = useCallback((d: object) => ringNow(d as Ring, level.current).speed, [level]);
  const ringPeriod = useCallback((d: object) => ringNow(d as Ring, level.current).period, [level]);
  const ringAlt = useCallback((d: object) => 0.003 + ((d as Ring).alt ?? 0), []);
  const ringColorFn = useCallback((d: object) => {
    const r = d as Ring;
    return (t: number) => ringColor(r, t, ringNow(r, level.current).alpha);
  }, [level]);

  const onPointHover = useCallback((d: object | null) => hover(!!d), [hover]);

  const siteCache = useRef(new Map<number, SiteDot>());
  const shownSites = useRef<readonly SiteDot[]>([]);
  const points = useMemo(() => (shownSites.current = stableSites(siteCache.current, shownSites.current, sites)), [sites]);
  const stars = useMemo(() => makeStars(), []);
  const pointAltitude = useCallback((d: object) => siteAltitude(d as SiteDot, (d as SiteDot).id === activeSite), [activeSite]);
  const pointRadius = useCallback((d: object) => {
    const id = (d as SiteDot).id;
    return siteRadius(d as SiteDot, id === activeSite, loggedSites.has(id), listenedSites.get(id));
  }, [activeSite, loggedSites, listenedSites]);
  const pointColor = useCallback((d: object) => {
    const id = (d as SiteDot).id;
    return siteColor(d as SiteDot, id === activeSite, loggedSites.has(id), listenedSites.get(id));
  }, [activeSite, loggedSites, listenedSites]);
  const pointLabel = useCallback((d: object) => siteTip(d as SiteDot, handlers.current.siteLabel(d as SiteDot)), []);
  const onGlobeClick = useCallback((c: { lat: number; lng: number }) => {
    props.onInteract();
    handlers.current.onQth({ name: `${c.lat.toFixed(1)}°, ${c.lng.toFixed(1)}°`, lat: c.lat, lon: c.lng });
  }, [props.onInteract]);
  const onPointClick = useCallback((d: object) => handlers.current.onSite((d as SiteDot).id), []);

  return (
    <>
      <div className="stars" style={{ backgroundImage: `url(${stars})` }} />
      <div className="globe-view" ref={view} />
      <div id="globe" ref={el}>
        {size.w > 0 && (
          <GlobeGL
            ref={globe}
            width={size.w}
            height={size.h}
            globeOffset={offset}
            animateIn={false}
            backgroundColor="rgba(0,0,0,0)"
            globeImageUrl={placeholderUrl}
            globeMaterial={material}
            atmosphereColor="#4d8fd6"
            atmosphereAltitude={0.17}
            showGraticules={false}
            onGlobeReady={onReady}
            onGlobeClick={onGlobeClick}
            pointsData={points as object[]}
            pointLat="lat"
            pointLng="lon"
            pointAltitude={pointAltitude}
            pointRadius={pointRadius}
            pointColor={pointColor}
            pointResolution={10}
            pointsMerge={false}
            pointsTransitionDuration={REDUCE ? 0 : TOWER_MS}
            pointLabel={pointLabel}
            onPointHover={onPointHover}
            onPointClick={onPointClick}
            htmlElementsData={stage.pins}
            htmlLat="lat"
            htmlLng="lng"
            htmlAltitude={0.004}
            htmlElement={pinEl}
            htmlTransitionDuration={0}
            ringsData={stage.rings}
            ringLat="lat"
            ringLng="lng"
            ringAltitude={ringAlt}
            ringColor={ringColorFn}
            ringMaxRadius={ringMaxR}
            ringPropagationSpeed={ringSpeed}
            ringRepeatPeriod={ringPeriod}
            arcsData={stage.arcs}
            arcStartLat="sLat"
            arcStartLng="sLng"
            arcEndLat="eLat"
            arcEndLng="eLng"
            arcColor="colors"
            arcStroke="stroke"
            arcDashLength="dash"
            arcDashGap="gap"
            arcDashAnimateTime="anim"
            arcAltitudeAutoScale={0.42}
            arcsTransitionDuration={ARC_IN_MS}
            pathsData={stage.sky}
            pathPoints="pts"
            pathPointAlt={hopAlt}
            pathColor="colors"
            pathStroke="stroke"
            pathDashLength="dash"
            pathDashGap="gap"
            pathDashAnimateTime="anim"
            pathTransitionDuration={REDUCE ? 0 : ROUTE_MS}
          />
        )}
      </div>
      <Controls stage={stage} mode={mode} onMode={onMode} framing={framing} onFrame={onFrame} />
    </>
  );
}

/** WebGL can be missing (old GPU, remote desktop): show the flat map instead. */
class GlobeBoundary extends Component<{ fallback: ReactNode; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

/** Renders again only when a prop changes, so pass stable callbacks. */
export const Globe = memo(function Globe(props: ViewProps & { failedMsg: string }) {
  if (props.mode === 'flat') return <FlatView key="flat" {...props} />;
  return (
    <GlobeBoundary key="globe" fallback={<FlatView {...props} noGlobe />}>
      <GlobeView {...props} />
    </GlobeBoundary>
  );
});
