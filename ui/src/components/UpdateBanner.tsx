// "Update available": a pill on the globe, opening a panel with the notes and what to do
// (install here, download by hand, or install on the host).

import { useState } from 'react';
import type { TransportKind } from '../api/transport';
import { updateView, type UpdateApi } from '../hooks/useUpdate';
import { useT } from '../i18n';

interface Props {
  update: UpdateApi;
  transport: TransportKind;
  onError: (e: unknown) => void;
}

export function UpdateBanner({ update, transport, onError }: Props) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const v = updateView(update.status, transport, update.skipped);
  if (!v) return null;

  const install = async () => {
    setBusy(true);
    try {
      await update.install();
    } catch (e) {
      onError(e);
    }
    setBusy(false);
  };

  const pill =
    v.phase === 'downloading' ? t.updateDownloadingPill(v.percent === null ? '' : `${v.percent}%`)
    : v.phase === 'installing' ? t.updateInstallingPill
    : v.phase === 'failed' ? t.updateFailedPill
    : t.updatePill(v.version);
  const working = v.phase === 'downloading' || v.phase === 'installing';

  return (
    <div className="hud hud-tr update">
      <button className={'toggle update-pill' + (v.phase === 'failed' ? ' warn' : '')} type="button" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <span className="lamp" />
        <span>{pill}</span>
      </button>

      {open && (
        <section className="update-panel" aria-label={t.updateTitle(v.version)}>
          <h2>{t.updateTitle(v.version)}</h2>
          {update.status && <p className="note">{t.updateCurrent(update.status.current)}</p>}

          {v.notes && (
            <details className="log" open={v.phase === 'offer'}>
              <summary>{t.updateNotes}</summary>
              <pre>{v.notes}</pre>
            </details>
          )}

          {v.phase === 'downloading' && (
            <>
              <progress max={100} value={v.percent ?? undefined} />
              <p className="note">{t.updateDownloading}</p>
            </>
          )}
          {v.phase === 'installing' && <p className="note">{t.updateInstalling}</p>}
          {v.error && (
            <div className="hint" role="alert">
              <b>{t.updateFailedPill}</b>
              <span>{v.error}</span>
            </div>
          )}

          {v.action.kind === 'install' && !working && <p className="note warn">{t.updateRestartNote}</p>}
          {v.action.kind === 'download' && (
            <>
              <p className="note">{t.updateManual[v.action.reason]}</p>
              <p className="note url">{v.url}</p>
            </>
          )}
          {v.action.kind === 'host' && <p className="note">{t.updateOnHost}</p>}

          {!working && (
            <div className="actions">
              {v.action.kind === 'install' && (
                <button className="btn primary" type="button" disabled={busy} onClick={install}>
                  {v.phase === 'failed' ? t.updateRetry : t.updateInstall}
                </button>
              )}
              {v.action.kind === 'download' && (
                <a className="btn primary" href={v.url} target="_blank" rel="noreferrer">{t.updateDownload}</a>
              )}
              <button className="btn" type="button" onClick={() => setOpen(false)}>{t.updateLater}</button>
              {v.phase === 'offer' && (
                <button className="btn" type="button" onClick={() => { update.skip(v.version); setOpen(false); }}>{t.updateSkip}</button>
              )}
            </div>
          )}
        </section>
      )}
    </div>
  );
}
