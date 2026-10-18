// The receiver faceplate: readout and modes; the band dial (a zoom on the band you are in, kHz
// by kHz, under its name: a list to jump to any band, between arrows to step); the S-meter and span switch beside the whole dial (one span of what the rig
// receives: LW/MW, HF, VHF or UHF), which shows broadcast bands on one lane and ham, CB and
// utility bands on another, with a lens over where the band dial looks; and the dual tuning
// knob at the right; under the band dial, the scan controls, and over it the strength profile of
// the band's last scan (or the one running) with the scan before it as a ghost, and under it a
// heat strip of the channels listened to with a valid signal. `rx-x` parts fold away on small
// screens until expanded.
// Tuned to an internet station (the NET band), the same faceplate becomes an internet receiver:
// the station's name in the readout, its stream where the modes were, the NET favorites as
// memory channels on the band dial (the arrows and the knob step through them), the audio held
// for the S-meter, the stream's buffer as a funnel in the whole dial's place (its sections pick
// the size: NetFunnel), a chip while the stream connects or fills it, and a line for the rig
// waiting to come back.

import { useCallback, useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react';
import { useNames } from '../catalog/names';
import { useMeter, type Meter } from '../hooks/meter';
import { sLabel, useT } from '../i18n';
import { altName, isMac } from '../keys/keymap';
import type { Band } from '../types/generated/Band';
import { Flag } from './Flag';
import type { NetPhase } from './net';
import { NetFunnel } from './NetFunnel';
import type { FreqHeat } from '../types/generated/FreqHeat';
import type { Mode } from '../types/generated/Mode';
import type { RxRange } from '../types/generated/RxRange';
import type { Span } from '../types/generated/Span';
import {
  adjacentBand, bandBars, bandEntryHz, bandWindow, dialPct, dialTicks, fitLabels, heatCells, markPaths, MARKS_W, rangeLabel, spanAt,
  tickLabel, uncovered, windowOf, windowPct, windowTicks, type DialWindow,
} from './dial';
import { Dropdown, type DropOpt } from './Dropdown';
import { Knob } from './Knob';
import { profilePath, type Profile } from './scan';

const MODES: Mode[] = ['AM', 'USB', 'LSB', 'CW', 'AMN', 'FM'];
/** Button labels that differ from the Hamlib mode name. */
const MODE_LABEL: Partial<Record<Mode, string>> = { AMN: 'AM-N' };
const ALT = altName(isMac(globalThis.navigator?.userAgent ?? ''));
const METER_SEGS = 11;
/** Where the band dial looks before the rig reports a frequency. */
const IDLE_KHZ = 9500;
/** Width of a band label's character (12px monospace), for fitting them. */
const LABEL_CHAR_PX = 7.4;
/** A heat cell's width where the dial's window is no band (kHz). */
const HEAT_STEP_KHZ = 5;

/** The signal meter: the one part of the faceplate that follows every reading. */
function SMeter({ on, meter }: { on: boolean; meter: Meter }) {
  const t = useT();
  const db = useMeter(meter);
  const lit = meterSegments(on ? db : null);
  return (
    <div className="sm" aria-label={t.signalMeter}>
      <span className="lab">{t.signal}</span>
      <div className="segs">
        {Array.from({ length: METER_SEGS }, (_, i) => <i key={i} className={i < lit ? 'on' : ''} />)}
      </div>
      <span className="sval">{on ? sLabel(db) : '--'}</span>
    </div>
  );
}

/** dB relative to S9 -> lit segments (9 up to S9, one per 10 dB over). */
export function meterSegments(db: number | null): number {
  if (db === null) return 0;
  const s = db <= 0 ? 9 + db / 6 : 9 + db / 10;
  return Math.round(Math.min(METER_SEGS, Math.max(0, s)));
}

/** The readout: Hz in dot groups, padded to 000.000.000. */
export function fmtReadout(hz: number): string {
  return String(Math.round(hz)).padStart(9, '0').replace(/\B(?=(\d{3})+$)/g, '.');
}

/** Typed text -> Hz: the readout's 000.000.000 form is Hz, a plain or decimal number is kHz. */
export function parseReadout(s: string): number {
  const t = String(s).replace(/[\s  ]/g, '');
  if (/^\d{1,3}\.\d{3}\.\d{3}$/.test(t)) return Number(t.replace(/\./g, ''));
  const n = Number(t.replace(',', '.'));
  return t !== '' && Number.isFinite(n) ? n * 1000 : NaN;
}

/** Station marks: off-air ones dim, on-air ones lit (a separate svg, so the glow stays round). */
function Marks({ off, on }: { off: string; on: string }) {
  return (
    <>
      <svg className="dial-marks" viewBox={`0 0 ${MARKS_W} 1`} preserveAspectRatio="none" aria-hidden="true"><path className="off" d={off} /></svg>
      <svg className="dial-marks lit" viewBox={`0 0 ${MARKS_W} 1`} preserveAspectRatio="none" aria-hidden="true"><path className="on" d={on} /></svg>
    </>
  );
}

/** The band dial's window, kept while you stay inside it (its margins lie outside the band).
 *  `jump` steps to the next band up or down and shows it, even where it overlaps the one you
 *  were in (40 m and 41 m), and `go` does the same for any band; both return where to tune. */
export function useBandWindow(freqHz: number | null, bands: readonly Band[], onAirHz: readonly number[]) {
  const khz = freqHz === null ? IDLE_KHZ : freqHz / 1000;
  const last = useRef<DialWindow | null>(null);
  const pinned = useRef<DialWindow | null>(null);
  const known = (b: Band) => bands.some((x) => x.id === b.id && x.lo_khz === b.lo_khz && x.hi_khz === b.hi_khz);
  const q = pinned.current, p = last.current;
  let w: DialWindow;
  if (q?.band && khz >= q.band.lo_khz && khz <= q.band.hi_khz) {
    w = q;
    pinned.current = null;
  } else {
    w = p?.band && known(p.band) && khz >= p.lo && khz <= p.hi ? p : bandWindow(khz, bands);
  }
  last.current = w;
  const step = useCallback((dir: 1 | -1) => adjacentBand(khz, bands, dir, w.band), [khz, bands, w.band]);
  const go = useCallback((b: Band): number => {
    pinned.current = windowOf(b);
    return bandEntryHz(b, onAirHz);
  }, [onAirHz]);
  const jump = useCallback((dir: 1 | -1): number | null => {
    const b = step(dir);
    return b ? go(b) : null;
  }, [step, go]);
  return { w, khz, step, jump, go };
}

/** An element's width, followed as it resizes. */
function useWidth<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(([e]) => setWidth(e.contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, width] as const;
}

/** The faceplate tuned to an internet station. */
export interface NetFace {
  name: string;
  flag: string | null;
  country: string;
  /** `MP3 128k`. */
  stream: string;
  /** Audio held ahead; `null` while nobody listens. */
  bufferMs: number | null;
  /** What the buffer fills to before it plays (the automatic one grows). */
  bufferTargetMs: number;
  /** Most it holds before the oldest goes. */
  bufferMaxMs: number;
  /** The listener's buffer size, or `null` for automatic. */
  bufferSetting: number | null;
  /** Connecting, buffering, stopped: the faceplate's chip. */
  phase: NetPhase;
  onBuffer: (ms: number | null) => void;
  /** The NET favorites, in channel order. */
  channels: { uuid: string; label: string }[];
  /** The station tuned. */
  uuid: string;
  /** The rig, waiting: `9 610 kHz AM`, or '' when it is off. */
  rig: string;
  onChannel: (uuid: string) => void;
  onStep: (dir: number) => void;
  onBackToRig: () => void;
}

/** A frequency in kHz with its unit, for screen readers. */
const spoken = (khz: number) => (khz >= 1000 ? `${khz / 1000} MHz` : `${khz} kHz`);

interface Props {
  on: boolean;
  freqHz: number | null;
  mode: string | null;
  /** The S-meter; only the meter re-renders with it. */
  meter: Meter;
  /** Every frequency in the schedule, and those on the air now. */
  freqsHz: readonly number[];
  onAirHz: readonly number[];
  bands: readonly Band[];
  spans: readonly Span[];
  coverage: readonly RxRange[];
  /** From `useBandWindow`. */
  win: ReturnType<typeof useBandWindow>;
  /** Scan profiles to draw over the band dial: the latest (or running) scan and the one before. */
  profiles: { now: Profile | null; ghost: Profile | null };
  /** The channels listened to with a valid signal, for the heat strip under the band dial. */
  heat: readonly FreqHeat[];
  /** The scan controls (`ScanBar`). */
  scanBar: ReactNode;
  /** The favorite star for the tuned frequency. */
  star: ReactNode;
  /** Phones: the folded controls are showing. */
  expanded: boolean;
  onExpand: () => void;
  onTune: (hz: number) => void;
  onMode: (m: Mode) => void;
  /** Tuned to an internet station: the NET faceplate instead. */
  net: NetFace | null;
  /** The span switch's NET: to the internet stations (while internet radio is on). */
  onNetSpan: (() => void) | null;
}

export function Tuner({ on, freqHz, mode, meter, freqsHz, onAirHz, bands, spans, coverage, win, profiles, heat, scanBar, star, expanded, onExpand, onTune, onMode, net, onNetSpan }: Props) {
  const t = useT();
  const names = useNames();
  const shown = freqHz === null ? '' : fmtReadout(freqHz);
  const [text, setText] = useState(shown);
  const [editing, setEditing] = useState(false);
  useEffect(() => {
    if (!editing) setText(shown);
  }, [shown, editing]);

  const commit = () => {
    setEditing(false);
    const hz = parseReadout(text);
    if (Number.isNaN(hz)) setText(shown);
    else onTune(hz);
  };
  const nudge = (khz: number) => { if (on && freqHz !== null) onTune(freqHz + khz * 1000); };

  const { w, khz } = win;
  const dialKhz = Math.round(khz);

  // the whole dial shows the frequency's span, or another one picked to look at until you tune
  const [looking, setLooking] = useState<string | null>(null);
  useEffect(() => setLooking(null), [dialKhz]);
  const here = spanAt(khz, spans);
  const span = spans.find((s) => s.id === looking) ?? here;
  const away = span.id !== here.id;

  const marks = useMemo(() => markPaths(freqsHz, onAirHz, span.lo_khz, span.hi_khz), [freqsHz, onAirHz, span]);
  const bars = useMemo(() => bandBars(bands, span), [bands, span]);
  const gaps = useMemo(() => uncovered(coverage, span), [coverage, span]);
  const ticks = useMemo(() => dialTicks(span), [span]);
  const [dialRef, dialWidth] = useWidth<HTMLDivElement>();
  const labelled = useMemo(() => fitLabels(bars, dialWidth, LABEL_CHAR_PX), [bars, dialWidth]);

  const wMarks = useMemo(() => markPaths(freqsHz, onAirHz, w.lo, w.hi), [freqsHz, onAirHz, w.lo, w.hi]);
  const wTicks = useMemo(() => windowTicks(w), [w.lo, w.hi]);
  const prof = useMemo(() => ({
    now: profiles.now ? profilePath(profiles.now, w.lo, w.hi, true) : '',
    ghost: profiles.ghost ? profilePath(profiles.ghost, w.lo, w.hi) : '',
  }), [profiles, w.lo, w.hi]);
  const hasProf = prof.now !== '' || prof.ghost !== '';
  const heatStep = w.band ? w.band.scan_step_hz / 1000 : HEAT_STEP_KHZ;
  const cells = useMemo(() => heatCells(heat, w.lo, w.hi, heatStep), [heat, w.lo, w.hi, heatStep]);
  const prev = win.step(-1), next = win.step(1);
  const jump = (dir: 1 | -1) => {
    const hz = win.jump(dir);
    if (hz !== null) onTune(hz);
  };
  // every band, low to high; those the rig does not receive can't be picked
  const bandId = useId();
  const bandOpts = useMemo<DropOpt[]>(() => [...bands].sort((a, b) => a.lo_khz - b.lo_khz || a.hi_khz - b.hi_khz).map((b) => ({
    value: b.id,
    label: names.dialBand(b),
    hint: rangeLabel(b),
    cls: 'k-' + b.kind,
    disabled: !coverage.some((r) => r.lo_khz < b.hi_khz && r.hi_khz > b.lo_khz),
  })), [bands, coverage, names]);
  const pickBand = (id: string) => {
    const b = bands.find((x) => x.id === id);
    if (b) onTune(win.go(b));
  };
  const lens = !away && w.hi > span.lo_khz && w.lo < span.hi_khz;
  const pct = (k: number) => dialPct(k, span);
  const netSpan = onNetSpan && (
    <button type="button" role="radio" aria-checked={!!net} className={net ? 'here' : undefined} onClick={net ? undefined : onNetSpan}>{t.netBand}</button>
  );

  if (net) {
    const n = net.channels.length;
    const ph = net.phase;
    const phaseText = ph.kind === 'connecting' ? t.netConnectingShort
      : ph.kind === 'buffering' ? t.netBuffering(ph.pct)
      : ph.kind === 'failed' ? t.netStoppedShort
      : null;
    return (
      <section className="rx net" aria-label={t.receiver}>
        <div className="rx-main">
          <div className="rx-top">
            <div className="readout">
              <span className="freq net-name" title={net.name}>{net.name}</span>
              <span className="unit">{t.netBand}</span>
              {star}
            </div>
            <div className="net-chips" aria-label={t.netStream}>
              {net.stream && <span className="chip">{net.stream}</span>}
              <span className="chip"><Flag flag={net.flag} title={net.country} />{net.country}</span>
              {phaseText && <span className={'chip net-state ' + ph.kind}>{phaseText}</span>}
            </div>
            <button
              className="icon-btn dock-more"
              type="button"
              aria-expanded={expanded}
              aria-label={expanded ? t.lessControls : t.moreControls}
              title={expanded ? t.lessControls : t.moreControls}
              onClick={onExpand}
            >
              {expanded ? '⌄' : '⌃'}
            </button>
          </div>

          <div className="bdial">
            <div className="bdial-head">
              <button className="icon-btn" type="button" disabled={n === 0} aria-label={t.netPrevChannel} title={t.keyHint(t.netPrevChannel, t.keyPgDn)} onClick={() => net.onStep(-1)}>‹</button>
              <span className="sr" id={bandId}>{t.bandPick}</span>
              <Dropdown labelledBy={bandId} className="bdial-pick in net" value="" none={t.netBand} opts={bandOpts} onChange={pickBand} />
              <button className="icon-btn" type="button" disabled={n === 0} aria-label={t.netNextChannel} title={t.keyHint(t.netNextChannel, t.keyPgUp)} onClick={() => net.onStep(1)}>›</button>
            </div>
            <div className="bdial-scale net-channels" role="radiogroup" aria-label={t.netChannels}>
              {n === 0 ? <p className="net-ch-hint">{t.netChannelsHint}</p> : net.channels.map((c, i) => (
                <button
                  key={c.uuid}
                  type="button"
                  role="radio"
                  aria-checked={c.uuid === net.uuid}
                  className="net-ch"
                  style={{ left: ((i + 0.5) / n) * 100 + '%' }}
                  title={c.label}
                  onClick={() => net.onChannel(c.uuid)}
                >
                  {i + 1}
                </button>
              ))}
            </div>
          </div>

          <div className="dial-row rx-x">
            <div className="dial-side">
              <div className="sm" aria-label={t.netBufferMeter}>
                <span className="lab">{t.netBuffer}</span>
                <span className="sval">{net.bufferMs === null ? '--' : `${(net.bufferMs / 1000).toFixed(1)} s`}</span>
              </div>
              {spans.length > 1 && (
                <div className="seg spans" role="radiogroup" aria-label={t.spansLabel}>
                  {spans.map((s) => (
                    <button key={s.id} type="button" role="radio" aria-checked={false} onClick={net.onBackToRig}>
                      {t.spans[s.id as keyof typeof t.spans] ?? s.id.toUpperCase()}
                    </button>
                  ))}
                  {netSpan}
                </div>
              )}
            </div>
            <div className="dial net-funnel">
              <NetFunnel
                heldMs={net.bufferMs}
                targetMs={net.bufferTargetMs}
                maxMs={net.bufferMaxMs}
                setting={net.bufferSetting}
                flowing={ph.kind === 'playing'}
                onBuffer={net.onBuffer}
              />
              <p className="net-wait">
                {net.rig ? t.netRigWaiting(net.rig) : t.netRigIdle}
                <button className="link" type="button" onClick={net.onBackToRig}>⟲ {t.netBackToRig}</button>
              </p>
            </div>
          </div>
        </div>
        <Knob disabled={n === 0} hz={null} coverage={coverage} onTune={() => {}} onStep={(clicks) => net.onStep(clicks)} />
      </section>
    );
  }

  return (
    <section className={'rx' + (on ? '' : ' off')} aria-label={t.receiver}>
      <div className="rx-main">
        <div className="rx-top">
          <label className="sr" htmlFor="freq">{t.freqLabel}</label>
          <div className="readout">
            <input
              className="freq"
              id="freq"
              type="text"
              inputMode="decimal"
              autoComplete="off"
              spellCheck={false}
              disabled={!on}
              value={on ? text : '---.---.---'}
              onFocus={(e) => { setEditing(true); e.currentTarget.select(); }}
              onChange={(e) => setText(e.currentTarget.value)}
              onBlur={commit}
              onKeyDown={(e) => { if (e.key === 'Enter') e.currentTarget.blur(); }}
              onWheel={(e) => nudge(e.deltaY < 0 ? 1 : -1)}
            />
            <span className="unit">Hz</span>
            {star}
          </div>
          <div className="seg" role="radiogroup" aria-label={t.mode}>
            {MODES.map((m, i) => (
              <button key={m} type="button" role="radio" aria-checked={on && mode === m} disabled={!on} title={`${ALT}${i + 1}`} onClick={() => onMode(m)}>{MODE_LABEL[m] ?? m}</button>
            ))}
          </div>
          <button
            className="icon-btn dock-more"
            type="button"
            aria-expanded={expanded}
            aria-label={expanded ? t.lessControls : t.moreControls}
            title={expanded ? t.lessControls : t.moreControls}
            onClick={onExpand}
          >
            {expanded ? '⌄' : '⌃'}
          </button>
        </div>

        <div className="bdial">
          <div className="bdial-head">
            <button className="icon-btn" type="button" disabled={!on || !prev} aria-label={t.prevBand} title={t.keyHint(t.prevBand, t.keyPgDn)} onClick={() => jump(-1)}>‹</button>
            <span className="sr" id={bandId}>{t.bandPick}</span>
            <Dropdown
              labelledBy={bandId}
              className={'bdial-pick' + (w.band ? ' in k-' + w.band.kind : '')}
              value={w.band?.id ?? ''}
              none={t.bandOob}
              disabled={!on}
              opts={bandOpts}
              onChange={pickBand}
            />
            <button className="icon-btn" type="button" disabled={!on || !next} aria-label={t.nextBand} title={t.keyHint(t.nextBand, t.keyPgUp)} onClick={() => jump(1)}>›</button>
          </div>
          <div className={'bdial-scale' + (hasProf ? ' has-prof' : '') + (cells.length ? ' has-heat' : '')}>
            {hasProf && (
              <svg className="scan-prof" viewBox={`0 0 ${MARKS_W} 1`} preserveAspectRatio="none" role="img" aria-label={t.scanProfile}>
                <path className="ghost" d={prof.ghost} />
                <path className="now" d={prof.now} />
              </svg>
            )}
            <Marks off={wMarks.off} on={wMarks.on} />
            <div className="dial-scale" aria-hidden="true">
              {wTicks.minor.map((k) => <i key={k} className="tk" style={{ left: windowPct(k, w) + '%' }} />)}
              {wTicks.major.map((k, i) => (
                <span key={k}>
                  <i className="tk maj" style={{ left: windowPct(k, w) + '%' }} />
                  <span className={'tl' + (i % 2 ? ' alt2' : '') + (i % 4 ? ' alt4' : '')} style={{ left: windowPct(k, w) + '%' }}>{tickLabel(k)}</span>
                </span>
              ))}
            </div>
            {cells.length > 0 && (
              <svg className="listen-heat" viewBox={`0 0 ${MARKS_W} 1`} preserveAspectRatio="none" role="img" aria-label={t.listenHeat}>
                {cells.map((c) => <rect key={c.x} className={'h' + c.level} x={c.x} y={0} width={c.w} height={1} />)}
              </svg>
            )}
            <input
              className="tune"
              type="range"
              min={w.lo}
              max={w.hi}
              step={1}
              value={Math.min(w.hi, Math.max(w.lo, dialKhz))}
              disabled={!on}
              aria-label={t.bandDialLabel(spoken(w.lo), spoken(w.hi))}
              onChange={(e) => onTune(Number(e.currentTarget.value) * 1000)}
              onWheel={(e) => nudge((e.deltaY < 0 ? 1 : -1) * (e.shiftKey ? 5 : 1))}
            />
          </div>
        </div>

        {scanBar}

        <div className="dial-row rx-x">
          <div className="dial-side">
            <SMeter on={on} meter={meter} />
            {spans.length > 1 && (
              <div className="seg spans" role="radiogroup" aria-label={t.spansLabel}>
                {spans.map((s) => (
                  <button key={s.id} type="button" role="radio" aria-checked={s.id === span.id} className={s.id === here.id ? 'here' : undefined} onClick={() => setLooking(s.id === here.id ? null : s.id)}>
                    {t.spans[s.id as keyof typeof t.spans] ?? s.id.toUpperCase()}
                  </button>
                ))}
                {netSpan}
              </div>
            )}
          </div>
          <div className={'dial' + (away ? ' away' : '')} ref={dialRef}>
            <Marks off={marks.off} on={marks.on} />
            {gaps.map((g) => <div key={g.left} className="dial-gap" aria-hidden="true" style={{ left: g.left + '%', width: g.width + '%' }} />)}
            <div className="band-bars" aria-hidden="true">
              {bars.map((b) => (
                <span key={b.id} className={`band-bar lane${b.lane} k-${b.kind}`} style={{ left: b.left + '%', width: b.width + '%' }}>
                  {labelled.has(b.id) && <span className="band-lab">{b.label}</span>}
                </span>
              ))}
            </div>
            {lens && <div className="lens" aria-hidden="true" style={{ left: pct(Math.max(w.lo, span.lo_khz)) + '%', width: pct(Math.min(w.hi, span.hi_khz)) - pct(Math.max(w.lo, span.lo_khz)) + '%' }} />}
            <div className="dial-scale" aria-hidden="true">
              {ticks.map((k) => (
                <span key={k.khz}>
                  <i className={'tk' + (k.major ? ' maj' : '')} style={{ left: pct(k.khz) + '%' }} />
                  {k.label && (
                    <span className="tl" style={{ left: pct(k.khz) + '%', transform: k.end ? 'translateX(-100%)' : undefined }}>{k.label}</span>
                  )}
                </span>
              ))}
            </div>
            <input
              className="tune"
              type="range"
              min={span.lo_khz}
              max={span.hi_khz}
              step={1}
              value={Math.min(span.hi_khz, Math.max(span.lo_khz, dialKhz))}
              disabled={!on}
              aria-label={t.dialLabel(spoken(span.lo_khz), spoken(span.hi_khz))}
              onChange={(e) => onTune(Number(e.currentTarget.value) * 1000)}
            />
          </div>
        </div>
      </div>
      <Knob disabled={!on || freqHz === null} hz={freqHz} coverage={coverage} onTune={onTune} />
    </section>
  );
}
