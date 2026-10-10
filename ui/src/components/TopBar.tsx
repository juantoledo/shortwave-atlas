// The window's top bar: name and version, what the rig is doing, the Ko-fi link, the UTC
// clock, and the update, help, settings and power buttons (turning off asks first).

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
  onHelp: () => void;
  onSettings: () => void;
  /** The update pill, when there is one. */
  update: ReactNode;
}

export function TopBar({ info, status, on, powerBusy, powerDisabled, onPower, askOff, onConfirmOff, onCancelOff, onSetup, onHelp, onSettings, update }: Props) {
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
