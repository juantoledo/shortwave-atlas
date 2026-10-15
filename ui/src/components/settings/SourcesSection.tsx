// Sources: where the stations come from (EiBi's schedule, and the listener's own), the switch for
// internet radio (the NET band, from radio-browser.info), and the listener's stations to a file and
// back: export, and import merged with theirs or instead of them.

import { useRef, useState } from 'react';
import type { Api } from '../../api/client';
import { downloadText, readText } from '../../api/files';
import type { UserStationsApi } from '../../hooks/useUserStations';
import { useT } from '../../i18n';
import type { CatalogMeta } from '../../types/generated/CatalogMeta';
import type { ImportReport } from '../../types/generated/ImportReport';
import { exportName, stationsIn } from '../sources';

/** Larger than any file of stations (500 of them are about 400 kB). */
const MAX_FILE = 2_000_000;

interface Props {
  api: Api;
  meta: CatalogMeta | null;
  mine: UserStationsApi;
  /** Internet radio (the NET band) is on. */
  net: boolean;
  onError: (e: unknown) => void;
  /** A setting changed: read the app info again. */
  onApplied: () => void;
}

export function SourcesSection({ api, meta, mine, net, onError, onApplied }: Props) {
  const t = useT();
  const input = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<{ name: string; json: string; n: number | null } | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);
  // the desktop webview saves to Downloads without a word: say so
  const [saved, setSaved] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [netBusy, setNetBusy] = useState(false);
  const setNet = async (on: boolean) => {
    setNetBusy(true);
    try {
      await api.setNet(on);
      onApplied();
    } catch (e) {
      onError(e);
    }
    setNetBusy(false);
  };

  const exportFile = async () => {
    try {
      const name = exportName(new Date());
      downloadText(name, await api.exportUserStations());
      setSaved(name);
      setReport(null);
    } catch (e) {
      onError(e);
    }
  };
  const pick = async (f: File | undefined) => {
    if (input.current) input.current.value = '';
    if (!f) return;
    setReport(null);
    setSaved(null);
    const json = await readText(f, MAX_FILE);
    if (json === null) return onError(t.importTooBig);
    setFile({ name: f.name, json, n: stationsIn(json) });
  };
  const bring = async (replace: boolean) => {
    if (!file) return;
    setBusy(true);
    const r = await mine.importJson(file.json, replace);
    setBusy(false);
    setFile(null);
    if (r) setReport(r);
  };

  return (
    <section className="sec form" aria-label={t.sourcesSection}>
      <h2 className="h"><span>{t.sourcesSection}</span></h2>
      {(meta?.sources ?? []).map((s) => (
        <div className="src" key={s.id}>
          <b>{s.kind === 'user' ? t.sourceMine : s.name}<span>{t.sourceRows(s.rows)}</span></b>
          {s.kind === 'user' ? (
            <p className="note">{t.sourceMineNote}</p>
          ) : (
            <p className="note">
              {s.version}
              {s.season && ` · ${t.sourceSeason(s.season, s.from ?? '', s.to ?? '')}`}
              {s.url && <> · <a className="link" href={s.url} target="_blank" rel="noreferrer">{s.url.replace(/^https?:\/\//, '').replace(/\/$/, '')}</a></>}
            </p>
          )}
          {s.kind === 'schedule' && !s.current && <p className="note warn">{t.seasonStale(s.season ?? '')}</p>}
        </div>
      ))}

      <div className="src">
        <b>{t.netSource}</b>
        <div className="seg" role="radiogroup" aria-label={t.netSource}>
          {[true, false].map((on) => (
            <button key={String(on)} type="button" role="radio" aria-checked={net === on} disabled={netBusy} onClick={() => setNet(on)}>
              {on ? t.netOn : t.netOff}
            </button>
          ))}
        </div>
        <p className="note">{t.netSourceNote}</p>
      </div>

      <div className="field">
        <span>{t.stationsFile}</span>
        <div className="actions">
          <button className="btn" type="button" disabled={mine.views.length === 0} onClick={exportFile}>{t.exportStations}</button>
          <button className="btn" type="button" disabled={busy} onClick={() => input.current?.click()}>{t.importStations}</button>
          <input ref={input} type="file" accept=".json,application/json" hidden onChange={(e) => void pick(e.currentTarget.files?.[0])} />
        </div>
        <small>{t.stationsFileHelp}</small>
        {saved && <p className="note" role="status">{t.exportSaved(saved)}</p>}
      </div>
      {file && (
        <div className="import-ask" role="alertdialog" aria-label={t.importStations}>
          <p>{t.importAsk(file.name, file.n ?? 0)}</p>
          <div className="actions">
            <button className="btn primary" type="button" disabled={busy} onClick={() => bring(false)}>{t.importMerge}</button>
            <button className="btn danger" type="button" disabled={busy || mine.views.length === 0} onClick={() => bring(true)}>{t.importReplace}</button>
            <button className="btn" type="button" disabled={busy} onClick={() => setFile(null)}>{t.cancel}</button>
          </div>
          <small>{t.importHow}</small>
        </div>
      )}
      {report && (
        <div className="import-report" role="status">
          <p>{t.importDone(report.added, report.updated, report.unchanged, report.skipped.length)}</p>
          {report.skipped.length > 0 && (
            <ul>
              {report.skipped.map((s, i) => <li key={i}><b>{s.name || '—'}</b>: {s.reason}</li>)}
            </ul>
          )}
        </div>
      )}
    </section>
  );
}
