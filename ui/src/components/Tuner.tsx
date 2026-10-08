// Readout, modes, S-meter, dial with station marks, and step buttons.

import { useEffect, useState } from 'react';
import { fmtKhz, sLabel, useT } from '../i18n';
import type { Candidate } from '../types/generated/Candidate';
import type { Mode } from '../types/generated/Mode';

const MODES: Mode[] = ['AM', 'USB', 'LSB', 'CW'];
const STEPS_KHZ = [-5, -1, 1, 5];
const DIAL_MIN_KHZ = 3000, DIAL_MAX_KHZ = 30000;
const METER_SEGS = 11;

/** dB relative to S9 -> lit segments (9 up to S9, one per 10 dB over). */
export function meterSegments(db: number | null): number {
  if (db === null) return 0;
  const s = db <= 0 ? 9 + db / 6 : 9 + db / 10;
  return Math.round(Math.min(METER_SEGS, Math.max(0, s)));
}

export function parseKhz(s: string): number {
  const n = Number(String(s).replace(/[\s  ]/g, '').replace(',', '.'));
  return Number.isFinite(n) ? n : NaN;
}

interface Props {
  on: boolean;
  freqHz: number | null;
  mode: string | null;
  strengthDb: number | null;
  stations: Candidate[];
  onTune: (hz: number) => void;
  onMode: (m: Mode) => void;
}

export function Tuner({ on, freqHz, mode, strengthDb, stations, onTune, onMode }: Props) {
  const t = useT();
  const shown = freqHz === null ? '' : fmtKhz(freqHz);
  const [text, setText] = useState(shown);
  const [editing, setEditing] = useState(false);
  useEffect(() => {
    if (!editing) setText(shown);
  }, [shown, editing]);

  const commit = () => {
    setEditing(false);
    const khz = parseKhz(text);
    if (Number.isNaN(khz)) setText(shown);
    else onTune(khz * 1000);
  };

  const lit = meterSegments(on ? strengthDb : null);
  const marks = [...new Map(stations.map((c) => [c.station.freq_hz, c])).keys()];
  const onAirAt = (hz: number) => stations.some((c) => c.station.freq_hz === hz && c.air.on);
  const dialKhz = freqHz === null ? DIAL_MIN_KHZ : Math.round(freqHz / 1000);
  const pct = (khz: number) => ((khz - DIAL_MIN_KHZ) / (DIAL_MAX_KHZ - DIAL_MIN_KHZ)) * 100;

  return (
    <section className={'rx sec' + (on ? '' : ' off')} aria-label={t.receiver}>
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
          value={on ? text : '----.-'}
          onFocus={(e) => { setEditing(true); e.currentTarget.select(); }}
          onChange={(e) => setText(e.currentTarget.value)}
          onBlur={commit}
          onKeyDown={(e) => { if (e.key === 'Enter') e.currentTarget.blur(); }}
          onWheel={(e) => { if (on && freqHz !== null) onTune(freqHz + (e.deltaY < 0 ? 1000 : -1000)); }}
        />
        <span className="unit">kHz</span>
      </div>
      <div className="rx-row">
        <div className="seg" role="radiogroup" aria-label={t.mode}>
          {MODES.map((m) => (
            <button key={m} type="button" role="radio" aria-checked={on && mode === m} disabled={!on} onClick={() => onMode(m)}>{m}</button>
          ))}
        </div>
        <div className="sm" aria-label={t.signalMeter}>
          <span className="lab">{t.signal}</span>
          <div className="segs">
            {Array.from({ length: METER_SEGS }, (_, i) => <i key={i} className={i < lit ? 'on' : ''} />)}
          </div>
          <span className="sval">{on ? sLabel(strengthDb) : '--'}</span>
        </div>
      </div>
      <div className="dial">
        <div className="dial-marks">
          {marks.map((hz) => <i key={hz} className={onAirAt(hz) ? 'on' : ''} style={{ left: pct(hz / 1000) + '%' }} />)}
        </div>
        <div className="dial-scale">
          {Array.from({ length: 28 }, (_, k) => k + 3).map((m) => (
            <span key={m}>
              <i className={'tk' + (m % 5 === 0 ? ' maj' : '')} style={{ left: pct(m * 1000) + '%' }} />
              {m % 5 === 0 && (
                <span className="tl" style={{ left: pct(m * 1000) + '%', transform: m === 30 ? 'translateX(-100%)' : undefined }}>
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
      <div className="steps">
        {STEPS_KHZ.map((s) => (
          <button key={s} type="button" disabled={!on || freqHz === null} onClick={() => freqHz !== null && onTune(freqHz + s * 1000)}>
            {s > 0 ? '+' + s : '−' + -s}
          </button>
        ))}
      </div>
    </section>
  );
}
