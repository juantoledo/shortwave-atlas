// How the stage shows the stations: the globe, the flat map, or the timetable. Over the maps it
// sits with their controls; over the timetable, in its header.

import { useT } from '../../i18n';
import type { MapMode } from '../../hooks/useDisplay';

const MODES: readonly MapMode[] = ['globe', 'flat', 'timetable'];

interface Props {
  mode: MapMode;
  onMode: (m: MapMode) => void;
  /** The globe could not start here (no WebGL). */
  noGlobe?: boolean;
}

export function MapModeSwitch({ mode, onMode, noGlobe }: Props) {
  const t = useT();
  const label = { globe: t.mapGlobe, flat: t.mapFlat, timetable: t.mapTimetable };
  return (
    <div className="seg map-mode" role="radiogroup" aria-label={t.mapView}>
      {MODES.map((m) => (
        <button key={m} type="button" role="radio" aria-checked={mode === m} disabled={m === 'globe' && noGlobe} onClick={() => onMode(m)}>
          {label[m]}
        </button>
      ))}
    </div>
  );
}
