// The station card: on-air status, transmitter, route, today's bar, other candidates, and every
// broadcast slot. It fills its panel: the top part is fixed, the slots scroll in their own box.

import { useEffect, useRef, useState } from 'react';
import { useNames } from '../catalog/names';
import { compass, fmtDate, fmtDays, fmtDur, fmtKhz, fmtKm, fmtMonth, hhmm, useT, weekdayName, type Messages } from '../i18n';
import type { AirStatus } from '../types/generated/AirStatus';
import type { Candidate } from '../types/generated/Candidate';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { Slot } from '../types/generated/Slot';
import { Flag } from './Flag';
import { FlagBackdrop } from './FlagBackdrop';

const wavelengthM = (hz: number) => 299_792_458 / hz;
/** Other candidates shown before "+n more". */
const FEW_CANDS = 2;
const nowFrac = (d: Date) => (d.getUTCHours() * 60 + d.getUTCMinutes() + d.getUTCSeconds() / 60) / 1440;

function Head({ t, air, now }: { t: Messages; air: AirStatus; now: Date }) {
  const at = hhmm(air.edge);
  if (air.on) {
    const sub = air.minutes === null ? t.allDay : t.endsIn(fmtDur(t, air.minutes), at);
    return <div className="d-head"><span className="pill on">{t.onAir}</span><span className="d-sub">{sub}</span></div>;
  }
  let sub = t.noSchedule;
  if (air.minutes !== null) {
    // past today's end: say which day
    const nowMin = now.getUTCHours() * 60 + now.getUTCMinutes();
    if (nowMin + air.minutes >= 1440) {
      const day = new Date(now.getTime() + air.minutes * 60_000);
      sub = t.backOn(weekdayName(t, (day.getUTCDay() + 6) % 7), at, fmtDur(t, air.minutes));
    } else {
      sub = t.backAt(at, fmtDur(t, air.minutes));
    }
  }
  return <div className="d-head"><span className="pill">{t.offAir}</span><span className="d-sub">{sub}</span></div>;
}

function SlotRow({ s, current }: { s: Slot; current: boolean }) {
  const t = useT();
  const names = useNames();
  const notes = [
    s.season === 'winter' ? t.winterOnly : s.season === 'summer' ? t.summerOnly : null,
    s.from !== null && s.to !== null ? t.validDates(fmtDate(t, s.from), fmtDate(t, s.to)) : null,
    s.heard ? t.lastHeard(fmtMonth(t, s.heard)) : null,
  ].filter(Boolean);
  return (
    <tr className={current ? 'cur' : undefined}>
      <td className="mono">{hhmm(s.start)}–{hhmm(s.end % 1440)}</td>
      <td>{fmtDays(t, s.days)}</td>
      <td>{s.lang ? names.lang(s.lang) : '—'}</td>
      <td>{s.target ? names.target(s.target) : '—'}{notes.length > 0 && <small>{notes.join(' · ')}</small>}</td>
    </tr>
  );
}

interface Props {
  on: boolean;
  freqHz: number | null;
  sel: Candidate | null;
  others: Candidate[];
  meta: CatalogMeta | null;
  now: Date;
  onChoose: (c: Candidate) => void;
}

export function Dossier({ on, freqHz, sel, others, meta, now, onChoose }: Props) {
  const t = useT();
  const names = useNames();
  const [allCands, setAllCands] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const stId = sel?.station.id ?? null, slotIx = sel?.slot ?? null;
  useEffect(() => setAllCands(false), [stId]);
  // bring the slot on the air now into view
  useEffect(() => {
    const b = box.current, row = b?.querySelector('tr.cur');
    if (!b || !row) {
      if (b) b.scrollTop = 0;
      return;
    }
    const br = b.getBoundingClientRect(), rr = row.getBoundingClientRect();
    b.scrollTop += rr.top - br.top - b.clientHeight / 3;
  }, [stId, slotIx]);

  if (!on) {
    return <div className="d-empty"><p className="empty-t">{t.rigOff}</p><p className="empty-d">{t.rigOffHint}</p></div>;
  }
  if (!sel) {
    const fs = meta?.freqs_hz ?? [];
    return (
      <div className="d-empty">
        <p className="empty-t">{t.noStation(freqHz === null ? '--' : fmtKhz(freqHz))}</p>
        {meta && fs.length > 0 && (
          <p className="empty-d">{t.noStationHint(meta.stations, String(fs[0] / 1000), String(fs[fs.length - 1] / 1000))}</p>
        )}
      </div>
    );
  }
  const { station: st, site, route, sun } = sel;
  const slot = sel.slot === null ? null : st.slots[sel.slot];
  const country = names.country(st.itu);
  const siteCountry = names.country(site.itu);
  const shownCands = allCands ? others : others.slice(0, FEW_CANDS);
  return (
    <div className="dossier">
      <FlagBackdrop key={st.flag ?? ''} flag={st.flag} />
      <div className="d-top" key={st.id}>
        <Head t={t} air={sel.air} now={now} />
        <h3 className="d-name"><Flag flag={st.flag} title={country} />{st.name}</h3>
        <p className="d-meta">
          {country}
          {slot && <> · {slot.lang ? names.lang(slot.lang) : ''}{slot.target && ` → ${names.target(slot.target)}`}</>}
        </p>
        <dl className="d-grid">
          <div className="wide">
            <dt>{t.transmitter}</dt>
            <dd>
              <Flag flag={site.flag} title={siteCountry} />
              {site.precise ? `${site.name} (${siteCountry})` : siteCountry}
              {!site.precise && <small className="approx">{t.approxSite}</small>}
            </dd>
          </div>
          <div><dt>{t.distance}</dt><dd>{fmtKm(t, route.km)}</dd></div>
          <div><dt>{t.bearing}</dt><dd>{Math.round(route.bearing)}° {compass(t, route.bearing)}</dd></div>
          <div className="minor"><dt>{t.wavelength}</dt><dd>{wavelengthM(st.freq_hz).toFixed(1)} m</dd></div>
          <div className="w2"><dt>{t.sunAtSite}</dt><dd><span className={'sun ' + sun.phase}>{t.sun[sun.phase]}</span> · {t.solarTime(hhmm(sun.solar_min))}</dd></div>
          <div className="minor"><dt>{t.mode}</dt><dd>{st.mode}</dd></div>
        </dl>
        <div className="sched">
          <h4 className="h"><span>{t.schedule}</span><span>UTC</span></h4>
          <div className="bar">
            {sel.today.map(([s, e]) => <i key={s} className="win" style={{ left: s / 14.4 + '%', width: (e - s) / 14.4 + '%' }} />)}
            <i className="now" style={{ left: nowFrac(now) * 100 + '%' }} />
          </div>
          <div className="axis"><span>00</span><span>06</span><span>12</span><span>18</span><span>24</span></div>
        </div>
        {others.length > 0 && (
          <div className="cands">
            <span className="lab">{t.alsoOn(fmtKhz(st.freq_hz))}</span>
            {shownCands.map((o) => (
              <button key={o.station.id} className="chip" type="button" onClick={() => onChoose(o)}>
                <span className={'dot' + (o.air.on ? ' on' : '')} />
                <Flag flag={o.station.flag} title={names.country(o.station.itu)} />
                <span className="chip-name">{o.station.name}</span>
              </button>
            ))}
            {others.length > FEW_CANDS && (
              <button className="chip more" type="button" aria-expanded={allCands} onClick={() => setAllCands((a) => !a)}>
                {allCands ? t.fewerCands : t.moreCands(others.length - FEW_CANDS)}
              </button>
            )}
          </div>
        )}
      </div>
      <div className="slots">
        <h4 className="h"><span>{t.broadcasts}</span><span>{st.slots.length}</span></h4>
        <div className="slots-box" ref={box}>
          <table>
            <thead>
              <tr><th>UTC</th><th>{t.days}</th><th>{t.language}</th><th>{t.target}</th></tr>
            </thead>
            <tbody>
              {st.slots.map((s, i) => <SlotRow key={i} s={s} current={i === sel.slot && sel.air.on} />)}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
