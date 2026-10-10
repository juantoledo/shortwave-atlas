// A yes/no question in a popover under the button that asked it. Used instead of
// `window.confirm`, which the Linux and macOS webviews of the desktop app never show (it just
// answers "no"). Cancel has the focus; Escape or a press outside the popover cancels.

import { useEffect, useId, useRef, type RefObject } from 'react';
import { useDismiss } from '../hooks/useDismiss';
import { useT } from '../i18n';

interface Props {
  message: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  /** Presses inside this element (the button and the popover) don't cancel. */
  anchor: RefObject<HTMLElement | null>;
}

export function Confirm({ message, confirmLabel, onConfirm, onCancel, anchor }: Props) {
  const t = useT();
  const id = useId();
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    cancel.current?.focus();
    return () => opener?.focus();
  }, []);
  useDismiss(anchor, onCancel);

  return (
    <section className="confirm-pop" role="alertdialog" aria-describedby={id}>
      <p id={id}>{message}</p>
      <div className="actions">
        <button className="btn primary" type="button" onClick={onConfirm}>{confirmLabel}</button>
        <button className="btn" type="button" ref={cancel} onClick={onCancel}>{t.cancel}</button>
      </div>
    </section>
  );
}
