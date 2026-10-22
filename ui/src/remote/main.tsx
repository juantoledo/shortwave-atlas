// The phone remote's page (remote.html): no globe, so it loads fast on a phone.

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { Remote } from './Remote';
import '../fonts.css';
import '../tokens.css';
import './remote.css';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Remote />
  </StrictMode>,
);
