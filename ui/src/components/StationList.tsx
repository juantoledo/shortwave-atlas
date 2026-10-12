// The station list: four lenses (the whole schedule, what the scans heard, the favorites, what
// the listening history has), search, filters with counts (the listener's own stations are a source to filter by), a
// transmitter picked on the globe or a scan to go through, and
// pages. Signals and favorites the schedule has no row for come first; in Heard they share one
// list with the logged stations, sorted as asked, each with its best reading and when it was
// last heard (nothing about the schedule there). It fills its panel: the controls are fixed,
// the rows scroll in their own box and load the next page when you reach the end.

import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type RefObject } from 'react';
import { useNames } from '../catalog/names';
import type { Filters } from '../hooks/useCore';
import { fmtStamp, sLabel, useT } from '../i18n';
import { queryHz } from '../keys/keymap';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { Facets } from '../types/generated/Facets';
import type { Favorite } from '../types/generated/Favorite';
import type { HeardRow } from '../types/generated/HeardRow';
import type { Lens } from '../types/generated/Lens';
import type { Logged } from '../types/generated/Logged';
import type { OffSchedule } from '../types/generated/OffSchedule';
import type { Region } from '../types/generated/Region';
import type { Sort } from '../types/generated/Sort';
import type { StationRow } from '../types/generated/StationRow';
import { MineBadge } from './Dossier';
import { Dropdown, type DropOpt } from './Dropdown';
import { Flag } from './Flag';
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
  /** Under a scan filter: what it read on the rows it heard. */
  scanDb: Record<number, number>;
  onPick: (r: StationRow) => void;
  onPickOff: (o: OffSchedule) => void;
  /** Add a station of one's own where a row off the schedule is. */
  onAddOff: (o: OffSchedule) => void;
  /** Shift+Enter on a kHz query: tune there exactly. */
  onTuneHz: (hz: number) => void;
  onMore: () => void;
  searchRef: RefObject<HTMLInputElement | null>;
}

/** A labelled filter dropdown; options that would give no rows are greyed out. */
function Filter({ label, all, value, opts, onChange }: { label: string; all: string; value: string; opts: DropOpt[]; onChange: (v: string) => void }) {
  const id = useId();
  return (
    <div className="flt">
      <span className="flt-lab" id={id}>{label}</span>
      <Dropdown labelledBy={id} all={all} lit={value !== ''} value={value} opts={opts.map((o) => ({ ...o, disabled: o.n === 0 }))} onChange={onChange} />
    </div>
  );
}

/** Most rows first for the top few, then the rest by name; selected values always included. */
function ranked(opts: DropOpt[]): DropOpt[] {
  const top = [...opts].sort((a, b) => (b.n ?? 0) - (a.n ?? 0)).slice(0, TOP);
  const rest = opts.filter((o) => !top.includes(o)).sort((a, b) => a.label.localeCompare(b.label));
  return [...top, ...rest];
}

/** A catalog row; `log` = in the Heard list, with what the scans read; `mine` = one of the
 *  listener's own stations. */
function StationItem({ r, p, log, mine }: { r: StationRow; p: Props; log?: Logged; mine: boolean }) {
  const t = useT();
  const names = useNames();
  const country = names.country(r.itu);
  const abroad = r.site_itu !== r.itu;
  const lg = p.logged.get(r.id);
  const fav = p.favs.has(r.id);
  const scan = p.filters.scan;
  const db = scan === null ? undefined : p.scanDb[r.id];
  const missed = scan !== null && db === undefined;
  // heard by the scan just run; `first` too: never logged before it
  const fresh = log && p.freshSince != null && log.last >= p.freshSince;
  const first = fresh && log.first >= p.freshSince!;
  return (
    <li>
      <button className={'row' + (log ? ' heard-row' : '') + (fresh ? ' fresh' : '') + (first ? ' first' : '') + (missed ? ' missed' : '')} type="button" aria-current={p.on && p.selId === r.id} disabled={!p.on} title={missed ? t.scanMissedRow : undefined} onClick={() => p.onPick(r)}>
        <span className="rf">{r.freq_hz / 1000}</span>
        <span className="rn">
          <b>
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
}

/** A row the schedule has none for: a signal, a favorite, or both; "+" adds a station there. */
function OffItem({ o, p }: { o: OffSchedule; p: Props }) {
  const t = useT();
  return (
    <li className="off-li">
      <button className="row off-row" type="button" disabled={!p.on} onClick={() => p.onPickOff(o)}>
        <span className="rf">{o.freq_hz / 1000}</span>
        <span className="rn">
          <b className={o.label || o.station ? undefined : 'v-unidentified'}>
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

export function StationList(p: Props) {
  const t = useT();
  const names = useNames();
  const f = p.filters;
  const set = (patch: Partial<Filters>) => p.onFilters({ ...f, ...patch });
  const count = (m: Record<string, number> | undefined, k: string) => m?.[k] ?? 0;

  const bands: DropOpt[] = [
    ...(p.meta?.bands ?? []).map((b) => ({ value: b.id, label: names.band(b.id), hint: t.bandRange(b.lo_khz, b.hi_khz), n: count(p.facets?.bands, b.id) })),
    { value: 'oob', label: names.band('oob'), n: count(p.facets?.bands, 'oob') },
  ];
  const langs = ranked((p.meta?.langs ?? []).map((l) => ({ value: l.code, label: names.lang(l.code), n: count(p.facets?.langs, l.code) })));
  const regions: DropOpt[] = REGIONS.map((r) => ({ value: r, label: names.region(r), n: count(p.facets?.regions, r) }));
  const countries = ranked((p.meta?.countries ?? []).map((c) => ({ value: c.itu, label: names.country(c.itu), n: count(p.facets?.countries, c.itu) })));
  const sources: DropOpt[] = (p.meta?.sources ?? []).map((s) => ({ value: s.id, label: s.kind === 'user' ? t.sourceMine : s.name, n: count(p.facets?.sources, s.id) }));
  const mineSrc = useMemo(() => userSources(p.meta), [p.meta]);
  const [showFilters, setShowFilters] = useState(false);
  const picked = f.bands.length + f.langs.length + f.regions.length + f.countries.length + f.sources.length;
  const filtered = picked > 0 || f.site !== null || f.scan !== null || f.q !== '';
  const heardList = f.lens === 'heard' && f.scan === null;
  const sortLabel = { freq: t.sortFreq, strongest: t.sortStrongest, recent: t.sortRecent };
  const sortId = useId();
  const lensLabel = { all: t.lensAll, heard: t.lensHeard, favorites: t.lensFav, listened: t.lensListened };
  const lensTitle = { all: t.lensAllTitle, heard: t.lensHeardTitle, favorites: t.lensFavTitle, listened: t.lensListenedTitle };
  const empty = filtered ? t.noResults
    : f.lens === 'heard' ? t.heardEmpty
    : f.lens === 'favorites' ? t.favEmpty
    : f.lens === 'listened' ? t.listenedEmpty
    : t.noResults;

  const box = useRef<HTMLDivElement>(null), end = useRef<HTMLDivElement>(null);
  const more = useRef(p.onMore);
  more.current = p.onMore;
  const asked = useRef(-1);
  const loaded = p.rows.length + p.heard.length;
  const hasMore = loaded < p.total;
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

  const rowButtons = () => [...(box.current?.querySelectorAll<HTMLButtonElement>('.row:not(:disabled)') ?? [])];
  const onSearchKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === 'ArrowDown') rowButtons()[0]?.focus();
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
          <button key={l} type="button" role="tab" aria-selected={f.lens === l} title={lensTitle[l]} onClick={() => set({ lens: l })}>
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
        <button className="btn flt-toggle" type="button" aria-expanded={showFilters} onClick={() => setShowFilters((o) => !o)}>
          {t.filters}{picked > 0 && ` · ${picked}`}
        </button>
      </div>
      <div className={'filters' + (showFilters ? ' open' : '')} role="group" aria-label={t.filters}>
        <Filter label={t.band} all={t.allBands} value={f.bands[0] ?? ''} opts={bands} onChange={(v) => set({ bands: v ? [v] : [] })} />
        <Filter label={t.language} all={t.allLanguages} value={f.langs[0] ?? ''} opts={langs} onChange={(v) => set({ langs: v ? [v] : [] })} />
        <Filter label={t.region} all={t.allRegions} value={f.regions[0] ?? ''} opts={regions} onChange={(v) => set({ regions: v ? [v as Region] : [] })} />
        <Filter label={t.country} all={t.allCountries} value={f.countries[0] ?? ''} opts={countries} onChange={(v) => set({ countries: v ? [v] : [] })} />
        <Filter label={t.source} all={t.allSources} value={f.sources[0] ?? ''} opts={sources} onChange={(v) => set({ sources: v ? [v] : [] })} />
      </div>
      {filtered && (
        <div className="active-filters">
          {f.scan !== null && (
            <button className="chip scan-filter" type="button" title={t.clearFilter} onClick={() => set({ scan: null })}>
              {t.scanFilter(p.scanName ?? '')} <span aria-hidden="true">✕</span>
            </button>
          )}
          {f.site !== null && (
            <button className="chip" type="button" title={t.clearFilter} onClick={() => set({ site: null })}>
              {t.siteFilter(p.siteName ?? '')} <span aria-hidden="true">✕</span>
            </button>
          )}
          <button className="link" type="button" onClick={() => p.onFilters({ ...f, q: '', bands: [], langs: [], regions: [], countries: [], sources: [], site: null, scan: null })}>{t.clearFilters}</button>
        </div>
      )}
      <div className="rows" ref={box} onKeyDown={onRowsKey}>
        {heardList ? (
          <ul className="list">
            {p.heard.map((h) => h.kind === 'station'
              ? <StationItem key={`s${h.row.id}`} r={h.row} p={p} log={h.log} mine={mineSrc.has(h.row.source)} />
              : <OffItem key={`o${offKey(h.row)}`} o={h.row} p={p} />)}
          </ul>
        ) : (
          <>
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
              {p.rows.map((r) => <StationItem key={r.id} r={r} p={p} mine={mineSrc.has(r.source)} />)}
            </ul>
          </>
        )}
        {!p.loading && p.total === 0 && p.off.length === 0 && <p className="empty-d">{empty}</p>}
        <div ref={end} />
        {hasMore && (
          <button className="btn more" type="button" onClick={p.onMore}>{t.showMore(Math.min(100, p.total - loaded))}</button>
        )}
      </div>
    </>
  );
}
