// The stage as a timetable: one lane per station the list's filters keep (by frequency), its
// windows on the air today across 00–24 UTC, and a line at the time now. With the whole schedule
// in the list a switch picks the dial's band or all of it (components/timetableView.ts). Lanes are drawn only where the
// scroll shows them. A lane tunes its station, like a row of the list.

import { memo, useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import { useNames } from '../catalog/names';
import { hhmm, useT } from '../i18n';
import type { MapMode } from '../hooks/useDisplay';
import type { TimetableState } from '../hooks/useTimetable';
import type { Band } from '../types/generated/Band';
import type { StationRow } from '../types/generated/StationRow';
import type { TimetableRow } from '../types/generated/TimetableRow';
import { Flag } from './Flag';
import { MapModeSwitch } from './stage/MapModeSwitch';
import { coChannel, dayPct, lanesInSight, LANE_PX, localAt, onAt, scrollToShow, startsFreq, utcMinute, type TimetableScope } from './timetableView';

/** UTC hours with a label (the grid draws every hour). */
const LABELED = [0, 3, 6, 9, 12, 15, 18, 21, 24];

/** The scope switch, while the list has the whole schedule. */
export interface ScopeSwitch {
  scope: TimetableScope;
  onScope: (s: TimetableScope) => void;
  /** The dial's broadcast band; `null` (another kind of band, or none) leaves only All. */
  band: Band | null;
}

interface Props {
  state: TimetableState;
  scope: ScopeSwitch | null;
  /** The NET band is in the list: internet stations keep no schedule. */
  net: boolean;
  now: Date;
  /** The radio is on: a lane tunes it. */
  on: boolean;
  selId: number | null;
  /** The frequency the radio is tuned to: the other stations there share the channel. */
  tunedHz: number | null;
  onPick: (r: StationRow) => void;
  mode: MapMode;
  onMode: (m: MapMode) => void;
}

export function Timetable({ state, scope, net, now, on, selId, tunedHz, onPick, mode, onMode }: Props) {
  const t = useT();
  const body = useRef<HTMLDivElement>(null);
  // the lanes get one callback for good, so the clock and the app's renders skip them
  const picker = useRef(onPick);
  picker.current = onPick;
  const pick = useCallback((r: StationRow) => picker.current(r), []);
  const [scroll, setScroll] = useState({ top: 0, h: 600 });
  useLayoutEffect(() => {
    const el = body.current;
    if (!el) return;
    const measure = () => setScroll({ top: el.scrollTop, h: el.clientHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const rows = net ? [] : state.table?.rows ?? [];
  // the tuned station comes into sight when it changes, or when new rows arrive
  useEffect(() => {
    const el = body.current;
    const i = selId === null ? -1 : rows.findIndex((r) => r.row.id === selId);
    if (!el || i < 0) return;
    const top = scrollToShow(i, el.scrollTop, el.clientHeight);
    if (top !== null) el.scrollTo({ top, behavior: 'smooth' });
  }, [selId, state.table]); // `rows` comes from `state.table`
  const total = net ? 0 : state.table?.total ?? 0;
  const [from, to] = lanesInSight(scroll.top, scroll.h, rows.length);
  const minute = utcMinute(now);
  const offset = -now.getTimezoneOffset();
  const empty = net ? t.timetableNet : !state.loading && state.table && !rows.length ? t.timetableEmpty : null;

  return (
    <div className="timetable" role="region" aria-label={t.mapTimetable} aria-busy={state.loading}>
      <header className="tt-head">
        <div className="tt-scope">
          {scope && !net && <ScopeSeg {...scope} />}
          {!net && state.table && <span className="tt-count">{t.timetableCount(total)}</span>}
        </div>
        <MapModeSwitch mode={mode} onMode={onMode} />
      </header>
      <div className="tt-axis" aria-hidden="true">
        <span className="tt-zone">UTC<br />{t.timetableLocal}</span>
        <div className="tt-hours">
          {LABELED.map((h) => (
            <span key={h} style={{ left: dayPct(h * 60) + '%' }}>
              {String(h % 24).padStart(2, '0')}<br /><small>{localAt(h, offset)}</small>
            </span>
          ))}
        </div>
      </div>
      <div className="tt-body" ref={body} onScroll={(e) => setScroll({ top: e.currentTarget.scrollTop, h: e.currentTarget.clientHeight })}>
        <div className="tt-lanes" style={{ height: rows.length * LANE_PX, '--now': minute / 1440 } as CSSProperties}>
          {rows.slice(from, to).map((r, i) => (
            <Lane
              key={r.row.id}
              r={r}
              top={(from + i) * LANE_PX}
              first={startsFreq((k) => rows[k].row.freq_hz, from + i)}
              minute={minute}
              on={on}
              current={r.row.id === selId}
              co={r.row.id !== selId && coChannel(r.row.freq_hz, tunedHz)}
              onPick={pick}
            />
          ))}
          {rows.length > 0 && <i className="tt-now" />}
        </div>
        {empty && <p className="tt-empty">{empty}</p>}
      </div>
      {rows.length < total && <p className="tt-more">{t.timetableTruncated(rows.length, total)}</p>}
    </div>
  );
}

/** The dial's band or the whole schedule. Without a broadcast band on the dial, only All. */
function ScopeSeg({ scope, onScope, band }: ScopeSwitch) {
  const t = useT();
  const shown = band ? scope : 'all';
  return (
    <div className="seg" role="radiogroup" aria-label={t.timetableScope}>
      <button type="button" role="radio" aria-checked={shown === 'band'} disabled={!band} title={band ? undefined : t.timetableScopeNoBand} onClick={() => onScope('band')}>
        {band ? t.timetableScopeBand(band.label) : t.timetableScopeDial}
      </button>
      <button type="button" role="radio" aria-checked={shown === 'all'} onClick={() => onScope('all')}>{t.timetableScopeAll}</button>
    </div>
  );
}

interface LaneProps {
  r: TimetableRow;
  top: number;
  /** The first lane of its frequency: it carries the label and the divider. */
  first: boolean;
  minute: number;
  on: boolean;
  current: boolean;
  /** On the tuned channel, but not the station chosen there. */
  co: boolean;
  onPick: (r: StationRow) => void;
}

/** One station's lane. Memoized: the clock renders the timetable again every few seconds. */
const Lane = memo(function Lane({ r, top, first, minute, on, current, co, onPick }: LaneProps) {
  const t = useT();
  const names = useNames();
  const { row, windows } = r;
  const hours = windows.map(([s, e]) => `${hhmm(s)}–${hhmm(e)}`).join(', ');
  const about = [row.lang && names.lang(row.lang), row.target && `→ ${names.target(row.target)}`].filter(Boolean).join(' ');
  const title = `${row.freq_hz / 1000} kHz · ${row.name} · ${row.site_name}\n${hours} UTC${about ? `\n${about}` : ''}${on ? '' : `\n${t.timetableOff}`}`;
  const lit = onAt(windows, minute);
  return (
    <button className={'tt-lane' + (first ? ' first' : '') + (lit ? ' lit' : '') + (co ? ' co' : '')} type="button" style={{ top }} aria-current={current || undefined} disabled={!on} title={title} onClick={() => onPick(row)}>
      <span className="tt-label">
        <span className="rf">{row.freq_hz / 1000}</span>
        <Flag flag={row.flag} title={names.country(row.itu)} />
        <span className="tt-name">{row.name}</span>
      </span>
      <span className="tt-track">
        {windows.map(([s, e]) => (
          <i key={s} className={minute >= s && minute < e ? 'on' : undefined} style={{ left: dayPct(s) + '%', width: dayPct(e - s) + '%' }} />
        ))}
      </span>
    </button>
  );
});
