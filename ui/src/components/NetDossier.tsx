// The station card for an internet station (the NET band), in the Dossier's place while one is
// tuned: whether it is playing, what it is (country, languages, tags, its stream), where it is
// from the listener and the Sun there, the directory's last check, your time with it, and the way
// back to the rig. A quiet line at the foot says the data comes from radio-browser.info.

import type { ReactNode } from 'react';
import { compass, fmtKm, fmtSecs, hhmm, useT, type Messages } from '../i18n';
import type { NetNow } from '../types/generated/NetNow';
import { Flag } from './Flag';
import { FlagBackdrop } from './FlagBackdrop';
import { bufferPct, streamLabel } from './net';

const DIRECTORY_URL = 'https://www.radio-browser.info/';

function Head({ t, n }: { t: Messages; n: NetNow }) {
  const st = n.state;
  const sub = st.state === 'tuned' ? t.netStateTuned
    : st.state === 'connecting' ? t.netConnecting
    : st.state === 'playing' ? t.netStatePlaying
    : st.state === 'buffering' ? t.netBuffering(bufferPct(n))
    : t.netFailed(st.message);
  return (
    <div className="d-head">
      <span className={'pill net' + (st.state === 'playing' ? ' on' : '')}>{t.netOnline}</span>
      <span className="d-sub">{sub}</span>
    </div>
  );
}

interface Props {
  now: NetNow;
  country: string;
  star: ReactNode;
  onBackToRig: () => void;
}

export function NetDossier({ now, country, star, onBackToRig }: Props) {
  const t = useT();
  const st = now.station;
  const stream = streamLabel(st);
  return (
    <div className="dossier net-dossier">
      <FlagBackdrop key={st.flag ?? ''} flag={st.flag} />
      <div className="d-top" key={st.uuid}>
        <Head t={t} n={now} />
        <div className="d-title">
          <h3 className="d-name"><Flag flag={st.flag} title={country} />{st.name}<span className="net-rf" title={t.netBadgeTitle}>{t.netBadge}</span></h3>
          {star}
        </div>
        <p className="d-meta">
          {country}
          {st.langs.length > 0 && ` · ${st.langs.join(', ')}`}
        </p>
        <dl className="d-grid">
          <div className="wide"><dt>{t.netStream}</dt><dd>{stream || '—'} <small className="approx">{t.netRelayed}</small></dd></div>
          {now.route && (
            <>
              <div><dt>{t.distance}</dt><dd>{fmtKm(t, now.route.km)}{now.place && !now.place.precise && <small className="approx">{t.netApproxPlace}</small>}</dd></div>
              <div><dt>{t.bearing}</dt><dd>{Math.round(now.route.bearing)}° {compass(t, now.route.bearing)}</dd></div>
            </>
          )}
          {now.sun && <div className="w2"><dt>{t.netSunThere}</dt><dd><span className={'sun ' + now.sun.phase}>{t.sun[now.sun.phase]}</span> · {t.solarTime(hhmm(now.sun.solar_min))}</dd></div>}
          <div className="w2"><dt>{t.netDirectory}</dt><dd>{st.ok ? t.netCheckOk : t.netCheckFailed} · {t.netVotes(st.votes)}</dd></div>
          <div className="w2"><dt>{t.netYourTime}</dt><dd>{now.listened_secs > 0 ? fmtSecs(t, now.listened_secs) : t.netNotYet}</dd></div>
        </dl>
        {st.tags.length > 0 && (
          <div className="cands net-tags">
            {st.tags.map((g) => <span key={g} className="chip">{g}</span>)}
          </div>
        )}
        <p className="d-notes">
          {t.netNotDx}
          {st.homepage && <a className="link" href={st.homepage} target="_blank" rel="noreferrer">{t.website}</a>}
        </p>
        <p><button className="btn" type="button" onClick={onBackToRig}>⟲ {t.netBackToRig}</button></p>
      </div>
      <p className="d-source">
        {t.dataSource} · <a href={DIRECTORY_URL} target="_blank" rel="noreferrer">radio-browser.info ↗</a>
      </p>
    </div>
  );
}
