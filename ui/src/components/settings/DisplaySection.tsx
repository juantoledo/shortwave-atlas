// Display: the globe behind the whole window (frosted panels) or framed in the middle. Kept on
// this device and applied at once.

import type { DisplayApi } from '../../hooks/useDisplay';
import { useT } from '../../i18n';

export function DisplaySection({ display }: { display: DisplayApi }) {
  const t = useT();
  return (
    <section className="sec form" aria-label={t.displaySection}>
      <h2 className="h"><span>{t.displaySection}</span></h2>
      <div className="field">
        <span>{t.globeLayout}</span>
        <div className="seg" role="radiogroup" aria-label={t.globeLayout}>
          {[true, false].map((on) => (
            <button key={String(on)} type="button" role="radio" aria-checked={display.fullGlobe === on} onClick={() => display.setFullGlobe(on)}>
              {on ? t.globeFull : t.globeFramed}
            </button>
          ))}
        </div>
        <small>{t.globeFullHelp}</small>
      </div>
    </section>
  );
}
