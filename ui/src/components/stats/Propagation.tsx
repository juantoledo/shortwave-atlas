// Stats › Propagation, the bands as the listener's own sessions show them: the band that came
// in best at each UTC hour, grey-line catches, day and night by band, the faintest signals
// they stayed on, and one station's signal week by week.

import { useNames } from '../../catalog/names';
import { fmtSecs, sLabel, useT } from '../../i18n';
import type { StationRef } from '../../types/generated/StationRef';
import type { Stats } from '../../types/generated/Stats';
import type { TrendPoint } from '../../types/generated/TrendPoint';
import { SplitBar } from './charts';
import { SessionRow, StationName, type Tune } from './Overview';
import { sparkPath } from './stats';

interface Props {
  s: Stats;
  /** The station whose trend shows, and its weeks. */
  station: StationRef | null;
  points: TrendPoint[] | null;
  onTune: Tune;
}

const SPARK_W = 300, SPARK_H = 60;

export function Propagation({ s, station, points, onTune }: Props) {
  const t = useT();
  const names = useNames();
  const p = s.propagation;
  const anyBest = p.best_by_hour.some((b) => b !== null);
  return (
    <div className="st-grid">
      <section className="st-card st-wide">
        <h3 className="h"><span>{t.bestBand}</span><span>UTC</span></h3>
        {anyBest ? (
          <ol className="st-best">
            {p.best_by_hour.map((b, h) => (
              <li key={h} className={b ? 'on' : ''} title={b ? t.bestBandTitle(String(h).padStart(2, '0'), names.band(b.band), sLabel(b.db), b.sessions) : `${String(h).padStart(2, '0')}:00`}>
                <small>{String(h).padStart(2, '0')}</small>
                <b>{b ? names.band(b.band) : ''}</b>
              </li>
            ))}
          </ol>
        ) : <p className="empty-d">{t.bestBandEmpty}</p>}
      </section>

      <section className="st-card">
        <h3 className="h"><span>{t.dayNight}</span></h3>
        {p.day_night.length === 0 ? <p className="empty-d">{t.statsEmpty}</p> : (
          <>
            <ul className="st-hbars">
              {p.day_night.map((b) => {
                const total = b.day_s + b.twilight_s + b.night_s;
                return (
                  <li key={b.band}>
                    <div className="st-hrow">
                      <span className="st-hl">{names.band(b.band)}</span>
                      <span className="st-hv">{fmtSecs(t, total)}</span>
                      <SplitBar parts={[b.day_s, b.twilight_s, b.night_s]} title={t.dayNightTitle(fmtSecs(t, b.day_s), fmtSecs(t, b.twilight_s), fmtSecs(t, b.night_s))} />
                    </div>
                  </li>
                );
              })}
            </ul>
            <p className="st-legend"><i className="day" />{t.sunDay} <i className="tw" />{t.sunTwilight} <i className="night" />{t.sunNight}</p>
          </>
        )}
      </section>

      <section className="st-card st-tall">
        <h3 className="h"><span>{t.greyLine}</span><span>{p.grey_count}</span></h3>
        <p className="note">{t.greyLineHelp}</p>
        {p.grey.length === 0 ? <p className="empty-d">{t.greyEmpty}</p> : (
          <ul className="list">{p.grey.map((x) => <SessionRow key={x.id} s={x} onTune={onTune} />)}</ul>
        )}
      </section>

      <section className="st-card st-tall">
        <h3 className="h"><span>{t.faint}</span></h3>
        <p className="note">{t.faintHelp}</p>
        {p.faint.length === 0 ? <p className="empty-d">{t.faintEmpty}</p> : (
          <ul className="list">{p.faint.map((x) => <SessionRow key={x.id} s={x} onTune={onTune} />)}</ul>
        )}
      </section>

      <section className="st-card st-wide">
        <h3 className="h"><span>{station ? <StationName r={station} /> : t.trend}</span><span>{t.trendWeeks}</span></h3>
        {!station ? <p className="empty-d">{t.trendPick}</p>
          : !points || points.every((x) => x.db === null) ? <p className="empty-d">{t.trendEmpty}</p>
          : (
            <div className="st-spark" role="img" aria-label={t.trend}>
              <svg viewBox={`0 0 ${SPARK_W} ${SPARK_H}`} preserveAspectRatio="none" aria-hidden="true">
                <path d={sparkPath(points, SPARK_W, SPARK_H)} />
              </svg>
              <div className="st-spark-dots">
                {points.map((x, i) => x.db !== null && (
                  <span key={i} style={{ left: `${(i / (points.length - 1)) * 100}%`, bottom: `${((Math.min(20, Math.max(-60, x.db)) + 60) / 80) * 100}%` }} title={t.trendTitle(new Date(x.start * 1000).toLocaleDateString(t.locale), sLabel(x.db), x.sessions)} />
                ))}
              </div>
              <div className="axis"><span>{t.trendAgo}</span><span>{t.trendNow}</span></div>
            </div>
          )}
      </section>
    </div>
  );
}
