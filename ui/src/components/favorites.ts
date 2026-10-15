// Small favorite lookups the stars need.

import type { Favorite } from '../types/generated/Favorite';
import type { FavoriteRow } from '../types/generated/FavoriteRow';
import type { FavoriteView } from '../types/generated/FavoriteView';
import type { Mode } from '../types/generated/Mode';
import type { StationRow } from '../types/generated/StationRow';

/** What a favorite says beyond its station's row (the label given, the mode kept), so two
 *  favorites of one station tell apart: `Evening · USB`, or '' when it says nothing more. */
export function favNote(f: Favorite, row: StationRow): string {
  return [f.label !== row.name ? f.label : '', f.mode !== row.mode ? f.mode : ''].filter(Boolean).join(' · ');
}

/** A station favorite's row as a click should tune it: in the favorite's mode. */
export function favTuning(r: Extract<FavoriteRow, { kind: 'station' }>): StationRow {
  return r.favorite.mode === r.row.mode ? r.row : { ...r.row, mode: r.favorite.mode };
}

export const MODES: readonly Mode[] = ['USB', 'LSB', 'CW', 'CWR', 'AM', 'AMN', 'FM'];

/** The rig's mode as one a favorite can keep (the rig may report others, e.g. PKTUSB). */
export const asMode = (m: string | null): Mode => (m && MODES.includes(m as Mode) ? (m as Mode) : 'AM');

/** The favorite kept on exactly `hz`: the one with `stationId` if there is one, else the first. */
export function favAt(views: FavoriteView[], hz: number | null, stationId: number | null): FavoriteView | null {
  if (hz === null) return null;
  const here = views.filter((v) => v.favorite.freq_hz === hz);
  return here.find((v) => stationId !== null && v.station_id === stationId) ?? here[0] ?? null;
}
