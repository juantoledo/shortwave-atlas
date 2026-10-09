// The station card's backdrop: the broadcaster's flag, large and faint, waving softly under a
// moving light. Pure CSS: the flag is cut into strips that rise and fall one after another.
// Positioned against the panel (not the scrolling dossier), so it stays put while the card scrolls.

import type { CSSProperties } from 'react';
import { flagUrl } from '../catalog/flags';

/** Vertical strips the flag is cut into; more is a smoother wave. Keep in step with styles.css. */
const STRIPS = 24;

export function FlagBackdrop({ flag }: { flag: string | null }) {
  const url = flagUrl(flag);
  if (!url) return null;
  return (
    <div className="flag-bg" aria-hidden="true">
      <div className="flag-stage" style={{ '--flag': `url("${url}")` } as CSSProperties}>
        {Array.from({ length: STRIPS }, (_, i) => <i key={i} style={{ '--i': i } as CSSProperties} />)}
        <b className="flag-sheen" />
      </div>
      <div className="flag-glow" />
    </div>
  );
}
