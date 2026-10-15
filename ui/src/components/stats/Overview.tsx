// Stats › Overview: what is being recorded now, the period in numbers against the one before,
// when it happened, what it was (station, country, language, target, band, mode), how far,
// the latest sessions, and cleaning the history up (each choice asks first). Internet stations
// (the NET band) have a card of their own, apart from every DX figure.

import { useMemo, useRef, useState, type ReactNode } from 'react';
import { useNames } from '../../catalog/names';
import { useNow } from '../../hooks/useCore';
import { fmtKm, fmtSecs, fmtStamp, sLabel, useT, weekdayName } from '../../i18n';
import type { Messages } from '../../i18n';
import type { Mode } from '../../types/generated/Mode';
import type { NetStats } from '../../types/generated/NetStats';
import type { Session } from '../../types/generated/Session';
import type { Share } from '../../types/generated/Share';
import type { StationRef } from '../../types/generated/StationRef';
import type { Stats } from '../../types/generated/Stats';
import type { Usual } from '../../types/generated/Usual';
import { Confirm } from '../Confirm';
import { asMode } from '../favorites';
import { Flag } from '../Flag';
import { Bars, Heat, HBars, type HRow } from './charts';
import { bucketLabel, trend, yearly } from './stats';

export type Breakdown = 'stations' | 'countries' | 'languages' | 'targets' | 'bands' | 'modes';
const BREAKDOWNS: Breakdown[] = ['stations', 'countries', 'languages', 'targets', 'bands', 'modes'];

const khz = (hz: number) => String(hz / 1000);

/** Tune there; with the station, it is also the listener's pick (the history credits it). */
export type Tune = (hz: number, mode: Mode | null, station?: { source: string; key: string } | null) => void;

export function StationName({ r }: { r: StationRef }) {
  const names = useNames();
  return <><Flag flag={r.flag} title={names.country(r.itu)} />{r.name}</>;
}

/** A session in a list: its station (or the frequency), when, how long; it tunes there. */
export function SessionRow({ s, aside, onTune }: { s: Session; aside?: ReactNode; onTune: Tune }) {
  const t = useT();
  return (
    <li>
      <button className="row scan-row" type="button" title={t.scanTune(khz(s.freq_hz))} onClick={() => onTune(s.freq_hz, s.mode ? asMode(s.mode) : null, s.station)}>
        <span className="rf">{khz(s.freq_hz)}</span>
        <span className="rn">
          <b>{s.station ? <StationName r={s.station} /> : t.statsUnidentified}</b>
          <em>{fmtStamp(t, s.start)} · {fmtSecs(t, s.end - s.start)}</em>
        </span>
        <span className="sval">{aside ?? sLabel(s.avg_db)}</span>
      </button>
    </li>
  );
}

function Delta({ now, before, fmt }: { now: number; before: number | null | undefined; fmt?: (n: number) => string }) {
  const t = useT();
  const tr = trend(now, before);
  if (!tr || before === null || before === undefined) return null;
  const arrow = tr.dir === 'up' ? '▲' : tr.dir === 'down' ? '▼' : '=';
  const text = tr.pct === null ? arrow : `${arrow} ${Math.abs(tr.pct)}%`;
  return <small className={'st-delta ' + tr.dir} title={t.statsBefore(fmt ? fmt(before) : String(before))}>{text}</small>;
}

/** Internet stations listened to in the period (the NET band): apart from every DX figure here. */
function NetCard({ n }: { n: NetStats }) {
  const t = useT();
  const dur = (v: number) => fmtSecs(t, v);
  const max = Math.max(0, ...n.top.map((s) => s.seconds));
  return (
    <section className="st-card st-net">
      <h3 className="h"><span>{t.statsNet}</span><span>{t.statsPeriods[n.period]}</span></h3>
      {n.sessions === 0 ? <p className="empty-d">{t.statsNetEmpty}</p> : (
        <>
          <p className="st-net-kpis">
            <b>{dur(n.seconds)}</b> · {t.statsNetCounts(n.stations, n.countries)}
          </p>
          <ul className="st-hbars">
            {n.top.map((s) => (
              <li key={s.uuid}>
                <div className="st-hrow" title={t.statsSessions(s.sessions)}>
                  <span className="st-hl"><Flag flag={s.flag} title={s.itu ?? ''} />{s.name}</span>
                  <span className="st-hv">{dur(s.seconds)}</span>
                  <span className="st-ht" aria-hidden="true"><i style={{ width: `${max ? (s.seconds / max) * 100 : 0}%` }} /></span>
                </div>
              </li>
            ))}
          </ul>
        </>
      )}
      <p className="note">{t.statsNetNote}</p>
    </section>
  );
}

function NowCard({ s, usual, onTune }: { s: Stats; usual: Usual | null; onTune: Tune }) {
  const t = useT();
  const clock = useNow(1000);
  // counted from the core's numbers, not this device's clock (a remote browser may be off)
  const got = useMemo(() => Date.now(), [s]);
  const n = s.now;
  if (!n) {
    return (
      <section className="st-card st-now">
        <h3 className="h"><span>{t.statsNow}</span></h3>
        <p className="empty-d">{t.statsNowEmpty}</p>
        {usual && (
          <button className="row scan-row st-usual" type="button" title={t.scanTune(khz(usual.station.freq_hz))} onClick={() => onTune(usual.station.freq_hz, usual.mode, usual.station)}>
            <span className="rf">{khz(usual.station.freq_hz)}</span>
            <span className="rn"><b><StationName r={usual.station} /></b><em>{t.statsUsualHere}</em></span>
            <span />
          </button>
        )}
      </section>
    );
  }
  const secs = Math.max(0, n.session.end - n.session.start + Math.floor((clock.getTime() - got) / 1000));
  return (
    <section className="st-card st-now on">
      <h3 className="h"><span><span className="st-lamp" aria-hidden="true" />{t.statsNow}</span>{n.session.station && <span>{n.session.picked ? t.statsPicked : t.statsGuess}</span>}</h3>
      <p className="st-now-name">{n.session.station ? <StationName r={n.session.station} /> : t.statsUnidentified}</p>
      <p className="st-now-meta"><b>{khz(n.session.freq_hz)} kHz</b> · {fmtSecs(t, secs)}{n.session.km !== null && ` · ${fmtKm(t, n.session.km)}`}</p>
    </section>
  );
}

function shareLabel(t: Messages, names: ReturnType<typeof useNames>, by: Breakdown, sh: Share): ReactNode {
  if (sh.key === '*') return t.statsOther;
  switch (by) {
    case 'stations': return sh.station ? <StationName r={sh.station} /> : t.statsUnidentified;
    case 'countries': return names.country(sh.key);
    case 'languages': return names.lang(sh.key);
    case 'targets': return names.target(sh.key);
    case 'bands': return names.band(sh.key);
    case 'modes': return sh.key;
  }
}

interface Props {
  s: Stats;
  /** Internet listening in the same period (the NET band), apart from the rest. */
  net: NetStats | null;
  /** When nothing is being recorded: the station usually listened to at this hour, if on. */
  usual: Usual | null;
  /** The breakdown tab, kept by the view (Propagation's trend follows its station). */
  by: Breakdown;
  onBy: (b: Breakdown) => void;
  onTune: Tune;
  onCountry: (itu: string) => void;
  onShowGlobe: () => void;
  /** Station picked for the trend (Propagation). */
  onTrend: (sh: Share) => void;
  /** Milestones and the streak live here when Collection is off. */
  withMilestones: boolean;
  onClear: (before: number | null) => Promise<void>;
}

export function Overview({ s, net, usual, by, onBy, onTune, onCountry, onShowGlobe, onTrend, withMilestones, onClear }: Props) {
  const t = useT();
  const names = useNames();
  const p = s.previous;
  const dur = (n: number) => fmtSecs(t, n);
  const kpis: [string, number, number | undefined, (n: number) => string][] = [
    [t.kpiTime, s.totals.seconds, p?.seconds, dur],
    [t.kpiStations, s.totals.stations, p?.stations, String],
    [t.kpiCountries, s.totals.countries, p?.countries, String],
    [t.kpiLongest, s.totals.longest, p?.longest, dur],
  ];
  const years = s.period === 'all' && yearly(s.timeline);
  const curBucket = s.timeline.length - 1;
  const shares = s.by[by];
  const rows: HRow[] = shares.map((sh) => ({
    key: sh.key || '?',
    label: shareLabel(t, names, by, sh),
    value: sh.seconds,
    text: dur(sh.seconds),
    aside: p && sh.key !== '*' ? <Delta now={sh.seconds} before={sh.seconds - sh.delta_s} fmt={dur} /> : null,
    title: t.statsSessions(sh.sessions),
    onClick:
      by === 'stations' && sh.station ? () => { onTrend(sh); onTune(sh.station!.freq_hz, null, sh.station); }
      : by === 'countries' && sh.key !== '*' ? () => onCountry(sh.key)
      : undefined,
  }));
  const distMax = Math.max(0, ...s.distance.map((d) => d.seconds));
  const empty = s.totals.sessions === 0;

  return (
    <div className="st-grid">
      <NowCard s={s} usual={usual} onTune={onTune} />
      <section className="st-card st-kpis" aria-label={t.statsPeriods[s.period]}>
        {kpis.map(([label, v, before, fmt]) => (
          <div key={label} className="st-kpi">
            <span>{label}</span>
            <b>{fmt(v)}</b>
            <Delta now={v} before={before} fmt={fmt} />
          </div>
        ))}
      </section>

      <section className="st-card st-wide">
        <h3 className="h"><span>{t.statsTimeline}</span><span>{t.statsPeriods[s.period]}</span></h3>
        {empty ? <p className="empty-d">{t.statsEmpty}</p> : (
          <Bars
            values={s.timeline.map((b) => b.seconds)}
            labels={s.timeline.map((b) => bucketLabel(t.locale, s.period, b, years))}
            titles={s.timeline.map((b) => `${bucketLabel(t.locale, s.period, b, years)}: ${dur(b.seconds)}`)}
            current={curBucket}
            room={s.period === 'month' ? 10 : 12}
          />
        )}
      </section>

      <section className="st-card">
        <h3 className="h"><span>{t.statsHeat}</span></h3>
        <Heat
          cells={s.heat}
          rowLabels={[0, 1, 2, 3, 4, 5, 6].map((d) => weekdayName(t, d))}
          colLabels={Array.from({ length: 24 }, (_, h) => (h % 6 === 0 ? String(h).padStart(2, '0') : ''))}
          title={(r, c, v) => `${weekdayName(t, r, 'long')} ${String(c).padStart(2, '0')}:00 UTC: ${dur(v)}`}
        />
      </section>

      <section className="st-card st-tall">
        <h3 className="h"><span>{t.statsBy}</span></h3>
        <div className="tabs st-subtabs" role="tablist" aria-label={t.statsBy}>
          {BREAKDOWNS.map((b) => (
            <button key={b} type="button" role="tab" aria-selected={by === b} onClick={() => onBy(b)}>{t.statsBreakdowns[b]}</button>
          ))}
        </div>
        {rows.length ? <HBars rows={rows} /> : <p className="empty-d">{t.statsEmpty}</p>}
        {by === 'stations' && rows.length > 0 && <p className="note">{t.statsStationsNote}</p>}
      </section>

      <section className="st-card">
        <h3 className="h"><span>{t.statsDistance}</span>{s.sites.length > 0 && <button className="link" type="button" title={t.statsShowGlobeTitle} onClick={onShowGlobe}>{t.statsShowGlobe}</button>}</h3>
        <ul className="st-hbars">
          {s.distance.map((d) => (
            <li key={d.lo_km}>
              <div className="st-hrow" title={t.statsSessions(d.sessions)}>
                <span className="st-hl">{d.hi_km === null ? `> ${d.lo_km.toLocaleString(t.locale)} km` : `${d.lo_km.toLocaleString(t.locale)}–${d.hi_km.toLocaleString(t.locale)} km`}</span>
                <span className="st-hv">{dur(d.seconds)}</span>
                <span className="st-ht" aria-hidden="true"><i style={{ width: `${distMax ? (d.seconds / distMax) * 100 : 0}%` }} /></span>
              </div>
            </li>
          ))}
        </ul>
        {s.farthest?.station && (
          <p className="note">{t.statsFarthest} <b><StationName r={s.farthest.station} /></b> · {fmtKm(t, s.farthest.km ?? 0)}</p>
        )}
      </section>

      {net && (net.sessions > 0 || net.now) && <NetCard n={net} />}

      {withMilestones && <Milestones s={s} />}

      <section className="st-card st-tall">
        <h3 className="h"><span>{t.statsRecent}</span><span>{s.recent.length}</span></h3>
        {s.recent.length === 0 ? <p className="empty-d">{t.statsEmpty}</p> : (
          <ul className="list">{s.recent.map((x) => <SessionRow key={x.id} s={x} onTune={onTune} />)}</ul>
        )}
      </section>

      <CleanUp s={s} onClear={onClear} />
    </div>
  );
}

/** The streak and the milestones (Collection, or Overview when Collection is off). */
export function Milestones({ s }: { s: Stats }) {
  const t = useT();
  const names = useNames();
  const text = (m: Stats['milestones'][number]) => {
    switch (m.what.kind) {
      case 'first_country': return t.msFirstCountry(names.country(m.what.itu));
      case 'countries': return t.msCountries(m.what.n);
      case 'stations': return t.msStations(m.what.n);
    }
  };
  return (
    <section className="st-card st-tall">
      <h3 className="h"><span>{t.statsStreak}</span></h3>
      <p className="st-streak"><b>{t.streakNow(s.streak.current_days)}</b> · {t.streakBest(s.streak.best_days)}</p>
      <h3 className="h"><span>{t.statsMilestones}</span><span>{s.milestones.length}</span></h3>
      {s.milestones.length === 0 ? <p className="empty-d">{t.msEmpty}</p> : (
        <ul className="st-ms">
          {s.milestones.slice(0, 12).map((m, i) => <li key={i}><span>{text(m)}</span><small>{fmtStamp(t, m.at)}</small></li>)}
        </ul>
      )}
    </section>
  );
}

const DAY = 86_400;

function CleanUp({ s, onClear }: { s: Stats; onClear: (before: number | null) => Promise<void> }) {
  const t = useT();
  const box = useRef<HTMLDivElement>(null);
  const [ask, setAsk] = useState<null | 'all' | 30 | 90 | 365>(null);
  const count = ask === null ? 0 : ask === 'all' ? s.clear.all : ask === 30 ? s.clear.older_30 : ask === 90 ? s.clear.older_90 : s.clear.older_365;
  const go = () => {
    const a = ask;
    setAsk(null);
    if (a !== null) void onClear(a === 'all' ? null : Math.floor(Date.now() / 1000) - a * DAY);
  };
  const choices: (30 | 90 | 365)[] = [30, 90, 365];
  return (
    <section className="st-card st-wide st-clean">
      <h3 className="h"><span>{t.statsClear}</span><span>{t.statsSessions(s.clear.all)}</span></h3>
      <div className="actions" ref={box}>
        {choices.map((d) => (
          <button key={d} className="btn" type="button" disabled={(d === 30 ? s.clear.older_30 : d === 90 ? s.clear.older_90 : s.clear.older_365) === 0} onClick={() => setAsk(d)}>
            {t.statsClearOlder[d]}
          </button>
        ))}
        <button className="btn" type="button" disabled={s.clear.all === 0} onClick={() => setAsk('all')}>{t.statsClearAll}</button>
        {ask !== null && (
          <Confirm message={t.statsClearAsk(count)} confirmLabel={ask === 'all' ? t.statsClearAll : t.statsClearOlder[ask]} onConfirm={go} onCancel={() => setAsk(null)} anchor={box} />
        )}
      </div>
      <p className="note">{t.statsClearNote}</p>
    </section>
  );
}
