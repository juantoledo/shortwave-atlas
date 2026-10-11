// The window's top bar: name and version, what the rig is doing, the Ko-fi link, the UTC
// clock, and the update, scans, favorites, help, settings and power buttons (turning off asks
// first).

import { useRef, type ReactNode } from 'react';
import { useNow } from '../hooks/useCore';
import { useT } from '../i18n';
import { Confirm } from './Confirm';
import type { Info } from '../types/generated/Info';

const two = (n: number) => String(n).padStart(2, '0');

function Clock() {
  const d = useNow(1000);
  return (
    <span className="clock" aria-label="UTC">
      UTC <b>{two(d.getUTCHours())}:{two(d.getUTCMinutes())}<span className="sec">:{two(d.getUTCSeconds())}</span></b>
    </span>
  );
}

interface Props {
  info: Info | null;
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
  /** The station list's favorites; `favOn`: it shows them now. */
  onFavorites: () => void;
  favOn: boolean;
  onHelp: () => void;
  onSettings: () => void;
  /** The update pill, when there is one. */
  update: ReactNode;
}

export function TopBar({ info, status, on, powerBusy, powerDisabled, onPower, askOff, onConfirmOff, onCancelOff, onSetup, onScans, scansNew, onFavorites, favOn, onHelp, onSettings, update }: Props) {
  const t = useT();
  const power = useRef<HTMLDivElement>(null);
  const setup = info && !info.configured && info.backend === 'sim';
  return (
    <header className="topbar">
      <div className="brand">
        <div className="brand-name">
          <span className="wordmark">{t.appName}</span>
          {info && <span className="ver">{t.versionTag(info.version)}</span>}
        </div>
        <div className="brand-sub">
          {setup ? (
            <button className="link sub" type="button" onClick={onSetup}>{t.subtitleSetup}</button>
          ) : (
            <span className="sub">{info ? (info.backend === 'sim' ? t.subtitleSim : t.subtitleRig) : ''}</span>
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
        <button className="top-btn" type="button" aria-label={scansNew ? t.scansNew : t.scans} title={scansNew ? t.scansNew : t.scans} onClick={onScans}>
          <svg className="top-ico" viewBox="0 0 16 16" aria-hidden="true"><path d="M1 14h2v-4H1zM5 14h2V5H5zM9 14h2V8H9zM13 14h2V2h-2z" /></svg>
          <span className="top-txt">{t.scans}</span>
          {scansNew && <span className="new-dot" />}
        </button>
        <button className="top-btn" type="button" aria-pressed={favOn} aria-label={t.favoritesBtn} title={t.favoritesBtn} onClick={onFavorites}>
          <span className="top-ico" aria-hidden="true">★</span>
          <span className="top-txt">{t.favoritesBtn}</span>
        </button>
        <button className="icon-btn" type="button" aria-label={t.help} title={`${t.help} (F1)`} onClick={onHelp}>?</button>
        <button className="icon-btn" type="button" aria-label={t.settings} title={t.settings} onClick={onSettings}>⚙</button>
        <div className="power-wrap" ref={power}>
          <button className="power" type="button" aria-pressed={on} aria-expanded={askOff} disabled={powerBusy || powerDisabled} onClick={askOff ? onCancelOff : onPower}>
            <span className="lamp" />
            <span className="power-txt">{powerBusy ? t.powerBusy : on ? t.powerOn : t.powerOff}</span>
          </button>
          {askOff && <Confirm message={t.confirmOff} confirmLabel={t.turnOff} onConfirm={onConfirmOff} onCancel={onCancelOff} anchor={power} />}
        </div>
      </div>
    </header>
  );
}
