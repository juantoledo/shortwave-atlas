// The phone remote (remote.html): the receiver on one screen, for a thumb. What is tuned and who is
// on it, the S-meter, steps, a drag strip and a keypad to tune, a band scan, four lists (this band's
// stations, on the air first; what the scans heard; the favorites; a search) and Listen at the
// bottom. The same core and calls as
// the atlas, without the globe; "Full atlas" opens that (page.ts remembers the choice).

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { makeNames, NamesProvider } from '../catalog/names';
import { asMode, favAt } from '../components/favorites';
import { DEFAULT_COVERAGE } from '../components/dial';
import { canListen, stepChannel } from '../components/net';
import { keyOf } from '../components/sources';
import { useBandWindow } from '../components/Tuner';
import { NO_FILTERS, useCandidates, useCore, useDial, useNow, useOverview, useStationSearch, useStationsMeta, useTuning } from '../hooks/useCore';
import { useFavorites } from '../hooks/useFavorites';
import { useNetFavorites } from '../hooks/useNetFavorites';
import { useNetNow } from '../hooks/useNetNow';
import { useNetSearch } from '../hooks/useNetSearch';
import { useScan } from '../hooks/useScan';
import { oneOf, usePersisted } from '../hooks/usePersisted';
import { fmtKhz, fmtStamp, I18nProvider, pickMessages, sLabel } from '../i18n';
import { goTo } from '../page';
import type { Band } from '../types/generated/Band';
import type { Candidate } from '../types/generated/Candidate';
import type { FavoriteView } from '../types/generated/FavoriteView';
import type { Mode } from '../types/generated/Mode';
import type { StationKey } from '../types/generated/StationKey';
import type { StationRow } from '../types/generated/StationRow';
import { Keypad, ListenBar, NetCard, NetRows, NowCard, Readout, Row, ScanButton, SMeter, Star, StationRows, TuneBar } from './parts';
import { fineStepHz, listBand, onAirFirst, utcNow } from './logic';
import { useListen } from './useListen';

type Tab = 'air' | 'heard' | 'favs' | 'search';

const MSG_MS = 6000;
const NO_HZ: number[] = [];
const NO_BANDS: Band[] = [];
const HEARD = { ...NO_FILTERS, lens: 'heard', sort: 'recent' } as const;

export function Remote() {
  const { api, info, rig, meter, status } = useCore();
  const t = useMemo(() => pickMessages(info?.lang, navigator.languages), [info?.lang]);
  useEffect(() => {
    document.documentElement.lang = t.locale;
    document.title = `${t.appName} · ${t.remoteSimple}`;
  }, [t]);

  const [msg, setMsg] = useState('');
  useEffect(() => {
    if (!msg) return;
    const id = setTimeout(() => setMsg(''), MSG_MS);
    return () => clearTimeout(id);
  }, [msg]);
  const onError = useCallback((e: unknown) => setMsg(e instanceof Error ? e.message : String(e)), []);

  const on = rig.power === 'on';
  const { dial } = useDial(api, `${rig.link},${rig.power}`);
  const { freqHz, tune } = useTuning(api, rig.freq_hz, dial?.coverage ?? DEFAULT_COVERAGE, onError);
  const now = useNow(20_000);
  const { overview } = useOverview(api);
  const generation = overview?.generation ?? null;
  const meta = useStationsMeta(api, generation);
  const win = useBandWindow(freqHz, dial?.bands ?? meta?.bands ?? NO_BANDS, overview?.on_hz ?? NO_HZ);
  const names = useMemo(() => makeNames(meta, t), [meta, t]);
  const qth = info?.qth;
  const cands = useCandidates(api, on ? freqHz : null, `${qth?.lat},${qth?.lon},${now.getTime()},${generation}`);
  // the station picked among those on the frequency, else the best one
  const [wanted, setWanted] = useState<string | null>(null);
  const sel: Candidate | null = on ? cands.find((c) => keyOf(c.station) === wanted) ?? cands[0] ?? null : null;
  const favs = useFavorites(api, onError, generation);
  const scan = useScan(api, onError, generation);

  const netEnabled = info?.net ?? false;
  const { netNow, setNetNow } = useNetNow(api, netEnabled);
  const netFavs = useNetFavorites(api, netEnabled, onError);
  const netTuned = netNow?.station.uuid ?? null;
  const country = useCallback((itu: string | null) => (itu ? names.country(itu) : ''), [names]);

  // the lists: this band's stations, what the scans heard, the favorites, a search (of the
  // schedule, or the internet's)
  const [tab, setTab] = usePersisted<Tab>('swatlas.remote.tab', 'air', oneOf('air', 'heard', 'favs', 'search'));
  const band = listBand(win.w.band);
  const airFilters = useMemo(() => ({ ...NO_FILTERS, bands: band ? [band] : [] }), [band]);
  const air = useStationSearch(tab === 'air' && band ? api : null, airFilters);
  const airRows = useMemo(() => onAirFirst(air.rows), [air.rows]);
  // Heard fills in while a scan runs (it logs as it goes) and again when one ends
  const heardKey = `${scan.status.state === 'running' ? scan.status.heard_rev : 0}|${scan.scans.length}|${scan.finished?.id ?? ''}`;
  const heard = useStationSearch(tab === 'heard' ? api : null, HEARD, heardKey);
  const [q, setQ] = useState('');
  const [internet, setInternet] = useState(false);
  const searchFilters = useMemo(() => ({ ...NO_FILTERS, q }), [q]);
  const searching = tab === 'search' && q.trim() !== '';
  const found = useStationSearch(searching && !internet ? api : null, searchFilters);
  const netFound = useNetSearch(api, searchFilters, searching && internet && netEnabled);

  const buffer = useMemo(() => ({ cushionMs: info?.audio_cushion_ms ?? 0, maxAheadMs: info?.audio_max_ahead_ms ?? 0 }), [info]);
  const openAudio = useMemo(() => (api ? api.transport.openAudio.bind(api.transport) : null), [api]);
  const can = canListen(info?.audio ?? false, netNow);
  const listen = useListen(openAudio, buffer, can);

  // tuning the rig leaves the internet station; the station chosen is the one the history credits
  const leaveNet = () => {
    if (!netNow) return;
    setNetNow(null);
    api?.netStop().catch(onError);
  };
  const hearing = (s: StationKey) => { api?.hearingStation({ source: s.source, key: s.key }).catch(() => {}); };
  const tuneTo = (hz: number, mode: Mode | null = null) => {
    leaveNet();
    tune(hz, mode);
  };
  const pickRow = (r: StationRow) => {
    tuneTo(r.freq_hz, r.mode);
    setWanted(keyOf(r));
    hearing(r);
  };
  const pickFav = (v: FavoriteView) => {
    const s = v.favorite.station;
    tuneTo(v.favorite.freq_hz, v.favorite.mode);
    setWanted(s && keyOf(s));
    if (s) hearing(s);
  };
  const choose = (c: Candidate) => {
    setWanted(keyOf(c.station));
    hearing(c.station);
  };
  const netAsked = useRef<string | null>(null);
  const tuneNet = (uuid: string) => {
    if (netAsked.current === uuid || (netTuned === uuid && netNow?.state.state !== 'failed')) return;
    netAsked.current = uuid;
    api?.netTune(uuid)
      .then(setNetNow, onError)
      .finally(() => { if (netAsked.current === uuid) netAsked.current = null; });
  };
  // a tap on an internet station tunes it and listens: inside the tap, as phones want
  const playNet = (uuid: string) => {
    tuneNet(uuid);
    listen.start();
  };
  const stepNet = (dir: number) => {
    const next = stepChannel(netFavs.list, netTuned, dir);
    if (next) tuneNet(next.uuid);
  };

  // the star: the internet station, else the frequency (with the station if it is on it)
  const tunedStation = sel && sel.station.freq_hz === freqHz ? sel.station : null;
  const tunedFav = on ? favAt(favs.views, freqHz, tunedStation?.id ?? null)?.favorite ?? null : null;
  const radioStar = (
    <Star
      on={tunedFav !== null}
      onToggle={() => {
        if (tunedFav) void favs.remove(tunedFav.id);
        else if (freqHz !== null) void favs.add({ freq_hz: freqHz, mode: asMode(rig.mode), label: null, station: tunedStation && { source: tunedStation.source, key: tunedStation.key } });
      }}
    />
  );
  const netFav = netTuned ? netFavs.byUuid.get(netTuned) ?? null : null;
  const netStar = netTuned && (
    <Star on={netFav !== null} onToggle={() => void (netFav ? netFavs.remove(netFav.id) : netFavs.add(netTuned))} />
  );

  // turning off asks first; a rig that goes off by itself drops the question
  const [askOff, setAskOff] = useState(false);
  useEffect(() => { if (!on) setAskOff(false); }, [on]);
  const setPower = (to: boolean) => { api?.rig({ cmd: 'set_power', on: to }).catch(onError); };
  const setMode = (mode: Mode) => { api?.rig({ cmd: 'set_mode', mode }).catch(onError); };
  // a scan started here shows what it hears
  const startScan = (b: Band) => {
    setTab('heard');
    void scan.start(b.id, b.scan_step_hz, 'normal');
  };
  const [keypad, setKeypad] = useState(false);
  const closeKeypad = useCallback(() => setKeypad(false), []);

  // the lock screen names what plays, and can stop it
  const playingName = netNow?.station.name ?? sel?.station.name ?? (freqHz !== null ? `${fmtKhz(freqHz)} kHz` : t.appName);
  useEffect(() => {
    const ms = typeof navigator !== 'undefined' ? navigator.mediaSession : undefined;
    if (!ms || !listen.playing) return;
    if (typeof MediaMetadata !== 'undefined') ms.metadata = new MediaMetadata({ title: playingName, artist: t.appName });
    ms.setActionHandler('pause', listen.stop);
    ms.setActionHandler('stop', listen.stop);
    return () => {
      ms.metadata = null;
      ms.setActionHandler('pause', null);
      ms.setActionHandler('stop', null);
    };
  }, [listen.playing, listen.stop, playingName, t]);

  const statusLine =
    status === 'connecting' ? t.connecting
    : status === 'lost' ? t.connectionLost
    : rig.link === 'off' ? t.stOff
    : rig.link === 'down' ? t.linkDown
    : rig.power === 'unknown' ? t.noAnswer
    : '';
  const radioFavs = favs.views;
  const noFavs = !radioFavs.length && !(netEnabled && netFavs.list.length);

  return (
    <I18nProvider value={t}>
      <NamesProvider value={names}>
        <div className="rm">
          <header className="rm-top">
            <span className="rm-lamp" data-ok={statusLine === '' ? '' : undefined} aria-hidden="true" />
            <span className="rm-status">{statusLine || t.appName}</span>
            <button type="button" className="rm-atlas" title={t.remoteFullAtlasTitle} onClick={() => goTo('atlas')}>{t.remoteFullAtlas}</button>
            <span className="rm-utc">{utcNow(now)} UTC</span>
            <button
              type="button"
              className="rm-power"
              data-on={on ? '' : undefined}
              disabled={status !== 'open' || rig.link === 'off'}
              aria-label={on ? t.turnOff : t.remoteTurnOn}
              onClick={() => (on ? setAskOff(true) : setPower(true))}
            >⏻</button>
          </header>
          {askOff && (
            <div className="rm-ask" role="alertdialog" aria-label={t.confirmOff}>
              <p>{t.confirmOff}</p>
              <button type="button" className="primary" onClick={() => { setAskOff(false); setPower(false); }}>{t.turnOff}</button>
              <button type="button" onClick={() => setAskOff(false)}>{t.cancel}</button>
            </div>
          )}

          <section className="rm-face">
            {netNow ? (
              <NetCard
                net={netNow}
                country={country(netNow.station.itu)}
                star={netStar}
                listening={listen.playing}
                channels={netFavs.list.length > 0}
                onStep={stepNet}
                onBack={leaveNet}
              />
            ) : (
              <>
                <Readout on={on} freqHz={freqHz} mode={rig.mode} onKeypad={() => setKeypad(true)} onMode={setMode} />
                <SMeter meter={meter} on={on} />
                <div className="rm-cardrow">
                {!on ? (
                  <p className="rm-off">
                    {t.remoteRadioOff}
                    {status === 'open' && rig.link !== 'off' && <button type="button" onClick={() => setPower(true)}>{t.remoteTurnOn}</button>}
                  </p>
                ) : sel ? (
                  <NowCard cands={cands} sel={sel} star={radioStar} onChoose={choose} />
                ) : (
                  <div className="rm-card empty">
                    <p className="rm-dim">{freqHz !== null && t.noStation(fmtKhz(freqHz))}</p>
                    {radioStar}
                  </div>
                )}
                <ScanButton on={on} band={win.w.band} status={scan.status} onStart={startScan} onStop={scan.stop} />
                </div>
                <TuneBar
                  on={on}
                  freqHz={freqHz}
                  stepHz={fineStepHz(rig.mode)}
                  band={win.w.band ? names.dialBand(win.w.band) : ''}
                  onTune={(hz) => tuneTo(hz)}
                  onBand={(dir) => {
                    const hz = win.jump(dir);
                    if (hz !== null) tuneTo(hz);
                  }}
                />
              </>
            )}
          </section>

          <section className="rm-lists">
            <div className="rm-tabs" role="tablist">
              {([['air', t.remoteTabOnAir], ['heard', t.lensHeard], ['favs', t.lensFav], ['search', t.search]] as const).map(([id, label]) => (
                <button key={id} type="button" role="tab" aria-selected={tab === id} onClick={() => setTab(id)}>{label}</button>
              ))}
            </div>
            {tab === 'search' && (
              <div className="rm-search">
                <input
                  type="search"
                  enterKeyHint="search"
                  placeholder={t.remoteSearchHint}
                  aria-label={t.search}
                  value={q}
                  onChange={(e) => setQ(e.target.value)}
                />
                {netEnabled && (
                  <button type="button" className="rm-chip" aria-pressed={internet} onClick={() => setInternet((x) => !x)} title={t.netBandHint}>{t.netBand}</button>
                )}
              </div>
            )}
            <ul className="rm-rows" role="tabpanel">
              {tab === 'air' && (band
                ? <StationRows rows={airRows} freqHz={netNow ? null : freqHz} onPick={pickRow} />
                : <li className="rm-empty">{t.remoteNoBand}</li>)}
              {tab === 'air' && band && !air.loading && !air.rows.length && <li className="rm-empty">{t.remoteBandEmpty}</li>}
              {tab === 'air' && band && air.total > air.rows.length && (
                <li><button type="button" className="rm-more" onClick={() => void air.more()}>{t.remoteShowMore}</button></li>
              )}

              {tab === 'heard' && heard.heard.map((h) => (h.kind === 'station' ? (
                <Row
                  key={`s${h.row.id}`}
                  khz={fmtKhz(h.row.freq_hz)}
                  flag={h.row.flag}
                  country={names.country(h.row.itu)}
                  name={h.row.name}
                  sub={[h.row.site_name, t.heardLine(h.log.count, fmtStamp(t, h.log.last))].join(' · ')}
                  sval={sLabel(h.log.best_db)}
                  current={!netNow && h.row.freq_hz === freqHz}
                  onPick={() => pickRow(h.row)}
                />
              ) : (
                <Row
                  key={`f${h.row.freq_hz}`}
                  khz={fmtKhz(h.row.freq_hz)}
                  flag={null}
                  country=""
                  name={h.row.label || t.unidentifiedRow}
                  sub={h.row.heard ? t.heardLine(h.row.heard.count, fmtStamp(t, h.row.heard.last)) : ''}
                  sval={h.row.heard ? sLabel(h.row.heard.best_db) : ''}
                  current={!netNow && h.row.freq_hz === freqHz}
                  onPick={() => tuneTo(h.row.freq_hz)}
                />
              )))}
              {tab === 'heard' && !heard.loading && !heard.heard.length && <li className="rm-empty">{t.remoteHeardEmpty}</li>}
              {tab === 'heard' && heard.total > heard.heard.length && (
                <li><button type="button" className="rm-more" onClick={() => void heard.more()}>{t.remoteShowMore}</button></li>
              )}

              {tab === 'favs' && radioFavs.map((v) => (
                <Row
                  key={v.favorite.id}
                  khz={fmtKhz(v.favorite.freq_hz)}
                  flag={v.favorite.station?.flag ?? null}
                  country={v.favorite.station ? names.country(v.favorite.station.itu) : ''}
                  name={v.favorite.label || v.favorite.station?.name || `${fmtKhz(v.favorite.freq_hz)} kHz`}
                  sub={[v.favorite.mode, v.favorite.station?.site_name].filter(Boolean).join(' · ')}
                  current={!netNow && v.favorite.freq_hz === freqHz}
                  onPick={() => pickFav(v)}
                />
              ))}
              {tab === 'favs' && netEnabled && netFavs.list.map((f) => (
                <Row
                  key={f.id}
                  khz={t.netBand}
                  flag={f.flag}
                  country={country(f.itu)}
                  name={f.label || f.name}
                  sub={[country(f.itu), f.codec].filter(Boolean).join(' · ')}
                  current={f.uuid === netTuned}
                  onPick={() => playNet(f.uuid)}
                />
              ))}
              {tab === 'favs' && noFavs && <li className="rm-empty">{t.remoteNoFavs}</li>}

              {searching && !internet && <StationRows rows={found.rows} freqHz={netNow ? null : freqHz} onPick={pickRow} />}
              {searching && !internet && found.total > found.rows.length && (
                <li><button type="button" className="rm-more" onClick={() => void found.more()}>{t.remoteShowMore}</button></li>
              )}
              {searching && internet && <NetRows items={netFound.items} playing={netTuned} country={country} onPick={playNet} />}
              {searching && internet && netFound.more && (
                <li><button type="button" className="rm-more" disabled={netFound.loadingMore} onClick={netFound.loadMore}>{t.remoteShowMore}</button></li>
              )}
              {searching && !(internet ? netFound.loading || netFound.items.length : found.loading || found.rows.length) && (
                <li className="rm-empty">{t.remoteNothing}</li>
              )}
            </ul>
          </section>

          <ListenBar listen={listen} can={can} />
          <p className="rm-toast" role="alert">{msg && t.error(msg)}</p>
        </div>
        {keypad && <Keypad onTune={(hz) => tuneTo(hz)} onClose={closeKeypad} />}
      </NamesProvider>
    </I18nProvider>
  );
}
