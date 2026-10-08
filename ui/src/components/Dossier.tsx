// The station card: on-air status, transmitter, route, schedule bar, other candidates.

import { compass, fmtDur, fmtKhz, fmtKm, hhmm, localTime, useT, type Messages } from '../i18n';
import type { AirStatus } from '../types/generated/AirStatus';
import type { Candidate } from '../types/generated/Candidate';

const wavelengthM = (hz: number) => 299_792_458 / hz;
const nowFrac = (d: Date) => (d.getUTCHours() * 60 + d.getUTCMinutes() + d.getUTCSeconds() / 60) / 1440;

function Head({ t, air }: { t: Messages; air: AirStatus }) {
  const at = hhmm(air.edge);
  if (air.on) {
    const sub = air.minutes === null ? t.allDay : t.endsIn(fmtDur(t, air.minutes), at);
    return <div className="d-head"><span className="pill on">{t.onAir}</span><span className="d-sub">{sub}</span></div>;
  }
  const sub = air.minutes === null ? t.noSchedule : t.backAt(at, fmtDur(t, air.minutes));
  return <div className="d-head"><span className="pill">{t.offAir}</span><span className="d-sub">{sub}</span></div>;
}

interface Props {
  on: boolean;
  freqHz: number | null;
  sel: Candidate | null;
  others: Candidate[];
  all: Candidate[];
  now: Date;
  onChoose: (c: Candidate) => void;
}

export function Dossier({ on, freqHz, sel, others, all, now, onChoose }: Props) {
  const t = useT();
  if (!on) {
    return <><p className="empty-t">{t.rigOff}</p><p className="empty-d">{t.rigOffHint}</p></>;
  }
  if (!sel) {
    const fs = all.map((c) => c.station.freq_hz);
    return (
      <>
        <p className="empty-t">{t.noStation(freqHz === null ? '--' : fmtKhz(freqHz))}</p>
        {fs.length > 0 && (
          <p className="empty-d">{t.noStationHint(all.length, String(Math.min(...fs) / 1000), String(Math.max(...fs) / 1000))}</p>
        )}
      </>
    );
  }
  const { station: st, site, route } = sel;
  return (
    <>
      <Head t={t} air={sel.air} />
      <h3 className="d-name">{st.name}</h3>
      <p className="d-meta">{st.lang} · {st.target}</p>
      <dl className="d-grid">
        <div className="wide"><dt>{t.transmitter}</dt><dd>{site.name}</dd></div>
        <div><dt>{t.distance}</dt><dd>{fmtKm(t, route.km)}</dd></div>
        <div><dt>{t.bearing}</dt><dd>{Math.round(route.bearing)}° {compass(t, route.bearing)}</dd></div>
        <div><dt>{t.localTime}</dt><dd>{localTime(t, site.tz, now)}</dd></div>
        <div><dt>{t.wavelength}</dt><dd>{wavelengthM(st.freq_hz).toFixed(1)} m</dd></div>
        <div><dt>{t.power}</dt><dd>{st.kw ? `${st.kw} kW` : t.na}</dd></div>
        <div><dt>{t.mode}</dt><dd>{st.mode}</dd></div>
      </dl>
      <div className="sched">
        <h4 className="h"><span>{t.schedule}</span><span>UTC</span></h4>
        <div className="bar">
          {sel.windows.map(([s, e]) => <i key={s} className="win" style={{ left: s / 14.4 + '%', width: (e - s) / 14.4 + '%' }} />)}
          <i className="now" style={{ left: nowFrac(now) * 100 + '%' }} />
        </div>
        <div className="axis"><span>00</span><span>06</span><span>12</span><span>18</span><span>24</span></div>
      </div>
      {others.length > 0 && (
        <div className="cands">
          <span className="lab">{t.alsoOn(fmtKhz(st.freq_hz))}</span>
          {others.map((o) => (
            <button key={o.station.id} className="chip" type="button" onClick={() => onChoose(o)}>
              <span className={'dot' + (o.air.on ? ' on' : '')} />{o.station.name}
            </button>
          ))}
        </div>
      )}
      <p className="d-note">{st.note}</p>
    </>
  );
}
