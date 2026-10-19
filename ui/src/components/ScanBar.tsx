// The scan controls under the band dial: Scan (or Stop with the progress) for the band you are
// in, its step and speed, and how long it will take; while it runs, a trace of the latest
// readings in place of the pickers; after a scan that heard nothing because the S-meter stuck
// (RF gain down), a warning until the band is scanned again. The saved scans open from the top bar.

import { useId, useState } from 'react';
import { useNames } from '../catalog/names';
import { fmtSecs, sLabel, useT } from '../i18n';
import type { Band } from '../types/generated/Band';
import type { ScanSpeed } from '../types/generated/ScanSpeed';
import type { ScanStatus } from '../types/generated/ScanStatus';
import { Dropdown } from './Dropdown';
import { gridPoints, scanSecs, SPEED_CHOICES, stepChoices, stepKhz, trail } from './scan';

interface Props {
  on: boolean;
  /** The band the band dial shows. */
  band: Band | null;
  status: ScanStatus;
  /** Where the S-meter stuck in the band's last scan, if it did (`ScanSummary::stuck_db`). */
  stuckDb: number | null;
  onStart: (bandId: string, stepHz: number, speed: ScanSpeed) => void;
  onStop: () => void;
}

export function ScanBar({ on, band, status, stuckDb, onStart, onStop }: Props) {
  const t = useT();
  const names = useNames();
  const stepId = useId();
  const speedId = useId();
  // the step picked per band, for this session
  const [steps, setSteps] = useState<Record<string, number>>({});
  const step = band ? steps[band.id] ?? band.scan_step_hz : 0;
  // one speed for every band, for this session
  const [speed, setSpeed] = useState<ScanSpeed>('normal');
  const running = status.state === 'running' ? status : null;
  const pct = running && running.total ? Math.round((running.done / running.total) * 100) : 0;

  return (
    <div className="scan-bar rx-x">
      <div className="scan-go">
        {running ? (
          <button className="btn scan-btn on" type="button" title={t.scanStopTitle} onClick={onStop}>{t.scanStop(pct)}</button>
        ) : (
          <button
            className="btn scan-btn"
            type="button"
            disabled={!on || !band}
            title={band ? t.scanTitle(names.dialBand(band)) : t.scanNoBand}
            onClick={() => band && onStart(band.id, step, speed)}
          >
            <svg className="scan-ico" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 8l4.6-4.6M14.5 8A6.5 6.5 0 1 1 8 1.5M11.5 8A3.5 3.5 0 1 1 8 4.5" /></svg>
            {t.scan}
          </button>
        )}
        {band && !running && (
          <div className="scan-step">
            <span className="sr" id={stepId}>{t.scanStep}</span>
            <Dropdown
              labelledBy={stepId}
              value={String(step)}
              opts={stepChoices(band).map((s) => ({ value: String(s), label: `${stepKhz(s)} kHz`, hint: s === band.scan_step_hz ? '•' : undefined }))}
              onChange={(v) => setSteps((m) => ({ ...m, [band.id]: Number(v) }))}
            />
          </div>
        )}
        {band && !running && (
          <div className="scan-step" title={t.scanSpeedTitle}>
            <span className="sr" id={speedId}>{t.scanSpeed}</span>
            <Dropdown
              labelledBy={speedId}
              value={speed}
              opts={SPEED_CHOICES.map((s) => ({ value: s, label: t.scanSpeeds[s] }))}
              onChange={(v) => setSpeed(v as ScanSpeed)}
            />
          </div>
        )}
        {running && <ScanTrail samples={running.samples} done={running.done} />}
      </div>
      <div className="scan-info">
        {running ? (
          <progress className="scan-progress" max={running.total} value={running.done} aria-label={t.scanStop(pct)} />
        ) : (
          band && <span className="scan-est">{t.scanEstimate(gridPoints(band, step), fmtSecs(t, scanSecs(gridPoints(band, step), speed)))}</span>
        )}
        {!running && band && stuckDb !== null && <span className="scan-stuck-last" role="status">{t.scanStuckLast(sLabel(stuckDb))}</span>}
      </div>
    </div>
  );
}

/** The latest readings as bars scrolling in from the right, the newest lit, under a sweeping
 *  beam. Each bar is keyed by its grid point, so only the new one rises. */
function ScanTrail({ samples, done }: { samples: readonly (number | null)[]; done: number }) {
  const cells = trail(samples, done);
  return (
    <div className="scan-trail" aria-hidden="true">
      {cells.map((c, k) => (
        <i
          key={c.i}
          className={(c.h === null ? 'gap' : '') + (k === cells.length - 1 ? ' new' : '')}
          style={{ height: `${Math.round((c.h ?? 0.08) * 100)}%`, opacity: 0.25 + (0.75 * (k + 1)) / cells.length }}
        />
      ))}
      <span className="scan-beam" />
    </div>
  );
}
