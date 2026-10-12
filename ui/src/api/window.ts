// The desktop window has no OS title bar: the top bar is the title bar. On Windows and Linux it
// draws its own minimize / maximize / close; on macOS the native traffic lights stay, over its
// left end (`titleBarStyle: "Overlay"` in `tauri.macos.conf.json`). The browser has neither.

import { detectKind, type TransportKind } from './transport';
import type { Os } from '../types/generated/Os';

/** `own`: our buttons; `mac`: room for the traffic lights; `null`: a browser tab. */
export type Chrome = 'own' | 'mac' | null;

export function chromeFor(kind: TransportKind, os: Os | undefined): Chrome {
  if (kind !== 'tauri' || !os) return null;
  return os === 'macos' ? 'mac' : 'own';
}

export interface WindowCtl {
  minimize(): void;
  toggleMaximize(): void;
  close(): void;
  /** Called now and whenever the window is maximized or restored; returns an unsubscribe. */
  onMaximized(cb: (max: boolean) => void): () => void;
}

/** The desktop window's controls; `null` in a browser. */
export async function desktopWindow(): Promise<WindowCtl | null> {
  if (detectKind(window) !== 'tauri') return null;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const w = getCurrentWindow();
  return {
    minimize: () => void w.minimize(),
    toggleMaximize: () => void w.toggleMaximize(),
    // ends in `RunEvent::Exit`, which stops rigctld and the audio capture
    close: () => void w.close(),
    onMaximized(cb) {
      let live = true;
      const check = () => void w.isMaximized().then((m) => live && cb(m));
      check();
      const un = w.onResized(check);
      return () => {
        live = false;
        void un.then((f) => f());
      };
    },
  };
}
