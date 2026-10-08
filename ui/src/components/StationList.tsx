import { useT } from '../i18n';
import type { Candidate } from '../types/generated/Candidate';

interface Props {
  on: boolean;
  all: Candidate[];
  selId: string | null;
  onPick: (c: Candidate) => void;
}

export function StationList({ on, all, selId, onPick }: Props) {
  const t = useT();
  return (
    <section className="sec" aria-label={t.stations}>
      <h2 className="h"><span>{t.inDatabase}</span><span>{t.stationCount(all.length)}</span></h2>
      <ul className="list">
        {all.map((c) => (
          <li key={c.station.id}>
            <button className="row" type="button" aria-current={on && selId === c.station.id} disabled={!on} onClick={() => onPick(c)}>
              <span className="rf">{c.station.freq_hz / 1000}</span>
              <span className="rn"><b>{c.station.name}</b><em>{c.site.short}</em></span>
              <span className={'dot' + (c.air.on ? ' on' : '')} />
            </button>
          </li>
        ))}
      </ul>
      <p className="foot">{t.listFoot}</p>
    </section>
  );
}
