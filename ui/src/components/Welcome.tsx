// The welcome splash at start: what Shortwave Atlas is, and where its data comes from. Only its
// button closes it (no Escape, no press outside), so it is read once; "Don't show this again"
// keeps it closed on this device (hooks/useWelcome.ts).

import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';
import { useT } from '../i18n';
import { DataNotice } from './DataNotice';

interface Props {
  onClose: (dontShow: boolean) => void;
}

export function Welcome({ onClose }: Props) {
  const t = useT();
  const id = useId();
  const [dontShow, setDontShow] = useState(false);
  const box = useRef<HTMLElement>(null);
  const start = useRef<HTMLButtonElement>(null);
  useEffect(() => { start.current?.focus(); }, []);

  // Tab stays inside; Escape does nothing here nor behind
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); return; }
    if (e.key !== 'Tab' || !box.current) return;
    const items = [...box.current.querySelectorAll<HTMLElement>('a[href], input, button')];
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  };

  return (
    <div className="welcome-layer" onKeyDown={onKeyDown}>
      <div className="sheet-backdrop" />
      <section className="welcome" role="dialog" aria-modal="true" aria-labelledby={id} ref={box}>
        <div className="welcome-body">
          <h1 id={id}>{t.welcomeTitle}</h1>
          <p>{t.welcomeBody}</p>
          <p className="welcome-enjoy">{t.welcomeEnjoy}</p>
          <DataNotice />
          <p className="note">{t.welcomeLater}</p>
        </div>
        <footer className="welcome-foot">
          <label className="check">
            <input type="checkbox" checked={dontShow} onChange={(e) => setDontShow(e.target.checked)} />
            <span>{t.welcomeDontShow}</span>
          </label>
          <button className="btn primary" type="button" ref={start} onClick={() => onClose(dontShow)}>{t.welcomeStart}</button>
        </footer>
      </section>
    </div>
  );
}
