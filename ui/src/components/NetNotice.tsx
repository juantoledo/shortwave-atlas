// Before the NET band is turned on: what it is, where its stations come from, what leaves this
// computer, how to use it, how far to trust it, and who answers for it. Turn on or Cancel; Escape and the backdrop
// cancel. Rendered on <body>: the Settings sheet it opens from is transformed, and would hold a
// fixed layer inside it.

import { useEffect, useId, useRef, type KeyboardEvent } from 'react';
import { createPortal } from 'react-dom';
import { useT } from '../i18n';

interface Props {
  /** Turning it on: the button waits. */
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function NetNotice({ busy, onConfirm, onCancel }: Props) {
  const t = useT();
  const id = useId();
  const box = useRef<HTMLElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    cancel.current?.focus();
    return () => opener?.focus();
  }, []);

  // Tab stays inside; Escape cancels here and does not reach the sheet behind
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onCancel(); return; }
    if (e.key !== 'Tab' || !box.current) return;
    const items = [...box.current.querySelectorAll<HTMLElement>('a[href], button:not(:disabled)')];
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  };

  return createPortal(
    <div className="welcome-layer" onKeyDown={onKeyDown}>
      <div className="sheet-backdrop" onClick={onCancel} />
      <section className="welcome net" role="dialog" aria-modal="true" aria-labelledby={id} ref={box}>
        <div className="welcome-body">
          <span className="pill net on">{t.netBand}</span>
          <h1 id={id}>{t.netNoticeTitle}</h1>
          <p className="net-tagline">{t.netNoticeTagline}</p>
          <section className="data-notice net-notice">
            <h2 className="h"><span>{t.netNoticeSourceTitle}</span></h2>
            <p>
              {t.netNoticeSource}{' '}
              <a className="link" href="https://www.radio-browser.info" target="_blank" rel="noreferrer">radio-browser.info</a>
              <svg className="net-heart" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 14.2 6.9 13.2C3 9.7.5 7.4.5 4.6.5 2.3 2.3.5 4.6.5c1.3 0 2.6.6 3.4 1.6C8.8 1.1 10.1.5 11.4.5c2.3 0 4.1 1.8 4.1 4.1 0 2.8-2.5 5.1-6.4 8.6L8 14.2z" /></svg>
            </p>
            <h2 className="h"><span>{t.netNoticeSentTitle}</span></h2>
            <ul>{t.netNoticeSent.map((s) => <li key={s}>{s}</li>)}</ul>
            <h2 className="h"><span>{t.netNoticeHowTitle}</span></h2>
            <ul>{t.netNoticeHow.map((s) => <li key={s}>{s}</li>)}</ul>
            <h2 className="h"><span>{t.netNoticeAsIsTitle}</span></h2>
            <p>{t.netNoticeAsIs}</p>
            <h2 className="h"><span>{t.netNoticeLegalTitle}</span></h2>
            <p>{t.netNoticeLegal}</p>
          </section>
        </div>
        <footer className="welcome-foot">
          <button className="btn" type="button" ref={cancel} onClick={onCancel}>{t.cancel}</button>
          <button className="btn primary" type="button" disabled={busy} onClick={onConfirm}>{t.netNoticeTurnOn}</button>
        </footer>
      </section>
    </div>,
    document.body,
  );
}
