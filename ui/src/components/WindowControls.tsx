// Minimize, maximize/restore and close, at the right end of the top bar in the desktop app on
// Windows and Linux (the window has no OS title bar; see api/window.ts).

import { useEffect, useState } from 'react';
import { desktopWindow, type WindowCtl } from '../api/window';
import { useT } from '../i18n';

export function WindowControls() {
  const t = useT();
  const [ctl, setCtl] = useState<WindowCtl | null>(null);
  const [max, setMax] = useState(false);
  useEffect(() => {
    let live = true;
    void desktopWindow().then((c) => live && setCtl(c));
    return () => void (live = false);
  }, []);
  useEffect(() => ctl?.onMaximized(setMax), [ctl]);
  if (!ctl) return null;
  const maxLabel = max ? t.winRestore : t.winMaximize;
  return (
    <div className="win-ctl">
      <button type="button" aria-label={t.winMinimize} title={t.winMinimize} onClick={ctl.minimize}>
        <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" /></svg>
      </button>
      <button type="button" aria-label={maxLabel} title={maxLabel} onClick={ctl.toggleMaximize}>
        <svg viewBox="0 0 10 10" aria-hidden="true">
          {max ? <path d="M.5 2.5h7v7h-7zM2.5 2.5v-2h7v7h-2" /> : <path d="M.5.5h9v9h-9z" />}
        </svg>
      </button>
      <button className="win-close" type="button" aria-label={t.winClose} title={t.winClose} onClick={ctl.close}>
        <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M.5.5l9 9M9.5.5l-9 9" /></svg>
      </button>
    </div>
  );
}
