// Updates: this version, automatic checks on or off, the channel, and a manual check.

import { useState } from 'react';
import type { UpdateApi } from '../../hooks/useUpdate';
import { useT, type Messages } from '../../i18n';
import type { Channel } from '../../types/generated/Channel';
import type { UpdateStatus } from '../../types/generated/UpdateStatus';

const CHANNELS: Channel[] = ['stable', 'beta'];

export function statusLine(t: Messages, s: UpdateStatus): string {
  const st = s.state;
  switch (st.state) {
    case 'disabled': return t.updateDisabled;
    case 'idle': return t.updateIdle;
    case 'checking': return t.updateChecking;
    case 'up_to_date': {
      const at = s.checked_at ? new Date(s.checked_at * 1000).toLocaleTimeString(t.locale, { hour: '2-digit', minute: '2-digit' }) : '';
      return t.updateUpToDate(at);
    }
    case 'available': return t.updateAvailable(st.release.version);
    case 'downloading':
    case 'installing': return t.updateBusy(st.release.version);
    case 'failed': return st.release ? t.updateAvailable(st.release.version) : t.updateCheckFailed(st.message);
  }
}

interface Props {
  update: UpdateApi;
  onError: (e: unknown) => void;
}

export function UpdatesSection({ update, onError }: Props) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const s = update.status;
  if (!s) return null;

  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    try {
      await f();
    } catch (e) {
      onError(e);
    }
    setBusy(false);
  };
  const st = s.state.state;
  const ok = st === 'up_to_date' || st === 'available' || st === 'downloading' || st === 'installing';
  const warn = st === 'failed';

  return (
    <section className="sec form" aria-label={t.updatesSection}>
      <h2 className="h"><span>{t.updatesSection}</span><span>{t.updateVersion(s.current)}</span></h2>

      <div className="field">
        <span>{t.updateAuto}</span>
        <div className="seg" role="radiogroup" aria-label={t.updateAuto}>
          {[true, false].map((on) => (
            <button key={String(on)} type="button" role="radio" aria-checked={s.check === on} disabled={busy} onClick={() => run(() => update.setPrefs(on, s.channel))}>
              {on ? t.updateOn : t.updateOff}
            </button>
          ))}
        </div>
      </div>
      <div className="field">
        <span>{t.updateChannel}</span>
        <div className="seg" role="radiogroup" aria-label={t.updateChannel}>
          {CHANNELS.map((c) => (
            <button key={c} type="button" role="radio" aria-checked={s.channel === c} disabled={busy} onClick={() => run(() => update.setPrefs(s.check, c))}>
              {c === 'stable' ? t.updateStable : t.updateBeta}
            </button>
          ))}
        </div>
        <small>{t.updateBetaHelp}</small>
      </div>
      <p className="note">{t.updatePrivacy}</p>
      {s.locked.length > 0 && <p className="note warn">{t.lockedByEnv(s.locked.join(', '))}</p>}

      <div className="st-box" aria-live="polite">
        <p className={'st-line' + (ok ? ' st-ok' : warn ? ' st-warn' : '')}>{statusLine(t, s)}</p>
      </div>
      <div className="actions">
        <button className="btn" type="button" disabled={busy || st === 'checking' || st === 'downloading' || st === 'installing'} onClick={() => run(update.check)}>
          {t.updateCheckNow}
        </button>
      </div>
    </section>
  );
}
