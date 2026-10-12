// The Stats view's charts, drawn by hand like the scan profiles: one amber hue for "how much"
// (the dial lamp), thin rounded bars with a gap between them, and a title on every mark for
// hover and screen readers. Text stays in the text colours, never the series colour.

import type { CSSProperties, ReactNode } from 'react';
import { heatLevel, labelEvery, shares, split } from './stats';

interface BarsProps {
  values: readonly number[];
  labels: readonly string[];
  /** Hover text per bar. */
  titles: readonly string[];
  /** Labels printed under the bars at most (the others stay in the titles). */
  room?: number;
  /** The bar of "now", drawn brighter. */
  current?: number;
}

/** Vertical bars over time. */
export function Bars({ values, labels, titles, room = 12, current }: BarsProps) {
  const h = shares(values);
  const every = labelEvery(values.length, room);
  return (
    <div className="st-bars" role="img" aria-label={titles.join('; ')}>
      <div className="st-bars-plot">
        {h.map((v, i) => (
          <span key={i} className={'st-bar' + (i === current ? ' cur' : '')} title={titles[i]}>
            <i style={{ height: `${v * 100}%` }} data-zero={values[i] === 0 || undefined} />
          </span>
        ))}
      </div>
      <div className="st-bars-axis" aria-hidden="true">
        {labels.map((l, i) => <span key={i}>{i % every === 0 ? l : ''}</span>)}
      </div>
    </div>
  );
}

interface HeatProps {
  /** Rows × columns of values. */
  cells: readonly (readonly number[])[];
  rowLabels: readonly string[];
  colLabels: readonly string[];
  title: (row: number, col: number, v: number) => string;
}

/** A cell's fill: amber by level, inline (older webviews have no `color-mix`). */
const heatStyle = (lv: number): CSSProperties | undefined =>
  lv > 0 ? { background: `rgba(255, 179, 63, ${(0.15 + 0.85 * lv).toFixed(2)})` } : undefined;

/** A grid of cells, darker to brighter amber by value. */
export function Heat({ cells, rowLabels, colLabels, title }: HeatProps) {
  const max = Math.max(0, ...cells.flat());
  return (
    <div className="st-heat" role="img" aria-label={rowLabels.join(', ')}>
      {cells.map((row, r) => (
        <div className="st-heat-row" key={r}>
          <span className="st-heat-lab">{rowLabels[r]}</span>
          {row.map((v, c) => (
            <span key={c} className="st-cell" title={title(r, c, v)} style={heatStyle(heatLevel(v, max))} />
          ))}
        </div>
      ))}
      <div className="st-heat-row st-heat-axis" aria-hidden="true">
        <span className="st-heat-lab" />
        {colLabels.map((l, c) => <span key={c}>{l}</span>)}
      </div>
    </div>
  );
}

export interface HRow {
  key: string;
  label: ReactNode;
  value: number;
  /** Right of the bar: the value as text. */
  text: string;
  /** Under the value: the change, as text. */
  aside?: ReactNode;
  title?: string;
  onClick?: () => void;
}

/** Horizontal bars, one row each, biggest first; a row may be a button. */
export function HBars({ rows }: { rows: readonly HRow[] }) {
  const w = shares(rows.map((r) => r.value));
  return (
    <ul className="st-hbars">
      {rows.map((r, i) => {
        const inner = (
          <>
            <span className="st-hl">{r.label}</span>
            <span className="st-hv">{r.text}{r.aside}</span>
            <span className="st-ht" aria-hidden="true"><i style={{ width: `${w[i] * 100}%` }} /></span>
          </>
        );
        return (
          <li key={r.key}>
            {r.onClick ? (
              <button type="button" className="st-hrow" title={r.title} onClick={r.onClick}>{inner}</button>
            ) : (
              <div className="st-hrow" title={r.title}>{inner}</div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** One bar split into day, twilight and night. */
export function SplitBar({ parts, title }: { parts: [number, number, number]; title: string }) {
  const [d, t, n] = split(...parts);
  return (
    <span className="st-split" title={title}>
      {d > 0 && <i className="day" style={{ flexGrow: d }} />}
      {t > 0 && <i className="tw" style={{ flexGrow: t }} />}
      {n > 0 && <i className="night" style={{ flexGrow: n }} />}
    </span>
  );
}

/** A small ring filled to `p` (0..1), for a badge on its way. */
export function Ring({ p, children }: { p: number; children: ReactNode }) {
  const r = 20, c = 2 * Math.PI * r;
  return (
    <span className="st-ring">
      <svg viewBox="0 0 48 48" aria-hidden="true">
        <circle className="track" cx="24" cy="24" r={r} />
        <circle className="fill" cx="24" cy="24" r={r} strokeDasharray={`${c * Math.min(1, p)} ${c}`} transform="rotate(-90 24 24)" />
      </svg>
      <span className="st-ring-in">{children}</span>
    </span>
  );
}
