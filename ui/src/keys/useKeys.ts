// The one document-wide key listener: maps keys with `keyAction` and hands the action to `run`.

import { useEffect, useRef } from 'react';
import { control, isMac, keyAction, typing, widget, type Action } from './keymap';

export function useKeys(sheet: boolean, run: (a: Action) => void) {
  const latest = useRef({ sheet, run });
  latest.current = { sheet, run };
  useEffect(() => {
    const mac = isMac(navigator.userAgent);
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return;
      const a = keyAction(
        { key: e.key, code: e.code, shiftKey: e.shiftKey, ctrlKey: e.ctrlKey, altKey: e.altKey, metaKey: e.metaKey, isComposing: e.isComposing, altGraph: e.getModifierState('AltGraph') },
        { typing: typing(e.target), control: control(e.target), widget: widget(e.target), sheet: latest.current.sheet, mac },
      );
      if (!a) return;
      e.preventDefault();
      latest.current.run(a);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);
}
