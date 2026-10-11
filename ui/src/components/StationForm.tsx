// The station form: a station of the listener's own, new (from what is tuned, a station shown or
// a row off the schedule) or kept. Frequency, mode, name and country; its times on the air, each
// with days, languages and target; where the transmitter is (a known site, a position, or just a
// country); notes and a link. Problems are listed once Save was pressed; the core checks again.

import { useEffect, useId, useMemo, useRef, useState } from 'react';
import type { Api } from '../api/client';
import { useT, weekdayName } from '../i18n';
import type { CodeTables } from '../types/generated/CodeTables';
import type { Qth } from '../types/generated/Qth';
import type { UserSite } from '../types/generated/UserSite';
import type { UserSlot } from '../types/generated/UserSlot';
import type { UserStationDraft } from '../types/generated/UserStationDraft';
import { Combobox, Dropdown, type DropOpt } from './Dropdown';
import { MODES } from './favorites';
import { Sheet } from './Sheet';
import {
  cleaned, daysMask, fmtHhmm, LIMITS, nextSlot, parseDeg, parseHhmm, parseKhz, problems, toggleDay, type Problem,
} from './stationDraft';

/** The code tables, asked once per connection. */
let tablesOf: { api: Api; tables: Promise<CodeTables> } | null = null;
function useCodeTables(api: Api) {
  const [tables, setTables] = useState<CodeTables | null>(null);
  useEffect(() => {
    if (tablesOf?.api !== api) tablesOf = { api, tables: api.codeTables() };
    const mine = tablesOf;
    mine.tables.then(setTables, () => { if (tablesOf === mine) tablesOf = null; });
  }, [api]);
  return tables;
}

/** A combobox showing the label of the value picked, and letting the text be typed over. */
function Pick({ labelledBy, value, opts, onPick, placeholder, clearOnPick }: {
  labelledBy: string; value: string; opts: DropOpt[]; onPick: (v: string) => void; placeholder?: string; clearOnPick?: boolean;
}) {
  const label = opts.find((o) => o.value === value)?.label ?? value;
  const [text, setText] = useState(label);
  useEffect(() => setText(label), [label]);
  return (
    <Combobox
      labelledBy={labelledBy} text={text} onText={setText} opts={opts} selected={value} placeholder={placeholder}
      onPick={(o) => { setText(clearOnPick ? '' : o.label); onPick(o.value); }}
    />
  );
}

interface SlotProps {
  slot: UserSlot;
  /** What is typed in the times. */
  times: [string, string];
  langOpts: DropOpt[];
  targetOpts: DropOpt[];
  onSlot: (s: UserSlot) => void;
  onTimes: (t: [string, string]) => void;
  onRemove: (() => void) | null;
}

function SlotEditor({ slot, times, langOpts, targetOpts, onSlot, onTimes, onRemove }: SlotProps) {
  const t = useT();
  const id = useId();
  const mask = daysMask(slot.days);
  const langs = slot.lang.split(',').filter(Boolean);
  const langName = (c: string) => langOpts.find((o) => o.value === c)?.label ?? c;
  const time = (i: 0 | 1, text: string) => {
    const next: [string, string] = [...times];
    next[i] = text;
    onTimes(next);
    const m = parseHhmm(text);
    if (m !== null) onSlot(i === 0 ? { ...slot, start: m % 1440 } : { ...slot, end: m === 0 ? 1440 : m });
  };
  return (
    <fieldset className="slot-ed">
      <div className="row-inline">
        <label className="field">
          <span>{t.fFrom}</span>
          <input value={times[0]} inputMode="numeric" placeholder="HH:MM" onChange={(e) => time(0, e.currentTarget.value)} onBlur={() => parseHhmm(times[0]) !== null && onTimes([fmtHhmm(parseHhmm(times[0])! % 1440), times[1]])} />
        </label>
        <label className="field">
          <span>{t.fTo}</span>
          <input value={times[1]} inputMode="numeric" placeholder="HH:MM" onChange={(e) => time(1, e.currentTarget.value)} />
        </label>
        {onRemove && <button className="icon-btn slot-del" type="button" aria-label={t.fRemoveSlot} title={t.fRemoveSlot} onClick={onRemove}>✕</button>}
      </div>
      <div className="field">
        <span id={`${id}-days`}>{t.fDays}</span>
        <div className="seg days" role="group" aria-labelledby={`${id}-days`}>
          {[0, 1, 2, 3, 4, 5, 6].map((wd) => (
            <button key={wd} type="button" aria-pressed={(mask & (1 << wd)) !== 0} onClick={() => onSlot({ ...slot, days: toggleDay(slot.days, wd) })}>
              {weekdayName(t, wd)}
            </button>
          ))}
        </div>
      </div>
      <div className="field">
        <span id={`${id}-langs`}>{t.fLangs}</span>
        {langs.length > 0 && (
          <div className="lang-chips">
            {langs.map((c) => (
              <button key={c} className="chip" type="button" title={t.fRemoveLang(langName(c))} aria-label={t.fRemoveLang(langName(c))} onClick={() => onSlot({ ...slot, lang: langs.filter((x) => x !== c).join(',') })}>
                {langName(c)} <span aria-hidden="true">✕</span>
              </button>
            ))}
          </div>
        )}
        {langs.length < LIMITS.langs && (
          <Pick
            labelledBy={`${id}-langs`} value="" placeholder={t.fLangAdd} clearOnPick
            opts={langOpts.filter((o) => !langs.includes(o.value))}
            onPick={(c) => onSlot({ ...slot, lang: [...langs, c].join(',') })}
          />
        )}
      </div>
      <div className="field">
        <span id={`${id}-target`}>{t.fTarget}</span>
        <Pick labelledBy={`${id}-target`} value={slot.target} opts={[{ value: '', label: t.fAnyTarget }, ...targetOpts]} onPick={(v) => onSlot({ ...slot, target: v })} />
      </div>
    </fieldset>
  );
}

const timesOf = (s: UserSlot): [string, string] => [fmtHhmm(s.start), fmtHhmm(s.end)];
const degText = (v: number) => (Number.isFinite(v) ? String(v) : '');

interface Props {
  api: Api;
  /** The kept station's id, when editing one. */
  id: string | null;
  draft: UserStationDraft;
  qth: Qth | null;
  /** Save it; true closes the form. */
  onSave: (d: UserStationDraft) => Promise<boolean>;
  onClose: () => void;
}

export function StationForm({ api, id, draft: initial, qth, onSave, onClose }: Props) {
  const t = useT();
  const uid = useId();
  const tables = useCodeTables(api);
  const [d, setD] = useState(initial);
  const [freqText, setFreqText] = useState(String(initial.freq_hz / 1000));
  const [times, setTimes] = useState(() => initial.slots.map(timesOf));
  const site = d.site;
  const [lat, setLat] = useState(site.kind === 'country' ? '' : degText(site.lat));
  const [lon, setLon] = useState(site.kind === 'country' ? '' : degText(site.lon));
  // Save presses with something to fix: the list of problems comes into view on each
  const [tried, setTried] = useState(0);
  const problemsBox = useRef<HTMLUListElement>(null);
  useEffect(() => { if (tried) problemsBox.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }); }, [tried]);
  const [busy, setBusy] = useState(false);
  const set = (patch: Partial<UserStationDraft>) => setD((cur) => ({ ...cur, ...patch }));
  const setSite = (s: UserSite) => set({ site: s });

  const es = t.locale === 'es';
  const countryOpts = useMemo<DropOpt[]>(() => (tables?.countries ?? []).map((c) => ({ value: c.itu, label: es ? c.es : c.en, hint: c.itu })).sort((a, b) => a.label.localeCompare(b.label)), [tables, es]);
  const langOpts = useMemo<DropOpt[]>(() => (tables?.langs ?? []).filter((l) => !l.code.startsWith('-')).map((l) => ({ value: l.code, label: es ? (l.es ?? l.en) : l.en, hint: l.code })).sort((a, b) => a.label.localeCompare(b.label)), [tables, es]);
  // target areas, then countries (a broadcast may target one)
  const targetOpts = useMemo<DropOpt[]>(() => [...(tables?.targets ?? []).map((x) => ({ value: x.code, label: es ? x.es : x.en, hint: x.code })), ...countryOpts], [tables, es, countryOpts]);
  const countryName = (itu: string) => countryOpts.find((o) => o.value === itu)?.label ?? itu;
  const siteOpts = useMemo<DropOpt[]>(() => (tables?.sites ?? []).map((s) => ({ value: String(s.id), label: s.name, hint: countryOpts.find((o) => o.value === s.itu)?.label ?? s.itu })), [tables, countryOpts]);
  const knownId = site.kind === 'known' ? tables?.sites.find((s) => s.itu === site.itu && (site.code ? s.code === site.code : s.name === site.name))?.id : undefined;

  const freqHz = parseKhz(freqText);
  const placeable = site.kind !== 'country' || !site.itu || !tables || tables.placeable.includes(site.itu);
  const found: Problem[] = [
    ...problems(d, freqHz !== null),
    ...(times.some(([a, b]) => parseHhmm(a) === null || parseHhmm(b) === null) ? ['time' as const] : []),
    ...(placeable ? [] : ['site' as const]),
  ];
  const shown = [...new Set(found)];

  // the station's country, until another is picked for a country-only site
  const pickCountry = (itu: string) => setD((cur) => ({
    ...cur,
    itu,
    site: cur.site.kind !== 'known' && (cur.site.itu === '' || cur.site.itu === cur.itu) ? { ...cur.site, itu } : cur.site,
  }));
  const kind = (k: UserSite['kind']) => {
    if (k === site.kind) return;
    const itu = site.itu || d.itu;
    if (k === 'country') setSite({ kind: 'country', itu });
    else if (k === 'custom') setSite({ kind: 'custom', itu, name: site.kind === 'known' ? site.name : '', lat: parseDeg(lat, 90) ?? NaN, lon: parseDeg(lon, 180) ?? NaN });
    else setSite({ kind: 'known', itu: '', code: null, name: '', lat: 0, lon: 0 });
  };
  const pos = (latText: string, lonText: string) => {
    setLat(latText);
    setLon(lonText);
    if (site.kind === 'custom') setSite({ ...site, lat: parseDeg(latText, 90) ?? NaN, lon: parseDeg(lonText, 180) ?? NaN });
  };
  const slotAt = (i: number, s: UserSlot) => set({ slots: d.slots.map((x, j) => (j === i ? s : x)) });
  const addSlot = () => {
    const s = nextSlot(d.slots[d.slots.length - 1], new Date());
    set({ slots: [...d.slots, s] });
    setTimes([...times, timesOf(s)]);
  };
  const removeSlot = (i: number) => {
    set({ slots: d.slots.filter((_, j) => j !== i) });
    setTimes(times.filter((_, j) => j !== i));
  };

  const save = async () => {
    if (shown.length || freqHz === null) {
      setTried((n) => n + 1);
      return;
    }
    setBusy(true);
    const ok = await onSave(cleaned({ ...d, freq_hz: freqHz }));
    setBusy(false);
    if (ok) onClose();
  };

  return (
    <Sheet title={id ? t.stationEdit : t.stationNew} onClose={onClose}>
      <form className="sec form station-form" onSubmit={(e) => { e.preventDefault(); void save(); }}>
        <div className="row-inline">
          <label className="field">
            <span>{t.fFreq}</span>
            <input value={freqText} inputMode="decimal" onChange={(e) => setFreqText(e.currentTarget.value)} />
          </label>
          <div className="field mode-f">
            <span id={`${uid}-mode`}>{t.fMode}</span>
            <Dropdown labelledBy={`${uid}-mode`} value={d.mode} opts={MODES.map((m) => ({ value: m, label: m }))} onChange={(v) => set({ mode: v as UserStationDraft['mode'] })} />
          </div>
        </div>
        <label className="field">
          <span>{t.fName}</span>
          <input value={d.name} maxLength={LIMITS.name} placeholder={t.fNamePh} onChange={(e) => set({ name: e.currentTarget.value })} />
        </label>
        <div className="field">
          <span id={`${uid}-country`}>{t.fCountry}</span>
          <Pick labelledBy={`${uid}-country`} value={d.itu} opts={countryOpts} placeholder={t.fCountryPh} onPick={pickCountry} />
        </div>

        <h3 className="h form-h"><span>{t.fSchedule}</span><span>{d.slots.length}</span></h3>
        {d.slots.map((s, i) => (
          <SlotEditor
            key={i} slot={s} times={times[i]} langOpts={langOpts} targetOpts={targetOpts}
            onSlot={(x) => slotAt(i, x)} onTimes={(x) => setTimes(times.map((y, j) => (j === i ? x : y)))}
            onRemove={d.slots.length > 1 ? () => removeSlot(i) : null}
          />
        ))}
        {d.slots.length < LIMITS.slots && <button className="btn add-slot" type="button" onClick={addSlot}>+ {t.fAddSlot}</button>}

        <h3 className="h form-h"><span id={`${uid}-site`}>{t.fSite}</span></h3>
        <div className="seg" role="radiogroup" aria-labelledby={`${uid}-site`}>
          {([['known', t.siteKnown], ['custom', t.siteCustom], ['country', t.siteCountry]] as const).map(([k, label]) => (
            <button key={k} type="button" role="radio" aria-checked={site.kind === k} onClick={() => kind(k)}>{label}</button>
          ))}
        </div>
        {site.kind === 'known' && (
          <div className="field">
            <span id={`${uid}-known`}>{t.siteKnown}</span>
            <Pick
              labelledBy={`${uid}-known`} value={knownId === undefined ? '' : String(knownId)} opts={siteOpts} placeholder={t.siteKnownPh}
              onPick={(v) => {
                const s = tables?.sites.find((x) => String(x.id) === v);
                if (s) setSite({ kind: 'known', itu: s.itu, code: s.code, name: s.name, lat: s.lat, lon: s.lon });
              }}
            />
            {site.name && knownId === undefined && <small>{site.name} · {countryName(site.itu)}</small>}
          </div>
        )}
        {site.kind === 'custom' && (
          <>
            <label className="field">
              <span>{t.fSiteName}</span>
              <input value={site.name} maxLength={LIMITS.siteName} onChange={(e) => setSite({ ...site, name: e.currentTarget.value })} />
            </label>
            <div className="field">
              <span id={`${uid}-sitec`}>{t.fCountry}</span>
              <Pick labelledBy={`${uid}-sitec`} value={site.itu} opts={countryOpts} placeholder={t.fCountryPh} onPick={(itu) => setSite({ ...site, itu })} />
            </div>
            <div className="row-inline">
              <label className="field">
                <span>{t.fLat}</span>
                <input value={lat} inputMode="decimal" placeholder="-33.45" onChange={(e) => pos(e.currentTarget.value, lon)} />
              </label>
              <label className="field">
                <span>{t.fLon}</span>
                <input value={lon} inputMode="decimal" placeholder="-70.67" onChange={(e) => pos(lat, e.currentTarget.value)} />
              </label>
            </div>
            {qth && (
              <button
                className="btn use-qth" type="button"
                onClick={() => {
                  setLat(String(qth.lat));
                  setLon(String(qth.lon));
                  setSite({ ...site, lat: qth.lat, lon: qth.lon, name: site.name || qth.name });
                }}
              >
                {t.useQth}
              </button>
            )}
          </>
        )}
        {site.kind === 'country' && (
          <div className="field">
            <span id={`${uid}-sitecc`}>{t.fCountry}</span>
            <Pick labelledBy={`${uid}-sitecc`} value={site.itu} opts={countryOpts} placeholder={t.fCountryPh} onPick={(itu) => setSite({ kind: 'country', itu })} />
            <small className={placeable ? undefined : 'warn'}>{placeable ? t.siteCountryNote : t.siteNotPlaceable}</small>
          </div>
        )}

        <label className="field">
          <span>{t.fNotes}</span>
          <textarea rows={3} value={d.notes} maxLength={LIMITS.notes} placeholder={t.fNotesPh} onChange={(e) => set({ notes: e.currentTarget.value })} />
        </label>
        <label className="field">
          <span>{t.fUrl}</span>
          <input type="url" value={d.url ?? ''} maxLength={LIMITS.url} placeholder="https://" onChange={(e) => set({ url: e.currentTarget.value })} />
        </label>

        {tried > 0 && shown.length > 0 && (
          <ul className="problems" role="alert" ref={problemsBox}>
            {shown.map((p) => <li key={p}>{t.problem[p]}</li>)}
          </ul>
        )}
        <div className="actions">
          <button className="btn primary" type="submit" disabled={busy || !tables}>{t.saveStation}</button>
          <button className="btn" type="button" onClick={onClose}>{t.cancel}</button>
        </div>
      </form>
    </Sheet>
  );
}
