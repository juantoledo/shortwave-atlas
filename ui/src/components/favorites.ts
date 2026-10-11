// Small favorite lookups the stars need.

import type { FavoriteView } from '../types/generated/FavoriteView';
import type { Mode } from '../types/generated/Mode';

const MODES: readonly string[] = ['USB', 'LSB', 'CW', 'CWR', 'AM', 'AMN', 'FM'] satisfies Mode[];

/** The rig's mode as one a favorite can keep (the rig may report others, e.g. PKTUSB). */
export const asMode = (m: string | null): Mode => (m && MODES.includes(m) ? (m as Mode) : 'AM');

/** The favorite kept on exactly `hz`: the one with `stationId` if there is one, else the first. */
export function favAt(views: FavoriteView[], hz: number | null, stationId: number | null): FavoriteView | null {
  if (hz === null) return null;
  const here = views.filter((v) => v.favorite.freq_hz === hz);
  return here.find((v) => stationId !== null && v.station_id === stationId) ?? here[0] ?? null;
}
