// The window is the receiver: a top bar, the globe in the middle with the station list on its
// left and the tuned station on its right, and the faceplate (dial and audio) along the bottom.
// Smaller screens fold the side panels into tabs (styles.css); `view` says which one shows.
// The side panels fold to rails and the faceplate's cells to one line (`data-folded`, useFolds).
// The globe may also be drawn behind the whole window (`data-globe`, Settings → Display).
// Stats (`data-page="stats"`) takes the middle of the window; the top bar and faceplate stay.

import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AudioOff, AudioPanel, type AudioKeys } from './components/AudioPanel';
import { Dossier } from './components/Dossier';
import { FavStar } from './components/FavStar';
import { asMode, favAt } from './components/favorites';
import { Globe } from './components/Globe';
import { Hud } from './components/Hud';
import { ScanBar } from './components/ScanBar';
import type { StatsTab } from './components/stats/StatsView';
import { PanelHead, ViewTabs, type View } from './components/NavBar';
import type { SettingsTab } from './components/settings/Settings';
import { draftFrom, draftFromOff, draftHere, draftOf } from './components/stationDraft';
import { StationList } from './components/StationList';
import { TopBar } from './components/TopBar';
import { Tuner, useBandWindow } from './components/Tuner';
import { DEFAULT_COVERAGE, HF_SPAN } from './components/dial';
import { bandOf, noiseFloor, scanFilters } from './components/scan';
import { keyOf, MINE } from './components/sources';
import { UpdateBanner } from './components/UpdateBanner';
import { chromeFor } from './api/window';
import { goTo } from './page';
import { makeNames, NamesProvider } from './catalog/names';
import { NO_FILTERS, useCandidates, useCore, useDial, useNow, useOverview, useStationSearch, useStationsMeta, useTuning, type Filters } from './hooks/useCore';
import { COARSE, PHONE, useMedia } from './hooks/useMedia';
import { useFolds } from './hooks/useFolds';
import { useFavorites } from './hooks/useFavorites';
import { findSite, oneOf, readFilters, readSiteRef, readString, usePersisted, type SiteRef } from './hooks/usePersisted';
import { useDisplay } from './hooks/useDisplay';
import { useBandScans, useScan } from './hooks/useScan';
import { useRemote } from './hooks/useRemote';
import { useUpdate } from './hooks/useUpdate';
import { useUserStations } from './hooks/useUserStations';
import { TipsProvider, useTipsState } from './hooks/useTips';
import { useTour } from './hooks/useTour';
import { useWelcome } from './hooks/useWelcome';
import { useFreqHeat, useListening, useStats } from './hooks/useStats';
import { useNetCountries, useNetOverview, useNetSearch } from './hooks/useNetSearch';
import { useNetFavorites } from './hooks/useNetFavorites';
import { NetDossier } from './components/NetDossier';
import type { NetFace } from './components/Tuner';
import type { NetPin } from './components/stage/useStage';
import { audioCell, dimSites, favFilter, isNet, leaveNet, mixesNet, NET_BAND, NET_DOT_ID, NET_RATE, netCountryOpts, netPhase, netSiteDots, stepChannel, streamLabel } from './components/net';
import { useNetNow } from './hooks/useNetNow';
import { fmtKhz, fmtStamp, I18nProvider, pickMessages } from './i18n';
import type { HelpTopic } from './i18n/en';
import { stepIndex, type Action } from './keys/keymap';
import { useKeys } from './keys/useKeys';
import type { TourStep } from './tour/tour';
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

// What opens on demand loads then (its own chunk), not with the app.
const Help = lazy(() => import('./components/Help').then((m) => ({ default: m.Help })));
const Scans = lazy(() => import('./components/Scans').then((m) => ({ default: m.Scans })));
const Settings = lazy(() => import('./components/settings/Settings').then((m) => ({ default: m.Settings })));
const StationForm = lazy(() => import('./components/StationForm').then((m) => ({ default: m.StationForm })));
const StatsView = lazy(() => import('./components/stats/StatsView').then((m) => ({ default: m.StatsView })));
const Tour = lazy(() => import('./components/Tour').then((m) => ({ default: m.Tour })));
const Welcome = lazy(() => import('./components/Welcome').then((m) => ({ default: m.Welcome })));

const MSG_MS = 6000;
/** How long the rows a finished scan heard stay lit. */
const FRESH_MS = 10000;
/** How often the top bar asks whether remote access is on and who is connected. */
const REMOTE_LAMP_MS = 5000;
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
  const { api, info, reloadInfo, rig, meter, status } = useCore();
  const update = useUpdate(api);
  // the top bar's remote access lamp, in the desktop window (a browser is the remote)
  const { status: remote } = useRemote(api, api?.transport.kind === 'tauri' ? REMOTE_LAMP_MS : null);
  const display = useDisplay();
  const welcome = useWelcome();
  const [sheet, setSheet] = useState<SheetState>(null);
  // the view, the list's filters, the station picked and the camera come back on this device
  // (usePersisted)
  const [view, setViewRaw] = usePersisted<View>('swatlas.view', 'globe', oneOf('globe', 'station', 'browse'));
  // Stats over the middle of the window, its tab and period
  const [page, setPage] = usePersisted<'main' | 'stats'>('swatlas.page', 'main', oneOf('main', 'stats'));
  const [statsTab, setStatsTab] = usePersisted<StatsTab>('swatlas.statsTab', 'overview', oneOf('overview', 'collection', 'propagation', 'hunt'));
  const [statsPeriod, setStatsPeriod] = usePersisted<Period>('swatlas.statsPeriod', 'week', oneOf('day', 'week', 'month', 'year', 'all'));
  /** Another view (the phone's bottom tabs, a list action) leaves Stats. */
  const setView = useCallback((v: View) => { setPage('main'); setViewRaw(v); }, []);
  const phone = useMedia(PHONE);
  // a phone's browser may go (back) to the simple remote (page.ts)
  const coarse = useMedia(COARSE);
  const simple = api?.transport.kind === 'ws' && phone && coarse;
  const folds = useFolds();
  /** Bring the station list into sight: its view on phones (leaving Stats), elsewhere its tab,
   *  unfolded. */
  const showList = () => {
    if (phone) return setView('browse');
    setViewRaw('browse');
    folds.reveal('browse');
  };
  /** A panel's tabs or rail: that view, unfolded. */
  const openView = (v: View) => { setView(v); if (v !== 'globe') folds.reveal(v); };
  // the tour walks the views and leaves the listener on the one they were on; the tips wait
  // while anything covers the window
  const tour = useTour();
  const tips = useTipsState(welcome.open || tour.open || sheet !== null);
  const tourFrom = useRef<View | null>(null);
  const startTour = () => {
    tourFrom.current = view;
    setSheet(null);
    setPage('main');
    tour.start();
  };
  const endTour = () => {
    tour.finish();
    if (tourFrom.current) setViewRaw(tourFrom.current);
    tourFrom.current = null;
  };
  const showStep = (s: TourStep) => {
    if (s.view) setView(s.view);
    if (s.reveal) folds.reveal(s.reveal);
  };
  const [touchedGlobe, setTouchedGlobe] = useState(false);
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
  const [filters, setFilters] = usePersisted<Filters>('swatlas.filters', NO_FILTERS, readFilters);
  // the lenses change with the logbook, the favorites and the listening history (a session
  // starting or ending), and every list with the catalog
  const sessionKey = filters.lens === 'listened' ? listening?.now?.session.id ?? null : null;
  // the band dial's heat strip: again when a session starts or ends, and on leaving Stats (where
  // the history can be cleared)
  const heat = useFreqHeat(api, `${listening?.now?.session.id ?? ''}|${page}`);
  const lensKey = useMemo(() => ({}), [scan.logged, favs.views, generation, sessionKey]);
  // the NET band (internet stations) has its own search; the schedule's doesn't run meanwhile
  const netEnabled = info?.net ?? false;
  const netActive = isNet(filters, netEnabled);
  // Favorites lists every favorite on any band, NET included
  const search = useStationSearch(netActive && filters.lens !== 'favorites' ? null : api, filters, lensKey);
  // the directory: the NET band's list (its Favorites lens lists the favorites kept instead), or
  // the internet stations shown among a radio search's results
  const netMixed = netEnabled && !netActive && mixesNet(filters);
  const netSearch = useNetSearch(api, filters, (netActive && filters.lens !== 'favorites') || netMixed);
  const netCountries = useNetCountries(api, netActive || netEnabled);
  const netOverview = useNetOverview(api, netActive);
  const { netNow, setNetNow, reloadNetNow } = useNetNow(api, netEnabled);
  const netFavs = useNetFavorites(api, netEnabled, onError);
  const netFavUuids = useMemo(() => new Set(netFavs.list.map((f) => f.uuid)), [netFavs.list]);
  const netCountryName = useMemo(() => {
    const m = new Map(netCountryOpts(netCountries, t.locale).map((c) => [c.value, c.label]));
    return (itu: string | null) => (itu ? m.get(itu) ?? names.country(itu) : '');
  }, [netCountries, t.locale, names]);
  // internet radio turned off while the list was on NET: back to the radio bands
  // (known once `info` is: a NET list kept from the last run waits for it)
  useEffect(() => {
    if (info && !netEnabled) setFilters((f) => leaveNet(f));
  }, [info, netEnabled]);
  const cands = useCandidates(api, on ? freqHz : null, `${qth?.lat},${qth?.lon},${now.getTime()},${generation}`);

  // Keep the chosen station while it is still a candidate; otherwise take the best one. It is
  // kept by source and key (`keyOf`), which hold when the catalog changes and the ids do not.
  const [sel, setSel] = useState<Candidate | null>(null);
  const [wanted, setWanted] = usePersisted<string | null>('swatlas.station', null, readString);
  // the globe's listened sites, while the list is on Listened
  const lensListened = filters.lens === 'listened' && page === 'main';
  const { stats: allTime } = useStats(api, 'all', lensListened);
  // in steps of 2 %, and the same map while they hold: a new one rebuilds the globe's towers
  const listenedPrev = useRef(new Map<number, number>());
  const listenedSites = useMemo(() => {
    const m = new Map<number, number>();
    if (lensListened && allTime) {
      const max = Math.max(1, ...allTime.sites.map((x) => x.seconds));
      for (const x of allTime.sites) m.set(x.site, Math.round((x.seconds / max) * 50) / 50);
    }
    const prev = listenedPrev.current;
    if (prev.size === m.size && [...m].every(([k, v]) => prev.get(k) === v)) return prev;
    return (listenedPrev.current = m);
  }, [lensListened, allTime]);
  useEffect(() => {
    if (!on || !cands.length) setSel(null);
    else setSel((cur) => cands.find((c) => keyOf(c.station) === (wanted ?? (cur && keyOf(cur.station)))) ?? cands[0]);
  }, [cands, on, wanted]);

  const sites = overview?.sites ?? NO_SITES;
  // the map: the countries with internet stations while the list is on NET; the transmitter sites
  // dimmed while an internet station is tuned
  const netDots = useMemo(() => netSiteDots(netOverview, netCountryName), [netOverview, netCountryName]);
  const netDotsRef = useRef(netDots);
  netDotsRef.current = netDots;
  const dimmed = useMemo(() => dimSites(sites), [sites]);
  const globeSites = netActive ? netDots : netNow ? dimmed : sites;
  const netPin = useMemo<NetPin | null>(() => {
    const p = netNow?.place;
    if (!netNow || !p) return null;
    const country = netCountryName(netNow.station.itu);
    const playing = netNow.state.state === 'playing';
    return { key: netNow.station.uuid, lat: p.lat, lon: p.lon, name: netNow.station.name, sub: p.precise ? country : `${country} · ${t.netApproxPlace}`, flag: netNow.station.flag, playing };
  }, [netNow?.station.uuid, netNow?.place?.lat, netNow?.place?.lon, netNow?.state.state, netCountryName, t]);
  const siteName = filters.site === null ? null : sites.find((s) => s.id === filters.site)?.name ?? null;
  const scanSel = filters.scan === null ? null : scan.scans.find((s) => s.id === filters.scan) ?? null;
  const scanName = scanSel && `${names.dialBand(bandOf(scanSel))} · ${fmtStamp(t, scanSel.started)}`;
  // the scan the list was narrowed to was deleted (or, kept from the last run, is gone)
  useEffect(() => {
    if (scan.loaded && filters.scan !== null && !scan.scans.some((s) => s.id === filters.scan)) setFilters((f) => ({ ...f, scan: null }));
  }, [filters.scan, scan.scans, scan.loaded]);
  // The transmitter site the list is narrowed to, kept by what it is (its id changes with the
  // catalog): found again whenever the sites come in, and the list unnarrowed if it isn't there.
  const [siteRef, setSiteRef] = usePersisted<SiteRef | null>('swatlas.site', null, readSiteRef);
  const siteRefNow = useRef(siteRef);
  siteRefNow.current = siteRef;
  const sitesIn = useRef(false);
  useEffect(() => {
    if (!overview) return;
    const ref = siteRefNow.current;
    const id = ref ? findSite(sites, ref)?.id ?? null : null;
    if (ref && id === null) setSiteRef(null);
    setFilters((f) => (f.site === id ? f : { ...f, site: id }));
    sitesIn.current = true;
  }, [sites]);
  // the list narrowed to another site, or to none
  useEffect(() => {
    if (!sitesIn.current) return;
    const s = filters.site === null ? null : sites.find((x) => x.id === filters.site);
    setSiteRef(s ? { name: s.name, lat: s.lat, lon: s.lon } : null);
  }, [filters.site]);
  const showListRef = useRef(showList);
  showListRef.current = showList;
  const siteLabel = useCallback((s: SiteDot) => (s.id >= NET_DOT_ID
    ? { country: t.netBand, onAir: t.netDotCount(s.total) }
    : { country: names.country(s.itu), onAir: t.siteOnAir(s.on, s.total) }), [names, t]);
  // the globe's callbacks are stable, so it only renders again when what it shows changes; a
  // country of the NET band narrows its list to that country
  const pickSite = useCallback((id: number) => {
    if (id >= NET_DOT_ID) {
      const dot = netDotsRef.current.find((d) => d.id === id);
      if (dot) setFilters((f) => ({ ...f, countries: f.countries[0] === dot.itu ? [] : [dot.itu] }));
    } else {
      setFilters((f) => ({ ...leaveNet(f), site: f.site === id ? null : id }));
    }
    showListRef.current();
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
    setFilters((f) => ({ ...leaveNet(f), countries: [itu], lens: 'all', scan: null, site: null }));
    setPage('main');
    showList();
  };
  const statsGlobe = () => {
    setFilters((f) => ({ ...leaveNet(f), lens: 'listened', scan: null }));
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
    showList();
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
    setFilters((f) => scanFilters(f, id));
    showList();
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
  // every favorite, radio and NET; the band and the other filters stay for the list it goes back to
  const toggleFavorites = () => {
    const to = filters.lens === 'favorites' ? 'all' : 'favorites';
    setFilters((f) => ({ ...f, lens: to }));
    if (to === 'favorites') showList();
  };

  // the stars: the station shown, and the tuned frequency (with the station if it is on it)
  const selFav = sel ? favs.byStation.get(sel.station.id) ?? null : null;
  const tunedStation = sel && sel.station.freq_hz === freqHz ? sel.station : null;
  const tunedFav = on ? favAt(favs.views, freqHz, tunedStation?.id ?? null)?.favorite ?? null : null;

  // the keyboard (keys/keymap.ts): typing anywhere starts a new search
  const searchRef = useRef<HTMLInputElement>(null);
  const audioKeys = useRef<AudioKeys>(null);
  // the audio panel plays: the map's tuned point beats
  const [audioOn, setAudioOn] = useState(false);
  // Listen pressed on an internet station: ask how its stream is doing now, not at the next poll
  useEffect(() => { if (audioOn) reloadNetNow(); }, [audioOn, reloadNetNow]);
  // an internet station instead of the rig: tuned, as a radio station is; Listen (the audio
  // panel) plays it, and while listening another one switches at once
  // A click on the station tuned (or being tuned) asks for no new stream: the second click of a
  // double-click would restart it. One that failed can be clicked again to retry.
  const netAsked = useRef<string | null>(null);
  const tuneNet = (uuid: string) => {
    if (netAsked.current === uuid || (netNow?.station.uuid === uuid && netNow.state.state !== 'failed')) return;
    netAsked.current = uuid;
    api?.netTune(uuid)
      .then(setNetNow, (e) => {
        if (listenFor.current === uuid) listenFor.current = null;
        onError(e);
      })
      .finally(() => { if (netAsked.current === uuid) netAsked.current = null; });
  };
  const stopNet = () => {
    setNetNow(null);
    api?.netStop().catch(onError);
  };
  // A double-click (or Shift+Enter) on a row in the list: tune it and listen. An internet station
  // can be listened to once its tune lands (with the rig's audio off there is nothing until then);
  // a radio one leaves the internet station for the rig.
  const listenFor = useRef<string | null>(null);
  useEffect(() => {
    if (!netNow || listenFor.current !== netNow.station.uuid) return;
    listenFor.current = null;
    audioKeys.current?.listen();
  }, [netNow]);
  const playNet = (uuid: string) => {
    tuneNet(uuid);
    if (netAsked.current === uuid) listenFor.current = uuid;
    else audioKeys.current?.listen();
  };
  const playRadio = (tuneIt: () => void) => {
    listenFor.current = null;
    if (netNow) stopNet();
    tuneIt();
    if (info?.audio) audioKeys.current?.listen();
  };
  const netTuned = netNow?.station.uuid ?? null;
  const netFav = netTuned ? netFavs.byUuid.get(netTuned) ?? null : null;
  const netStar = netTuned && (
    <FavStar
      fav={netFav}
      onAdd={() => netFavs.add(netTuned)}
      onRename={(label) => netFav && netFavs.rename(netFav.id, label)}
      onRemove={() => netFav && netFavs.remove(netFav.id)}
    />
  );
  // the knob, the arrows and the tuning keys step the memory channels (the favorites), or the
  // list when there are none
  const stepNet = (dir: number) => {
    const list: readonly { uuid: string }[] = netFavs.list.length ? netFavs.list : netSearch.items;
    const next = stepChannel(list, netTuned, dir);
    if (next) tuneNet(next.uuid);
  };
  const netFace: NetFace | null = netNow && {
    name: netNow.station.name,
    flag: netNow.station.flag,
    country: netCountryName(netNow.station.itu),
    stream: streamLabel(netNow.station),
    bufferMs: ['playing', 'connecting', 'buffering'].includes(netNow.state.state) ? netNow.buffer_ms : null,
    bufferTargetMs: netNow.buffer_target_ms,
    bufferMaxMs: netNow.buffer_max_ms,
    bufferSetting: netNow.buffer_setting_ms,
    phase: netPhase(netNow, audioOn),
    onBuffer: (ms) => { api?.setNetBuffer(ms).then(reloadNetNow, onError); },
    channels: netFavs.list.map((f) => ({ uuid: f.uuid, label: f.label || f.name })),
    uuid: netNow.station.uuid,
    rig: on && freqHz !== null ? `${fmtKhz(freqHz)} kHz${rig.mode ? ` ${rig.mode}` : ''}` : '',
    onChannel: tuneNet,
    onStep: stepNet,
    onBackToRig: stopNet,
  };
  const toNetBand = () => {
    setFilters((f) => ({ ...f, bands: [NET_BAND], lens: 'all' }));
    showList();
  };
  const focusSearch = (select: boolean) => {
    const hidden = phone || folds.folded('browse');
    showList();
    const go = () => {
      const el = searchRef.current;
      el?.focus();
      if (select) el?.select();
    };
    go();
    // a hidden panel only shows after the change renders
    if (hidden) requestAnimationFrame(go);
  };
  useKeys(sheet !== null || askOff || welcome.open || tour.open, (a: Action) => {
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
    if (netNow) {
      // tuned to an internet station: the tuning keys step its memory channels
      switch (a.kind) {
        case 'nudge': return stepNet(a.khz > 0 ? 1 : -1);
        case 'band': return stepNet(a.dir);
      }
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
  const favList = filters.lens === 'favorites';
  const counting = search.loading && !search.rows.length && !search.heard.length && !search.favorites.length;
  const netCount = netSearch.loading && !netSearch.items.length ? '…' : t.netCount(netSearch.items.length, netSearch.more);
  // Favorites counts both kinds the text keeps: radio and NET
  const favTotal = search.total + (netEnabled ? favFilter(netFavs.list, filters.q).length : 0);
  const count = favList ? (counting ? '…' : t.favCount(favTotal))
    : netActive ? netCount : counting ? '…' : heardList ? t.heardCount(search.total) : t.resultCount(search.total, search.onAir);
  const shortCount = favList ? (counting ? '' : favTotal.toLocaleString(t.locale))
    : netActive ? `${netSearch.items.length}${netSearch.more ? '+' : ''}` : counting ? '' : search.total.toLocaleString(t.locale);

  return (
    <I18nProvider value={t}>
      <NamesProvider value={names}>
      <TipsProvider value={tips}>
      <div className="app" data-view={view} data-page={page === 'stats' ? 'stats' : undefined} data-folded={folds.attr || undefined} data-globe={display.fullGlobe ? 'full' : 'framed'} data-map={display.map}>
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
          onScans={() => { tips.dismiss('scans'); openScans(); }}
          scansNew={scansNew}
          onStats={() => { tips.dismiss('stats'); toggleStats(); }}
          statsOn={page === 'stats'}
          listening={listening}
          onFavorites={() => { tips.dismiss('favorites'); toggleFavorites(); }}
          favOn={filters.lens === 'favorites'}
          onHelp={() => openHelp()}
          onSettings={() => openSettings()}
          remote={remote?.running ? remote.clients : null}
          onRemote={() => openSettings('remote')}
          onSimple={simple ? () => goTo('remote') : null}
          update={api && <UpdateBanner update={update} transport={api.transport.kind} onError={onError} />}
        />

        <main className="stage" data-tour="globe" aria-label={t.globe}>
          {qth && (
            <Globe
              sel={on && !netNow ? sel : null}
              net={netPin}
              listening={audioOn}
              qth={qth}
              frame={frame}
              onFrame={toggleFrame}
              paused={page === 'stats' || (phone && view !== 'globe')}
              onInteract={onGlobeTouched}
              onQth={moveQth}
              failedMsg={t.globeFailed}
              sites={globeSites}
              meter={on && !netNow ? meter : null}
              activeSite={netActive ? netDots.find((d) => d.itu === filters.countries[0])?.id ?? null : filters.site}
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

        <section className={'panel browse' + (folds.folded('browse') ? ' folded' : '')} id="panel-browse" data-tour="browse" aria-label={t.stations}>
          <PanelHead own="browse" onView={openView} aside={count} count={shortCount} folded={folds.folded('browse')} onFold={() => folds.toggle('browse')} panelId="panel-browse" />
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
            favorites={search.favorites}
            scanDb={search.scanDb}
            onPick={pick}
            onPickOff={pickOff}
            onPlay={(r) => playRadio(() => pick(r))}
            onPlayOff={(o) => playRadio(() => pickOff(o))}
            onAddOff={addStationOff}
            onTuneHz={tune}
            onMore={search.more}
            searchRef={searchRef}
            net={{
              enabled: netEnabled,
              active: netActive,
              mixed: netMixed,
              search: netSearch,
              countries: netCountries,
              tuned: netTuned,
              favs: netFavs.list,
              favUuids: netFavUuids,
              onTune: tuneNet,
              onPlay: playNet,
            }}
          />
        </section>

        <section className={'panel station' + (folds.folded('station') ? ' folded' : '')} id="panel-station" data-tour="station" aria-label={t.station} aria-live="polite">
          <PanelHead own="station" onView={openView} count={shortCount} folded={folds.folded('station')} onFold={() => folds.toggle('station')} panelId="panel-station" />
          {netNow ? (
            <NetDossier now={netNow} country={netCountryName(netNow.station.itu)} star={netStar} onBackToRig={stopNet} />
          ) : (
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
          )}
        </section>

        <footer className="dock">
          <Tuner
            on={on}
            freqHz={freqHz}
            mode={rig.mode}
            meter={meter}
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
                stuckDb={bandScans.summary?.stuck_db ?? null}
                onStart={startScan}
                onStop={scan.stop}
              />
            }
            star={netStar || (
              <FavStar
                fav={tunedFav}
                disabled={!on || freqHz === null}
                onAdd={() => freqHz !== null && favs.add({ freq_hz: freqHz, mode: asMode(rig.mode), label: null, station: tunedStation && { source: tunedStation.source, key: tunedStation.key } })}
                onRename={(label) => tunedFav && favs.rename(tunedFav.id, label)}
                onRemove={() => tunedFav && favs.remove(tunedFav.id)}
              />
            )}
            expanded={!folds.folded('rx')}
            onExpand={() => folds.toggle('rx')}
            onTune={tune}
            onMode={setMode}
            net={netFace}
            onNetSpan={netEnabled ? () => { tips.dismiss('net'); toNetBand(); } : null}
          />
          {/* always mounted, so audio keeps playing whatever else opens */}
          {info && api && audioCell(info.audio, netEnabled) === 'panel' ? (
            <AudioPanel
              openAudio={api.transport.openAudio.bind(api.transport)}
              rate={netNow ? NET_RATE : info.audio_rate}
              buffer={{ cushionMs: info.audio_cushion_ms, maxAheadMs: info.audio_max_ahead_ms }}
              mode={rig.mode}
              cwPitch={rig.cw_pitch_hz}
              freqHz={freqHz}
              onTune={tune}
              keys={audioKeys}
              rigAudio={info.audio}
              net={netNow}
              onSetUp={() => openSettings('audio')}
              onListening={setAudioOn}
              expanded={!folds.folded('audio')}
              onExpand={() => folds.toggle('audio')}
            />
          ) : (
            info && <AudioOff onSetUp={() => openSettings('audio')} />
          )}
        </footer>

        <Suspense fallback={null}>
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
        </Suspense>

        <ViewTabs className="nav" views={['globe', 'station', 'browse']} current={view} onView={setView} count={shortCount} />

        <p className="toast" role="alert">{msg && t.error(msg)}</p>

        <Suspense fallback={null}>
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
            net={netEnabled}
            onNetOn={toNetBand}
            listen={info?.audio ?? false}
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
            api={api}
            offered={update.status && 'release' in update.status.state ? update.status.state.release ?? null : null}
            onTour={startTour}
            onTipsReset={tips.reset}
            onClose={() => setSheet(null)}
          />
        )}
        {welcome.open && (
          <Welcome
            offerTour={!tour.done}
            onClose={(dontShow, go) => { welcome.close(dontShow); if (go) startTour(); }}
          />
        )}
        {tour.open && <Tour index={tour.index} onShow={showStep} onIndex={tour.go} onClose={endTour} />}
        </Suspense>
      </div>
      </TipsProvider>
      </NamesProvider>
    </I18nProvider>
  );
}
