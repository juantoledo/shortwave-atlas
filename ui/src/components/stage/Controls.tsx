// Over the stage, bottom right: globe or flat map, zoom, back to the QTH, and whether a route is
// framed once it lands.

import { useT } from '../../i18n';
import type { MapMode } from '../../hooks/useDisplay';
import { ZOOM_STEP } from './fx';
import type { Stage } from './useStage';

interface Props {
  stage: Stage;
  mode: MapMode;
  onMode: (m: MapMode) => void;
  /** The globe could not start here (no WebGL): only the flat map. */
  noGlobe?: boolean;
  framing: boolean;
  onFrame: () => void;
}

export function Controls({ stage, mode, onMode, noGlobe, framing, onFrame }: Props) {
  const t = useT();
  const off = !stage.ready;
  return (
    <div className="globe-ctl" role="group" aria-label={t.globeControls}>
      <div className="seg map-mode" role="radiogroup" aria-label={t.mapView}>
        {(['globe', 'flat'] as const).map((m) => (
          <button key={m} type="button" role="radio" aria-checked={mode === m} disabled={m === 'globe' && noGlobe} onClick={() => onMode(m)}>
            {m === 'globe' ? t.mapGlobe : t.mapFlat}
          </button>
        ))}
      </div>
      <div className="zoom">
        <button className="icon-btn" type="button" disabled={off} aria-label={t.zoomIn} title={t.zoomIn} onClick={() => stage.zoom(ZOOM_STEP)}>+</button>
        <button className="icon-btn" type="button" disabled={off} aria-label={t.zoomOut} title={t.zoomOut} onClick={() => stage.zoom(1 / ZOOM_STEP)}>−</button>
        <button className="icon-btn" type="button" disabled={off} aria-label={t.centerQth} title={t.centerQth} onClick={stage.goHome}>⌖</button>
      </div>
      <button className="toggle" type="button" aria-pressed={framing} onClick={onFrame}>{t.frameRoute}</button>
    </div>
  );
}
