// The globe and the prototype's "lock on" sequence:
//  1. once the choice settles, zoom out and swing to the transmitter (slerp + ease, duration by
//     distance; a flight that takes over a moving camera carries on its speed, `hermiteEase`)
//  2. land, show the label and pulsing rings
//  3. after 450 ms the arc, and the great-circle path drawing itself from the transmitter home
//  4. when the path lands, a blue "received" ring at the QTH, sized and paced by the S-meter
//  5. optionally reframe the whole route
// Off-air stations get the flight and the label only. With nothing tuned the camera goes back
// to the QTH and, left alone, the globe eases into a turn again.
// One frame loop drives the camera flights, the turn, the path's drawing, the S-meter's smoothing
// and the sun: night falls in the globe's shader (geo/night.ts), the terminator moving as it does.
// Towers rise and sink as their sites come on and go off the air (`stableSites`).
// While a band scan runs, lock-on rests: the camera holds over the QTH, a sonar pulse spreads
// from it, and at each step the sites on the air on that channel flash, sized by the reading
// (a signal also sends an arc home). When the scan ends, the sites it logged pulse green, framed
// together with the QTH, and lock-on resumes. Every altitude is fitted to the view's
// shape (`fitAltitude`), so a tall phone view shows as much as a wide window.
// The canvas may cover the whole window, behind the panels (styles.css, `data-globe="full"`):
// the globe is then centred on the stage (`globeOffset`) and sized for it, the rest is overflow.

import { Component, memo, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import GlobeGL, { type GlobeMethods } from 'react-globe.gl';
import { BufferAttribute, BufferGeometry, CanvasTexture, Line, LineBasicMaterial, MeshPhongMaterial, SRGBColorSpace } from 'three';
import { altitudeAt, angleDeg, clamp, easeInOut, fitAltitude, frameAltitude, frameWithHome, fromVec, greatCircle, hermiteEase, initialSlope, midpoint, pingAlpha, pingRadius, receivedRing, slerp, sunVec, toVec, type LatLon, type Vec3 } from '../geo/geo';
import { applyNight, type Night } from '../geo/night';
import { GlobeTexture, makeStars } from '../geo/texture';
import { flagUrl } from '../catalog/flags';
import { useT } from '../i18n';
import { SIGNAL_DB } from './scan';
import { stableSites } from './stableSites';
import type { Candidate } from '../types/generated/Candidate';
import type { Qth } from '../types/generated/Qth';
import type { ScanPing } from '../types/generated/ScanPing';
import type { SiteDot } from '../types/generated/SiteDot';

const REDUCE = typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
/** How often the sun moves (a uniform: this costs nothing). */
const SUN_MS = 1000;
/** Texture width; capped by the GPU's limit. */
const TEXTURE_WIDTH = 4096;
/** Camera altitudes, in globe radii, for a square view. */
const START_ALT = 2.6, LOCK_ALT = 1.5, HOME_ALT = 2.4, MIN_ALT = 0.3, MAX_ALT = 4;
/** One press of + or −. */
const ZOOM_STEP = 0.72;
/** Turn again after this long without a station or a touch. */
const IDLE_MS = 30_000;
/** Scans: the camera's altitude over the QTH; how long a ping's ring, a signal's arc and the
 *  finale last; how many of each at most; and how recently a touch keeps the camera still. */
const SCAN_ALT = 2.8, PING_MS = 1600, ARC_MS = 2400, FINALE_MS = 4000, MAX_PINGS = 30, MAX_ARCS = 8, HANDS_OFF_MS = 10_000;
/** While scanning the camera eases this often towards the sites pinged in the last `FOLLOW_SPAN_MS`
 *  (most transmitters may face away from the QTH), unless it is nearly there already. */
const FOLLOW_MS = 6000, FOLLOW_SPAN_MS = 15_000, FOLLOW_MIN_DEG = 10;
/** The turn's speed, and how fast it eases in and out (ms; out quicker, so a hovered dot stays put). */
const SPIN = 0.45, SPIN_IN_MS = 800, SPIN_OUT_MS = 250;
/** A station chosen this soon after another one (tuning through them) flies once, to the last. */
const SETTLE_MS = 150;
/** How long the route takes to draw itself home; the S-meter's smoothing; a tower's rise. */
const ROUTE_MS = 1300, LEVEL_MS = 400, TOWER_MS = 700;
/** A flight leaving slower than this (ease slope) leaves from rest. */
const MOVING = 0.05;

/** A 1×1 ocean-coloured image: three-globe only reports "ready" after loading an image. */
function placeholder(): string {
  const c = document.createElement('canvas');
  c.width = c.height = 1;
  const g = c.getContext('2d')!;
  g.fillStyle = '#0b1d31';
  g.fillRect(0, 0, 1, 1);
  return c.toDataURL('image/png');
}
const AMBER = '255,179,63', COOL = '127,208,255';
/** A scan ping's size on screen per degree of `pingRadius`. */
const PING_PX_PER_DEG = 8;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, REDUCE ? 0 : ms));

interface Pin { lat: number; lng: number; el: HTMLElement }
/** `live`: the "received" ring, sized and paced by the S-meter as it reads. */
interface Ring { lat: number; lng: number; maxR: number; speed: number; period: number; rgb: string; alpha?: number; live?: boolean }
interface Arc { sLat: number; sLng: number; eLat: number; eLng: number; dash: number; gap: number; anim: number; stroke: number; colors: string[] }
/** The great-circle path from a transmitter home (drawn by `GlobeView` itself, to draw it on). */
interface Path { site: LatLon }
interface Fx { station: Pin | null; rings: Ring[]; arcs: Arc[]; path: Path | null }
const NO_FX: Fx = { station: null, rings: [], arcs: [], path: null };
/** A scan's pings (glowing markers that pulse once) and arcs, each until a time (performance.now()). */
interface ScanFx { marks: (Pin & { until: number })[]; arcs: (Arc & { until: number })[] }
const NO_SCAN_FX: ScanFx = { marks: [], arcs: [] };

function mkPin(cls: string, title: string, sub: string, flag: string | null = null): HTMLElement {
  const el = document.createElement('div');
  el.className = 'pin ' + cls;
  el.innerHTML = '<span class="pin-dot"></span><span class="pin-tag"><b></b><i></i></span>';
  el.querySelector('b')!.textContent = title;
  el.querySelector('i')!.textContent = sub;
  const url = flagUrl(flag);
  if (url) {
    const img = document.createElement('img');
    img.className = 'flag';
    img.src = url;
    img.alt = '';
    el.querySelector('b')!.prepend(img);
  }
  return el;
}

/** A pulse at a site: `ping` (a scan step; `sig` for a signal) or `finale` (logged, pulses
 *  three times). Size in px and opacity as CSS variables; styles.css animates it. */
function mkPulse(cls: string, sizePx: number, alpha: number): HTMLElement {
  const el = document.createElement('div');
  el.className = 'pulse ' + cls;
  el.style.setProperty('--s', `${Math.round(sizePx)}px`);
  el.style.setProperty('--a', alpha.toFixed(2));
  return el;
}

const esc = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

/** Transmitter sites: short towers where something is on the air (taller with more), flat
 *  dim dots elsewhere, so they stand out from the city lights; the active site is white, sites
 *  of stations a scan logged are green (a little larger), and sites of the listener's own
 *  stations violet. */
const siteColor = (s: SiteDot, active: boolean, logged: boolean) =>
  active ? '#ffffff'
  : logged ? `rgba(79,200,164,${s.on ? 1 : 0.7})`
  : s.mine > 0 ? `rgba(190,160,255,${s.on ? 1 : 0.75})`
  : s.on ? `rgba(255,226,160,${s.precise ? 1 : 0.75})` : `rgba(150,185,220,${s.precise ? 0.6 : 0.35})`;
const siteRadius = (s: SiteDot, active: boolean, logged: boolean) => (active ? 0.6 : s.on ? 0.45 : 0.32) + (logged && !active ? 0.1 : 0);
const siteAltitude = (s: SiteDot, active: boolean) => (active ? 0.05 : s.on ? 0.012 + Math.min(0.03, s.on * 0.002) : 0.003);

function route(site: LatLon, qth: LatLon): Pick<Fx, 'arcs' | 'path'> {
  const [a, b] = [site, qth];
  return {
    arcs: [
      { sLat: a.lat, sLng: a.lon, eLat: b.lat, eLng: b.lon, dash: 1, gap: 0, anim: 0, stroke: 0.22, colors: [`rgba(${AMBER},0.4)`, `rgba(${COOL},0.4)`] },
      { sLat: a.lat, sLng: a.lon, eLat: b.lat, eLng: b.lon, dash: 0.16, gap: 1.2, anim: 2600, stroke: 0.55, colors: ['rgba(255,228,175,1)', `rgba(${COOL},1)`] },
    ],
    path: { site },
  };
}

/** The great-circle path from `site` to `qth` as a line just above the ground (`coords`: the
 *  globe's), a vertex per degree, to be drawn on with its draw range. */
function routeLine(site: LatLon, qth: LatLon, coords: GlobeMethods['getCoords']): Line {
  const pts = greatCircle(site, qth, Math.max(16, Math.round(angleDeg(site, qth))));
  const pos = new Float32Array(pts.length * 3);
  pts.forEach((p, i) => {
    const c = coords(p.lat, p.lon, 0.004);
    pos.set([c.x, c.y, c.z], i * 3);
  });
  const geom = new BufferGeometry();
  geom.setAttribute('position', new BufferAttribute(pos, 3));
  return new Line(geom, new LineBasicMaterial({ color: 0xffc882, transparent: true, opacity: 0.4, depthWrite: false }));
}

/** A camera flight under way: from `a` to `b` (unit vectors) and altitude `alt0` to `alt1` over
 *  `ms` from `t0`, leaving with ease slope `v0`. */
interface Flight { id: number; a: Vec3; b: Vec3; alt0: number; alt1: number; bump: number; ms: number; t0: number; v0: number; done: (landed: boolean) => void }

// Accessors live out here (or in useCallback) so each render hands the globe the same functions:
// a new `htmlElement` makes three-globe rebuild every pin, which replays their fade-in (a blink),
// and any other new accessor re-applies its whole layer.
const pinEl = (d: object) => (d as Pin).el;

const qthRing = (q: LatLon): Ring => ({ lat: q.lat, lng: q.lon, maxR: 4.5, speed: 2, period: 2200, rgb: COOL, live: true });
const sonarRing = (q: LatLon): Ring => ({ lat: q.lat, lng: q.lon, maxR: 25, speed: 6, period: 2500, rgb: COOL, alpha: 0.6 });

interface Props {
  sel: Candidate | null;
  qth: Qth;
  frame: boolean;
  onFrame: () => void;
  /** Hidden behind another view (phones): stop rendering. */
  paused: boolean;
  /** The user dragged, clicked or zoomed the globe. */
  onInteract: () => void;
  onQth: (q: Qth) => void;
  failedMsg: string;
  /** Every transmitter site with its on-air count. */
  sites: SiteDot[];
  /** The S-meter while the rig is on (dB over S9). */
  strengthDb: number | null;
  activeSite: number | null;
  /** Sites of stations a band scan logged. */
  loggedSites: ReadonlySet<number>;
  /** While a band scan runs: its last steps measured, and the noise floor so far. */
  scan: { pings: readonly ScanPing[]; floorDb: number | null } | null;
  /** A scan that just ended (`key`: its id) and the sites of the stations it logged. */
  finale: { key: string; sites: readonly number[] } | null;
  onSite: (id: number) => void;
  /** Tooltip text for a site: country name and "n of m on the air". */
  siteLabel: (s: SiteDot) => { country: string; onAir: string };
}

function GlobeView({ sel, qth, frame: framing, onFrame, paused, onInteract, onQth, sites, strengthDb, activeSite, loggedSites, scan, finale, onSite, siteLabel }: Props) {
  const t = useT();
  const stage = useRef<HTMLDivElement>(null);
  const globe = useRef<GlobeMethods>(undefined);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const placeholderUrl = useMemo(placeholder, []);
  // our own material (three-globe's default), so the drawn texture can be swapped in
  const material = useMemo(() => new MeshPhongMaterial({ color: 0x000000 }), []);
  const texture = useRef<{ tex: CanvasTexture; night: Night } | null>(null);
  const [fx, setFx] = useState<Fx>(NO_FX);
  const [ready, setReady] = useState(false);
  const seq = useRef(0), flight = useRef(0), grabbed = useRef(-1);
  /** The flight under way, and the camera's motion (per ms) for the next one to carry on. */
  const flying = useRef<Flight | null>(null);
  const motion = useRef({ at: -1, v: [0, 0, 1] as Vec3, ln: 0, vel: [0, 0, 0] as Vec3, lnRate: 0 });
  /** The turn's target speed (the frame loop eases the controls' speed to it). */
  const spin = useRef(0);
  /** The route's line and when it started drawing (`ms` 0: drawn). */
  const routing = useRef<{ line: Line; t0: number; ms: number } | null>(null);
  /** The S-meter as the received ring shows it: smoothed, null without a reading. */
  const strength = useRef(strengthDb);
  strength.current = strengthDb;
  const level = useRef<number | null>(null);
  const qthRef = useRef(qth);
  qthRef.current = qth;
  const frameRef = useRef(framing);
  frameRef.current = framing;
  const [scanFx, setScanFx] = useState<ScanFx>(NO_SCAN_FX);
  const scanning = scan !== null;
  /** The last scan step that pinged, and the sites pinged lately (for the camera to follow). */
  const pinged = useRef(-1);
  const recent = useRef<{ at: number; site: LatLon }[]>([]);
  /** The finale already played (lock-on rests until then). */
  const [finaleDone, setFinaleDone] = useState<string | null>(null);
  const finaleOn = !!finale && finale.sites.length > 0 && finale.key !== finaleDone;
  const siteIx = useMemo(() => new Map(sites.map((s) => [s.id, s])), [sites]);
  const handsOff = () => performance.now() - touched.current > HANDS_OFF_MS;
  const selRef = useRef(sel);
  selRef.current = sel;
  const interact = useRef(onInteract);
  interact.current = onInteract;
  const handlers = useRef({ onQth, onSite, siteLabel });
  handlers.current = { onQth, onSite, siteLabel };
  /** When the user last touched the globe (performance.now()). */
  const touched = useRef(-Infinity);
  /** The pointer is on a transmitter site. */
  const hovering = useRef(false);
  /** Nothing tuned, nothing hovered, untouched for a while: the globe may turn. */
  const idle = useCallback(
    () => !REDUCE && !selRef.current && !hovering.current && performance.now() - touched.current > IDLE_MS,
    [],
  );
  /** The part of the canvas left visible (the stage): the canvas may cover the whole window. */
  const view = useRef<HTMLDivElement>(null);
  const shape = useRef({ viewW: 1, viewH: 1, canvasH: 1 });
  const fit = useCallback((alt: number) => fitAltitude(alt, shape.current.viewW, shape.current.viewH, shape.current.canvasH), []);
  /** Moves the globe's centre from the canvas's to the view's. */
  const [offset, setOffset] = useState<[number, number]>([0, 0]);

  useEffect(() => {
    const el = stage.current!, v = view.current!;
    const ro = new ResizeObserver(() => {
      if (!el.clientWidth || !el.clientHeight || !v.clientWidth || !v.clientHeight) return;
      const c = el.getBoundingClientRect(), r = v.getBoundingClientRect();
      shape.current = { viewW: r.width, viewH: r.height, canvasH: c.height };
      const dx = Math.round(r.left + r.width / 2 - (c.left + c.width / 2));
      const dy = Math.round(r.top + r.height / 2 - (c.top + c.height / 2));
      setOffset((o) => (o[0] === dx && o[1] === dy ? o : [dx, dy]));
      setSize({ w: el.clientWidth, h: el.clientHeight });
    });
    ro.observe(el);
    ro.observe(v);
    return () => ro.disconnect();
  }, []);

  // a taller view may pull back further
  useEffect(() => {
    if (ready) globe.current!.controls().maxDistance = 100 * (1 + fit(MAX_ALT));
  }, [ready, size, fit]);

  useEffect(() => {
    if (!ready) return;
    if (paused) globe.current!.pauseAnimation();
    else globe.current!.resumeAnimation();
  }, [ready, paused]);

  // left alone with nothing tuned, turn again
  useEffect(() => {
    if (!ready || REDUCE) return;
    const id = setInterval(() => {
      if (idle()) spin.current = SPIN;
    }, 5000);
    return () => clearInterval(id);
  }, [ready, idle]);

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
    const gt = await GlobeTexture.create(Math.min(TEXTURE_WIDTH, caps.maxTextureSize));
    if (texture.current || !globe.current) return;
    const tex = new CanvasTexture(gt.base);
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
    g.pointOfView({ lat: qthRef.current.lat, lng: qthRef.current.lon, altitude: fit(START_ALT) }, 0);
    const ctl = g.controls();
    // left on: the frame loop eases its speed in and out (`spin`)
    ctl.autoRotate = !REDUCE;
    ctl.autoRotateSpeed = 0;
    spin.current = SPIN;
    ctl.enablePan = false;
    ctl.minDistance = 100 * (1 + MIN_ALT);
    ctl.maxDistance = 100 * (1 + fit(MAX_ALT));
    ctl.addEventListener('start', () => {
      spin.current = ctl.autoRotateSpeed = 0;
      flight.current++;
      grabbed.current = seq.current;
      touched.current = performance.now();
      interact.current();
    });
    setReady(true);
    loadTexture(g).catch((e) => console.warn('globe texture:', e));
  }, [loadTexture, fit]);

  /** Fly the camera (the frame loop moves it): true when it lands, false when something else
   *  took the camera first. A flight carries on the camera's motion, a turn included. */
  const fly = useCallback((lat: number, lon: number, alt: number, ms: number, bump: number) => {
    const g = globe.current!;
    const id = ++flight.current;
    flying.current?.done(false);
    flying.current = null;
    return new Promise<boolean>((resolve) => {
      const p0 = g.pointOfView();
      if (REDUCE || ms <= 0) {
        g.pointOfView({ lat, lng: lon, altitude: alt }, 0);
        resolve(true);
        return;
      }
      spin.current = g.controls().autoRotateSpeed = 0; // the flight takes the turn's speed over
      const a = toVec(p0.lat, p0.lng), b = toVec(lat, lon), m = motion.current;
      const v0 = initialSlope(a, m.vel, b, m.lnRate, Math.log1p(p0.altitude), Math.log1p(alt), ms);
      flying.current = { id, a, b, alt0: p0.altitude, alt1: alt, bump, ms, t0: performance.now(), v0, done: resolve };
    });
  }, []);

  /** The user took the camera: no automatic framing for this lock-on, no turning for a while. */
  const grab = useCallback(() => {
    spin.current = globe.current!.controls().autoRotateSpeed = 0;
    grabbed.current = seq.current;
    touched.current = performance.now();
    interact.current();
  }, []);

  /** Fly back over the QTH. */
  const home = useCallback(() => {
    const g = globe.current!, q = qthRef.current;
    const p0 = g.pointOfView();
    const mv = angleDeg({ lat: p0.lat, lon: p0.lng }, q);
    return fly(q.lat, q.lon, fit(HOME_ALT), Math.round(1200 + 1200 * Math.min(1, mv / 150)), 0.3 * Math.min(1, mv / 90));
  }, [fly, fit]);

  const zoom = useCallback((k: number) => {
    grab();
    const p = globe.current!.pointOfView();
    fly(p.lat, p.lng, clamp(p.altitude * k, MIN_ALT, fit(MAX_ALT)), 450, 0);
  }, [grab, fly, fit]);

  // Lock on when the selected station (or its on-air status) changes; not during a scan or its
  // finale (they own the globe), and again when they end.
  const lockKey = sel ? `${sel.station.id}:${sel.air.on}` : null;
  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    if (!ready) return;
    if (scanning || finaleOn) {
      lastKey.current = null;
      return;
    }
    const my = ++seq.current;
    const was = lastKey.current;
    lastKey.current = lockKey;
    if (!sel) {
      flight.current++;
      setFx(NO_FX);
      // the station went away (rig off, tuned off it): back home unless the user is steering
      if (was !== null) {
        sleep(800).then(() => {
          if (my === seq.current && performance.now() - touched.current > 10_000) home();
        });
      }
      return;
    }
    const site = { lat: sel.site.lat, lon: sel.site.lon };
    const on = sel.air.on;
    setFx(NO_FX);
    (async () => {
      // tuning through stations: fly to where it stops
      if (was !== null) {
        await sleep(SETTLE_MS);
        if (my !== seq.current) return;
      }
      const g = globe.current!;
      spin.current = 0;
      const p0 = g.pointOfView();
      const mv = angleDeg({ lat: p0.lat, lon: p0.lng }, site);
      await fly(site.lat, site.lon, fit(LOCK_ALT), Math.round(1500 + 1700 * Math.min(1, mv / 150)), 0.5 + 1.4 * Math.min(1, mv / 120));
      if (my !== seq.current) return;
      const station = { lat: site.lat, lng: site.lon, el: mkPin(on ? '' : 'off', sel.station.name, sel.site.name, sel.station.flag) };
      const siteRing: Ring = { lat: site.lat, lng: site.lon, maxR: 7, speed: 3.2, period: 1400, rgb: AMBER };
      setFx({ ...NO_FX, station, rings: on ? [siteRing] : [] });
      if (!on) return;
      await sleep(450); if (my !== seq.current) return;
      setFx((f) => ({ ...f, ...route(site, qthRef.current) }));
      // received when the path lands
      await sleep(ROUTE_MS); if (my !== seq.current) return;
      setFx((f) => ({ ...f, rings: [...f.rings, qthRing(qthRef.current)] }));
      if (!frameRef.current || grabbed.current === my) return;
      await sleep(1100); if (my !== seq.current || grabbed.current === my) return;
      const mid = midpoint(qthRef.current, site);
      fly(mid.lat, mid.lon, fit(clamp(0.55 + angleDeg(qthRef.current, site) / 42, 1.5, 3.4)), 2200, 0.15);
    })();
  }, [lockKey, ready, fly, fit, home, scanning, finaleOn]);

  // A scan starts: drop the station and its route, hold over the QTH. It ends: clear its layers.
  useEffect(() => {
    if (!ready) return;
    if (!scanning) {
      setScanFx(NO_SCAN_FX);
      return;
    }
    seq.current++;
    flight.current++;
    pinged.current = -1;
    recent.current = [];
    setFx(NO_FX);
    setScanFx(NO_SCAN_FX);
    spin.current = 0;
    if (handsOff()) {
      const q = qthRef.current;
      fly(q.lat, q.lon, fit(SCAN_ALT), 1600, 0.2);
    }
  }, [scanning, ready, fly, fit]);

  // Each new step measured: its sites flash; a signal also sends an arc home.
  useEffect(() => {
    if (!scan || !ready) return;
    const fresh = scan.pings.filter((p) => p.index > pinged.current);
    if (!fresh.length) return;
    pinged.current = fresh[fresh.length - 1].index;
    const now = performance.now(), q = qthRef.current;
    const marks: ScanFx['marks'] = [], arcs: ScanFx['arcs'] = [];
    for (const p of fresh) {
      const signal = p.db !== null && scan.floorDb !== null && p.db >= scan.floorDb + SIGNAL_DB;
      for (const id of p.sites) {
        const s = siteIx.get(id);
        if (!s) continue;
        recent.current.push({ at: now, site: { lat: s.lat, lon: s.lon } });
        const el = mkPulse(signal ? 'ping sig' : 'ping', pingRadius(p.db) * PING_PX_PER_DEG, pingAlpha(p.db));
        marks.push({ lat: s.lat, lng: s.lon, el, until: now + PING_MS });
        if (signal && !REDUCE) {
          arcs.push({
            sLat: s.lat, sLng: s.lon, eLat: q.lat, eLng: q.lon, dash: 0.25, gap: 1.2, anim: 1100, stroke: 0.45,
            colors: ['rgba(255,228,175,0.95)', `rgba(${COOL},0.9)`], until: now + ARC_MS,
          });
        }
      }
    }
    if (marks.length) setScanFx((f) => ({ marks: [...f.marks, ...marks].slice(-MAX_PINGS), arcs: [...f.arcs, ...arcs].slice(-MAX_ARCS) }));
  }, [scan, ready, siteIx]);

  // The scan ended: the sites it logged pulse green, framed with the QTH; then lock-on resumes.
  const finaleKey = finaleOn ? finale!.key : null;
  useEffect(() => {
    if (!ready || !finaleKey || !finale) return;
    const pts = finale.sites.map((id) => siteIx.get(id)).filter((s): s is SiteDot => !!s);
    const now = performance.now();
    seq.current++;
    setFx(NO_FX);
    setScanFx({ marks: pts.map((s) => ({ lat: s.lat, lng: s.lon, el: mkPulse('finale', 40, 1), until: now + FINALE_MS })), arcs: [] });
    if (pts.length && handsOff()) {
      const f = frameWithHome(pts.map((s) => ({ lat: s.lat, lon: s.lon })), qthRef.current);
      fly(f.center.lat, f.center.lon, fit(frameAltitude(f.spreadDeg)), 2000, 0.2);
    }
    const id = setTimeout(() => {
      setScanFx(NO_SCAN_FX);
      setFinaleDone(finaleKey);
    }, FINALE_MS);
    return () => clearTimeout(id);
  }, [finaleKey, ready]); // the finale plays once per scan, whatever else changes

  // While scanning, ease towards where the pings are (and home, when it fits in the view).
  useEffect(() => {
    if (!scanning || !ready || REDUCE) return;
    const id = setInterval(() => {
      const now = performance.now();
      recent.current = recent.current.filter((r) => now - r.at < FOLLOW_SPAN_MS);
      if (!recent.current.length || !handsOff()) return;
      const f = frameWithHome(recent.current.map((r) => r.site), qthRef.current);
      const p = globe.current!.pointOfView();
      if (angleDeg({ lat: p.lat, lon: p.lng }, f.center) < FOLLOW_MIN_DEG) return;
      fly(f.center.lat, f.center.lon, fit(frameAltitude(f.spreadDeg)), 3000, 0.1);
    }, FOLLOW_MS);
    return () => clearInterval(id);
  }, [scanning, ready, fly, fit]);

  // Pings and arcs go when their time is up.
  useEffect(() => {
    if (!scanning) return;
    const id = setInterval(() => {
      const now = performance.now();
      setScanFx((f) => {
        const marks = f.marks.filter((m) => m.until > now), arcs = f.arcs.filter((a) => a.until > now);
        return marks.length === f.marks.length && arcs.length === f.arcs.length ? f : { marks, arcs };
      });
    }, 250);
    return () => clearInterval(id);
  }, [scanning]);

  // The route draws itself from the transmitter home; moving the QTH redraws it at once.
  const drawn = useRef<Path | null>(null);
  useEffect(() => {
    const g = globe.current;
    if (!ready || !g || !fx.path) return;
    const line = routeLine(fx.path.site, qth, g.getCoords);
    const ms = REDUCE || drawn.current === fx.path ? 0 : ROUTE_MS;
    drawn.current = fx.path;
    line.geometry.setDrawRange(0, ms ? 0 : Infinity);
    g.scene().add(line);
    routing.current = { line, t0: performance.now(), ms };
    return () => {
      g.scene().remove(line);
      line.geometry.dispose();
      (line.material as LineBasicMaterial).dispose();
      routing.current = null;
    };
  }, [ready, fx.path, qth]);

  // The frame loop: flights, the turn, the route's drawing, the S-meter and the sun.
  useEffect(() => {
    if (!ready || paused) return;
    const g = globe.current!, ctl = g.controls();
    let raf = 0, last = 0, sunAt = 0;
    const tick = (now: number) => {
      raf = requestAnimationFrame(tick);
      const dt = last ? clamp(now - last, 1, 100) : 16;
      last = now;

      const f = flying.current;
      if (f && f.id !== flight.current) {
        flying.current = null;
        f.done(false);
      } else if (f) {
        const k = clamp((now - f.t0) / f.ms, 0, 1);
        const e = f.v0 > MOVING ? hermiteEase(k, f.v0) : easeInOut(k);
        const v = fromVec(slerp(f.a, f.b, e));
        g.pointOfView({ lat: v.lat, lng: v.lon, altitude: altitudeAt(f.alt0, f.alt1, f.bump, e) }, 0);
        if (k >= 1) {
          flying.current = null;
          f.done(true);
        }
      }

      // the camera's motion (smoothed a little), for the next flight to carry on
      const p = g.pointOfView(), m = motion.current;
      const v = toVec(p.lat, p.lng), ln = Math.log1p(p.altitude);
      if (m.at >= 0) {
        const s = 1 - Math.exp(-dt / 50);
        for (let i = 0; i < 3; i++) m.vel[i] += ((v[i] - m.v[i]) / dt - m.vel[i]) * s;
        m.lnRate += ((ln - m.ln) / dt - m.lnRate) * s;
      }
      Object.assign(m, { at: now, v, ln });

      if (ctl.autoRotate) {
        const tau = spin.current < ctl.autoRotateSpeed ? SPIN_OUT_MS : SPIN_IN_MS;
        ctl.autoRotateSpeed += (spin.current - ctl.autoRotateSpeed) * (1 - Math.exp(-dt / tau));
        if (Math.abs(ctl.autoRotateSpeed - spin.current) < 1e-3) ctl.autoRotateSpeed = spin.current;
      }

      const r = routing.current;
      if (r && r.ms) {
        const k = clamp((now - r.t0) / r.ms, 0, 1);
        r.line.geometry.setDrawRange(0, Math.ceil(easeInOut(k) * r.line.geometry.getAttribute('position').count));
        if (k >= 1) r.ms = 0;
      }

      const db = REDUCE ? null : strength.current, lv = level.current;
      level.current = db === null ? null : lv === null ? db : lv + (db - lv) * (1 - Math.exp(-dt / LEVEL_MS));

      if (now - sunAt > SUN_MS) {
        sunAt = now;
        texture.current?.night.setSun(sunVec(new Date()));
      }
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [ready, paused]);

  // the received ring follows the S-meter ring by ring (three-globe reads these as it sends one)
  const ringMaxR = useCallback((d: object) => ((d as Ring).live ? receivedRing(level.current).maxR : (d as Ring).maxR), []);
  const ringSpeed = useCallback((d: object) => ((d as Ring).live ? receivedRing(level.current).speed : (d as Ring).speed), []);
  const ringPeriod = useCallback((d: object) => ((d as Ring).live ? receivedRing(level.current).period : (d as Ring).period), []);
  const ringColor = useCallback((d: object) => {
    const r = d as Ring;
    return (t: number) => {
      const a = r.live ? receivedRing(level.current).alpha : r.alpha ?? 1;
      return `rgba(${r.rgb},${(Math.pow(1 - t, 1.6) * 0.9 * a).toFixed(3)})`;
    };
  }, []);

  const sonar = useMemo(() => sonarRing(qth), [qth]);
  const rings = useMemo(() => (scanning && !REDUCE ? [...fx.rings, sonar] : fx.rings), [fx.rings, scanning, sonar]);
  const arcs = useMemo(() => [...fx.arcs, ...scanFx.arcs], [fx.arcs, scanFx.arcs]);

  // Moving the QTH redraws the route in place.
  useEffect(() => {
    setFx((f) => {
      if (!sel || !f.arcs.length) return f;
      return {
        ...f,
        ...route({ lat: sel.site.lat, lon: sel.site.lon }, qth),
        path: f.path, // the same path: redrawn at once, not drawn on again
        rings: f.rings.filter((r) => r.rgb !== COOL).concat([qthRing(qth)]),
      };
    });
  }, [qth.lat, qth.lon]);

  /** Hold the turn while the pointer is on a site, or the dot slides out from under it and the
   *  tooltip blinks. */
  const onPointHover = useCallback((d: object | null) => {
    const ctl = globe.current?.controls();
    if (!ctl) return;
    hovering.current = !!d;
    if (d) spin.current = 0;
    else if (idle()) spin.current = SPIN;
  }, [idle]);

  const siteCache = useRef(new Map<number, SiteDot>());
  const shownSites = useRef<readonly SiteDot[]>([]);
  const points = useMemo(() => (shownSites.current = stableSites(siteCache.current, shownSites.current, sites)), [sites]);
  const qthPin = useMemo<Pin>(() => ({ lat: qth.lat, lng: qth.lon, el: mkPin('qth', 'QTH', qth.name) }), [qth]);
  const pins = useMemo(() => [qthPin, ...(fx.station ? [fx.station] : []), ...scanFx.marks], [qthPin, fx.station, scanFx.marks]);
  const stars = useMemo(() => makeStars(), []);
  const pointAltitude = useCallback((d: object) => siteAltitude(d as SiteDot, (d as SiteDot).id === activeSite), [activeSite]);
  const pointRadius = useCallback((d: object) => siteRadius(d as SiteDot, (d as SiteDot).id === activeSite, loggedSites.has((d as SiteDot).id)), [activeSite, loggedSites]);
  const pointColor = useCallback((d: object) => siteColor(d as SiteDot, (d as SiteDot).id === activeSite, loggedSites.has((d as SiteDot).id)), [activeSite, loggedSites]);
  const siteTip = useCallback((d: object) => {
    const s = d as SiteDot;
    const l = handlers.current.siteLabel(s);
    const url = flagUrl(s.flag);
    const flag = url ? `<img class="flag" src="${esc(url)}" alt="">` : '';
    return `<div class="site-tip"><b>${flag}${esc(s.name)}</b><span>${esc(l.country)}</span><span>${esc(l.onAir)}</span></div>`;
  }, []);
  const onGlobeClick = useCallback((c: { lat: number; lng: number }) => {
    interact.current();
    handlers.current.onQth({ name: `${c.lat.toFixed(1)}°, ${c.lng.toFixed(1)}°`, lat: c.lat, lon: c.lng });
  }, []);
  const onPointClick = useCallback((d: object) => handlers.current.onSite((d as SiteDot).id), []);

  return (
    <>
      <div className="stars" style={{ backgroundImage: `url(${stars})` }} />
      <div className="globe-view" ref={view} />
      <div id="globe" ref={stage}>
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
            pointLabel={siteTip}
            onPointHover={onPointHover}
            onPointClick={onPointClick}
            htmlElementsData={pins}
            htmlLat="lat"
            htmlLng="lng"
            htmlAltitude={0.004}
            htmlElement={pinEl}
            htmlTransitionDuration={0}
            ringsData={rings}
            ringLat="lat"
            ringLng="lng"
            ringAltitude={0.003}
            ringColor={ringColor}
            ringMaxRadius={ringMaxR}
            ringPropagationSpeed={ringSpeed}
            ringRepeatPeriod={ringPeriod}
            arcsData={arcs}
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
            arcsTransitionDuration={1400}
          />
        )}
      </div>
      <div className="globe-ctl" role="group" aria-label={t.globeControls}>
        <div className="zoom">
          <button className="icon-btn" type="button" disabled={!ready} aria-label={t.zoomIn} title={t.zoomIn} onClick={() => zoom(ZOOM_STEP)}>+</button>
          <button className="icon-btn" type="button" disabled={!ready} aria-label={t.zoomOut} title={t.zoomOut} onClick={() => zoom(1 / ZOOM_STEP)}>−</button>
          <button className="icon-btn" type="button" disabled={!ready} aria-label={t.centerQth} title={t.centerQth} onClick={() => { grab(); home(); }}>⌖</button>
        </div>
        <button className="toggle" type="button" aria-pressed={framing} onClick={onFrame}>{t.frameRoute}</button>
      </div>
    </>
  );
}

/** WebGL can be missing (old GPU, remote desktop): keep the rest of the app usable. */
class GlobeBoundary extends Component<{ msg: string; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    return this.state.failed ? <div className="stage-msg">{this.props.msg}</div> : this.props.children;
  }
}

/** Renders again only when a prop changes, so pass stable callbacks. */
export const Globe = memo(function Globe(props: Props) {
  return (
    <GlobeBoundary msg={props.failedMsg}>
      <GlobeView {...props} />
    </GlobeBoundary>
  );
});
