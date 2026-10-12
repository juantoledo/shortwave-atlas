// Stats › Hunt: stations on the air now that the history hasn't got, with why each is worth a
// try (a path that suits the band, a new country, rare, far), and the stations the band scans
// logged that the listener never stayed on. A row tunes there; the view stays.

import { useNames } from '../../catalog/names';
import { fmtKm, fmtStamp, sLabel, useT } from '../../i18n';
import type { Messages } from '../../i18n';
import type { Hunt } from '../../types/generated/Hunt';
import type { HuntReason } from '../../types/generated/HuntReason';
import { StationName, type Tune } from './Overview';

const khz = (hz: number) => String(hz / 1000);

function reasonText(t: Messages, r: HuntReason): string {
  switch (r.kind) {
    case 'dark_path': return t.huntReasons.dark;
    case 'lit_path': return t.huntReasons.lit;
    case 'new_country': return t.huntReasons.country;
    case 'new_station': return t.huntReasons.station;
    case 'rare': return t.huntReasons.rare(r.stars);
    case 'far': return fmtKm(t, r.km);
  }
}

export function HuntTab({ hunt, onTune }: { hunt: Hunt | null; onTune: Tune }) {
  const t = useT();
  const names = useNames();
  return (
    <div className="st-grid">
      <section className="st-card st-wide">
        <h3 className="h"><span>{t.huntNow}</span><span>{hunt ? hunt.now.length : '…'}</span></h3>
        <p className="note">{t.huntNowHelp}</p>
        {hunt && hunt.now.length === 0 && <p className="empty-d">{t.huntEmpty}</p>}
        <ul className="list">
          {hunt?.now.map((r) => (
            <li key={r.station_id}>
              <button className="row scan-row" type="button" title={t.scanTune(khz(r.station.freq_hz))} onClick={() => onTune(r.station.freq_hz, r.mode, r.station)}>
                <span className="rf">{khz(r.station.freq_hz)}</span>
                <span className="rn">
                  <b><StationName r={r.station} /></b>
                  <em>
                    {r.station.lang && names.lang(r.station.lang)}
                    {r.station.target && ` → ${names.target(r.station.target)}`}
                  </em>
                  <span className="st-chips">{r.reasons.map((x, i) => <span key={i} className={'st-chip ' + x.kind}>{reasonText(t, x)}</span>)}</span>
                </span>
                <span className="sval" title={t.rarityTitle}>{'★'.repeat(r.rarity)}</span>
              </button>
            </li>
          ))}
        </ul>
      </section>

      <section className="st-card st-wide">
        <h3 className="h"><span>{t.scannedNever}</span><span>{hunt ? hunt.scanned.length : '…'}</span></h3>
        {hunt && hunt.scanned.length === 0 && <p className="empty-d">{t.scannedEmpty}</p>}
        <ul className="list">
          {hunt?.scanned.map((r) => (
            <li key={r.station_id}>
              <button className="row scan-row" type="button" title={t.scanTune(khz(r.station.freq_hz))} onClick={() => onTune(r.station.freq_hz, r.mode, r.station)}>
                <span className="rf">{khz(r.station.freq_hz)}</span>
                <span className="rn">
                  <b><StationName r={r.station} /></b>
                  <em>{fmtStamp(t, r.last)}</em>
                </span>
                <span className="sval">{sLabel(r.best_db)}</span>
              </button>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
