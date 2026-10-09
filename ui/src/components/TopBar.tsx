// The window's top bar: name and version, what the rig is doing, the UTC clock, and the
// update, help, settings and power buttons.

import type { ReactNode } from 'react';
import { useNow } from '../hooks/useCore';
import { useT } from '../i18n';
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
  onSetup: () => void;
  onHelp: () => void;
  onSettings: () => void;
  /** The update pill, when there is one. */
  update: ReactNode;
}

export function TopBar({ info, status, on, powerBusy, powerDisabled, onPower, onSetup, onHelp, onSettings, update }: Props) {
  const t = useT();
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
        <Clock />
        {update}
        <button className="icon-btn" type="button" aria-label={t.help} title={`${t.help} (F1)`} onClick={onHelp}>?</button>
        <button className="icon-btn" type="button" aria-label={t.settings} title={t.settings} onClick={onSettings}>⚙</button>
        <button className="power" type="button" aria-pressed={on} disabled={powerBusy || powerDisabled} onClick={onPower}>
          <span className="lamp" />
          <span className="power-txt">{powerBusy ? t.powerBusy : on ? t.powerOn : t.powerOff}</span>
        </button>
      </div>
    </header>
  );
}
