// The dual tuning knob at the right of the faceplate: the outer ring tunes 5 kHz a detent, the
// inner knob 1 kHz, each landing on its own grid (odd hertz from the rig's knob go first).
// Drag round it, scroll over it, or focus a ring and use the arrows. Within one gesture it steps
// from where it last tuned, so detents that arrive faster than the frequency comes back are not lost.

import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent, type WheelEvent } from 'react';
import { useT } from '../i18n';
import type { RxRange } from '../types/generated/RxRange';
import { clampToCoverage } from './dial';
import { angleDelta, COARSE_HZ, DETENT_DEG, detents, FINE_HZ, pointerDeg, stepHz, wheelDetents } from './detents';

type Ring = 'outer' | 'inner';
const STEP_HZ: Record<Ring, number> = { outer: COARSE_HZ, inner: FINE_HZ };
/** A wheel or key gesture ends after this long without a detent. */
const IDLE_MS = 250;
/** Closer than this to the centre the angle is noise. */
const DEAD_PX = 6;

interface Props {
  disabled: boolean;
  hz: number | null;
  /** What the rig receives: the knob stops at its edges. */
  coverage: readonly RxRange[];
  onTune: (hz: number) => void;
}

export function Knob({ disabled, hz, coverage, onTune }: Props) {
  const t = useT();
  const root = useRef<HTMLDivElement>(null);
  const [deg, setDeg] = useState<Record<Ring, number>>({ outer: 0, inner: 0 });
  /** Where this gesture last tuned. */
  const target = useRef<number | null>(null);
  const drag = useRef<{ ring: Ring; id: number; deg: number; acc: number } | null>(null);
  const wheel = useRef(0);
  const idle = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(idle.current), []);

  const end = () => {
    clearTimeout(idle.current);
    target.current = null;
  };

  const turn = (ring: Ring, clicks: number) => {
    if (disabled || clicks === 0) return;
    const from = target.current ?? hz;
    if (from === null) return;
    const to = clampToCoverage(stepHz(from, clicks, STEP_HZ[ring]), coverage);
    target.current = to;
    onTune(to);
    setDeg((d) => ({ ...d, [ring]: d[ring] + clicks * DETENT_DEG }));
    if (!drag.current) {
      clearTimeout(idle.current);
      idle.current = setTimeout(end, IDLE_MS);
    }
  };

  /** The pointer's angle round the knob, or null too near its centre. */
  const angle = (e: PointerEvent) => {
    const r = root.current?.getBoundingClientRect();
    if (!r) return null;
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    return Math.hypot(e.clientX - cx, e.clientY - cy) < DEAD_PX ? null : pointerDeg(cx, cy, e.clientX, e.clientY);
  };

  const ring = (which: Ring, label: string, keys: string) => ({
    role: 'slider',
    tabIndex: disabled ? -1 : 0,
    'aria-label': label,
    'aria-disabled': disabled || undefined,
    'aria-valuemin': coverage[0]?.lo_khz,
    'aria-valuemax': coverage.at(-1)?.hi_khz,
    'aria-valuenow': hz === null ? undefined : hz / 1000,
    'aria-valuetext': hz === null ? undefined : `${(hz / 1000).toLocaleString(t.locale)} kHz`,
    title: t.keyHint(label, keys),
    style: { '--a': deg[which] + 'deg' } as CSSProperties,
    onPointerDown: (e: PointerEvent<HTMLDivElement>) => {
      if (disabled || drag.current || e.button !== 0) return;
      e.currentTarget.setPointerCapture(e.pointerId);
      end();
      // NaN: started at the centre, the first move off it sets the angle
      drag.current = { ring: which, id: e.pointerId, deg: angle(e) ?? NaN, acc: 0 };
    },
    onPointerMove: (e: PointerEvent) => {
      const d = drag.current;
      if (!d || d.ring !== which || d.id !== e.pointerId) return;
      const a = angle(e);
      if (a === null) return;
      if (Number.isNaN(d.deg)) {
        d.deg = a;
        return;
      }
      const r = detents(d.acc, angleDelta(d.deg, a));
      d.deg = a;
      d.acc = r.rest;
      turn(which, r.clicks);
    },
    onPointerUp: (e: PointerEvent) => {
      if (drag.current?.id !== e.pointerId) return;
      drag.current = null;
      end();
    },
    onPointerCancel: (e: PointerEvent) => {
      if (drag.current?.id !== e.pointerId) return;
      drag.current = null;
      end();
    },
    onWheel: (e: WheelEvent) => {
      const r = wheelDetents(wheel.current, e.deltaY, e.deltaMode);
      wheel.current = r.rest;
      turn(which, r.clicks);
    },
    onKeyDown: (e: KeyboardEvent) => {
      const dir = e.key === 'ArrowUp' || e.key === 'ArrowRight' ? 1 : e.key === 'ArrowDown' || e.key === 'ArrowLeft' ? -1 : 0;
      if (!dir || e.ctrlKey || e.metaKey || e.altKey) return;
      e.preventDefault();
      turn(which, dir);
    },
  });

  return (
    <div className="knob" ref={root}>
      <div className="knob-outer" {...ring('outer', t.knobCoarse, 'Shift+← →')} />
      <div className="knob-inner" {...ring('inner', t.knobFine, '← →')} />
    </div>
  );
}
