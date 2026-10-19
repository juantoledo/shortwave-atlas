// The faceplate's frequency readout: type a frequency into it, or scroll over a digit to tune by
// that digit (with carry, like a rig's: 009.999.000 up one in the 1 kHz place is 010.000.000).
// While you are not typing, a row of digits lies over the input, one hit box each; pressing it
// starts typing. Within one wheel gesture it steps from where it last tuned (as the knob does).

import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import type { RxRange } from '../types/generated/RxRange';
import { clampToCoverage } from './dial';
import { wheelDetents } from './detents';

/** A wheel gesture ends after this long without a detent. */
const IDLE_MS = 250;

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

/** Each character of a readout -> the Hz its digit is worth, or null for a dot. */
export function readoutPlaces(s: string): (number | null)[] {
  let p = s.replace(/\D/g, '').length;
  return [...s].map((c) => (/\d/.test(c) ? 10 ** --p : null));
}

interface Props {
  on: boolean;
  freqHz: number | null;
  /** What the rig receives: the wheel stops at its edges. */
  coverage: readonly RxRange[];
  onTune: (hz: number) => void;
}

export function Readout({ on, freqHz, coverage, onTune }: Props) {
  const t = useT();
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

  const input = useRef<HTMLInputElement>(null);
  const digits = useRef<HTMLSpanElement>(null);
  const live = useRef({ freqHz, coverage, onTune });
  live.current = { freqHz, coverage, onTune };
  const show = on && !editing && freqHz !== null;

  // the wheel tunes by the digit under it (a listener of our own: React's is passive)
  useEffect(() => {
    const node = digits.current;
    if (!node) return;
    let acc = 0;
    let target: number | null = null;
    let idle: ReturnType<typeof setTimeout> | undefined;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const place = Number((e.target as HTMLElement).dataset?.p);
      if (!place) return;
      const r = wheelDetents(acc, e.deltaY, e.deltaMode);
      acc = r.rest;
      const { freqHz: hz, coverage: cov, onTune: tune } = live.current;
      const from = target ?? hz;
      if (r.clicks === 0 || from === null) return;
      target = clampToCoverage(Math.max(0, from + r.clicks * place), cov);
      tune(target);
      clearTimeout(idle);
      idle = setTimeout(() => { target = null; }, IDLE_MS);
    };
    node.addEventListener('wheel', onWheel, { passive: false });
    return () => {
      node.removeEventListener('wheel', onWheel);
      clearTimeout(idle);
    };
  }, [show]);

  const places = readoutPlaces(shown);
  return (
    <span className={'freq-wrap' + (show ? ' show' : '')}>
      <input
        ref={input}
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
      />
      {show && (
        <span
          ref={digits}
          className="freq freq-digits"
          aria-hidden="true"
          title={t.freqWheelHint}
          onPointerDown={(e) => {
            if (e.button !== 0) return;
            e.preventDefault();
            input.current?.focus();
          }}
        >
          {[...shown].map((c, i) => <span key={i} data-p={places[i] ?? undefined}>{c}</span>)}
        </span>
      )}
    </span>
  );
}
