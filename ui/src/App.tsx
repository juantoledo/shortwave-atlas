// The window is the receiver: a top bar, the globe in the middle with the station list on its
// left and the tuned station on its right, and the faceplate (dial and audio) along the bottom.
// Smaller screens fold the side panels into tabs (styles.css); `view` says which one shows.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AudioOff, AudioPanel, type AudioKeys } from './components/AudioPanel';
import { Dossier } from './components/Dossier';
import { Globe } from './components/Globe';
import { Help } from './components/Help';
import { Hud } from './components/Hud';
import { PanelHead, ViewTabs, type View } from './components/NavBar';
import { Settings, type SettingsTab } from './components/settings/Settings';
import { StationList } from './components/StationList';
import { TopBar } from './components/TopBar';
import { Tuner } from './components/Tuner';
import { adjacentBand, bandEntryHz } from './components/dial';
import { UpdateBanner } from './components/UpdateBanner';
import { makeNames, NamesProvider } from './catalog/names';
import { NO_FILTERS, useCandidates, useCore, useNow, useOverview, useStationSearch, useStationsMeta, useTuning, type Filters } from './hooks/useCore';
import { PHONE, useMedia } from './hooks/useMedia';
import { useUpdate } from './hooks/useUpdate';
import { I18nProvider, pickMessages } from './i18n';
import type { HelpTopic } from './i18n/en';
import { stepIndex, type Action } from './keys/keymap';
import { useKeys } from './keys/useKeys';
import type { Candidate } from './types/generated/Candidate';
import type { Mode } from './types/generated/Mode';
import type { Qth } from './types/generated/Qth';
import type { SiteDot } from './types/generated/SiteDot';
import type { StationRow } from './types/generated/StationRow';

const MSG_MS = 6000;
const NO_HZ: number[] = [];

type SheetState = { kind: 'settings'; tab: SettingsTab } | { kind: 'help'; topic: HelpTopic | null } | null;

export function App() {
  const { api, info, reloadInfo, rig, status } = useCore();
  const update = useUpdate(api);
  const [sheet, setSheet] = useState<SheetState>(null);
  const [view, setView] = useState<View>('globe');
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
  const { freqHz, tune } = useTuning(api, rig.freq_hz, onError);
  const [qth, setQth] = useState<Qth | null>(null);
  useEffect(() => { if (info) setQth(info.qth); }, [info]);

  const now = useNow(20_000);
  const meta = useStationsMeta(api);
  const overview = useOverview(api);
  const names = useMemo(() => makeNames(meta, t), [meta, t]);
  const [filters, setFilters] = useState<Filters>(NO_FILTERS);
  const search = useStationSearch(api, filters);
  const cands = useCandidates(api, on ? freqHz : null, `${qth?.lat},${qth?.lon},${now.getTime()}`);

  // Keep the chosen station while it is still a candidate; otherwise take the best one.
  const [sel, setSel] = useState<Candidate | null>(null);
  const [wanted, setWanted] = useState<number | null>(null);
  useEffect(() => {
    if (!on || !cands.length) setSel(null);
    else setSel((cur) => cands.find((c) => c.station.id === (wanted ?? cur?.station.id)) ?? cands[0]);
  }, [cands, on, wanted]);

  const sites = overview?.sites ?? [];
  const siteName = filters.site === null ? null : sites.find((s) => s.id === filters.site)?.name ?? null;
  const siteLabel = useCallback((s: SiteDot) => ({ country: names.country(s.itu), onAir: t.siteOnAir(s.on, s.total) }), [names, t]);
  const pickSite = (id: number) => {
    setFilters((f) => ({ ...f, site: f.site === id ? null : id }));
    setView('browse');
  };

  const [frame, setFrame] = useState(true);
  const [powerBusy, setPowerBusy] = useState(false);

  const setMode = (mode: Mode) => api?.rig({ cmd: 'set_mode', mode }).catch(onError);
  const pick = (r: StationRow) => {
    if (rig.mode !== r.mode) setMode(r.mode);
    tune(r.freq_hz);
    setWanted(r.id);
  };
  const choose = (c: Candidate) => {
    setWanted(c.station.id);
    setSel(c);
  };
  const togglePower = async () => {
    if (!api || powerBusy) return;
    if (on && !window.confirm(t.confirmOff)) return;
    setPowerBusy(true);
    try {
      await api.rig({ cmd: 'set_power', on: !on });
    } catch (e) {
      onError(e);
    }
    setPowerBusy(false);
  };
  const moveQth = (q: Qth) => {
    setQth(q);
    api?.setQth(q).catch(onError);
  };
  const onGlobeTouched = useCallback(() => setTouchedGlobe(true), []);

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
  useKeys(sheet !== null, (a: Action) => {
    switch (a.kind) {
      case 'search':
        setFilters((f) => ({ ...f, q: a.text }));
        return focusSearch(false);
      case 'focusSearch': return focusSearch(true);
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
        const b = freqHz === null ? null : adjacentBand(freqHz / 1000, meta?.bands ?? [], a.dir);
        return b && tune(bandEntryHz(b, overview?.on_hz ?? NO_HZ));
      }
      case 'station': {
        const rows = search.rows, cur = wanted ?? sel?.station.id;
        const i = stepIndex(rows.length, rows.findIndex((r) => r.id === cur), a.dir);
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
  const count = search.loading && !search.rows.length ? '…' : t.resultCount(search.total, search.onAir);
  const shortCount = search.loading && !search.rows.length ? '' : search.total.toLocaleString(t.locale);

  return (
    <I18nProvider value={t}>
      <NamesProvider value={names}>
      <div className="app" data-view={view} data-dock={dockOpen ? 'open' : undefined}>
        <TopBar
          info={info}
          status={statusLine}
          on={on}
          powerBusy={powerBusy}
          powerDisabled={status !== 'open' || rig.link === 'off'}
          onPower={togglePower}
          onSetup={() => openSettings('rig')}
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
              onFrame={() => setFrame((f) => !f)}
              paused={phone && view !== 'globe'}
              onInteract={onGlobeTouched}
              onQth={moveQth}
              failedMsg={t.globeFailed}
              sites={sites}
              activeSite={filters.site}
              onSite={pickSite}
              siteLabel={siteLabel}
            />
          )}
          <Hud faded={touchedGlobe} />
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
            selId={sel?.station.id ?? null}
            onPick={pick}
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
            onChoose={choose}
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
            bands={meta?.bands ?? []}
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

        <ViewTabs className="nav" views={['globe', 'station', 'browse']} current={view} onView={setView} count={shortCount} />

        <p className="toast" role="alert">{msg && t.error(msg)}</p>

        {sheet?.kind === 'settings' && api && (
          <Settings
            api={api}
            os={info?.os ?? 'linux'}
            rig={rig}
            update={update}
            tab={sheet.tab}
            onTab={(tab) => setSheet({ kind: 'settings', tab })}
            onClose={() => setSheet(null)}
            onError={onError}
            onApplied={reloadInfo}
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
      </div>
      </NamesProvider>
    </I18nProvider>
  );
}
