// The station list: search, filters with counts, a transmitter picked on the globe, and pages.
// It fills its panel: the controls are fixed, the rows scroll in their own box and load the
// next page when you reach the end.

import { useEffect, useId, useRef, useState } from 'react';
import { useNames } from '../catalog/names';
import type { Filters } from '../hooks/useCore';
import { useT } from '../i18n';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { Facets } from '../types/generated/Facets';
import type { Region } from '../types/generated/Region';
import type { StationRow } from '../types/generated/StationRow';
import { Dropdown, type DropOpt } from './Dropdown';
import { Flag } from './Flag';

const REGIONS: Region[] = ['am', 'eu', 'af', 'me', 'as', 'oc', 'other'];
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
  selId: number | null;
  onPick: (r: StationRow) => void;
  onMore: () => void;
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
  const [showFilters, setShowFilters] = useState(false);
  const picked = f.bands.length + f.langs.length + f.regions.length + f.countries.length;
  const filtered = f.bands.length + f.langs.length + f.regions.length + f.countries.length > 0 || f.site !== null || f.on_air || f.q !== '';
  const season = p.meta?.source.season ?? '';

  const box = useRef<HTMLDivElement>(null), end = useRef<HTMLDivElement>(null);
  const more = useRef(p.onMore);
  more.current = p.onMore;
  const asked = useRef(-1);
  const hasMore = p.rows.length < p.total;
  // next page when the end of the list comes into view (once per length)
  useEffect(() => {
    const b = box.current, e = end.current;
    if (!b || !e || !hasMore || typeof IntersectionObserver === 'undefined') return;
    const n = p.rows.length;
    const io = new IntersectionObserver(([en]) => {
      if (en.isIntersecting && asked.current !== n) {
        asked.current = n;
        more.current();
      }
    }, { root: b, rootMargin: '0px 0px 240px 0px' });
    io.observe(e);
    return () => io.disconnect();
  }, [p.rows.length, hasMore]);
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

  return (
    <>
      <div className="finder">
        <input
          type="search"
          aria-label={t.search}
          placeholder={t.searchPlaceholder}
          value={f.q}
          onChange={(e) => set({ q: e.currentTarget.value })}
        />
        <label className="chk">
          <input type="checkbox" checked={f.on_air} onChange={(e) => set({ on_air: e.currentTarget.checked })} />
          <span>{t.onAirNow}</span>
        </label>
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
      </div>
      {(f.site !== null || filtered) && (
        <div className="active-filters">
          {f.site !== null && (
            <button className="chip" type="button" title={t.clearFilter} onClick={() => set({ site: null })}>
              {t.siteFilter(p.siteName ?? '')} <span aria-hidden="true">✕</span>
            </button>
          )}
          {filtered && <button className="link" type="button" onClick={() => p.onFilters({ ...f, q: '', on_air: false, bands: [], langs: [], regions: [], countries: [], site: null })}>{t.clearFilters}</button>}
        </div>
      )}
      <div className="rows" ref={box}>
        <ul className="list">
          {p.rows.map((r) => {
            const country = names.country(r.itu);
            const abroad = r.site_itu !== r.itu;
            return (
              <li key={r.id}>
                <button className="row" type="button" aria-current={p.on && p.selId === r.id} disabled={!p.on} onClick={() => p.onPick(r)}>
                  <span className="rf">{r.freq_hz / 1000}</span>
                  <span className="rn">
                    <b><Flag flag={r.flag} title={country} />{r.name}</b>
                    <em>
                      {abroad && <>{t.via} <Flag flag={r.site_flag} title={names.country(r.site_itu)} /></>}
                      {r.site_name}
                      {r.lang && ` · ${names.lang(r.lang)}`}
                      {r.target && ` → ${names.target(r.target)}`}
                    </em>
                  </span>
                  <span className={'dot' + (r.on ? ' on' : '')} />
                </button>
              </li>
            );
          })}
        </ul>
        {!p.loading && p.total === 0 && <p className="empty-d">{t.noResults}</p>}
        <div ref={end} />
        {hasMore && (
          <button className="btn more" type="button" onClick={p.onMore}>{t.showMore(Math.min(100, p.total - p.rows.length))}</button>
        )}
      </div>
      {p.meta && (
        <p className="foot">
          {t.listFoot(season)}
          {!p.meta.source.current && <><br /><span className="warn">{t.seasonStale(season)}</span></>}
        </p>
      )}
    </>
  );
}
