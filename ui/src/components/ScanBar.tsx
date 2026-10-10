// The scan controls under the band dial: Scan (or Stop with the progress) for the band you are
// in, its step, how long it will take, and the chip of the band's last scan (noise floor, and
// the stations logged of those on the air), which opens the scans.

import { useId, useState } from 'react';
import { useNames } from '../catalog/names';
import { fmtSecs, sLabel, useT } from '../i18n';
import type { Band } from '../types/generated/Band';
import type { ScanStatus } from '../types/generated/ScanStatus';
import type { ScanSummary } from '../types/generated/ScanSummary';
import { Dropdown } from './Dropdown';
import { gridPoints, scanSecs, stepChoices, stepKhz, tally } from './scan';

interface Props {
  on: boolean;
  /** The band the band dial shows. */
  band: Band | null;
  status: ScanStatus;
  /** The band's last scan. */
  last: ScanSummary | null;
  onStart: (bandId: string, stepHz: number) => void;
  onStop: () => void;
  onOpen: (id: string) => void;
}

export function ScanBar({ on, band, status, last, onStart, onStop, onOpen }: Props) {
  const t = useT();
  const names = useNames();
  const stepId = useId();
  // the step picked per band, for this session
  const [steps, setSteps] = useState<Record<string, number>>({});
  const step = band ? steps[band.id] ?? band.scan_step_hz : 0;
  const running = status.state === 'running' ? status : null;
  const pct = running && running.total ? Math.round((running.done / running.total) * 100) : 0;
  const tl = last && tally(last);

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
            onClick={() => band && onStart(band.id, step)}
          >
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
      </div>
      <div className="scan-info">
        {running ? (
          <progress className="scan-progress" max={running.total} value={running.done} aria-label={t.scanStop(pct)} />
        ) : (
          band && <span className="scan-est">{t.scanEstimate(gridPoints(band, step), fmtSecs(t, scanSecs(gridPoints(band, step))))}</span>
        )}
        {last && tl && !running && (
          <button className="chip scan-chip" type="button" title={t.scanChipTitle} onClick={() => onOpen(last.id)}>
            {t.scanChip(sLabel(last.noise_db), tl.logged, tl.expected)}
          </button>
        )}
      </div>
    </div>
  );
}
