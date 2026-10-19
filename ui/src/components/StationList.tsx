// The station list: four lenses (the whole schedule, what the scans heard, the favorites, what
// the listening history has), search, filters with counts (the listener's own stations are a source to filter by), a
// transmitter picked on the globe or a scan to go through, and
// pages. With internet radio on, the band filter also offers NET: internet stations from
// radio-browser.info, searched by text and country; a click tunes the receiver to one (Listen, in
// the audio panel, plays it). With it on, a search of words on All also shows the first internet
// stations it finds, above the radio ones, with a link to the rest on the NET band.
// Signals and favorites the schedule has no row for come first; in Heard they share one
// list with the logged stations, sorted as asked, each with its best reading and when it was
// last heard (nothing about the schedule there). Favorites is every favorite whatever the band
// and the filters (only the text narrows it): the radio ones by frequency, then the NET ones.
// Each row shows its kind, radio or internet. It fills its panel: the controls are fixed,
// the rows scroll in their own box and load the next page when you reach the end.

import { memo, useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type RefObject } from 'react';
import { useNames } from '../catalog/names';
import type { Filters } from '../hooks/useCore';
import type { NetSearch } from '../hooks/useNetSearch';
import { readBool, usePersisted } from '../hooks/usePersisted';
import { fmtStamp, sLabel, useT } from '../i18n';
import { queryHz } from '../keys/keymap';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { Facets } from '../types/generated/Facets';
import type { Favorite } from '../types/generated/Favorite';
import type { FavoriteRow } from '../types/generated/FavoriteRow';
import type { HeardRow } from '../types/generated/HeardRow';
import type { Lens } from '../types/generated/Lens';
import type { Logged } from '../types/generated/Logged';
import type { NetCountry } from '../types/generated/NetCountry';
import type { NetFavorite } from '../types/generated/NetFavorite';
import type { NetStation } from '../types/generated/NetStation';
import type { OffSchedule } from '../types/generated/OffSchedule';
import type { Region } from '../types/generated/Region';
import type { Sort } from '../types/generated/Sort';
import type { StationRow } from '../types/generated/StationRow';
import { MineBadge } from './Dossier';
import { Dropdown, type DropOpt } from './Dropdown';
import { favNote, favTuning } from './favorites';
import { Flag } from './Flag';
import { favFilter, lensFilters, NET_BAND, NET_MIX, netCountryOpts, streamLabel } from './net';
import { userSources } from './sources';

const REGIONS: Region[] = ['am', 'eu', 'af', 'me', 'as', 'oc', 'other'];
const LENSES: Lens[] = ['all', 'heard', 'favorites', 'listened'];
const SORTS: Sort[] = ['freq', 'strongest', 'recent'];
/** Languages and countries offered in the filters: the most common first, then by name. */
const TOP = 12;

interface Props {
  on: boolean;
  meta: CatalogMeta | null;
  filters: Filters;
  onFilters: (f: Filters) => void;
  rows: StationRow[];
  total: number;
  facets: Facets | null;
  loading: boolean;
  siteName: string | null;
  /** The scan filter's name (band and when). */
  scanName: string | null;
  selId: number | null;
  /** Stations a band scan logged, by id. */
  logged: ReadonlyMap<number, Logged>;
  /** Heard rows last heard since then (unix s) light up: the scan just run. */
  freshSince?: number | null;
  /** Favorites by station id. */
  favs: ReadonlyMap<number, Favorite>;
  /** Rows the schedule has none for. */
  off: OffSchedule[];
  /** The Heard lens, instead of `rows` and `off`. */
  heard: HeardRow[];
  /** The Favorites lens (the radio ones), instead of `rows` and `off`. */
  favorites: FavoriteRow[];
  /** Under a scan filter: what it read on the rows it heard. */
  scanDb: Record<number, number>;
  onPick: (r: StationRow) => void;
  onPickOff: (o: OffSchedule) => void;
  /** A double-click (or Shift+Enter) on a row: tune it and listen. */
  onPlay: (r: StationRow) => void;
  onPlayOff: (o: OffSchedule) => void;
  /** Add a station of one's own where a row off the schedule is. */
  onAddOff: (o: OffSchedule) => void;
  /** Shift+Enter on a kHz query: tune there exactly. */
  onTuneHz: (hz: number) => void;
  onMore: () => void;
  searchRef: RefObject<HTMLInputElement | null>;
  /** The NET band: offered while internet radio is on; `active` = the list shows it; `mixed`: a
   *  radio search also shows the internet stations it finds (`search`); `tuned`: the
   *  uuid of the station the receiver is tuned to; `favs`: the NET favorites (its Favorites lens). */
  net: {
    enabled: boolean;
    active: boolean;
    mixed: boolean;
    search: NetSearch;
    countries: NetCountry[];
    tuned: string | null;
    favs: readonly NetFavorite[];
    favUuids: ReadonlySet<string>;
    onTune: (uuid: string) => void;
    /** Tune it and listen. */
    onPlay: (uuid: string) => void;
  };
}

/** A labelled filter dropdown; options that would give no rows are greyed out. */
function Filter({ label, all, value, opts, onChange, className }: { label: string; all: string; value: string; opts: DropOpt[]; onChange: (v: string) => void; className?: string }) {
  const id = useId();
  return (
    <div className="flt">
      <span className="flt-lab" id={id}>{label}</span>
      <Dropdown labelledBy={id} all={all} lit={value !== ''} className={className} value={value} opts={opts.map((o) => ({ ...o, disabled: o.n === 0 }))} onChange={onChange} />
    </div>
  );
}

/** Most rows first for the top few, then the rest by name; selected values always included. */
function ranked(opts: DropOpt[]): DropOpt[] {
  const top = [...opts].sort((a, b) => (b.n ?? 0) - (a.n ?? 0)).slice(0, TOP);
  const rest = opts.filter((o) => !top.includes(o)).sort((a, b) => a.label.localeCompare(b.label));
  return [...top, ...rest];
}

interface ItemProps {
  r: StationRow;
  on: boolean;
  /** The station shown (and tuned). */
  current: boolean;
  /** What the scans logged of it. */
  lg: Logged | undefined;
  fav: boolean;
  /** Under a scan filter: what that scan read here, `undefined` if it missed it. */
  scanned: boolean;
  db: number | undefined;
  freshSince: number | null | undefined;
  /** In the Heard list, with what the scans read. */
  log?: Logged;
  /** One of the listener's own stations. */
  mine: boolean;
  /** In Favorites: what the favorite adds to the row (its label, its mode). */
  note?: string;
  onPick: (r: StationRow) => void;
  onPlay: (r: StationRow) => void;
}

/** A row's keys besides its click: Shift+Enter plays it (Enter alone tunes, the button's own). */
const playKey = (play: () => void) => (e: KeyboardEvent<HTMLButtonElement>) => {
  if (e.key !== 'Enter' || !e.shiftKey || e.altKey || e.ctrlKey || e.metaKey) return;
  e.preventDefault();
  play();
};

/** What kind of row: a station on the dial, or an internet one. */
function RowKind({ kind }: { kind: 'radio' | 'net' }) {
  const t = useT();
  const label = kind === 'net' ? t.kindNet : t.kindRadio;
  return (
    <svg className="kind-ic" viewBox="0 0 12 12" role="img" aria-label={label}>
      <title>{label}</title>
      {kind === 'net' ? (
        <>
          <circle cx="6" cy="6" r="4.6" />
          <ellipse cx="6" cy="6" rx="2" ry="4.6" />
          <path d="M1.4 6h9.2" />
        </>
      ) : (
        <>
          <path d="M6 5.2 3.6 11M6 5.2 8.4 11M4.4 9h3.2" />
          <circle cx="6" cy="4" r="0.9" />
          <path d="M3.6 1.8a3.2 3.2 0 0 0 0 4.4M8.4 1.8a3.2 3.2 0 0 1 0 4.4" />
        </>
      )}
    </svg>
  );
}

/** A catalog row. Memoized, with props of its own: the list re-renders with the app, and a long
 *  one would otherwise render every row each time. */
const StationItem = memo(function StationItem({ r, on, current, lg, fav, scanned, db, freshSince, log, mine, note, onPick, onPlay }: ItemProps) {
  const t = useT();
  const names = useNames();
  const country = names.country(r.itu);
  const abroad = r.site_itu !== r.itu;
  const missed = scanned && db === undefined;
  // heard by the scan just run; `first` too: never logged before it
  const fresh = log && freshSince != null && log.last >= freshSince;
  const first = fresh && log.first >= freshSince!;
  return (
    <li>
      <button className={'row' + (log ? ' heard-row' : '') + (fresh ? ' fresh' : '') + (first ? ' first' : '') + (missed ? ' missed' : '')} type="button" aria-current={on && current} disabled={!on} title={missed ? t.scanMissedRow : t.rowPlayHint} onClick={() => onPick(r)} onDoubleClick={() => onPlay(r)} onKeyDown={playKey(() => onPlay(r))}>
        <span className="rf">{r.freq_hz / 1000}</span>
        <span className="rn">
          <b>
            <RowKind kind="radio" />
            <Flag flag={r.flag} title={country} />{r.name}
            {mine && <MineBadge />}
            {lg && <span className="logged" title={t.loggedBadge(lg.count)} aria-label={t.loggedBadge(lg.count)}>✓</span>}
            {fav && <span className="fav" title={t.favBadge} aria-label={t.favBadge}>★</span>}
            {db !== undefined && <span className="sv" title={t.scanHeardAt(sLabel(db))}>{sLabel(db)}</span>}
            {first && (
              <svg className="new-ring" viewBox="0 0 12 12" role="img" aria-label={t.heardNew}>
                <title>{t.heardNewTitle}</title>
                <circle cx="6" cy="6" r="4.5" />
              </svg>
            )}
            {log && !log.sure && <span className="maybe" title={t.heardPossibleTitle}>{t.heardPossible}</span>}
          </b>
          <em>
            {note && <span className="fav-note">{note} · </span>}
            {abroad && <>{t.via} <Flag flag={r.site_flag} title={names.country(r.site_itu)} /></>}
            {r.site_name}
            {/* the slot's language and target are the schedule's now, not what was heard */}
            {log ? ` · ${t.heardLine(log.count, fmtStamp(t, log.last))}` : <>
              {r.lang && ` · ${names.lang(r.lang)}`}
              {r.target && ` → ${names.target(r.target)}`}
            </>}
          </em>
        </span>
        {log ? <span className="sval">{sLabel(log.best_db)}</span> : <span className={'dot' + (r.on ? ' on' : '')} />}
      </button>
    </li>
  );
});

/** A row the schedule has none for: a signal, a favorite, or both; "+" adds a station there. */
function OffItem({ o, p }: { o: OffSchedule; p: Props }) {
  const t = useT();
  return (
    <li className="off-li">
      <button className="row off-row" type="button" disabled={!p.on} title={t.rowPlayHint} onClick={() => p.onPickOff(o)} onDoubleClick={() => p.onPlayOff(o)} onKeyDown={playKey(() => p.onPlayOff(o))}>
        <span className="rf">{o.freq_hz / 1000}</span>
        <span className="rn">
          <b className={o.label || o.station ? undefined : 'v-unidentified'}>
            <RowKind kind="radio" />
            {o.label || o.station?.name || t.unidentifiedRow}
            {o.favorite && <span className="fav" title={t.favBadge} aria-label={t.favBadge}>★</span>}
          </b>
          <em>
            {[
              o.mode,
              o.station && `${o.station.name} · ${t.favGone}`,
              o.heard && t.heardLine(o.heard.count, fmtStamp(t, o.heard.last)),
            ].filter(Boolean).join(' · ')}
          </em>
        </span>
        <span className="sval">{o.heard ? sLabel(o.heard.best_db) : ''}</span>
      </button>
      <button className="d-act row-add" type="button" title={t.addStation} aria-label={t.addStation} onClick={() => p.onAddOff(o)}>+</button>
    </li>
  );
}

const offKey = (o: OffSchedule) => `${o.freq_hz}-${o.favorite ?? ''}`;

/** An internet station: NET where the kHz would be; a click tunes the receiver to it (it tunes
 *  nothing on the rig, so it works with the rig off too), a double-click plays it too. */
function NetItem({ name, flag, country, line, stream, tuned, fav, onTune, onPlay }: {
  name: string; flag: string | null; country: string; line: string; stream: string; tuned: boolean; fav: boolean; onTune: () => void; onPlay: () => void;
}) {
  const t = useT();
  return (
    <li>
      <button className="row net-row" type="button" aria-current={tuned} title={t.rowPlayHint} onClick={onTune} onDoubleClick={onPlay} onKeyDown={playKey(onPlay)}>
        <span className="rf net-rf" title={t.netBadgeTitle}>{t.netBadge}</span>
        <span className="rn">
          <b>
            <RowKind kind="net" />
            <Flag flag={flag} title={country} />{name}
            {fav && <span className="fav" title={t.favBadge} aria-label={t.favBadge}>★</span>}
            {tuned && <span className="net-on" title={t.netTunedNow} aria-label={t.netTunedNow}>♪</span>}
          </b>
          <em>{line}</em>
        </span>
        <span className="sval">{stream}</span>
        {/* an internet station is always on the air */}
        <span className="dot net" />
      </button>
    </li>
  );
}

const stationLine = (s: NetStation, country: string) =>
  [country, s.langs.join(', '), s.tags.slice(0, 3).join(', ')].filter(Boolean).join(' · ');

/** How long a NET wait goes before the list says the directory is slow. */
const SLOW_MS = 4000;
/** The skeleton's name and line widths, row by row (a list of equal bars reads as a table). */
const SKEL_W = [[62, 44], [48, 70], [74, 38], [55, 60], [68, 46], [42, 64], [58, 52], [50, 40]];

/** True once `on` has held for `ms`. */
function useAfter(on: boolean, ms: number): boolean {
  const [late, setLate] = useState(false);
  useEffect(() => {
    setLate(false);
    if (!on) return;
    const id = setTimeout(() => setLate(true), ms);
    return () => clearTimeout(id);
  }, [on, ms]);
  return late;
}

/** Rows to come, shaped like the real ones, while a list's first page loads. */
function SkeletonRows({ net }: { net: boolean }) {
  return (
    <ul className={'list skel' + (net ? ' net' : '')} aria-hidden="true">
      {SKEL_W.map(([a, b], i) => (
        <li key={i}>
          <span className="row skel-row">
            <i className="skel-b skel-f" />
            <span className="rn">
              <i className="skel-b" style={{ width: a + '%' }} />
              <i className="skel-b skel-sub" style={{ width: b + '%' }} />
            </span>
            <i className="skel-dot" />
          </span>
        </li>
      ))}
    </ul>
  );
}

export function StationList(p: Props) {
  const t = useT();
  const names = useNames();
  const f = p.filters;
  const set = (patch: Partial<Filters>) => p.onFilters({ ...f, ...patch });
  const count = (m: Record<string, number> | undefined, k: string) => m?.[k] ?? 0;

  const net = p.net.active;
  const bands: DropOpt[] = [
    ...(p.meta?.bands ?? []).map((b) => ({ value: b.id, label: names.band(b.id), hint: t.bandRange(b.lo_khz, b.hi_khz), n: count(p.facets?.bands, b.id) })),
    { value: 'oob', label: names.band('oob'), n: count(p.facets?.bands, 'oob') },
    ...(p.net.enabled ? [{ value: NET_BAND, label: t.netBand, hint: t.netBandHint }] : []),
  ];
  const langs = useMemo(
    () => ranked((p.meta?.langs ?? []).map((l) => ({ value: l.code, label: names.lang(l.code), n: p.facets?.langs[l.code] ?? 0 }))),
    [p.meta, p.facets, names],
  );
  const regions: DropOpt[] = REGIONS.map((r) => ({ value: r, label: names.region(r), n: count(p.facets?.regions, r) }));
  const netCountries = useMemo(() => netCountryOpts(p.net.countries, t.locale), [p.net.countries, t.locale]);
  const netCountry = useMemo(() => new Map(netCountries.map((c) => [c.value, c.label])), [netCountries]);
  const schedCountries = useMemo(
    () => ranked((p.meta?.countries ?? []).map((c) => ({ value: c.itu, label: names.country(c.itu), n: p.facets?.countries[c.itu] ?? 0 }))),
    [p.meta, p.facets, names],
  );
  const countries = net ? netCountries : schedCountries;
  // Favorites: every favorite, on any band; the NET ones from their store (no directory query)
  const favView = f.lens === 'favorites';
  const netFavRows = useMemo(
    () => (favView && p.net.enabled ? favFilter(p.net.favs, f.q) : []),
    [favView, p.net.enabled, p.net.favs, f.q],
  );
  // a station favorite tunes in its own mode (a new row only when that differs)
  const favTunings = useMemo(
    () => new Map(p.favorites.flatMap((r) => (r.kind === 'station' ? [[r.favorite.id, favTuning(r)] as const] : []))),
    [p.favorites],
  );
  const countryOf = (itu: string | null) => (itu ? netCountry.get(itu) ?? names.country(itu) : '');
  const sources: DropOpt[] = (p.meta?.sources ?? []).map((s) => ({ value: s.id, label: s.kind === 'user' ? t.sourceMine : s.name, n: count(p.facets?.sources, s.id) }));
  const mineSrc = useMemo(() => userSources(p.meta), [p.meta]);
  // the filter drawer, open or shut as this device left it
  const [showFilters, setShowFilters] = usePersisted('swatlas.filtersOpen', false, readBool);
  const picked = net ? 1 + f.countries.length : f.bands.length + f.langs.length + f.regions.length + f.countries.length + f.sources.length;
  // a radio search of words also shows the first internet stations it finds
  const netMix = p.net.mixed ? p.net.search.items.slice(0, NET_MIX) : [];
  const netMixMore = p.net.search.items.length > NET_MIX || p.net.search.more;
  const netStation = (s: NetStation) => (
    <NetItem
      key={s.uuid}
      name={s.name}
      flag={s.flag}
      country={countryOf(s.itu)}
      line={stationLine(s, countryOf(s.itu))}
      stream={streamLabel(s)}
      tuned={p.net.tuned === s.uuid}
      fav={p.net.favUuids.has(s.uuid)}
      onTune={() => p.net.onTune(s.uuid)}
      onPlay={() => p.net.onPlay(s.uuid)}
    />
  );
  // Favorites only uses the text: the other filters stay set, hidden, for the other lenses
  const filtered = favView ? f.q !== '' : net || picked > 0 || f.site !== null || f.scan !== null || f.q !== '';
  const heardList = !net && f.lens === 'heard' && f.scan === null;
  const sortLabel = { freq: t.sortFreq, strongest: t.sortStrongest, recent: t.sortRecent };
  const sortId = useId();
  const lensLabel = { all: t.lensAll, heard: t.lensHeard, favorites: t.lensFav, listened: t.lensListened };
  const lensTitle = { all: t.lensAllTitle, heard: t.lensHeardTitle, favorites: t.lensFavTitle, listened: t.lensListenedTitle };
  const empty = favView ? (f.q ? t.noResults : t.favEmpty)
    : net && p.net.search.failed ? t.netUnreachable
    : filtered ? t.noResults
    : f.lens === 'heard' ? t.heardEmpty
    : f.lens === 'favorites' ? t.favEmpty
    : f.lens === 'listened' ? t.listenedEmpty
    : t.noResults;

  const box = useRef<HTMLDivElement>(null), end = useRef<HTMLDivElement>(null);
  const more = useRef(p.onMore);
  more.current = net && !favView ? p.net.search.loadMore : p.onMore;
  const asked = useRef(-1);
  const loaded = favView ? p.favorites.length : net ? p.net.search.items.length : p.rows.length + p.heard.length;
  const hasMore = favView ? loaded < p.total : net ? p.net.search.more : loaded < p.total;
  // loading: a bar over the list, skeleton rows while it is empty, the rows shown dimmed while
  // they are replaced; NET also says when the directory is slow
  const busy = net && !favView ? p.net.search.loading : p.loading;
  const shownCount = favView ? loaded + netFavRows.length : net ? loaded : loaded + p.off.length + netMix.length;
  const netWaiting = net && !favView && (p.net.search.loading || p.net.search.loadingMore);
  const slow = useAfter(netWaiting, SLOW_MS);
  // a radio search of words: the internet stations it will show are on their way
  const netMixWaiting = p.net.mixed && p.net.search.loading && netMix.length === 0;
  // next page when the end of the list comes into view (once per length)
  useEffect(() => {
    const b = box.current, e = end.current;
    if (!b || !e || !hasMore || typeof IntersectionObserver === 'undefined') return;
    const n = loaded;
    const io = new IntersectionObserver(([en]) => {
      if (en.isIntersecting && asked.current !== n) {
        asked.current = n;
        more.current();
      }
    }, { root: b, rootMargin: '0px 0px 240px 0px' });
    io.observe(e);
    return () => io.disconnect();
  }, [loaded, hasMore]);
  // a scan running, and its rows just arrived: kept at the top, where Most recent puts them
  useEffect(() => {
    if (heardList && p.freshSince != null && box.current) box.current.scrollTop = 0;
  }, [heardList, p.freshSince, p.heard]);
  // new filters: back to the top
  const key = JSON.stringify(f);
  useEffect(() => { if (box.current) box.current.scrollTop = 0; }, [key]);
  // a station chosen elsewhere (dial, globe): show its row
  useEffect(() => {
    const b = box.current, row = b?.querySelector('[aria-current="true"]');
    if (!b || !row) return;
    const br = b.getBoundingClientRect(), rr = row.getBoundingClientRect();
    if (rr.top < br.top || rr.bottom > br.bottom) b.scrollTop += rr.top - br.top - b.clientHeight / 3;
  }, [p.selId]);

  // the rows' click handler stays the same function across renders (StationItem is memoized)
  const pickNow = useRef(p.onPick);
  pickNow.current = p.onPick;
  const onPick = useCallback((r: StationRow) => pickNow.current(r), []);
  const playNow = useRef(p.onPlay);
  playNow.current = p.onPlay;
  const onPlay = useCallback((r: StationRow) => playNow.current(r), []);
  const item = (r: StationRow, log?: Logged, fav?: Favorite) => {
    // Favorites ignores the scan filter
    const scanned = f.scan !== null && !fav;
    return (
      <StationItem
        key={fav ? fav.id : log ? `s${r.id}` : r.id}
        r={fav ? favTunings.get(fav.id) ?? r : r}
        on={p.on}
        current={p.selId === r.id}
        lg={p.logged.get(r.id)}
        fav={p.favs.has(r.id)}
        scanned={scanned}
        db={scanned ? p.scanDb[r.id] : undefined}
        freshSince={p.freshSince}
        log={log}
        mine={mineSrc.has(r.source)}
        note={fav ? favNote(fav, r) : undefined}
        onPick={onPick}
        onPlay={onPlay}
      />
    );
  };

  const rowButtons = () => [...(box.current?.querySelectorAll<HTMLButtonElement>('.row:not(:disabled)') ?? [])];
  const onSearchKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === 'ArrowDown') rowButtons()[0]?.focus();
    else if (e.key === 'Enter' && (net || favView)) rowButtons()[0]?.focus();
    else if (e.key === 'Enter') {
      const hz = e.shiftKey ? queryHz(f.q) : null;
      if (!p.on) return;
      const h = p.heard[0];
      if (hz !== null) p.onTuneHz(hz);
      else if (h) {
        if (h.kind === 'station') p.onPick(h.row);
        else p.onPickOff(h.row);
      } else if (p.rows[0]) p.onPick(p.rows[0]);
    } else if (e.key === 'Escape') {
      if (f.q) set({ q: '' });
      else e.currentTarget.blur();
    } else return;
    e.preventDefault();
  };
  // rows: arrows move the focus, Enter/Space tune (the button's own click), Esc back to search
  const onRowsKey = (e: KeyboardEvent<HTMLElement>) => {
    const rows = rowButtons(), i = rows.indexOf(document.activeElement as HTMLButtonElement);
    if (i < 0 || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    const to = e.key === 'ArrowDown' ? Math.min(i + 1, rows.length - 1)
      : e.key === 'ArrowUp' ? i - 1
      : e.key === 'Home' ? 0
      : e.key === 'End' ? rows.length - 1
      : e.key === 'Escape' ? -1
      : null;
    if (to === null) return;
    if (to < 0) p.searchRef.current?.focus();
    else rows[to].focus();
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <>
      <div className="tabs lenses" role="tablist" aria-label={t.lenses}>
        {LENSES.map((l) => (
          <button key={l} type="button" role="tab" aria-selected={favView ? l === 'favorites' : net ? l === 'all' : f.lens === l} title={lensTitle[l]} onClick={() => p.onFilters(lensFilters(f, l))}>
            {l === 'favorites' && <span aria-hidden="true">★</span>}{lensLabel[l]}
          </button>
        ))}
      </div>
      <div className="finder">
        <input
          ref={p.searchRef}
          type="search"
          aria-label={t.search}
          placeholder={t.searchPlaceholder}
          title={t.searchKeys}
          value={f.q}
          onChange={(e) => set({ q: e.currentTarget.value })}
          onKeyDown={onSearchKey}
        />
        {heardList && (
          <div className="sort">
            <span className="flt-lab" id={sortId}>{t.sort}</span>
            <Dropdown labelledBy={sortId} lit={f.sort !== 'freq'} value={f.sort} opts={SORTS.map((v) => ({ value: v, label: sortLabel[v] }))} onChange={(v) => set({ sort: v as Sort })} />
          </div>
        )}
        {/* phones only: the selects fold away to leave room for the rows */}
        {!favView && (
          <button className="btn flt-toggle" type="button" aria-expanded={showFilters} onClick={() => setShowFilters((o) => !o)}>
            {t.filters}{picked > 0 && ` · ${picked}`}
          </button>
        )}
      </div>
      {favView && picked + (f.site !== null ? 1 : 0) + (f.scan !== null ? 1 : 0) > 0 && <p className="note fav-nofilters">{t.favNoFilters}</p>}
      {!favView && <div className={'filters' + (showFilters ? ' open' : '')} role="group" aria-label={t.filters}>
        <Filter label={t.band} all={t.allBands} value={f.bands[0] ?? ''} opts={bands} className={net ? 'net' : undefined} onChange={(v) => set({ bands: v ? [v] : [] })} />
        {/* on NET only the text and the country apply; the others stay set for the radio bands */}
        {!net && <Filter label={t.language} all={t.allLanguages} value={f.langs[0] ?? ''} opts={langs} onChange={(v) => set({ langs: v ? [v] : [] })} />}
        {!net && <Filter label={t.region} all={t.allRegions} value={f.regions[0] ?? ''} opts={regions} onChange={(v) => set({ regions: v ? [v as Region] : [] })} />}
        <Filter label={t.country} all={t.allCountries} value={f.countries[0] ?? ''} opts={countries} onChange={(v) => set({ countries: v ? [v] : [] })} />
        {!net && <Filter label={t.source} all={t.allSources} value={f.sources[0] ?? ''} opts={sources} onChange={(v) => set({ sources: v ? [v] : [] })} />}
      </div>}
      {filtered && (
        <div className="active-filters">
          {!net && !favView && f.scan !== null && (
            <button className="chip scan-filter" type="button" title={t.clearFilter} onClick={() => set({ scan: null })}>
              {t.scanFilter(p.scanName ?? '')} <span aria-hidden="true">✕</span>
            </button>
          )}
          {!net && !favView && f.site !== null && (
            <button className="chip" type="button" title={t.clearFilter} onClick={() => set({ site: null })}>
              {t.siteFilter(p.siteName ?? '')} <span aria-hidden="true">✕</span>
            </button>
          )}
          <button className="link" type="button" onClick={() => p.onFilters(favView ? { ...f, q: '' } : { ...f, q: '', bands: [], langs: [], regions: [], countries: [], sources: [], site: null, scan: null })}>{t.clearFilters}</button>
        </div>
      )}
      <div className="list-bar" data-on={busy} data-net={net && !favView ? '' : undefined} aria-hidden="true" />
      {slow && <p className="note net-slow" role="status">{t.netSlow}</p>}
      <div className={'rows' + (busy && shownCount > 0 ? ' stale' : '')} ref={box} onKeyDown={onRowsKey} aria-busy={busy}>
        {busy && shownCount === 0 && <SkeletonRows net={net && !favView} />}
        {favView ? (
          <>
            <ul className="list">
              {p.favorites.map((r) => r.kind === 'station'
                ? item(r.row, undefined, r.favorite)
                : <OffItem key={offKey(r.row)} o={r.row} p={p} />)}
            </ul>
            {netFavRows.length > 0 && (
              <>
                <h4 className="h list-h"><span>{t.netHead}</span><span>{netFavRows.length}</span></h4>
                <ul className="list">
                  {netFavRows.map((v) => (
                    <NetItem
                      key={v.uuid}
                      name={v.label || v.name}
                      flag={v.flag}
                      country={countryOf(v.itu)}
                      line={[countryOf(v.itu), v.label ? v.name : ''].filter(Boolean).join(' · ')}
                      stream={v.codec ?? ''}
                      tuned={p.net.tuned === v.uuid}
                      fav
                      onTune={() => p.net.onTune(v.uuid)}
                      onPlay={() => p.net.onPlay(v.uuid)}
                    />
                  ))}
                </ul>
              </>
            )}
          </>
        ) : net ? (
          <ul className="list">
            {p.net.search.items.map(netStation)}
          </ul>
        ) : heardList ? (
          <ul className="list">
            {p.heard.map((h) => h.kind === 'station'
              ? item(h.row, h.log)
              : <OffItem key={`o${offKey(h.row)}`} o={h.row} p={p} />)}
          </ul>
        ) : (
          <>
            {/* a search of words: the internet stations it finds first, a few of them */}
            {netMixWaiting && (
              <>
                <h4 className="h list-h"><span>{t.netHead}</span></h4>
                <p className="net-searching" role="status">{t.netSearching}</p>
              </>
            )}
            {netMix.length > 0 && (
              <>
                <h4 className="h list-h"><span>{t.netHead}</span><span>{netMix.length}{netMixMore && '+'}</span></h4>
                <ul className="list">
                  {netMix.map(netStation)}
                </ul>
                {netMixMore && (
                  <button className="link net-all" type="button" onClick={() => p.onFilters({ ...f, bands: [NET_BAND] })}>{t.netSeeAll}</button>
                )}
                {p.off.length === 0 && p.rows.length > 0 && <h4 className="h list-h"><span>{t.inSchedule}</span><span>{p.total}</span></h4>}
              </>
            )}
            {p.off.length > 0 && (
              <>
                <h4 className="h list-h"><span>{t.offSchedule}</span><span>{p.off.length}</span></h4>
                <ul className="list off-list">
                  {p.off.map((o) => <OffItem key={offKey(o)} o={o} p={p} />)}
                </ul>
                {p.rows.length > 0 && <h4 className="h list-h"><span>{t.inSchedule}</span><span>{p.total}</span></h4>}
              </>
            )}
            <ul className="list">
              {p.rows.map((r) => item(r))}
            </ul>
          </>
        )}
        {favView
          ? !p.loading && p.total === 0 && netFavRows.length === 0 && <p className="empty-d">{empty}</p>
          : net
          ? !p.net.search.loading && p.net.search.items.length === 0 && (
            <>
              <p className="empty-d">{empty}</p>
              {p.net.search.failed && <button className="btn net-retry" type="button" onClick={p.net.search.retry}>⟲ {t.retry}</button>}
            </>
          )
          : !p.loading && p.total === 0 && p.off.length === 0 && netMix.length === 0 && !netMixWaiting && <p className="empty-d">{empty}</p>}
        <div ref={end} />
        {hasMore && (net && !favView
          ? (
            <button className={'btn more net' + (p.net.search.loadingMore ? ' loading' : '')} type="button" disabled={p.net.search.loading || p.net.search.loadingMore} onClick={p.net.search.loadMore}>
              {p.net.search.loadingMore ? t.netLoadingMore : t.netMore}
            </button>
          )
          : <button className="btn more" type="button" onClick={p.onMore}>{t.showMore(Math.min(100, p.total - loaded))}</button>
        )}
        {(net || netFavRows.length > 0 || netMix.length > 0) && <p className="note net-foot">{t.netFoot}</p>}
      </div>
    </>
  );
}
