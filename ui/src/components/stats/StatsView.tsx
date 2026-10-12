// Stats: the listening history in the middle of the window (the globe and the side panels give
// way; the top bar and the faceplate stay, so tuning and audio go on). Tabs: Overview,
// Collection (when Settings › Display keeps it), Propagation and Hunt; the period applies to
// Overview and Propagation only. It asks the core again every few seconds while it is open.

import { useCallback, useEffect, useState } from 'react';
import type { Api } from '../../api/client';
import { useHunt, useStats } from '../../hooks/useStats';
import { useT } from '../../i18n';
import type { Period } from '../../types/generated/Period';
import type { Share } from '../../types/generated/Share';
import type { StationRef } from '../../types/generated/StationRef';
import type { TrendPoint } from '../../types/generated/TrendPoint';
import type { Usual } from '../../types/generated/Usual';
import { Collection } from './Collection';
import { HuntTab } from './HuntTab';
import { Overview, type Breakdown, type Tune } from './Overview';
import { Propagation } from './Propagation';
import { newestEarned } from './stats';

export type StatsTab = 'overview' | 'collection' | 'propagation' | 'hunt';
const PERIODS: Period[] = ['day', 'week', 'month', 'year', 'all'];
const SEEN_KEY = 'swatlas.badgesSeen';

function loadSeen(): number {
  try { return Number(localStorage.getItem(SEEN_KEY) ?? 0) || 0; } catch { return 0; }
}
function saveSeen(at: number) {
  try { localStorage.setItem(SEEN_KEY, String(at)); } catch { /* this session only */ }
}

interface Props {
  api: Api | null;
  /** The Collection tab is shown (Settings › Display). */
  collection: boolean;
  tab: StatsTab;
  onTab: (t: StatsTab) => void;
  period: Period;
  onPeriod: (p: Period) => void;
  /** Hunt asks again with the app's clock. */
  clock: Date;
  /** The station usually listened to at this hour (when nothing is being recorded). */
  usual: Usual | null;
  onBack: () => void;
  onTune: Tune;
  /** Leave Stats with the station list on this country. */
  onCountry: (itu: string) => void;
  /** Leave Stats with the list on Listened, so the globe shows the sites. */
  onShowGlobe: () => void;
  onError: (e: unknown) => void;
}

export function StatsView({ api, collection, tab: asked, onTab, period, onPeriod, clock, usual, onBack, onTune, onCountry, onShowGlobe, onError }: Props) {
  const t = useT();
  const tab: StatsTab = asked === 'collection' && !collection ? 'overview' : asked;
  const { stats, reload } = useStats(api, period, true);
  const hunt = useHunt(api, tab === 'hunt', clock.getTime());
  const [by, setBy] = useState<Breakdown>('stations');
  const [trendOf, setTrendOf] = useState<StationRef | null>(null);
  const [points, setPoints] = useState<TrendPoint[] | null>(null);

  // the trend: the station picked in Overview, else the most listened
  const top = stats?.by.stations.find((s) => s.station)?.station ?? null;
  const station = trendOf ?? top;
  const trendKey = station ? `${station.source}\u001f${station.key}` : null;
  useEffect(() => {
    setPoints(null);
    if (!api || !station || tab !== 'propagation') return;
    let alive = true;
    api.stationTrend({ source: station.source, key: station.key }).then((p) => { if (alive) setPoints(p); }, () => {});
    return () => { alive = false; };
    // `trendKey` stands for the station
  }, [api, trendKey, tab]);

  // a badge newer than the last one seen marks the Collection tab, until it is opened
  const [seen, setSeen] = useState(loadSeen);
  const newest = stats ? newestEarned(stats.collection.badges.map((b) => b.earned)) : 0;
  useEffect(() => {
    if (tab === 'collection' && newest > seen) {
      saveSeen(newest);
      setSeen(newest);
    }
  }, [tab, newest, seen]);

  const clear = useCallback(async (before: number | null) => {
    if (!api) return;
    try {
      await api.clearListening(before);
    } catch (e) {
      onError(e);
    }
    reload();
  }, [api, onError, reload]);

  const tabs: StatsTab[] = collection ? ['overview', 'collection', 'propagation', 'hunt'] : ['overview', 'propagation', 'hunt'];
  const withPeriod = tab === 'overview' || tab === 'propagation';
  return (
    <section className="stats-view" aria-label={t.statsTitle}>
      <header className="st-head">
        <button className="link st-back" type="button" onClick={onBack} title={t.statsBackTitle}>‹ {t.back}</button>
        <h2 className="st-title">{t.statsTitle}</h2>
        <div className="tabs st-tabs" role="tablist" aria-label={t.statsTitle}>
          {tabs.map((x) => (
            <button key={x} type="button" role="tab" aria-selected={tab === x} onClick={() => onTab(x)}>
              {t.statsTabs[x]}
              {x === 'collection' && newest > seen && <span className="new-dot" aria-label={t.badgeNew} />}
            </button>
          ))}
        </div>
        {withPeriod && (
          <div className="seg st-period" role="radiogroup" aria-label={t.statsPeriod}>
            {PERIODS.map((p) => (
              <button key={p} type="button" role="radio" aria-checked={period === p} onClick={() => onPeriod(p)}>{t.statsPeriods[p]}</button>
            ))}
          </div>
        )}
      </header>
      <div className="st-body">
        {!stats ? <p className="empty-d">…</p>
          : tab === 'overview' ? (
            <Overview
              s={stats}
              usual={usual}
              by={by}
              onBy={setBy}
              onTune={onTune}
              onCountry={onCountry}
              onShowGlobe={onShowGlobe}
              onTrend={(sh: Share) => setTrendOf(sh.station)}
              withMilestones={!collection}
              onClear={clear}
            />
          )
          : tab === 'collection' ? <Collection s={stats} onShowGlobe={onShowGlobe} />
          : tab === 'propagation' ? <Propagation s={stats} station={station} points={points} onTune={onTune} />
          : <HuntTab hunt={hunt} onTune={onTune} />}
      </div>
    </section>
  );
}
