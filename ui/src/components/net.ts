// The NET band: internet stations (radio-browser.info) in the station list. To the listener it is
// one more band in the band filter; it has its own rows and search, and it never tunes the rig or
// credits the listening history. These rules decide when the list is on NET and what it keeps of
// the other filters, so leaving NET brings the previous list back as it was.

import type { Filters } from '../hooks/useCore';
import type { NetCountry } from '../types/generated/NetCountry';
import type { NetQuery } from '../types/generated/NetQuery';
import type { NetStation } from '../types/generated/NetStation';

/** The band filter's value for internet stations (`atlas_core::net::NET_BAND`). */
export const NET_BAND = 'net';

/** The list shows internet stations: NET is picked and internet radio is on. */
export function isNet(f: Filters, enabled: boolean): boolean {
  return enabled && f.bands[0] === NET_BAND;
}

/** Back to the radio bands, every other filter as it was. */
export function leaveNet(f: Filters): Filters {
  return f.bands[0] === NET_BAND ? { ...f, bands: [] } : f;
}

/** What the NET search uses of the filters: the text and the country. Language, region, source,
 *  a transmitter site and a scan are about the schedule; they stay set for when NET is left. */
export function netQuery(f: Filters, offset: number, limit: number): NetQuery {
  return { q: f.q.trim(), country: f.countries[0] ?? null, offset, limit };
}

/** Filters a lens tab sets: All keeps NET, the others (about the schedule) leave it. */
export function lensFilters(f: Filters, lens: Filters['lens']): Filters {
  return lens === 'all' ? { ...f, lens } : { ...leaveNet(f), lens };
}

/** `MP3 128k`, `AAC+`, `HLS`, or '' when the directory doesn't say. */
export function streamLabel(s: Pick<NetStation, 'codec' | 'bitrate' | 'hls'>): string {
  const parts = [s.codec ?? (s.hls ? 'HLS' : ''), s.bitrate ? `${s.bitrate}k` : ''].filter(Boolean);
  return parts.join(' ');
}

/** The country filter's choices on NET, by name in the UI language. */
export function netCountryOpts(list: NetCountry[], locale: string): { value: string; label: string }[] {
  const es = locale === 'es';
  return list
    .map((c) => ({ value: c.itu, label: es ? c.es : c.en }))
    .sort((a, b) => a.label.localeCompare(b.label, locale));
}
