// A panel that slides in from the right over the app (Settings, Help); full screen on phones.
// Escape or the backdrop closes it; focus moves to its title and back to the opener after.

import { useEffect, useId, useRef, type ReactNode } from 'react';
import { useDismiss } from '../hooks/useDismiss';
import { useT } from '../i18n';

interface Props {
  title: string;
  onClose: () => void;
  /** Under the title, outside the scrolling body (tabs). */
  head?: ReactNode;
  children: ReactNode;
}

export function Sheet({ title, onClose, head, children }: Props) {
  const t = useT();
  const id = useId();
  const box = useRef<HTMLElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    heading.current?.focus();
    return () => opener?.focus();
  }, []);
  useDismiss(box, onClose, true, false);

  return (
    <div className="sheet-layer">
      <div className="sheet-backdrop" onClick={onClose} />
      <section className="sheet" role="dialog" aria-modal="true" aria-labelledby={id} ref={box}>
        <header className="sheet-head">
          <h1 id={id} ref={heading} tabIndex={-1}>{title}</h1>
          <button className="icon-btn" type="button" aria-label={t.close} title={t.close} onClick={onClose}>✕</button>
        </header>
        {head}
        <div className="sheet-body">{children}</div>
      </section>
    </div>
  );
}
