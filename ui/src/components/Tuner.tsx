// The receiver faceplate: readout, modes and steps; the band dial (a zoom on the band you are
// in, kHz by kHz); and the S-meter beside the whole 1.7–30 MHz dial, which shows the bands and a
// lens over where the band dial looks. `rx-x` parts fold away on small screens until expanded.

import { useMemo, useRef, useState, useEffect } from 'react';
import { useNames } from '../catalog/names';
import { sLabel, useT } from '../i18n';
import { altName, isMac } from '../keys/keymap';
import type { Band } from '../types/generated/Band';
import type { Mode } from '../types/generated/Mode';
import {
  adjacentBand, bandBars, bandEntryHz, bandWindow, DIAL_MAX_KHZ, DIAL_MIN_KHZ, dialPct, dialTicks, markPaths, MARKS_W,
  windowPct, windowTicks, type DialWindow,
} from './dial';

const MODES: Mode[] = ['AM', 'USB', 'LSB', 'CW'];
const STEPS_KHZ = [-5, -1, 1, 5];
const ALT = altName(isMac(globalThis.navigator?.userAgent ?? ''));
const METER_SEGS = 11;
/** Where the band dial looks before the rig reports a frequency. */
const IDLE_KHZ = 9500;

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

/** The band window, kept while you stay inside it (its margins lie outside the band). */
function useWindow(khz: number, bands: readonly Band[]): DialWindow {
  const last = useRef<DialWindow | null>(null);
  const p = last.current;
  const w = p?.band && khz >= p.lo && khz <= p.hi ? p : bandWindow(khz, bands);
  last.current = w;
  return w;
}

interface Props {
  on: boolean;
  freqHz: number | null;
  mode: string | null;
  strengthDb: number | null;
  /** Every frequency in the schedule, and those on the air now. */
  freqsHz: readonly number[];
  onAirHz: readonly number[];
  bands: readonly Band[];
  /** Phones: the folded controls are showing. */
  expanded: boolean;
  onExpand: () => void;
  onTune: (hz: number) => void;
  onMode: (m: Mode) => void;
}

export function Tuner({ on, freqHz, mode, strengthDb, freqsHz, onAirHz, bands, expanded, onExpand, onTune, onMode }: Props) {
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

  const lit = meterSegments(on ? strengthDb : null);
  const khz = freqHz === null ? IDLE_KHZ : freqHz / 1000;
  const marks = useMemo(() => markPaths(freqsHz, onAirHz), [freqsHz, onAirHz]);
  const bars = useMemo(() => bandBars(bands), [bands]);
  const dialKhz = Math.round(khz);

  const w = useWindow(khz, bands);
  const wMarks = useMemo(() => markPaths(freqsHz, onAirHz, w.lo, w.hi), [freqsHz, onAirHz, w.lo, w.hi]);
  const wTicks = useMemo(() => windowTicks(w), [w.lo, w.hi]);
  const prev = adjacentBand(khz, bands, -1), next = adjacentBand(khz, bands, 1);

  return (
    <section className={'rx' + (on ? '' : ' off')} aria-label={t.receiver}>
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
        </div>
        <div className="seg" role="radiogroup" aria-label={t.mode}>
          {MODES.map((m, i) => (
            <button key={m} type="button" role="radio" aria-checked={on && mode === m} disabled={!on} title={`${ALT}${i + 1}`} onClick={() => onMode(m)}>{m}</button>
          ))}
        </div>
        <div className="steps">
          {STEPS_KHZ.map((s) => (
            <button key={s} type="button" className={Math.abs(s) > 1 ? 'rx-x' : undefined} disabled={!on || freqHz === null} title={(Math.abs(s) > 1 ? 'Shift+' : '') + (s > 0 ? '→' : '←')} onClick={() => nudge(s)}>
              {s > 0 ? '+' + s : '−' + -s}
            </button>
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
          <button className="icon-btn" type="button" disabled={!on || !prev} aria-label={t.prevBand} title={t.keyHint(t.prevBand, t.keyPgDn)} onClick={() => prev && onTune(bandEntryHz(prev, onAirHz))}>‹</button>
          <span className={'bdial-name' + (w.band ? ' in' : '')}>
            <b>{w.band ? names.band(w.band.id) : t.bandOob}</b>
            {w.band && <span className="bdial-range">{w.band.lo_khz}–{w.band.hi_khz}</span>}
          </span>
          <button className="icon-btn" type="button" disabled={!on || !next} aria-label={t.nextBand} title={t.keyHint(t.nextBand, t.keyPgUp)} onClick={() => next && onTune(bandEntryHz(next, onAirHz))}>›</button>
        </div>
        <div className="bdial-scale">
          <Marks off={wMarks.off} on={wMarks.on} />
          <div className="dial-scale" aria-hidden="true">
            {wTicks.minor.map((k) => <i key={k} className="tk" style={{ left: windowPct(k, w) + '%' }} />)}
            {wTicks.major.map((k, i) => (
              <span key={k}>
                <i className="tk maj" style={{ left: windowPct(k, w) + '%' }} />
                <span className={'tl' + (i % 2 ? ' alt2' : '') + (i % 4 ? ' alt4' : '')} style={{ left: windowPct(k, w) + '%' }}>{k}</span>
              </span>
            ))}
          </div>
          <input
            className="tune"
            type="range"
            min={w.lo}
            max={w.hi}
            step={1}
            value={Math.min(w.hi, Math.max(w.lo, dialKhz))}
            disabled={!on}
            aria-label={t.bandDialLabel(String(w.lo), String(w.hi))}
            onChange={(e) => onTune(Number(e.currentTarget.value) * 1000)}
            onWheel={(e) => nudge((e.deltaY < 0 ? 1 : -1) * (e.shiftKey ? 5 : 1))}
          />
        </div>
      </div>

      <div className="dial-row rx-x">
        <div className="sm" aria-label={t.signalMeter}>
          <span className="lab">{t.signal}</span>
          <div className="segs">
            {Array.from({ length: METER_SEGS }, (_, i) => <i key={i} className={i < lit ? 'on' : ''} />)}
          </div>
          <span className="sval">{on ? sLabel(strengthDb) : '--'}</span>
        </div>
        <div className="dial">
          <Marks off={marks.off} on={marks.on} />
          <div className="band-bars" aria-hidden="true">
            {bars.map((b) => (
              <span key={b.id} className="band-bar" style={{ left: b.left + '%', width: b.width + '%' }}>
                <span className="band-lab">{b.label}</span>
              </span>
            ))}
          </div>
          <div className="lens" aria-hidden="true" style={{ left: dialPct(w.lo) + '%', width: dialPct(w.hi) - dialPct(w.lo) + '%' }} />
          <div className="dial-scale" aria-hidden="true">
            {dialTicks().map((m) => (
              <span key={m}>
                <i className={'tk' + (m % 5 === 0 ? ' maj' : '')} style={{ left: dialPct(m * 1000) + '%' }} />
                {m % 5 === 0 && (
                  <span className="tl" style={{ left: dialPct(m * 1000) + '%', transform: m === 30 ? 'translateX(-100%)' : undefined }}>
                    {m === 5 ? '5 MHz' : m}
                  </span>
                )}
              </span>
            ))}
          </div>
          <input
            className="tune"
            type="range"
            min={DIAL_MIN_KHZ}
            max={DIAL_MAX_KHZ}
            step={1}
            value={Math.min(DIAL_MAX_KHZ, Math.max(DIAL_MIN_KHZ, dialKhz))}
            disabled={!on}
            aria-label={t.dialLabel}
            onChange={(e) => onTune(Number(e.currentTarget.value) * 1000)}
          />
        </div>
      </div>
    </section>
  );
}
