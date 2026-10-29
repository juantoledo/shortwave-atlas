// A one-time tip on a feature (hooks/useTips.ts): a dot on the corner of what it wraps until
// this device has seen it; pressing the dot says what the feature is, and Got it puts it away
// for good (as using the feature does: App dismisses it there).

import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { placeStyle, usePopup } from '../hooks/usePopup';
import { useTips } from '../hooks/useTips';
import { useT } from '../i18n';
import type { TipId } from '../tour/tour';

/** `off`: the feature isn't there now (no dot, though it still wraps). */
export function Tip({ id, off = false, children }: { id: TipId; off?: boolean; children: ReactNode }) {
  const t = useT();
  const tips = useTips();
  const titleId = useId();
  const shown = !off && tips.shown(id);
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLSpanElement>(null);
  const dot = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const ok = useRef<HTMLButtonElement>(null);
  // under (or over) the feature itself, not just its dot
  const place = usePopup(open && shown, box, box, pop, () => setOpen(false));
  useEffect(() => { if (open && shown) ok.current?.focus(); }, [open, shown]);

  const onKeyDown = (e: KeyboardEvent) => {
    e.stopPropagation();
    if (e.key === 'Escape') { setOpen(false); dot.current?.focus(); }
  };

  const [title, body] = t.tips[id];
  return (
    <span className="tip-wrap" ref={box}>
      {children}
      {shown && <button className="tip-dot" type="button" ref={dot} aria-label={t.tipOpen(title)} title={title} aria-expanded={open} onClick={() => setOpen((o) => !o)} />}
      {open && shown && createPortal(
        <div className="tip-pop" role="dialog" aria-labelledby={titleId} ref={pop} style={placeStyle(place)} onKeyDown={onKeyDown}>
          <b id={titleId}>{title}</b>
          <p>{body}</p>
          <div className="actions">
            <button className="btn primary" type="button" ref={ok} onClick={() => tips.dismiss(id)}>{t.tipGotIt}</button>
          </div>
        </div>,
        document.body,
      )}
    </span>
  );
}
