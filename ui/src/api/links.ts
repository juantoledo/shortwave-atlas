// The desktop webview drops `target="_blank"` links, so in the desktop app they open in the
// system browser instead. The browser client keeps its normal new tab.

import { detectKind } from './transport';

/** The web address a click should hand to the system browser, or null to leave the click alone. */
export function externalHref(target: EventTarget | null): string | null {
  const a = target instanceof Element ? target.closest('a[target="_blank"]') : null;
  const href = a?.getAttribute('href');
  return href && /^https?:\/\//i.test(href) ? href : null;
}

export function openLinksInBrowser() {
  if (detectKind(window) !== 'tauri') return;
  document.addEventListener('click', (e) => {
    if (e.defaultPrevented || e.button !== 0) return;
    const href = externalHref(e.target);
    if (!href) return;
    e.preventDefault();
    void import('@tauri-apps/plugin-opener').then(({ openUrl }) => openUrl(href));
  });
}
