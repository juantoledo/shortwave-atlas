// The phone remote's pieces (Remote.tsx lays them out): the readout and S-meter, the station card,
// the tuning bar and its drag strip, the scan button, the keypad, the lists and the Listen bar.

import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent, type ReactNode } from 'react';
import type { PlayerStatus } from '../audio/player';
import { useNames } from '../catalog/names';
import { Flag } from '../components/Flag';
import { MODES } from '../components/favorites';
import { bufferPct, streamLabel } from '../components/net';
import { parseReadout } from '../components/Readout';
import { meterSegments } from '../components/Tuner';
import { useMeter, type Meter } from '../hooks/meter';
import { fmtKhz, sLabel, useT, type Messages } from '../i18n';
import type { Band } from '../types/generated/Band';
import type { Candidate } from '../types/generated/Candidate';
import type { NetNow } from '../types/generated/NetNow';
import type { NetStation } from '../types/generated/NetStation';
import type { ScanStatus } from '../types/generated/ScanStatus';
import type { StationRow } from '../types/generated/StationRow';
import { candLines, dragSteps, keypadPress, scanPct, STEPS_KHZ } from './logic';
import type { Listen } from './useListen';

const METER_SEGS = 11;

export function Readout({ on, freqHz, mode, onKeypad, onMode }: {
  on: boolean;
  freqHz: number | null;
  mode: string | null;
  onKeypad: () => void;
  onMode: (m: (typeof MODES)[number]) => void;
}) {
  const t = useT();
  // the rig may be in a mode the UI doesn't set (PKTUSB): shown, not offered
  const modes: string[] = mode && !MODES.includes(mode as never) ? [mode, ...MODES] : [...MODES];
  return (
    <div className="rm-readout">
      <button className="rm-freq" type="button" disabled={!on} onClick={onKeypad} aria-label={`${t.freqLabel}: ${freqHz === null ? '' : fmtKhz(freqHz)}`}>
        {on && freqHz !== null ? fmtKhz(freqHz) : '-----.-'}
        <small>kHz</small>
      </button>
      <label className="rm-mode">
        <span className="sr">{t.mode}</span>
        <select value={mode ?? ''} disabled={!on} onChange={(e) => onMode(e.target.value as (typeof MODES)[number])}>
          {!mode && <option value="">--</option>}
          {modes.map((m) => <option key={m} value={m} disabled={!MODES.includes(m as never)}>{m}</option>)}
        </select>
      </label>
    </div>
  );
}

export function SMeter({ meter, on }: { meter: Meter; on: boolean }) {
  const db = useMeter(meter);
  const lit = on ? meterSegments(db) : 0;
  return (
    <div className="rm-meter" aria-label={`S ${on ? sLabel(db) : '--'}`}>
      <span className="rm-segs">
        {Array.from({ length: METER_SEGS }, (_, i) => <i key={i} className={i < lit ? (i >= 9 ? 'on over' : 'on') : ''} />)}
      </span>
      <span className="rm-sval">{on ? sLabel(db) : '--'}</span>
    </div>
  );
}

export function Star({ on, onToggle }: { on: boolean; onToggle: () => void }) {
  const t = useT();
  return (
    <button className="rm-star" type="button" aria-pressed={on} aria-label={on ? t.favRemove : t.favAdd} onClick={onToggle}>
      {on ? '★' : '☆'}
    </button>
  );
}

export function NowCard({ cands, sel, star, onChoose }: {
  cands: readonly Candidate[];
  sel: Candidate;
  star: ReactNode;
  onChoose: (c: Candidate) => void;
}) {
  const t = useT();
  const names = useNames();
  const { what, where } = candLines(t, names, sel);
  const i = Math.max(0, cands.indexOf(sel));
  const step = (d: number) => onChoose(cands[(i + d + cands.length) % cands.length]);
  return (
    <div className="rm-card">
      <div className="rm-card-head">
        <Flag flag={sel.station.flag} title={names.country(sel.station.itu)} />
        <b className="rm-name">{sel.station.name}</b>
        {sel.air.on && <span className="rm-onair">{t.onAir}</span>}
        {star}
      </div>
      <p>{what}</p>
      <p className="rm-dim">{where}</p>
      {cands.length > 1 && (
        <div className="rm-cands">
          <button type="button" aria-label={t.remotePrevCand} onClick={() => step(-1)}>‹</button>
          <span>{t.remoteCand(i + 1, cands.length)}</span>
          <button type="button" aria-label={t.remoteNextCand} onClick={() => step(1)}>›</button>
        </div>
      )}
    </div>
  );
}

/** What the internet station's stream is doing, if it isn't simply playing. */
function netLine(t: Messages, n: NetNow): string {
  switch (n.state.state) {
    case 'tuned': return t.netStateTuned;
    case 'playing': return '';
    case 'connecting': return t.netConnecting;
    case 'buffering': return t.netBuffering(bufferPct(n));
    case 'failed': return t.netFailed(n.state.message);
  }
}

export function NetCard({ net, country, star, listening, channels, onStep, onBack }: {
  net: NetNow;
  country: string;
  star: ReactNode;
  listening: boolean;
  /** There are favorites to step through. */
  channels: boolean;
  onStep: (dir: number) => void;
  onBack: () => void;
}) {
  const t = useT();
  const line = net.state.state === 'tuned' && listening ? '' : netLine(t, net);
  return (
    <div className="rm-card net">
      <div className="rm-card-head">
        <Flag flag={net.station.flag} title={country} />
        <b className="rm-name">{net.station.name}</b>
        <span className="rm-onair net">{t.netBand}</span>
        {star}
      </div>
      <p>{[country, streamLabel(net.station)].filter(Boolean).join(' · ')}</p>
      {line && <p className="rm-dim">{line}</p>}
      <div className="rm-cands">
        <button type="button" disabled={!channels} aria-label={t.remotePrevChannel} onClick={() => onStep(-1)}>‹</button>
        <button type="button" className="rm-back" onClick={onBack}>{t.netBackToRig}</button>
        <button type="button" disabled={!channels} aria-label={t.remoteNextChannel} onClick={() => onStep(1)}>›</button>
      </div>
    </div>
  );
}

/** One row: the band arrows at the ends, the kHz steps, and the drag strip in the middle (it keeps
 *  the finger once it has it, so it can be narrow). */
export function TuneBar({ on, freqHz, stepHz, band, onTune, onBand }: {
  on: boolean;
  freqHz: number | null;
  /** One step of the drag strip. */
  stepHz: number;
  band: string;
  onTune: (hz: number) => void;
  onBand: (dir: 1 | -1) => void;
}) {
  const t = useT();
  const off = !on || freqHz === null;
  const step = (k: number) => (
    <button key={k} type="button" className="rm-step" disabled={off} aria-label={`${k > 0 ? '+' : '−'}${Math.abs(k)} kHz`} onClick={() => freqHz !== null && onTune(freqHz + k * 1000)}>
      {k > 0 ? '+' : '−'}{Math.abs(k)}
    </button>
  );
  return (
    <div className="rm-tune">
      <button type="button" className="rm-bandbtn" disabled={off} aria-label={t.prevBand} onClick={() => onBand(-1)}>◀</button>
      {STEPS_KHZ.filter((k) => k < 0).map(step)}
      <DragStrip disabled={off} freqHz={freqHz} stepHz={stepHz} label={band} onTune={onTune} />
      {STEPS_KHZ.filter((k) => k > 0).map(step)}
      <button type="button" className="rm-bandbtn" disabled={off} aria-label={t.nextBand} onClick={() => onBand(1)}>▶</button>
    </div>
  );
}

/** Drag sideways to tune, a step every few px. The scale moves with the finger under a fixed
 *  pointer, so dragging left tunes up. The arrow keys step too (right or up is up). */
function DragStrip({ disabled, freqHz, stepHz, label, onTune }: {
  disabled: boolean;
  freqHz: number | null;
  stepHz: number;
  label: string;
  onTune: (hz: number) => void;
}) {
  const t = useT();
  const drag = useRef<{ x: number; hz: number; steps: number } | null>(null);
  const [shift, setShift] = useState(0);
  const down = (e: PointerEvent<HTMLDivElement>) => {
    if (disabled || freqHz === null) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { x: e.clientX, hz: freqHz, steps: 0 };
  };
  const move = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const dx = e.clientX - d.x;
    setShift(dx);
    const steps = dragSteps(dx);
    if (steps !== d.steps) {
      d.steps = steps;
      onTune(d.hz - steps * stepHz);
    }
  };
  const up = () => {
    drag.current = null;
    setShift(0);
  };
  const key = (e: KeyboardEvent<HTMLDivElement>) => {
    const dir = e.key === 'ArrowRight' || e.key === 'ArrowUp' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowDown' ? -1 : 0;
    if (!dir || disabled || freqHz === null) return;
    e.preventDefault();
    onTune(freqHz + dir * stepHz);
  };
  return (
    <div
      className="rm-drag"
      role="slider"
      tabIndex={disabled ? -1 : 0}
      aria-label={t.remoteDrag}
      aria-valuenow={freqHz ?? undefined}
      aria-valuetext={freqHz === null ? undefined : `${fmtKhz(freqHz)} kHz`}
      aria-disabled={disabled}
      data-dragging={drag.current ? '' : undefined}
      style={{ backgroundPositionX: `${shift}px` }}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
      onKeyDown={key}
    >
      <span>{label || t.remoteDrag}</span>
    </div>
  );
}

/** Scan the band you are in (its usual step, normal speed); while one runs, Stop with how far it
 *  has got. Tuning stops it too (the core does). */
export function ScanButton({ on, band, status, onStart, onStop }: {
  on: boolean;
  band: Band | null;
  status: ScanStatus;
  onStart: (b: Band) => void;
  onStop: () => void;
}) {
  const t = useT();
  const names = useNames();
  if (status.state === 'running') {
    const pct = scanPct(status);
    return (
      <button type="button" className="rm-scan" data-running="" title={t.scanStopTitle} aria-label={t.scanStop(pct)} onClick={onStop}>
        <span className="rm-scan-bar" style={{ height: `${pct}%` }} aria-hidden="true" />
        <b aria-hidden="true">■</b>
        <span>{t.scanStop(pct)}</span>
      </button>
    );
  }
  const name = band ? names.dialBand(band) : '';
  return (
    <button type="button" className="rm-scan" disabled={!on || !band} title={band ? t.scanTitle(name) : t.scanNoBand} aria-label={band ? t.remoteScan(name) : t.scan} onClick={() => band && onStart(band)}>
      <b aria-hidden="true">◎</b>
      <span>{t.scan}</span>
      {band && <small>{name}</small>}
    </button>
  );
}

/** A frequency typed in kHz, in a sheet over the page. */
export function Keypad({ onTune, onClose }: { onTune: (hz: number) => void; onClose: () => void }) {
  const t = useT();
  const [text, setText] = useState('');
  const hz = parseReadout(text);
  const ok = Number.isFinite(hz) && hz > 0;
  const press = (k: string) => setText((s) => keypadPress(s, k));
  const go = () => {
    if (!ok) return;
    onTune(hz);
    onClose();
  };
  const goRef = useRef(go);
  goRef.current = go;
  // a hardware keyboard types too
  useEffect(() => {
    const kd = (e: globalThis.KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
      else if (e.key === 'Enter') goRef.current();
      else if (e.key === 'Backspace') press('back');
      else if (/^[\d.,]$/.test(e.key)) press(e.key === ',' ? '.' : e.key);
      else return;
      e.preventDefault();
    };
    document.addEventListener('keydown', kd);
    return () => document.removeEventListener('keydown', kd);
  }, [onClose]);
  return (
    <div className="rm-scrim" onClick={onClose}>
      <section className="rm-keypad" role="dialog" aria-modal="true" aria-label={t.remoteKeypad} onClick={(e) => e.stopPropagation()}>
        <p className="rm-dim">{t.remoteKeypad}</p>
        <output className="rm-typed">{text || ' '}<small>kHz</small></output>
        <div className="rm-keys">
          {['1', '2', '3', '4', '5', '6', '7', '8', '9', '.', '0', 'back'].map((k) => (
            <button key={k} type="button" onClick={() => press(k)} aria-label={k === 'back' ? '⌫' : k}>{k === 'back' ? '⌫' : k}</button>
          ))}
        </div>
        <div className="rm-actions">
          <button type="button" onClick={onClose}>{t.cancel}</button>
          <button type="button" className="primary" disabled={!ok} onClick={go}>{t.remoteTune}</button>
        </div>
      </section>
    </div>
  );
}

/** A tappable row of a list: frequency (or a tag), flag, name and a line under it; on the right
 *  the S reading a scan heard (`sval`), else whether it is on the air. */
export function Row({ khz, flag, country, name, sub, on, sval, current, onPick }: {
  khz: string;
  flag: string | null;
  country: string;
  name: string;
  sub: string;
  on?: boolean;
  sval?: string;
  current?: boolean;
  onPick: () => void;
}) {
  return (
    <li>
      <button type="button" className="rm-row" data-on={on ? '' : undefined} aria-current={current || undefined} onClick={onPick}>
        <span className="rm-khz">{khz}</span>
        <span className="rm-row-text">
          <span><Flag flag={flag} title={country} />{name}</span>
          <small>{sub}</small>
        </span>
        {sval !== undefined ? <span className="rm-rowsval">{sval}</span> : on && <i className="rm-dot" />}
      </button>
    </li>
  );
}

export function StationRows({ rows, freqHz, onPick }: { rows: readonly StationRow[]; freqHz: number | null; onPick: (r: StationRow) => void }) {
  const names = useNames();
  return (
    <>
      {rows.map((r) => (
        <Row
          key={r.id}
          khz={fmtKhz(r.freq_hz)}
          flag={r.flag}
          country={names.country(r.itu)}
          name={r.name}
          sub={[names.lang(r.lang), r.site_name].filter(Boolean).join(' · ')}
          on={r.on}
          current={r.freq_hz === freqHz}
          onPick={() => onPick(r)}
        />
      ))}
    </>
  );
}

export function NetRows({ items, playing, country, onPick }: { items: readonly NetStation[]; playing: string | null; country: (itu: string | null) => string; onPick: (uuid: string) => void }) {
  const t = useT();
  return (
    <>
      {items.map((s) => (
        <Row
          key={s.uuid}
          khz={t.netBand}
          flag={s.flag}
          country={country(s.itu)}
          name={s.name}
          sub={[country(s.itu), streamLabel(s)].filter(Boolean).join(' · ')}
          current={s.uuid === playing}
          onPick={() => onPick(s.uuid)}
        />
      ))}
    </>
  );
}

function statusText(t: Messages, s: PlayerStatus | null): string {
  if (!s) return '';
  switch (s.kind) {
    case 'connecting': return t.connecting;
    case 'playing': return t.audioPlaying(s.bufferMs);
    case 'ended': return t.audioEnded;
    case 'error': return t.audioError(s.message);
  }
}

export function ListenBar({ listen, can }: { listen: Listen; can: boolean }) {
  const t = useT();
  return (
    <div className="rm-listen">
      <button type="button" className="rm-play" aria-pressed={listen.playing} disabled={!listen.playing && !can} onClick={listen.toggle}>
        <span aria-hidden="true">{listen.playing ? '■' : '▶'}</span> {t.listen}
      </button>
      <div className="rm-listen-side">
        <label className="rm-vol">
          <span>{t.volume}</span>
          <input type="range" min={0} max={1} step={0.05} value={listen.volume} onChange={(e) => listen.setVolume(Number(e.target.value))} />
        </label>
        <small className="rm-dim">{listen.playing ? statusText(t, listen.status) : can ? '' : t.remoteListenOff}</small>
      </div>
    </div>
  );
}
