// Remote access: serve this UI (with the rig's audio) to browsers on this computer, the LAN or
// Tailscale. Start and stop, who can connect and the port; where to open it, with a QR code for
// phones. There is no password, so it warns before listening beyond this computer. Only the
// desktop window changes it: browsers and swatlas-server only show it.

import { useEffect, useId, useState } from 'react';
import type { Api } from '../../api/client';
import { useRemote } from '../../hooks/useRemote';
import { useT, type Messages } from '../../i18n';
import type { Os } from '../../types/generated/Os';
import type { RemoteChoice } from '../../types/generated/RemoteChoice';
import type { RemoteStatus } from '../../types/generated/RemoteStatus';
import type { RemoteUrl } from '../../types/generated/RemoteUrl';
import type { Scope } from '../../types/generated/Scope';

const STATUS_MS = 2000;
/** `atlas_core::remote`'s binds. */
export const LOOPBACK_BIND = '127.0.0.1';
export const NETWORK_BIND = '0.0.0.0';

/** The form: who can connect and the port. */
export interface Draft {
  scope: Scope;
  /** A custom bind, kept as the file has it. */
  bind: string;
  port: number;
}

export const draftOf = (s: RemoteStatus): Draft => ({ scope: s.scope, bind: s.choice.bind, port: s.choice.port });

export function choiceOf(d: Draft, enabled: boolean): RemoteChoice {
  const bind = d.scope === 'loopback' ? LOOPBACK_BIND : d.scope === 'network' ? NETWORK_BIND : d.bind;
  return { enabled, bind, port: d.port };
}

/** The draft would serve something else than `c` (a restart applies it). */
export const changed = (d: Draft, c: RemoteChoice) => {
  const n = choiceOf(d, true);
  return n.bind !== c.bind || n.port !== c.port;
};

function statusLine(t: Messages, s: RemoteStatus): string {
  if (s.running) return t.remoteRunning(s.clients);
  const f = s.failure;
  if (!f) return t.remoteStopped;
  if (f === 'invalid' || f === 'other') return t.remoteFailed(s.error ?? '');
  return t.remoteFailures[f](s.choice.port, s.choice.bind);
}

function copy(text: string) {
  void navigator.clipboard?.writeText(text).catch(() => {});
}

function Address({ t, u, showQr, onQr }: { t: Messages; u: RemoteUrl; showQr: boolean; onQr: () => void }) {
  return (
    <li>
      <div className="remote-url">
        <span className="remote-kind">{t.remoteKinds[u.kind]}</span>
        <a href={u.url} target="_blank" rel="noreferrer">{u.url}</a>
        <span className="row-inline">
          <button className="btn" type="button" onClick={() => copy(u.url)}>{t.copy}</button>
          <button className="btn" type="button" aria-pressed={showQr} onClick={onQr}>{t.remoteQr}</button>
        </span>
      </div>
      {showQr && u.qr.size > 0 && (
        <svg className="remote-qr" viewBox={`0 0 ${u.qr.size} ${u.qr.size}`} role="img" aria-label={t.remoteQrLabel(u.url)}>
          <rect width={u.qr.size} height={u.qr.size} fill="#fff" />
          <path d={u.qr.path} fill="#000" />
        </svg>
      )}
    </li>
  );
}

interface Props {
  api: Api;
  /** Where the core runs: the firewall note is worded for it. */
  os: Os;
  /** The rig's audio is on (Settings → Audio): without it browsers hear nothing. */
  listen: boolean;
  onAudio: () => void;
  onError: (e: unknown) => void;
}

export function RemoteSection({ api, os, listen, onAudio, onError }: Props) {
  const t = useT();
  const uid = useId();
  const { status, setStatus } = useRemote(api, STATUS_MS);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [qr, setQr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // start from what is served (or was chosen last), once
  useEffect(() => {
    if (status && !draft) setDraft(draftOf(status));
  }, [status, draft]);

  if (!status || !draft) return null;

  const host = api.transport.kind === 'tauri' && !status.headless;
  const set = (patch: Partial<Draft>) => setDraft({ ...draft, ...patch });
  const firstQr = status.urls.find((u) => u.kind !== 'loopback')?.url ?? null;
  const qrShown = qr ?? firstQr;

  const apply = async (enabled: boolean) => {
    setBusy(true);
    try {
      const s = await api.setRemote(choiceOf(draft, enabled));
      setStatus(s);
      setDraft(draftOf(s));
    } catch (e) {
      onError(e);
    }
    setBusy(false);
  };

  const scopes: Scope[] = draft.scope === 'custom' ? ['loopback', 'network', 'custom'] : ['loopback', 'network'];

  return (
    <section className="sec form" aria-label={t.remoteSection}>
      <h2 className="h"><span>{t.remoteSection}</span></h2>
      <p className="note">{t.remoteHelp}</p>
      {status.headless && <p className="note warn">{t.remoteHeadless}</p>}
      {!host && !status.headless && <p className="note warn">{t.remoteFromHost}</p>}
      {host && status.locked.length > 0 && <p className="note warn">{t.lockedByEnv(status.locked.join(', '))}</p>}

      <div className="field">
        <span id={`${uid}-scope`}>{t.remoteWho}</span>
        <div className="seg" role="radiogroup" aria-labelledby={`${uid}-scope`}>
          {scopes.map((s) => (
            <button key={s} type="button" role="radio" aria-checked={draft.scope === s} disabled={!host} onClick={() => set({ scope: s })}>
              {s === 'custom' ? draft.bind : t.remoteScopes[s]}
            </button>
          ))}
        </div>
        <small>{t.remoteScopeHelp[draft.scope]}</small>
      </div>

      <label className="field">
        <span>{t.remotePort}</span>
        <input
          type="number"
          inputMode="numeric"
          min={1}
          max={65535}
          value={draft.port}
          disabled={!host}
          onChange={(e) => set({ port: Math.min(65535, Math.max(0, Math.round(e.currentTarget.valueAsNumber || 0))) })}
        />
      </label>

      {/* before it listens beyond this computer, and while it does */}
      {draft.scope !== 'loopback' && <p className="note warn" role="alert">{t.remoteNoPassword}</p>}

      {host && (
        <div className="actions">
          {status.running ? (
            <>
              <button className="btn primary" type="button" disabled={busy} onClick={() => apply(false)}>{t.remoteStop}</button>
              {changed(draft, status.choice) && (
                <button className="btn" type="button" disabled={busy} onClick={() => apply(true)}>{t.remoteRestart}</button>
              )}
            </>
          ) : (
            <button className="btn primary" type="button" disabled={busy || draft.port < 1} onClick={() => apply(true)}>{t.remoteStart}</button>
          )}
        </div>
      )}
      {host && status.config_path && <p className="note">{t.savedTo(status.config_path)}</p>}

      <div className="st-box" aria-live="polite">
        <h3 className="h"><span>{t.status}</span></h3>
        <p className={'st-line' + (status.running ? ' st-ok' : status.failure ? ' st-warn' : '')}>{statusLine(t, status)}</p>
        {status.failure && status.error && status.failure !== 'other' && status.failure !== 'invalid' && (
          <p className="note">{t.lastError(status.error)}</p>
        )}
        {status.running && (
          <ul className="remote-urls">
            {status.urls.map((u) => (
              <Address key={u.url} t={t} u={u} showQr={qrShown === u.url} onQr={() => setQr(qrShown === u.url ? '' : u.url)} />
            ))}
          </ul>
        )}
        {status.running && !listen && (
          <div className="hint" role="alert">
            <b>{t.remoteNoAudio[0]}</b>
            <span>{t.remoteNoAudio[1]}</span>
            {host && <button className="btn" type="button" onClick={onAudio}>{t.audioSection}</button>}
          </div>
        )}
        {host && draft.scope !== 'loopback' && <p className="note">{t.remoteFirewall[os](draft.port)}</p>}
      </div>
    </section>
  );
}
