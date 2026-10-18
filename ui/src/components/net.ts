// The NET band: internet stations (radio-browser.info) in the station list. To the listener it is
// one more band in the band filter; it has its own rows and search, and it never tunes the rig or
// credits the listening history. These rules decide when the list is on NET and what it keeps of
// the other filters, so leaving NET brings the previous list back as it was.

import type { Filters } from '../hooks/useCore';
import type { NetCountry } from '../types/generated/NetCountry';
import type { NetDot } from '../types/generated/NetDot';
import type { NetFavorite } from '../types/generated/NetFavorite';
import type { SiteDot } from '../types/generated/SiteDot';
import type { NetNow } from '../types/generated/NetNow';
import type { NetQuery } from '../types/generated/NetQuery';
import type { NetStation } from '../types/generated/NetStation';

/** The band filter's value for internet stations (`atlas_core::net::NET_BAND`). */
export const NET_BAND = 'net';
/** Sample rate of an internet station's audio (`atlas_core::net::NET_RATE`). */
export const NET_RATE = 32_000;
/** The stream buffers the listener can pick (`atlas_core::net::NET_BUFFER_STEPS_MS`). */
export const NET_BUFFER_STEPS_MS = [1_000, 2_000, 4_000, 8_000, 16_000] as const;

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

/** The radio list's search also asks the internet directory: words typed on the All lens, with no
 *  filter NET can't follow (a band, language, region, source, transmitter site or scan). A kHz
 *  query is about the dial. The country applies to both. */
export function mixesNet(f: Filters): boolean {
  const q = f.q.trim();
  return f.lens === 'all' && q !== '' && !/^[\d.]+$/.test(q)
    && !f.bands.length && !f.langs.length && !f.regions.length && !f.sources.length
    && f.site === null && f.scan === null;
}

/** Internet stations shown among the radio search's results; the rest are on the NET band. */
export const NET_MIX = 5;

/** Filters a lens tab sets: All and Favorites keep NET (Favorites lists both kinds on any band, and
 *  All goes back to the band it was on); Heard and Listened are about the schedule and leave it. */
export function lensFilters(f: Filters, lens: Filters['lens']): Filters {
  return lens === 'all' || lens === 'favorites' ? { ...f, lens } : { ...leaveNet(f), lens };
}

/** The NET favorites a search shows (by name or label, any case). */
export function favFilter(list: readonly NetFavorite[], q: string): NetFavorite[] {
  const k = q.trim().toLocaleLowerCase();
  return k ? list.filter((f) => `${f.label} ${f.name}`.toLocaleLowerCase().includes(k)) : [...list];
}

/** The memory channel `dir` steps from the station tuned (`uuid`), round the ends; the first or
 *  last when nothing of `list` is tuned. `null` for an empty list. */
export function stepChannel<T extends { uuid: string }>(list: readonly T[], uuid: string | null, dir: number): T | null {
  if (!list.length || dir === 0) return null;
  const i = uuid === null ? -1 : list.findIndex((x) => x.uuid === uuid);
  if (i < 0) return dir > 0 ? list[0] : list[list.length - 1];
  const n = list.length;
  return list[(((i + dir) % n) + n) % n];
}

/** What the faceplate says about the stream: reaching it, filling its buffer (how far, of what it
 *  fills to before playing), stopped, or nothing to say (playing, or tuned and not listened to).
 *  Listening while the server still says tuned is the moment before it starts: connecting. */
export type NetPhase =
  | { kind: 'tuned' | 'playing' | 'failed' | 'connecting' }
  | { kind: 'buffering'; pct: number };
export function netPhase(n: NetNow, listening: boolean): NetPhase {
  const st = n.state.state;
  if (st === 'failed' || st === 'playing') return { kind: st };
  if (st === 'tuned') return { kind: listening ? 'connecting' : 'tuned' };
  return n.buffer_ms <= 0 ? { kind: 'connecting' } : { kind: 'buffering', pct: bufferPct(n) };
}

/** How full the buffer is of what it fills to before playing, 1..99 % while it fills. */
export function bufferPct(n: Pick<NetNow, 'buffer_ms' | 'buffer_target_ms'>): number {
  return Math.min(99, Math.max(1, Math.round((n.buffer_ms / Math.max(1, n.buffer_target_ms)) * 100)));
}

/** Countries with internet stations as globe dots (ids apart from the transmitter sites'); a
 *  tower's height follows the count, roughly by its order of magnitude. */
export const NET_DOT_ID = 1_000_000;
export function netSiteDots(dots: readonly NetDot[], names: (itu: string) => string): SiteDot[] {
  return dots.map((d, i) => ({
    id: NET_DOT_ID + i,
    lat: d.lat,
    lon: d.lon,
    name: names(d.itu),
    itu: d.itu,
    flag: d.flag,
    precise: false,
    on: Math.min(15, Math.ceil(Math.log10(d.stations + 1) * 4)),
    total: d.stations,
    mine: 0,
  }));
}

/** The transmitter sites shown dim while an internet station is tuned (flat, as off the air). */
export function dimSites(sites: readonly SiteDot[]): SiteDot[] {
  return sites.map((s) => (s.on ? { ...s, on: 0 } : s));
}

/** `MP3 128k`, `AAC+`, `HLS`, or '' when the directory doesn't say. */
export function streamLabel(s: Pick<NetStation, 'codec' | 'bitrate' | 'hls'>): string {
  const parts = [s.codec ?? (s.hls ? 'HLS' : ''), s.bitrate ? `${s.bitrate}k` : ''].filter(Boolean);
  return parts.join(' ');
}

/** The audio cell: the player while there is something to hear (the rig's audio, or internet radio,
 *  which plays even with the rig's audio off), else the invitation to set the rig's audio up. */
export function audioCell(rigAudio: boolean, netEnabled: boolean): 'panel' | 'off' {
  return rigAudio || netEnabled ? 'panel' : 'off';
}

/** Something to listen to now: the rig's audio, or an internet station chosen. */
export function canListen(rigAudio: boolean, net: NetNow | null): boolean {
  return rigAudio || net !== null;
}

/** The uuid of the station playing, for the list to mark. */
export function playingUuid(net: NetNow | null): string | null {
  return net && net.state.state !== 'failed' ? net.station.uuid : null;
}

/** The country filter's choices on NET, by name in the UI language. */
export function netCountryOpts(list: NetCountry[], locale: string): { value: string; label: string }[] {
  const es = locale === 'es';
  return list
    .map((c) => ({ value: c.itu, label: es ? c.es : c.en }))
    .sort((a, b) => a.label.localeCompare(b.label, locale));
}
