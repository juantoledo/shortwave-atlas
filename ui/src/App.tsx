import { useCallback, useEffect, useMemo, useState } from 'react';
import { AudioPanel } from './components/AudioPanel';
import { Dossier } from './components/Dossier';
import { Globe } from './components/Globe';
import { Hud } from './components/Hud';
import { Settings } from './components/settings/Settings';
import { StationList } from './components/StationList';
import { Tuner } from './components/Tuner';
import { UpdateBanner } from './components/UpdateBanner';
import { useCandidates, useCore, useNow, useStationList, useTuning } from './hooks/useCore';
import { useUpdate } from './hooks/useUpdate';
import { I18nProvider, pickMessages } from './i18n';
import type { Candidate } from './types/generated/Candidate';
import type { Mode } from './types/generated/Mode';
import type { Qth } from './types/generated/Qth';

const MSG_MS = 6000;

export function App() {
  const { api, info, reloadInfo, rig, status } = useCore();
  const update = useUpdate(api);
  const [view, setView] = useState<'atlas' | 'settings'>('atlas');
  const t = useMemo(() => pickMessages(info?.lang, navigator.languages), [info?.lang]);
  useEffect(() => { document.documentElement.lang = t.locale; }, [t]);

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
  const all = useStationList(api, qth);
  const cands = useCandidates(api, on ? freqHz : null, `${qth?.lat},${qth?.lon},${now.getTime()}`);

  // Keep the chosen station while it is still a candidate; otherwise take the best one.
  const [sel, setSel] = useState<Candidate | null>(null);
  useEffect(() => {
    if (!on || !cands.length) setSel(null);
    else setSel((cur) => cands.find((c) => c.station.id === cur?.station.id) ?? cands[0]);
  }, [cands, on]);

  const [frame, setFrame] = useState(true);
  const [powerBusy, setPowerBusy] = useState(false);

  const setMode = (mode: Mode) => api?.rig({ cmd: 'set_mode', mode }).catch(onError);
  const pick = (c: Candidate) => {
    const m = c.station.mode as Mode;
    if (rig.mode !== m) setMode(m);
    tune(c.station.freq_hz);
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

  const statusLine =
    status === 'connecting' ? t.connecting
    : status === 'lost' ? t.connectionLost
    : rig.link === 'off' ? t.stOff
    : rig.link === 'down' ? t.linkDown
    : rig.power === 'unknown' ? t.noAnswer
    : '';

  return (
    <I18nProvider value={t}>
      <div className="app">
        <main className="stage" aria-label={t.globe}>
          {qth && <Globe sel={on ? sel : null} qth={qth} frame={frame} onQth={moveQth} failedMsg={t.globeFailed} />}
          <Hud frame={frame} onFrame={() => setFrame((f) => !f)} />
          {api && <UpdateBanner update={update} transport={api.transport.kind} onError={onError} />}
        </main>

        <aside className="console" aria-label={view === 'settings' ? t.settings : t.stations}>
          {view === 'settings' && api && (
            <Settings api={api} os={info?.os ?? 'linux'} rig={rig} update={update} onBack={() => setView('atlas')} onError={onError} onApplied={reloadInfo} />
          )}
          {/* kept mounted while Settings is open, so audio keeps playing */}
          <div className="pane" hidden={view === 'settings'}>
            <header className="brand">
              <div>
                <h1>SW Atlas</h1>
                {info && !info.configured && info.backend === 'sim' ? (
                  <p><button className="link" type="button" onClick={() => setView('settings')}>{t.subtitleSetup}</button></p>
                ) : (
                  <p>{info ? (info.backend === 'sim' ? t.subtitleSim : t.subtitleRig) : ' '}</p>
                )}
                <p className="status" role="status">{statusLine}</p>
              </div>
              <div className="brand-actions">
                <button className="icon-btn" type="button" aria-label={t.settings} title={t.settings} onClick={() => setView('settings')}>⚙</button>
                <button className="power" type="button" aria-pressed={on} disabled={powerBusy || status !== 'open' || rig.link === 'off'} onClick={togglePower}>
                  <span className="lamp" />
                  <span>{powerBusy ? t.powerBusy : on ? t.powerOn : t.powerOff}</span>
                </button>
              </div>
            </header>

            <Tuner on={on} freqHz={freqHz} mode={rig.mode} strengthDb={rig.strength_db} stations={all} onTune={tune} onMode={setMode} />
            <p className="msg" role="alert">{msg && t.error(msg)}</p>

            {info?.audio && api && (
              <AudioPanel openAudio={api.transport.openAudio.bind(api.transport)} rate={info.audio_rate} mode={rig.mode} cwPitch={rig.cw_pitch_hz} freqHz={freqHz} onTune={tune} />
            )}

            <section className="sec" aria-label={t.station} aria-live="polite">
              <h2 className="h"><span>{t.station}</span></h2>
              <Dossier
                on={on}
                freqHz={freqHz}
                sel={sel}
                others={sel ? cands.filter((c) => c.station.id !== sel.station.id) : []}
                all={all}
                now={now}
                onChoose={setSel}
              />
            </section>

            <StationList on={on} all={all} selId={sel?.station.id ?? null} onPick={pick} />
          </div>
          {view === 'settings' && <p className="msg" role="alert">{msg && t.error(msg)}</p>}
        </aside>
      </div>
    </I18nProvider>
  );
}
