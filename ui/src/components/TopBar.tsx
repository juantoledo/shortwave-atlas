// The window's top bar: name and version, what the rig is doing, the Ko-fi link, the UTC
// clock, and the update, scans, stats, favorites, remote access, help, settings and power
// buttons (turning off asks first). The Stats button's lamp glows while a listening session is
// being recorded; the remote access one shows only while browsers may connect.
// In the desktop app it is also the title bar: it drags the window (not from its buttons, links
// or popovers), and on Windows and Linux ends in the window controls (api/window.ts).

import { useRef, type ReactNode } from 'react';
import { useNow } from '../hooks/useCore';
import { fmtSecs, useT, type Messages } from '../i18n';
import type { Chrome } from '../api/window';
import { Confirm } from './Confirm';
import { WindowControls } from './WindowControls';
import type { Info } from '../types/generated/Info';
import type { ListeningNow } from '../types/generated/ListeningNow';

const two = (n: number) => String(n).padStart(2, '0');

function Clock() {
  const d = useNow(1000);
  return (
    <span className="clock" aria-label="UTC">
      UTC <b>{two(d.getUTCHours())}:{two(d.getUTCMinutes())}<span className="sec">:{two(d.getUTCSeconds())}</span></b>
    </span>
  );
}

/** The rig in use: simulated, the Hamlib model we drive, or a remote `rigctld`. */
function rigLabel(info: Info, t: Messages): string {
  if (info.backend === 'sim') return t.subtitleSim;
  if (info.backend === 'external') return t.subtitleRemote(info.rig_host ?? '');
  return info.rig_model ? `${info.rig_model.mfg} ${info.rig_model.model}` : t.subtitleHamlib;
}

interface Props {
  info: Info | null;
  /** The window chrome it stands in for (desktop app only). */
  chrome: Chrome;
  /** Connection or rig trouble, empty when all is well. */
  status: string;
  on: boolean;
  powerBusy: boolean;
  powerDisabled: boolean;
  onPower: () => void;
  /** Asking whether to turn the rig off. */
  askOff: boolean;
  onConfirmOff: () => void;
  onCancelOff: () => void;
  onSetup: () => void;
  /** The saved scans; `scansNew`: one finished since they were last opened. */
  onScans: () => void;
  scansNew: boolean;
  /** The Stats view; `statsOn`: it shows now. `listening`: what is being recorded. */
  onStats: () => void;
  statsOn: boolean;
  listening: ListeningNow | null;
  /** The station list's favorites; `favOn`: it shows them now. */
  onFavorites: () => void;
  favOn: boolean;
  onHelp: () => void;
  onSettings: () => void;
  /** Remote access is on: how many browsers are connected (`null`: off). */
  remote: number | null;
  onRemote: () => void;
  /** The update pill, when there is one. */
  update: ReactNode;
}

export function TopBar({ info, chrome, status, on, powerBusy, powerDisabled, onPower, askOff, onConfirmOff, onCancelOff, onSetup, onScans, scansNew, onStats, statsOn, listening, onFavorites, favOn, onHelp, onSettings, remote, onRemote, update }: Props) {
  const t = useT();
  const power = useRef<HTMLDivElement>(null);
  const setup = info && !info.configured && info.backend === 'sim';
  const rec = listening?.now ?? null;
  const netRec = listening?.net ?? null;
  const statsTitle = rec
    ? t.statsRecording(rec.session.station?.name ?? `${rec.session.freq_hz / 1000} kHz`, fmtSecs(t, Math.max(0, rec.session.end - rec.session.start)))
    : netRec ? t.statsRecordingNet(netRec.name, fmtSecs(t, Math.max(0, netRec.end - netRec.start)))
    : listening?.usual ? t.statsUsual(listening.usual.station.name) : t.statsTitle;
  return (
    <header className="topbar" data-chrome={chrome ?? undefined} data-tauri-drag-region={chrome ? 'deep' : undefined}>
      <div className="brand">
        <div className="brand-name">
          <span className="wordmark">{t.appName}</span>
          {info && <span className="ver">{t.versionTag(info.version)}</span>}
        </div>
        <div className="brand-sub">
          {setup ? (
            <button className="link sub" type="button" onClick={onSetup}>{t.subtitleSetup}</button>
          ) : (
            <span className="sub">{info ? rigLabel(info, t) : ''}</span>
          )}
          <span className="status" role="status">{status}</span>
        </div>
      </div>
      <div className="top-actions">
        <a className="kofi" href="https://ko-fi.com/J6F024AKJE" target="_blank" rel="noreferrer">
          <img height="36" src="https://storage.ko-fi.com/cdn/kofi6.png?v=6" alt={t.kofi} />
        </a>
        <Clock />
        {update}
        <button className="top-btn scans" type="button" aria-label={scansNew ? t.scansNew : t.scans} title={scansNew ? t.scansNew : t.scans} onClick={onScans}>
          <svg className="top-ico" viewBox="0 0 16 16" aria-hidden="true"><path d="M1 14h2v-4H1zM5 14h2V5H5zM9 14h2V8H9zM13 14h2V2h-2z" /></svg>
          <span className="top-txt">{t.scans}</span>
          {scansNew && <span className="new-dot" />}
        </button>
        <button className="top-btn stats" type="button" aria-pressed={statsOn} aria-label={statsTitle} title={statsTitle} onClick={onStats}>
          <svg className="top-ico" viewBox="0 0 16 16" aria-hidden="true"><path d="M1 14h14v1H1zM1 11.6l4.2-5.1 3 3L14 2.8l1 .9-6.7 7.6-3-3L2.1 12.5z" /></svg>
          <span className="top-txt">{t.statsBtn}</span>
          {(rec || netRec) && <span className="rec-lamp" />}
        </button>
        <button className="top-btn fav" type="button" aria-pressed={favOn} aria-label={t.favoritesBtn} title={t.favoritesBtn} onClick={onFavorites}>
          <span className="top-ico" aria-hidden="true">★</span>
          <span className="top-txt">{t.favoritesBtn}</span>
        </button>
        {remote !== null && (
          <button className="top-btn remote" type="button" aria-label={t.remoteLamp(remote)} title={t.remoteLamp(remote)} onClick={onRemote}>
            <svg className="top-ico" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 9.6a1.6 1.6 0 1 0 0-3.2 1.6 1.6 0 0 0 0 3.2zM4.6 4.6l1 1a3.4 3.4 0 0 0 0 4.8l-1 1a4.8 4.8 0 0 1 0-6.8zm6.8 0a4.8 4.8 0 0 1 0 6.8l-1-1a3.4 3.4 0 0 0 0-4.8zM2.4 2.4l1 1a6.5 6.5 0 0 0 0 9.2l-1 1a7.9 7.9 0 0 1 0-11.2zm11.2 0a7.9 7.9 0 0 1 0 11.2l-1-1a6.5 6.5 0 0 0 0-9.2z" /></svg>
            <span className="top-txt">{remote > 0 ? `${t.remoteBtn} · ${remote}` : t.remoteBtn}</span>
            <span className="rec-lamp" />
          </button>
        )}
        <button className="icon-btn" type="button" aria-label={t.help} title={`${t.help} (F1)`} onClick={onHelp}>?</button>
        <button className="icon-btn" type="button" aria-label={t.settings} title={t.settings} onClick={onSettings}>⚙</button>
        <div className="power-wrap" ref={power} data-tauri-drag-region="false">
          <button className="power" type="button" aria-pressed={on} aria-expanded={askOff} disabled={powerBusy || powerDisabled} onClick={askOff ? onCancelOff : onPower}>
            <span className="lamp" />
            <span className="power-txt">{powerBusy ? t.powerBusy : on ? t.powerOn : t.powerOff}</span>
          </button>
          {askOff && <Confirm message={t.confirmOff} confirmLabel={t.turnOff} onConfirm={onConfirmOff} onCancel={onCancelOff} anchor={power} />}
        </div>
        {chrome === 'own' && <WindowControls />}
      </div>
    </header>
  );
}
