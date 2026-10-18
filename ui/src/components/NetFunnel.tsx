// The NET faceplate's stream buffer, in the whole dial's place: a funnel the stream pours into
// (funnel.ts), filled with the audio held, a gate at the size it fills to before playing and a
// dashed line where the oldest starts to go. Its sections are the sizes the listener can pick:
// pressing one fixes the buffer there, Auto lets it grow each time the stream cuts out.

import { useT } from '../i18n';
import { FUNNEL_W, funnelFill, funnelLine, funnelOutline, funnelTiers } from './funnel';

const OUTLINE = funnelOutline();
const TIERS = funnelTiers();

interface Props {
  /** Audio held; `null` while nobody listens. */
  heldMs: number | null;
  /** What it fills to before it plays. */
  targetMs: number;
  /** Most it holds before the oldest goes. */
  maxMs: number;
  /** The listener's size, or `null` for automatic. */
  setting: number | null;
  /** The stream is playing: it flows through. */
  flowing: boolean;
  onBuffer: (ms: number | null) => void;
}

export function NetFunnel({ heldMs, targetMs, maxMs, setting, flowing, onBuffer }: Props) {
  const t = useT();
  const secs = (ms: number) => t.netBufferSecs(ms / 1000);
  const held = heldMs ?? 0;
  const gate = funnelLine(targetMs), max = funnelLine(maxMs);
  const auto = setting === null;
  return (
    <div className="funnel" role="radiogroup" aria-label={t.netBufferPick}>
      <button
        type="button"
        role="radio"
        aria-checked={auto}
        className="funnel-auto"
        title={t.netBufferAutoHint}
        onClick={() => { if (!auto) onBuffer(null); }}
      >
        {auto ? t.netBufferAutoAt(secs(targetMs)) : t.netBufferAuto}
      </button>
      <div className="funnel-box">
        <svg
          className={'funnel-svg' + (flowing ? ' flowing' : '')}
          viewBox={`0 0 ${FUNNEL_W} 1`}
          preserveAspectRatio="none"
          role="img"
          aria-label={t.netFunnelLabel(heldMs === null ? '--' : `${(held / 1000).toFixed(1)} s`, secs(targetMs))}
        >
          <path className="funnel-body" d={OUTLINE} />
          {TIERS.map((s) => {
            const l = funnelLine(s.ms);
            return <line key={s.ms} className="funnel-sec" x1={l.x} x2={l.x} y1={l.y1} y2={l.y2} />;
          })}
          <path className="funnel-fill" d={funnelFill(held)} />
          <line className="funnel-flow" x1={0} x2={FUNNEL_W} y1={0.5} y2={0.5} />
          <line className="funnel-max" x1={max.x} x2={max.x} y1={max.y1} y2={max.y2}><title>{t.netFunnelMax}</title></line>
          <line className="funnel-gate" x1={gate.x} x2={gate.x} y1={gate.y1} y2={gate.y2} />
        </svg>
        <div className="funnel-tiers">
          {TIERS.map((s) => (
            <button
              key={s.ms}
              type="button"
              role="radio"
              aria-checked={setting === s.ms}
              className={auto && s.ms === targetMs ? 'auto-at' : undefined}
              style={{ left: s.pct + '%' }}
              onClick={() => { if (setting !== s.ms) onBuffer(s.ms); }}
            >
              {secs(s.ms)}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
