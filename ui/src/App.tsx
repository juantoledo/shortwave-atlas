// The window is the receiver: a top bar, the globe in the middle with the station list on its
// left and the tuned station on its right, and the faceplate (dial and audio) along the bottom.
// Smaller screens fold the side panels into tabs (styles.css); `view` says which one shows.
// The globe may also be drawn behind the whole window (`data-globe`, Settings → Display).
// Stats (`data-page="stats"`) takes the middle of the window; the top bar and faceplate stay.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AudioOff, AudioPanel, type AudioKeys } from './components/AudioPanel';
import { Dossier } from './components/Dossier';
import { FavStar } from './components/FavStar';
import { asMode, favAt } from './components/favorites';
import { Globe } from './components/Globe';
import { Help } from './components/Help';
import { Hud } from './components/Hud';
import { ScanBar } from './components/ScanBar';
import { Scans } from './components/Scans';
import { StatsView, type StatsTab } from './components/stats/StatsView';
import { PanelHead, ViewTabs, type View } from './components/NavBar';
import { Settings, type SettingsTab } from './components/settings/Settings';
import { StationForm } from './components/StationForm';
import { draftFrom, draftFromOff, draftHere, draftOf } from './components/stationDraft';
import { StationList } from './components/StationList';
import { TopBar } from './components/TopBar';
import { Tuner, useBandWindow } from './components/Tuner';
import { DEFAULT_COVERAGE, HF_SPAN } from './components/dial';
import { bandOf, noiseFloor } from './components/scan';
import { keyOf, MINE } from './components/sources';
import { UpdateBanner } from './components/UpdateBanner';
import { chromeFor } from './api/window';
import { Welcome } from './components/Welcome';
import { makeNames, NamesProvider } from './catalog/names';
import { NO_FILTERS, useCandidates, useCore, useDial, useNow, useOverview, useStationSearch, useStationsMeta, useTuning, type Filters } from './hooks/useCore';
import { PHONE, useMedia } from './hooks/useMedia';
import { useFavorites } from './hooks/useFavorites';
import { useDisplay } from './hooks/useDisplay';
import { useBandScans, useScan } from './hooks/useScan';
import { useUpdate } from './hooks/useUpdate';
import { useUserStations } from './hooks/useUserStations';
import { useWelcome } from './hooks/useWelcome';
import { useFreqHeat, useListening, useStats } from './hooks/useStats';
import { fmtStamp, I18nProvider, pickMessages } from './i18n';
import type { HelpTopic } from './i18n/en';
import { stepIndex, type Action } from './keys/keymap';
import { useKeys } from './keys/useKeys';
import type { Band } from './types/generated/Band';
import type { Candidate } from './types/generated/Candidate';
import type { Mode } from './types/generated/Mode';
import type { OffSchedule } from './types/generated/OffSchedule';
import type { Period } from './types/generated/Period';
import type { Qth } from './types/generated/Qth';
import type { ScanSpeed } from './types/generated/ScanSpeed';
import type { SiteDot } from './types/generated/SiteDot';
import type { StationRow } from './types/generated/StationRow';
import type { UserStationDraft } from './types/generated/UserStationDraft';

const MSG_MS = 6000;
/** How long the rows a finished scan heard stay lit. */
const FRESH_MS = 10000;
const NO_HZ: number[] = [];
const NO_BANDS: Band[] = [];
const NO_SITES: SiteDot[] = [];
const HF_ONLY = [HF_SPAN];

type SheetState =
  | { kind: 'settings'; tab: SettingsTab }
  | { kind: 'help'; topic: HelpTopic | null }
  | { kind: 'scans'; id: string | null }
  /** A station of the listener's own: new (`id` null) or kept. */
  | { kind: 'station'; id: string | null; draft: UserStationDraft }
  | null;

export function App() {
  const { api, info, reloadInfo, rig, status } = useCore();
  const update = useUpdate(api);
  const display = useDisplay();
  const welcome = useWelcome();
  const [sheet, setSheet] = useState<SheetState>(null);
  const [view, setViewRaw] = useState<View>('globe');
  // Stats over the middle of the window, its tab and period
  const [page, setPage] = useState<'main' | 'stats'>('main');
  const [statsTab, setStatsTab] = useState<StatsTab>('overview');
  const [statsPeriod, setStatsPeriod] = useState<Period>('week');
  /** Another view (the phone's bottom tabs, a list action) leaves Stats. */
  const setView = useCallback((v: View) => { setPage('main'); setViewRaw(v); }, []);
  const [dockOpen, setDockOpen] = useState(false);
  const [touchedGlobe, setTouchedGlobe] = useState(false);
  const phone = useMedia(PHONE);
  const t = useMemo(() => pickMessages(info?.lang, navigator.languages), [info?.lang]);
  useEffect(() => { document.documentElement.lang = t.locale; }, [t]);
  useEffect(() => {
    document.title = info ? `${t.appName} ${t.versionTag(info.version)}` : t.appName;
  }, [info, t]);

  const openHelp = useCallback((topic: HelpTopic | null = null) => setSheet({ kind: 'help', topic }), []);
  const openSettings = useCallback((tab: SettingsTab = 'rig') => setSheet({ kind: 'settings', tab }), []);

  const [msg, setMsg] = useState('');
  useEffect(() => {
    if (!msg) return;
    const id = setTimeout(() => setMsg(''), MSG_MS);
    return () => clearTimeout(id);
  }, [msg]);
  const onError = useCallback((e: unknown) => setMsg(e instanceof Error ? e.message : String(e)), []);

  const on = rig.power === 'on';
  // the rig's coverage and its dial's bands: asked again when the rig comes or goes
  const { dial, reload: reloadDial } = useDial(api, `${rig.link},${rig.power}`);
  const coverage = dial?.coverage ?? DEFAULT_COVERAGE;
  const { freqHz, tune } = useTuning(api, rig.freq_hz, coverage, onError);
  const [qth, setQth] = useState<Qth | null>(null);
  useEffect(() => { if (info) setQth(info.qth); }, [info]);

  const now = useNow(20_000);
  // the catalog's generation: station ids hold within one, so whatever keeps them reloads
  const { overview, reload: reloadOverview } = useOverview(api);
  const generation = overview?.generation ?? null;
  const meta = useStationsMeta(api, generation);
  const dialBands = dial?.bands ?? meta?.bands ?? NO_BANDS;
  const win = useBandWindow(freqHz, dialBands, overview?.on_hz ?? NO_HZ);
  const names = useMemo(() => makeNames(meta, t), [meta, t]);
  const scan = useScan(api, onError, generation);
  const favs = useFavorites(api, onError, generation);
  const mine = useUserStations(api, onError, generation, reloadOverview);
  const bandScans = useBandScans(scan, win.w.band?.id ?? null);
  const { listening, reload: reloadListening } = useListening(api);
  const [filters, setFilters] = useState<Filters>(NO_FILTERS);
  // the lenses change with the logbook, the favorites and the listening history (a session
  // starting or ending), and every list with the catalog
  const sessionKey = filters.lens === 'listened' ? listening?.now?.session.id ?? null : null;
  // the band dial's heat strip: again when a session starts or ends, and on leaving Stats (where
  // the history can be cleared)
  const heat = useFreqHeat(api, `${listening?.now?.session.id ?? ''}|${page}`);
  const lensKey = useMemo(() => ({}), [scan.logged, favs.views, generation, sessionKey]);
  const search = useStationSearch(api, filters, lensKey);
  const cands = useCandidates(api, on ? freqHz : null, `${qth?.lat},${qth?.lon},${now.getTime()},${generation}`);

  // Keep the chosen station while it is still a candidate; otherwise take the best one. It is
  // kept by source and key (`keyOf`), which hold when the catalog changes and the ids do not.
  const [sel, setSel] = useState<Candidate | null>(null);
  const [wanted, setWanted] = useState<string | null>(null);
  // the globe's listened sites, while the list is on Listened
  const lensListened = filters.lens === 'listened' && page === 'main';
  const { stats: allTime } = useStats(api, 'all', lensListened);
  const listenedSites = useMemo(() => {
    const m = new Map<number, number>();
    if (!lensListened || !allTime) return m;
    const max = Math.max(1, ...allTime.sites.map((x) => x.seconds));
    for (const x of allTime.sites) m.set(x.site, x.seconds / max);
    return m;
  }, [lensListened, allTime]);
  useEffect(() => {
    if (!on || !cands.length) setSel(null);
    else setSel((cur) => cands.find((c) => keyOf(c.station) === (wanted ?? (cur && keyOf(cur.station)))) ?? cands[0]);
  }, [cands, on, wanted]);

  const sites = overview?.sites ?? NO_SITES;
  const siteName = filters.site === null ? null : sites.find((s) => s.id === filters.site)?.name ?? null;
  const scanSel = filters.scan === null ? null : scan.scans.find((s) => s.id === filters.scan) ?? null;
  const scanName = scanSel && `${names.dialBand(bandOf(scanSel))} · ${fmtStamp(t, scanSel.started)}`;
  // the scan the list was narrowed to was deleted
  useEffect(() => {
    if (filters.scan !== null && !scan.scans.some((s) => s.id === filters.scan)) setFilters((f) => ({ ...f, scan: null }));
  }, [filters.scan, scan.scans]);
  const siteLabel = useCallback((s: SiteDot) => ({ country: names.country(s.itu), onAir: t.siteOnAir(s.on, s.total) }), [names, t]);
  // the globe's callbacks are stable, so it only renders again when what it shows changes
  const pickSite = useCallback((id: number) => {
    setFilters((f) => ({ ...f, site: f.site === id ? null : id }));
    setView('browse');
  }, []);

  const [frame, setFrame] = useState(true);
  const toggleFrame = useCallback(() => setFrame((f) => !f), []);
  const [powerBusy, setPowerBusy] = useState(false);

  const setMode = (mode: Mode) => api?.rig({ cmd: 'set_mode', mode }).catch(onError);
  // a station's mode is set by the core once the rig is on its frequency (AM-N stays for AM)
  const tuneAs = (hz: number, mode: Mode | null) => tune(hz, mode);
  // the station the listener chose is the one the listening history credits
  const hearing = (st: { source: string; key: string }) => { api?.hearingStation({ source: st.source, key: st.key }).then(reloadListening, () => {}); };
  const pick = (r: StationRow) => {
    tuneAs(r.freq_hz, r.mode);
    setWanted(keyOf(r));
    hearing(r);
  };
  const pickOff = (o: OffSchedule) => {
    tuneAs(o.freq_hz, o.mode);
    setWanted(null);
  };
  const choose = (c: Candidate) => {
    setWanted(keyOf(c.station));
    setSel(c);
    hearing(c.station);
  };
  // Stats: open and close; leave it for the list on a country, or for the globe's Listened sites
  const toggleStats = () => setPage((p) => (p === 'stats' ? 'main' : 'stats'));
  const statsCountry = (itu: string) => {
    setFilters((f) => ({ ...f, countries: [itu], lens: 'all', scan: null, site: null }));
    setView(phone ? 'browse' : view);
  };
  const statsGlobe = () => {
    setFilters((f) => ({ ...f, lens: 'listened', scan: null }));
    setView('globe');
  };
  useEffect(() => {
    if (page !== 'stats' || sheet !== null || welcome.open) return;
    const esc = (e: KeyboardEvent) => {
      // a confirm popover takes its own Escape
      if (e.key === 'Escape' && !e.defaultPrevented && !document.querySelector('.confirm-pop')) setPage('main');
    };
    document.addEventListener('keydown', esc);
    return () => document.removeEventListener('keydown', esc);
  }, [page, sheet, welcome.open]);
  // turning off asks first (components/Confirm.tsx); a rig that goes off by itself drops the question
  const [askOff, setAskOff] = useState(false);
  useEffect(() => { if (!on) setAskOff(false); }, [on]);
  const setPower = async (to: boolean) => {
    if (!api || powerBusy) return;
    setPowerBusy(true);
    try {
      await api.rig({ cmd: 'set_power', on: to });
    } catch (e) {
      onError(e);
    }
    setPowerBusy(false);
  };
  const moveQth = useCallback((q: Qth) => {
    setQth(q);
    // the QTH's ITU region sets some band edges
    api?.setQth(q).then(reloadDial, onError);
  }, [api, reloadDial, onError]);
  const onGlobeTouched = useCallback(() => setTouchedGlobe(true), []);

  // the band dial draws the running scan of its band, else the band's last scan, each over the
  // scan before it
  const running = scan.status.state === 'running' && scan.status.band_id === win.w.band?.id ? scan.status : null;
  const { latest: last, ghost: before } = bandScans;
  const latest = useMemo(() => last && { ...last.summary, samples: last.samples }, [last]);
  const ghost = useMemo(() => before && { ...before.summary, samples: before.samples }, [before]);
  const profiles = useMemo(() => (running ? { now: running, ghost: latest } : { now: latest, ghost }), [running, latest, ghost]);
  // the globe's scan animation: the running scan's last steps, and the finale of one just ended
  const st = scan.status;
  const globeScan = useMemo(
    () => (st.state === 'running' ? { pings: st.pings, floorDb: noiseFloor(st.samples) } : null),
    [st],
  );
  const finale = useMemo(() => scan.finished && { key: scan.finished.id, sites: scan.finished.sites }, [scan.finished]);
  // a scan finished: the top bar's Scans button says so until they are opened
  const [scansNew, setScansNew] = useState(false);
  useEffect(() => { if (scan.finished) setScansNew(true); }, [scan.finished]);
  const openScans = (id: string | null = null) => {
    setScansNew(false);
    setSheet({ kind: 'scans', id });
  };
  // a scan started here turns the list to what the scans heard, newest first, and the rows it
  // hears light up when it ends (the logbook is only written then)
  const startScan = (bandId: string, stepHz: number, speed: ScanSpeed) => {
    setFilters({ ...NO_FILTERS, lens: 'heard', sort: 'recent' });
    if (phone) setView('browse');
    void scan.start(bandId, stepHz, speed);
  };
  const runningSince = scan.status.state === 'running' ? scan.status.started : null;
  const [freshSince, setFreshSince] = useState<number | null>(null);
  useEffect(() => { if (runningSince !== null) setFreshSince(runningSince); }, [runningSince]);
  useEffect(() => {
    if (!scan.finished) return;
    const id = setTimeout(() => setFreshSince(null), FRESH_MS);
    return () => clearTimeout(id);
  }, [scan.finished]);
  // the station list narrowed to a scan, or showing the favorites
  const showScan = (id: string) => {
    setSheet(null);
    setFilters((f) => ({ ...f, scan: id, lens: 'all' }));
    if (phone) setView('browse');
  };
  // the listener's own stations: the form, and where it opens from
  const addStation = (c: Candidate | null) => {
    if (freqHz === null) return;
    setSheet({ kind: 'station', id: null, draft: c ? draftFrom(c, new Date()) : draftHere(freqHz, asMode(rig.mode), new Date()) });
  };
  const addStationOff = (o: OffSchedule) => setSheet({ kind: 'station', id: null, draft: draftFromOff(o, asMode(rig.mode), new Date()) });
  const shownMine = sel?.station.source === MINE ? mine.byId.get(sel.station.key) ?? null : null;
  const editStation = () => shownMine && setSheet({ kind: 'station', id: shownMine.station.id, draft: draftOf(shownMine.station) });
  const deleteStation = async () => {
    if (!shownMine) return;
    if ((await mine.remove(shownMine.station.id)) !== null) setWanted(null);
  };
  /** Save the form's station and show it: tuned there if it moved off the dial's frequency. */
  const saveStation = async (id: string | null, d: UserStationDraft) => {
    const v = id ? await mine.update(id, d) : await mine.add(d);
    if (!v) return false;
    setWanted(keyOf({ source: MINE, key: v.station.id }));
    if (on && v.station.freq_hz !== freqHz) tuneAs(v.station.freq_hz, v.station.mode);
    return true;
  };
  const toggleFavorites = () => {
    const to = filters.lens === 'favorites' ? 'all' : 'favorites';
    setFilters((f) => ({ ...f, lens: to }));
    if (phone && to === 'favorites') setView('browse');
  };

  // the stars: the station shown, and the tuned frequency (with the station if it is on it)
  const selFav = sel ? favs.byStation.get(sel.station.id) ?? null : null;
  const tunedStation = sel && sel.station.freq_hz === freqHz ? sel.station : null;
  const tunedFav = on ? favAt(favs.views, freqHz, tunedStation?.id ?? null)?.favorite ?? null : null;

  // the keyboard (keys/keymap.ts): typing anywhere starts a new search
  const searchRef = useRef<HTMLInputElement>(null);
  const audioKeys = useRef<AudioKeys>(null);
  const focusSearch = (select: boolean) => {
    if (phone) setView('browse');
    const go = () => {
      const el = searchRef.current;
      el?.focus();
      if (select) el?.select();
    };
    go();
    // on phones the panel only shows after the view change renders
    if (phone) requestAnimationFrame(go);
  };
  useKeys(sheet !== null || askOff || welcome.open, (a: Action) => {
    switch (a.kind) {
      case 'search':
        setPage('main');
        setFilters((f) => ({ ...f, q: a.text }));
        return focusSearch(false);
      case 'focusSearch':
        setPage('main');
        return focusSearch(true);
      case 'help': return openHelp();
      case 'audio': return audioKeys.current?.toggle();
      case 'mute': return audioKeys.current?.mute();
      case 'volume': return audioKeys.current?.volume(a.d);
    }
    if (!on) return;
    switch (a.kind) {
      case 'freq': return document.getElementById('freq')?.focus();
      case 'nudge': return freqHz !== null && tune(freqHz + a.khz * 1000);
      case 'mode': return setMode(a.mode);
      case 'band': {
        const hz = freqHz === null ? null : win.jump(a.dir);
        return hz !== null && tune(hz);
      }
      case 'station': {
        const rows = search.rows, cur = wanted ?? (sel && keyOf(sel.station));
        const i = stepIndex(rows.length, rows.findIndex((r) => keyOf(r) === cur), a.dir);
        return i !== null && pick(rows[i]);
      }
      case 'cand': {
        // wraps: there are only a few on one frequency
        const i = cands.findIndex((c) => c.station.id === sel?.station.id);
        return sel && cands.length > 1 && choose(cands[(i + a.dir + cands.length) % cands.length]);
      }
    }
  });

  const statusLine =
    status === 'connecting' ? t.connecting
    : status === 'lost' ? t.connectionLost
    : rig.link === 'off' ? t.stOff
    : rig.link === 'down' ? t.linkDown
    : rig.power === 'unknown' ? t.noAnswer
    : '';
  // Heard is what the scans heard: no schedule count there
  const heardList = filters.lens === 'heard' && filters.scan === null;
  const counting = search.loading && !search.rows.length && !search.heard.length;
  const count = counting ? '…' : heardList ? t.heardCount(search.total) : t.resultCount(search.total, search.onAir);
  const shortCount = counting ? '' : search.total.toLocaleString(t.locale);

  return (
    <I18nProvider value={t}>
      <NamesProvider value={names}>
      <div className="app" data-view={view} data-page={page === 'stats' ? 'stats' : undefined} data-dock={dockOpen ? 'open' : undefined} data-globe={display.fullGlobe ? 'full' : 'framed'} data-map={display.map}>
        <TopBar
          info={info}
          chrome={api ? chromeFor(api.transport.kind, info?.os) : null}
          status={statusLine}
          on={on}
          powerBusy={powerBusy}
          powerDisabled={status !== 'open' || rig.link === 'off'}
          onPower={() => (on ? setAskOff(true) : setPower(true))}
          askOff={askOff}
          onConfirmOff={() => { setAskOff(false); setPower(false); }}
          onCancelOff={() => setAskOff(false)}
          onSetup={() => openSettings('rig')}
          onScans={() => openScans()}
          scansNew={scansNew}
          onStats={toggleStats}
          statsOn={page === 'stats'}
          listening={listening}
          onFavorites={toggleFavorites}
          favOn={filters.lens === 'favorites'}
          onHelp={() => openHelp()}
          onSettings={() => openSettings()}
          update={api && <UpdateBanner update={update} transport={api.transport.kind} onError={onError} />}
        />

        <main className="stage" aria-label={t.globe}>
          {qth && (
            <Globe
              sel={on ? sel : null}
              qth={qth}
              frame={frame}
              onFrame={toggleFrame}
              paused={page === 'stats' || (phone && view !== 'globe')}
              onInteract={onGlobeTouched}
              onQth={moveQth}
              failedMsg={t.globeFailed}
              sites={sites}
              strengthDb={on ? rig.strength_db : null}
              activeSite={filters.site}
              loggedSites={scan.loggedSites}
              listenedSites={listenedSites}
              scan={globeScan}
              finale={finale}
              onSite={pickSite}
              siteLabel={siteLabel}
              mode={display.map}
              onMode={display.setMap}
            />
          )}
          <Hud faded={touchedGlobe} flat={display.map === 'flat'} />
        </main>

        <section className="panel browse" aria-label={t.stations}>
          <PanelHead own="browse" onView={setView} aside={count} count={shortCount} />
          <StationList
            on={on}
            meta={meta}
            filters={filters}
            onFilters={setFilters}
            rows={search.rows}
            total={search.total}
            facets={search.facets}
            loading={search.loading}
            siteName={siteName}
            scanName={scanName}
            selId={sel?.station.id ?? null}
            logged={scan.logged}
            freshSince={freshSince}
            favs={favs.byStation}
            off={search.off}
            heard={search.heard}
            scanDb={search.scanDb}
            onPick={pick}
            onPickOff={pickOff}
            onAddOff={addStationOff}
            onTuneHz={tune}
            onMore={search.more}
            searchRef={searchRef}
          />
        </section>

        <section className="panel station" aria-label={t.station} aria-live="polite">
          <PanelHead own="station" onView={setView} count={shortCount} />
          <Dossier
            on={on}
            freqHz={freqHz}
            sel={sel}
            others={sel ? cands.filter((c) => c.station.id !== sel.station.id) : []}
            meta={meta}
            now={now}
            logged={scan.logged}
            mine={shownMine}
            star={sel && (
              <FavStar
                fav={selFav}
                onAdd={() => favs.add({ freq_hz: sel.station.freq_hz, mode: sel.station.mode, label: null, station: { source: sel.station.source, key: sel.station.key } })}
                onRename={(label) => selFav && favs.rename(selFav.id, label)}
                onRemove={() => selFav && favs.remove(selFav.id)}
              />
            )}
            onChoose={choose}
            onAdd={addStation}
            onEdit={editStation}
            onDelete={deleteStation}
          />
        </section>

        <footer className="dock">
          <Tuner
            on={on}
            freqHz={freqHz}
            mode={rig.mode}
            strengthDb={rig.strength_db}
            freqsHz={meta?.freqs_hz ?? NO_HZ}
            onAirHz={overview?.on_hz ?? NO_HZ}
            bands={dialBands}
            spans={dial?.spans ?? HF_ONLY}
            coverage={coverage}
            win={win}
            profiles={profiles}
            heat={heat}
            scanBar={
              <ScanBar
                on={on}
                band={win.w.band}
                status={scan.status}
                onStart={startScan}
                onStop={scan.stop}
              />
            }
            star={
              <FavStar
                fav={tunedFav}
                disabled={!on || freqHz === null}
                onAdd={() => freqHz !== null && favs.add({ freq_hz: freqHz, mode: asMode(rig.mode), label: null, station: tunedStation && { source: tunedStation.source, key: tunedStation.key } })}
                onRename={(label) => tunedFav && favs.rename(tunedFav.id, label)}
                onRemove={() => tunedFav && favs.remove(tunedFav.id)}
              />
            }
            expanded={dockOpen}
            onExpand={() => setDockOpen((o) => !o)}
            onTune={tune}
            onMode={setMode}
          />
          {/* always mounted, so audio keeps playing whatever else opens */}
          {info?.audio && api ? (
            <AudioPanel openAudio={api.transport.openAudio.bind(api.transport)} rate={info.audio_rate} buffer={{ cushionMs: info.audio_cushion_ms, maxAheadMs: info.audio_max_ahead_ms }} mode={rig.mode} cwPitch={rig.cw_pitch_hz} freqHz={freqHz} onTune={tune} keys={audioKeys} />
          ) : (
            info && <AudioOff onSetUp={() => openSettings('audio')} />
          )}
        </footer>

        {page === 'stats' && (
          <StatsView
            api={api}
            collection={display.collection}
            tab={statsTab}
            onTab={setStatsTab}
            period={statsPeriod}
            onPeriod={setStatsPeriod}
            clock={now}
            onBack={() => setPage('main')}
            usual={listening?.now ? null : listening?.usual ?? null}
            onTune={(hz, mode, st) => {
              tuneAs(hz, mode);
              if (st) {
                setWanted(keyOf(st));
                hearing(st);
              }
            }}
            onCountry={statsCountry}
            onShowGlobe={statsGlobe}
            onError={onError}
          />
        )}

        <ViewTabs className="nav" views={['globe', 'station', 'browse']} current={view} onView={setView} count={shortCount} />

        <p className="toast" role="alert">{msg && t.error(msg)}</p>

        {sheet?.kind === 'settings' && api && (
          <Settings
            api={api}
            os={info?.os ?? 'linux'}
            rig={rig}
            update={update}
            display={display}
            meta={meta}
            mine={mine}
            tab={sheet.tab}
            onTab={(tab) => setSheet({ kind: 'settings', tab })}
            onClose={() => setSheet(null)}
            onError={onError}
            onApplied={reloadInfo}
          />
        )}
        {sheet?.kind === 'scans' && (
          <Scans
            scan={scan}
            openId={sheet.id}
            onOpen={(id) => openScans(id)}
            onTune={tune}
            onShow={showScan}
            onClose={() => setSheet(null)}
          />
        )}
        {sheet?.kind === 'station' && api && (
          <StationForm
            key={sheet.id ?? 'new'}
            api={api}
            id={sheet.id}
            draft={sheet.draft}
            qth={qth}
            onSave={(d) => saveStation(sheet.id, d)}
            onClose={() => setSheet(null)}
          />
        )}
        {sheet?.kind === 'help' && (
          <Help
            topic={sheet.topic}
            onTopic={(topic) => setSheet({ kind: 'help', topic })}
            version={info?.version ?? null}
            meta={meta}
            onClose={() => setSheet(null)}
          />
        )}
        {welcome.open && <Welcome onClose={welcome.close} />}
      </div>
      </NamesProvider>
    </I18nProvider>
  );
}
