import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { openLinksInBrowser } from './api/links';
import { detectKind } from './api/transport';
import { openPage, pageFacts, REMOTE_PAGE } from './page';
import './fonts.css';
import './tokens.css';
import './styles.css';

// A phone goes to the remote (page.ts) before the atlas, globe and all, is downloaded. A server
// without the remote answers its path with this page: the atlas then, not another redirect.
const here = location.pathname.endsWith(REMOTE_PAGE);
if (!here && openPage(pageFacts(detectKind(window))) === 'remote') {
  location.replace(REMOTE_PAGE);
} else {
  openLinksInBrowser();
  void import('./App').then(({ App }) => {
    createRoot(document.getElementById('root')!).render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
  });
}
