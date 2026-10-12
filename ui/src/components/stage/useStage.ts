// The stage's choreography, shared by the globe and the flat map: the prototype's "lock on"
// sequence:
//  1. once the choice settles, zoom out and swing to the transmitter (slerp + ease, duration by
//     distance; a flight that takes over a moving camera carries on its speed, `hermiteEase`)
//  2. land, show the label and pulsing rings
//  3. after 450 ms the arc, and the great-circle path drawing itself from the transmitter home
//  4. when the path lands, a blue "received" ring at the QTH, sized and paced by the S-meter
//  5. optionally reframe the whole route
// Off-air stations get the flight and the label only. With nothing tuned the camera goes back
// to the QTH and, left alone, the view eases into a turn again.
// One frame loop drives the camera flights, the turn, the S-meter's smoothing and the view's own
// per-frame work (`Camera.frame`: the path's drawing, the sun).
// While a band scan runs, lock-on rests: the camera holds over the QTH, a sonar pulse spreads
// from it, and at each step the sites on the air on that channel flash, sized by the reading
// (a signal also sends an arc home). When the scan ends, the sites it logged pulse green, framed
// together with the QTH, and lock-on resumes. Every altitude is fitted to the view's
// shape (`Camera.fit`), so a tall phone view shows as much as a wide window.
// The camera is the globe's in both views: a point to look down on and an altitude in globe
// radii; each view turns it into its own (`Camera`).

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { altitudeAt, angleDeg, clamp, easeInOut, frameAltitude, frameWithHome, fromVec, hermiteEase, initialSlope, midpoint, pingAlpha, pingRadius, slerp, toVec, type LatLon, type Vec3 } from '../../geo/geo';
import { SIGNAL_DB } from '../scan';
import { ARC_MS, COOL, HOME_ALT, LOCK_ALT, mkPin, mkPulse, NO_FX, NO_SCAN_FX, PING_MS, PING_PX_PER_DEG, qthRing, REDUCE, ROUTE_MS, route, sonarRing, SPIN, START_ALT, AMBER, type Arc, type Fx, type Path, type Pin, type Ring, type ScanFx } from './fx';
import type { MapMode } from '../../hooks/useDisplay';
import type { Candidate } from '../../types/generated/Candidate';
import type { Qth } from '../../types/generated/Qth';
import type { ScanPing } from '../../types/generated/ScanPing';
import type { SiteDot } from '../../types/generated/SiteDot';

/** Turn again after this long without a station or a touch. */
const IDLE_MS = 30_000;
/** Scans: the camera's altitude over the QTH; how long the finale lasts; how many pings and arcs
 *  at most; and how recently a touch keeps the camera still. */
const SCAN_ALT = 2.8, FINALE_MS = 4000, MAX_PINGS = 30, MAX_ARCS = 8, HANDS_OFF_MS = 10_000;
/** While scanning the camera eases this often towards the sites pinged in the last `FOLLOW_SPAN_MS`
 *  (most transmitters may face away from the QTH), unless it is nearly there already. */
const FOLLOW_MS = 6000, FOLLOW_SPAN_MS = 15_000, FOLLOW_MIN_DEG = 10;
/** How fast the turn eases in and out (ms; out quicker, so a hovered dot stays put). */
const SPIN_IN_MS = 800, SPIN_OUT_MS = 250;
/** A station chosen this soon after another one (tuning through them) flies once, to the last. */
const SETTLE_MS = 150;
/** The S-meter's smoothing. */
const LEVEL_MS = 400;
/** A flight leaving slower than this (ease slope) leaves from rest. */
const MOVING = 0.05;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, REDUCE ? 0 : ms));

/** Where the camera is: over `lat, lon`, `alt` globe radii up. */
export interface Pov { lat: number; lon: number; alt: number }

/** A view's camera, as the choreography drives it. */
export interface Camera {
  pov(): Pov;
  /** Put the camera there now (the frame loop moves it this way during flights). */
  set(p: Pov): void;
  /** The turn at `speed` (`SPIN` at full), for a frame `dt` ms long. */
  turn(speed: number, dt: number): void;
  /** `e` of the way from `a` to `b` in a flight; a great circle unless the view says otherwise. */
  between?(a: LatLon, b: LatLon, e: number): LatLon;
  /** An altitude for a square view, fitted to this view's shape. */
  fit(alt: number): number;
  /** The altitudes + and − may reach. */
  range(): [number, number];
  /** The view's own work each frame, after the camera has moved. */
  frame?(now: number, dt: number): void;
}

/** A camera flight under way: from `a` to `b` (unit vectors; `la`, `lb` as points) and altitude
 *  `alt0` to `alt1` over `ms` from `t0`, leaving with ease slope `v0`. */
interface Flight { id: number; a: Vec3; b: Vec3; la: LatLon; lb: LatLon; alt0: number; alt1: number; bump: number; ms: number; t0: number; v0: number; done: (landed: boolean) => void }

export interface StageInput {
  sel: Candidate | null;
  qth: Qth;
  frame: boolean;
  /** Hidden behind another view (phones): stop the frame loop. */
  paused: boolean;
  /** The user dragged, clicked or zoomed the view. */
  onInteract: () => void;
  sites: SiteDot[];
  /** The S-meter while the rig is on (dB over S9). */
  strengthDb: number | null;
  /** While a band scan runs: its last steps measured, and the noise floor so far. */
  scan: { pings: readonly ScanPing[]; floorDb: number | null } | null;
  /** A scan that just ended (`key`: its id) and the sites of the stations it logged. */
  finale: { key: string; sites: readonly number[] } | null;
}

/** What App hands either view (Globe.tsx picks one). */
export interface ViewProps extends StageInput {
  onFrame: () => void;
  onQth: (q: Qth) => void;
  activeSite: number | null;
  /** Sites of stations a band scan logged. */
  loggedSites: ReadonlySet<number>;
  /** Sites the listener listened to, by time (0..1); empty unless the list is on Listened. */
  listenedSites: ReadonlyMap<number, number>;
  onSite: (id: number) => void;
  /** Tooltip text for a site: country name and "n of m on the air". */
  siteLabel: (s: SiteDot) => { country: string; onAir: string };
  mode: MapMode;
  onMode: (m: MapMode) => void;
}

export interface Stage {
  /** The view handed its camera over (`attach`). */
  ready: boolean;
  attach(c: Camera): void;
  fx: Fx;
  rings: Ring[];
  arcs: Arc[];
  /** The QTH's pin, the station's and the scan's pulses. */
  pins: Pin[];
  /** The S-meter as the received ring shows it: smoothed, null without a reading. */
  level: { readonly current: number | null };
  /** How much of the route is drawn at `now` (0..1). */
  routeProgress(now: number): number;
  zoom(k: number): void;
  /** Fly back over the QTH, the user's doing. */
  goHome(): void;
  /** The user took the camera (a press, a drag, the wheel). */
  userStart(): void;
  /** The pointer is on a transmitter site (or left it). */
  hover(on: boolean): void;
}

export function useStage({ sel, qth, frame: framing, paused, onInteract, sites, strengthDb, scan, finale }: StageInput): Stage {
  const cam = useRef<Camera | null>(null);
  const [ready, setReady] = useState(false);
  const [fx, setFx] = useState<Fx>(NO_FX);
  const seq = useRef(0), flight = useRef(0), grabbed = useRef(-1);
  /** The flight under way, and the camera's motion (per ms) for the next one to carry on. */
  const flying = useRef<Flight | null>(null);
  const motion = useRef({ at: -1, v: [0, 0, 1] as Vec3, ln: 0, vel: [0, 0, 0] as Vec3, lnRate: 0 });
  /** The turn's target speed, and its speed now (the frame loop eases one to the other). */
  const spin = useRef(0), spinNow = useRef(0);
  /** The route being drawn and when it started (`ms` 0: drawn at once). */
  const routing = useRef<{ path: Path; t0: number; ms: number } | null>(null);
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
  /** When the user last touched the view (performance.now()). */
  const touched = useRef(-Infinity);
  /** The pointer is on a transmitter site. */
  const hovering = useRef(false);
  /** Nothing tuned, nothing hovered, untouched for a while: the view may turn. */
  const idle = useCallback(
    () => !REDUCE && !selRef.current && !hovering.current && performance.now() - touched.current > IDLE_MS,
    [],
  );
  const fit = useCallback((alt: number) => cam.current!.fit(alt), []);

  const attach = useCallback((c: Camera) => {
    cam.current = c;
    const q = qthRef.current;
    c.set({ lat: q.lat, lon: q.lon, alt: c.fit(START_ALT) });
    spin.current = REDUCE ? 0 : SPIN;
    setReady(true);
  }, []);

  /** Stop turning at once (not eased). */
  const stopSpin = useCallback(() => {
    spin.current = spinNow.current = 0;
    cam.current?.turn(0, 0);
  }, []);

  // left alone with nothing tuned, turn again
  useEffect(() => {
    if (!ready || REDUCE) return;
    const id = setInterval(() => {
      if (idle()) spin.current = SPIN;
    }, 5000);
    return () => clearInterval(id);
  }, [ready, idle]);

  /** Fly the camera (the frame loop moves it): true when it lands, false when something else
   *  took the camera first. A flight carries on the camera's motion, a turn included. */
  const fly = useCallback((lat: number, lon: number, alt: number, ms: number, bump: number) => {
    const c = cam.current!;
    const id = ++flight.current;
    flying.current?.done(false);
    flying.current = null;
    return new Promise<boolean>((resolve) => {
      const p0 = c.pov();
      if (REDUCE || ms <= 0) {
        c.set({ lat, lon, alt });
        resolve(true);
        return;
      }
      stopSpin(); // the flight takes the turn's speed over
      const a = toVec(p0.lat, p0.lon), b = toVec(lat, lon), m = motion.current;
      const v0 = initialSlope(a, m.vel, b, m.lnRate, Math.log1p(p0.alt), Math.log1p(alt), ms);
      flying.current = { id, a, b, la: { lat: p0.lat, lon: p0.lon }, lb: { lat, lon }, alt0: p0.alt, alt1: alt, bump, ms, t0: performance.now(), v0, done: resolve };
    });
  }, [stopSpin]);

  /** The user took the camera: no automatic framing for this lock-on, no turning for a while. */
  const grab = useCallback(() => {
    stopSpin();
    grabbed.current = seq.current;
    touched.current = performance.now();
    interact.current();
  }, [stopSpin]);

  /** The user pressed on the view: it stops any flight, then as `grab`. */
  const userStart = useCallback(() => {
    flight.current++;
    grab();
  }, [grab]);

  /** Fly back over the QTH. */
  const home = useCallback(() => {
    const q = qthRef.current, p0 = cam.current!.pov();
    const mv = angleDeg({ lat: p0.lat, lon: p0.lon }, q);
    return fly(q.lat, q.lon, fit(HOME_ALT), Math.round(1200 + 1200 * Math.min(1, mv / 150)), 0.3 * Math.min(1, mv / 90));
  }, [fly, fit]);

  const goHome = useCallback(() => {
    grab();
    home();
  }, [grab, home]);

  const zoom = useCallback((k: number) => {
    grab();
    const c = cam.current!, p = c.pov(), [lo, hi] = c.range();
    fly(p.lat, p.lon, clamp(p.alt * k, lo, hi), 450, 0);
  }, [grab, fly]);

  // Lock on when the selected station (or its on-air status) changes; not during a scan or its
  // finale (they own the camera), and again when they end.
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
      spin.current = 0;
      const p0 = cam.current!.pov();
      const mv = angleDeg({ lat: p0.lat, lon: p0.lon }, site);
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
      const p = cam.current!.pov();
      if (angleDeg({ lat: p.lat, lon: p.lon }, f.center) < FOLLOW_MIN_DEG) return;
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

  // The route draws itself from the transmitter home; the same path again (the QTH moved) is
  // drawn at once.
  useEffect(() => {
    if (!fx.path) {
      routing.current = null;
      return;
    }
    if (routing.current?.path === fx.path) return;
    routing.current = { path: fx.path, t0: performance.now(), ms: REDUCE ? 0 : ROUTE_MS };
  }, [fx.path]);
  const routeProgress = useCallback((now: number) => {
    const r = routing.current;
    return !r ? 0 : r.ms ? easeInOut(clamp((now - r.t0) / r.ms, 0, 1)) : 1;
  }, []);

  // The frame loop: flights, the turn, the S-meter, then the view's own work.
  useEffect(() => {
    if (!ready || paused) return;
    const c = cam.current!;
    let raf = 0, last = 0;
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
        const v = c.between ? c.between(f.la, f.lb, e) : fromVec(slerp(f.a, f.b, e));
        c.set({ lat: v.lat, lon: v.lon, alt: altitudeAt(f.alt0, f.alt1, f.bump, e) });
        if (k >= 1) {
          flying.current = null;
          f.done(true);
        }
      }

      // the camera's motion (smoothed a little), for the next flight to carry on
      const p = c.pov(), m = motion.current;
      const v = toVec(p.lat, p.lon), ln = Math.log1p(p.alt);
      if (m.at >= 0) {
        const s = 1 - Math.exp(-dt / 50);
        for (let i = 0; i < 3; i++) m.vel[i] += ((v[i] - m.v[i]) / dt - m.vel[i]) * s;
        m.lnRate += ((ln - m.ln) / dt - m.lnRate) * s;
      }
      Object.assign(m, { at: now, v, ln });

      if (!REDUCE) {
        const tau = spin.current < spinNow.current ? SPIN_OUT_MS : SPIN_IN_MS;
        spinNow.current += (spin.current - spinNow.current) * (1 - Math.exp(-dt / tau));
        if (Math.abs(spinNow.current - spin.current) < 1e-3) spinNow.current = spin.current;
        c.turn(spinNow.current, dt);
      }

      const db = REDUCE ? null : strength.current, lv = level.current;
      level.current = db === null ? null : lv === null ? db : lv + (db - lv) * (1 - Math.exp(-dt / LEVEL_MS));

      c.frame?.(now, dt);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [ready, paused]);

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
  const hover = useCallback((on: boolean) => {
    if (!cam.current) return;
    hovering.current = on;
    if (on) spin.current = 0;
    else if (idle()) spin.current = SPIN;
  }, [idle]);

  const qthPin = useMemo<Pin>(() => ({ lat: qth.lat, lng: qth.lon, el: mkPin('qth', 'QTH', qth.name) }), [qth]);
  const pins = useMemo(() => [qthPin, ...(fx.station ? [fx.station] : []), ...scanFx.marks], [qthPin, fx.station, scanFx.marks]);

  return { ready, attach, fx, rings, arcs, pins, level, routeProgress, zoom, goHome, userStart, hover };
}
