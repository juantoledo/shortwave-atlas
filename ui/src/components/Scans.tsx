// The scans: every saved band scan, newest first, each deletable (or all at once, after
// asking), and one scan opened: its strength profile with the signals marked, the signals
// strongest first with who they are, and the stations that were on the air but not heard. A row
// tunes there and the sheet stays open; "Show in the station list" closes it and narrows the
// list to the scan, to go through it with the globe in view.

import { useEffect, useRef, useState } from 'react';
import { useNames } from '../catalog/names';
import type { ScanApi } from '../hooks/useScan';
import { fmtStamp, sLabel, useT } from '../i18n';
import type { Scan } from '../types/generated/Scan';
import type { ScanSummary } from '../types/generated/ScanSummary';
import type { StationRef } from '../types/generated/StationRef';
import { Confirm } from './Confirm';
import { MARKS_W } from './dial';
import { Flag } from './Flag';
import { bandOf, profilePath, speedOf, stepKhz, tally } from './scan';
import { Sheet } from './Sheet';

/** A scan's speed by name, or its dwell when no speed has it. */
function paceLabel(t: ReturnType<typeof useT>, s: ScanSummary): string {
  const sp = speedOf(s);
  return sp ? t.scanSpeeds[sp] : `${s.dwell_ms} ms`;
}

interface Props {
  scan: ScanApi;
  /** The scan opened, or `null` for the list. */
  openId: string | null;
  onOpen: (id: string | null) => void;
  onTune: (hz: number) => void;
  /** Narrow the station list to this scan. */
  onShow: (id: string) => void;
  onClose: () => void;
}

const khz = (hz: number) => String(hz / 1000);

function Station({ r }: { r: StationRef }) {
  const names = useNames();
  return <><Flag flag={r.flag} title={names.country(r.itu)} />{r.name}</>;
}

interface DetailProps {
  s: ScanSummary;
  full: Scan | null;
  onTune: (hz: number) => void;
  onShow: () => void;
  onDelete: () => void;
}

function Detail({ s, full, onTune, onShow, onDelete }: DetailProps) {
  const t = useT();
  const names = useNames();
  const [asking, setAsking] = useState(false);
  const del = useRef<HTMLDivElement>(null);
  const lo = s.first_hz / 1000, hi = (s.first_hz + (s.points - 1) * s.step_hz) / 1000;
  const prof = full ? profilePath({ ...s, samples: full.samples }, lo, hi, true) : '';
  const marks = s.peaks.map((p) => `M${Math.round(((p.freq_hz / 1000 - lo) / (hi - lo || 1)) * MARKS_W * 2) / 2} 0V1`).join('');
  const peaks = [...s.peaks].sort((a, b) => b.db - a.db);
  const tl = tally(s);
  return (
    <div className="scan-detail">
      <h2>{names.dialBand(bandOf(s))} <small>{fmtStamp(t, s.started)}</small></h2>
      <p className="note">
        {t.scanMeta(stepKhz(s.step_hz), paceLabel(t, s), sLabel(s.noise_db))} · {t.scanOutcome[s.outcome.kind]}
        {s.outcome.kind === 'failed' && <span className="warn">{` · ${s.outcome.message}`}</span>}
      </p>
      {s.stuck_db !== null && <p className="note warn" role="note">{t.scanStuck(sLabel(s.stuck_db))}</p>}
      <div className="scan-big" aria-hidden="true">
        <svg viewBox={`0 0 ${MARKS_W} 1`} preserveAspectRatio="none">
          <path className="peaks" d={marks} />
          <path className="now" d={prof} />
        </svg>
        <div className="axis"><span>{khz(s.first_hz)}</span><span>kHz</span><span>{String(hi)}</span></div>
      </div>
      <p className="scan-count">{t.scanCount(tl.signals, tl.logged, tl.expected)}</p>
      <button className="btn primary scan-show" type="button" title={t.scanShowTitle} onClick={onShow}>{t.scanShowInList}</button>

      <h4 className="h"><span>{t.scanSignals}</span><span>{peaks.length}</span></h4>
      {peaks.length === 0 && <p className="empty-d">{t.scanNoSignals}</p>}
      <ul className="list">
        {peaks.map((p) => (
          <li key={p.freq_hz}>
            <button className="row scan-row" type="button" title={t.scanTune(khz(p.freq_hz))} onClick={() => onTune(p.freq_hz)}>
              <span className="rf">{khz(p.freq_hz)}</span>
              <span className="rn">
                <b className={'v-' + p.verdict}>
                  {p.stations.length ? p.stations.map((r, i) => <span key={i}>{i > 0 && ' / '}<Station r={r} /></span>) : t.verdicts.unidentified}
                </b>
                <em>
                  {p.stations.length > 0 && t.verdicts[p.verdict]}
                  {p.stations[0]?.lang && ` · ${names.lang(p.stations[0].lang)}`}
                  {p.stations[0]?.target && ` → ${names.target(p.stations[0].target)}`}
                </em>
              </span>
              <span className="sval">{sLabel(p.db)}</span>
            </button>
          </li>
        ))}
      </ul>

      <h4 className="h scan-missed-h"><span>{t.scanMissed}</span><span>{s.missed.length}</span></h4>
      {s.missed.length === 0 && <p className="empty-d">{t.scanMissedNone}</p>}
      <ul className="list">
        {s.missed.map((r, i) => (
          <li key={i}>
            <button className="row scan-row" type="button" title={t.scanTune(khz(r.freq_hz))} onClick={() => onTune(r.freq_hz)}>
              <span className="rf">{khz(r.freq_hz)}</span>
              <span className="rn">
                <b><Station r={r} /></b>
                <em>{r.site_name}{r.lang && ` · ${names.lang(r.lang)}`}{r.target && ` → ${names.target(r.target)}`}</em>
              </span>
              <span />
            </button>
          </li>
        ))}
      </ul>

      <div className="actions scan-del" ref={del}>
        <button className="btn" type="button" onClick={() => setAsking(true)}>{t.scanDelete}</button>
        {asking && <Confirm message={t.scanDeleteAsk} confirmLabel={t.scanDelete} onConfirm={() => { setAsking(false); onDelete(); }} onCancel={() => setAsking(false)} anchor={del} />}
      </div>
    </div>
  );
}

interface ItemProps {
  s: ScanSummary;
  /** Its delete question is open. */
  asking: boolean;
  onOpen: () => void;
  onAsk: (on: boolean) => void;
  onDelete: () => void;
}

/** A saved scan in the list: opens it, or deletes it after asking. */
function ScanItem({ s, asking, onOpen, onAsk, onDelete }: ItemProps) {
  const t = useT();
  const names = useNames();
  const row = useRef<HTMLLIElement>(null);
  const tl = tally(s);
  return (
    <li ref={row}>
      <button type="button" onClick={onOpen}>
        <b>{names.dialBand(bandOf(s))} <small>{fmtStamp(t, s.started)}</small></b>
        <span>
          {t.scanCount(tl.signals, tl.logged, tl.expected)}
          {s.outcome.kind !== 'complete' && ` · ${t.scanOutcome[s.outcome.kind]}`}
          {s.stuck_db !== null && <em className="scan-stuck-tag">{` · ${t.scanStuckTag}`}</em>}
        </span>
      </button>
      <button className="icon-btn scan-del-one" type="button" aria-label={t.scanDeleteOne} title={t.scanDeleteOne} onClick={() => onAsk(!asking)}>✕</button>
      {asking && <Confirm message={t.scanDeleteAsk} confirmLabel={t.scanDelete} onConfirm={() => { onAsk(false); onDelete(); }} onCancel={() => onAsk(false)} anchor={row} />}
    </li>
  );
}

export function Scans({ scan, openId, onOpen, onTune, onShow, onClose }: Props) {
  const t = useT();
  const open = openId === null ? null : scan.scans.find((s) => s.id === openId) ?? null;
  const [asking, setAsking] = useState<string | null>(null);
  const [askAll, setAskAll] = useState(false);
  const delAll = useRef<HTMLDivElement>(null);
  const [full, setFull] = useState<Scan | null>(null);
  const { load } = scan;
  useEffect(() => {
    setFull(null);
    if (!openId) return;
    let alive = true;
    load(openId).then((s) => { if (alive) setFull(s); }, () => {});
    return () => { alive = false; };
  }, [openId, load]);

  return (
    <Sheet title={t.scans} onClose={onClose}>
      {open ? (
        <div className="help-page">
          <button className="link" type="button" onClick={() => onOpen(null)}>‹ {t.scanAll}</button>
          <Detail s={open} full={full} onTune={onTune} onShow={() => onShow(open.id)} onDelete={() => { onOpen(null); void scan.remove(open.id); }} />
        </div>
      ) : scan.scans.length === 0 ? (
        <p className="empty-d">{t.scansEmpty}</p>
      ) : (
        <>
          <ul className="help-index scan-index">
            {scan.scans.map((s) => (
              <ScanItem
                key={s.id}
                s={s}
                asking={asking === s.id}
                onOpen={() => onOpen(s.id)}
                onAsk={(on) => setAsking(on ? s.id : null)}
                onDelete={() => void scan.remove(s.id)}
              />
            ))}
          </ul>
          <div className="actions scan-del" ref={delAll}>
            <button className="btn" type="button" onClick={() => setAskAll(true)}>{t.scanDeleteAll}</button>
            {askAll && <Confirm message={t.scanDeleteAllAsk(scan.scans.length)} confirmLabel={t.scanDeleteAll} onConfirm={() => { setAskAll(false); void scan.removeAll(); }} onCancel={() => setAskAll(false)} anchor={delAll} />}
          </div>
        </>
      )}
    </Sheet>
  );
}
