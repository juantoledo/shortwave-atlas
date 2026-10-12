// Stats › Collection, the calm game layer: the passport (countries heard, by region), the
// continents, this month's rarity points, the badge shelf and the bingo card, the streak and
// milestones. No fanfare: an earned badge just sits on the shelf. Settings › Display hides it.

import { useNames } from '../../catalog/names';
import { fmtDate, fmtStamp, useT } from '../../i18n';
import type { Messages } from '../../i18n';
import type { Challenge } from '../../types/generated/Challenge';
import type { Stats } from '../../types/generated/Stats';
import { Flag } from '../Flag';
import { Ring } from './charts';
import { Milestones } from './Overview';

const CONTINENTS = ['am', 'eu', 'af', 'as', 'oc'] as const;

function challengeText(t: Messages, names: ReturnType<typeof useNames>, c: Challenge): string {
  switch (c.kind) {
    case 'region': return t.chRegion(names.region(c.region));
    case 'lang': return t.chLang(names.lang(c.lang));
    case 'band': return t.chBand(names.band(c.band));
    case 'below_khz': return t.chBelow(c.khz / 1000);
    case 'above_khz': return t.chAbove(c.khz / 1000);
    case 'over_km': return t.chOverKm(c.km.toLocaleString(t.locale));
    case 'under_km': return t.chUnderKm(c.km.toLocaleString(t.locale));
    case 'rarity': return t.chRarity(c.stars);
    case 'grey_line': return t.chGrey;
    case 'dark_path': return t.chDark;
    case 'minutes': return t.chMinutes(c.minutes);
    case 'new_country': return t.chNewCountry;
  }
}

export function Collection({ s, onShowGlobe }: { s: Stats; onShowGlobe: () => void }) {
  const t = useT();
  const names = useNames();
  const c = s.collection;
  const month = new Intl.DateTimeFormat(t.locale, { month: 'long', year: 'numeric', timeZone: 'UTC' }).format(new Date(Date.UTC(c.bingo.year, c.bingo.month - 1, 1)));
  return (
    <div className="st-grid">
      <section className="st-card st-wide">
        <h3 className="h">
          <span>{t.passport}</span>
          <span className="st-h-aside">
            {t.passportCount(c.passport.length)}
            {s.sites.length > 0 && <button className="link" type="button" title={t.statsShowGlobeTitle} onClick={onShowGlobe}>{t.statsShowGlobe}</button>}
          </span>
        </h3>
        <div className="st-regions">
          {c.regions.map((r) => (
            <div key={r.region} className="st-region" title={`${names.region(r.region)}: ${r.heard} / ${r.total}`}>
              <span>{names.region(r.region)}</span>
              <span className="st-ht" aria-hidden="true"><i style={{ width: `${r.total ? (r.heard / r.total) * 100 : 0}%` }} /></span>
              <small>{r.heard} / {r.total}</small>
            </div>
          ))}
        </div>
        {c.passport.length === 0 ? <p className="empty-d">{t.passportEmpty}</p> : (
          <ul className="st-stamps">
            {c.passport.map((p) => (
              <li key={p.itu} title={`${names.country(p.itu)} · ${t.passportFirst(fmtStamp(t, p.first))}`}>
                <Flag flag={p.flag} title={names.country(p.itu)} />
                <span>{names.country(p.itu)}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="st-card">
        <h3 className="h"><span>{t.continents}</span><span>{c.continents.length} / 5</span></h3>
        <ul className="st-conts">
          {CONTINENTS.map((k) => {
            const on = c.continents.includes(k);
            return <li key={k} className={on ? 'on' : ''}><span aria-hidden="true">{on ? '●' : '○'}</span>{names.region(k)}</li>;
          })}
        </ul>
        {c.continents.length === 5 && <p className="note">{t.continentsDone}</p>}
        <h3 className="h st-gap"><span>{t.points}</span><b className="st-points">{c.points}</b></h3>
        <p className="note">{t.pointsHelp}</p>
      </section>

      <section className="st-card st-wide">
        <h3 className="h"><span>{t.badges}</span><span>{c.badges.filter((b) => b.earned !== null).length} / {c.badges.length}</span></h3>
        <ul className="st-badges">
          {c.badges.map((b) => {
            const [name, what] = t.badgeNames[b.id];
            const earned = b.earned !== null;
            return (
              <li key={b.id} className={earned ? 'earned' : ''} title={what}>
                <Ring p={earned ? 1 : b.goal ? b.progress / b.goal : 0}>{earned ? '★' : `${Math.round((b.progress / (b.goal || 1)) * 100)}%`}</Ring>
                <span className="st-badge-txt">
                  <b>{name}</b>
                  <small>{earned ? t.badgeEarned(fmtDate(t, Math.floor(b.earned! / 86_400))) : what}</small>
                </span>
              </li>
            );
          })}
        </ul>
      </section>

      <section className="st-card">
        <h3 className="h"><span>{t.bingo}</span><span>{month}</span></h3>
        <ol className="st-bingo">
          {c.bingo.cells.map((cell, i) => (
            <li key={i} className={cell.done !== null ? 'done' : ''} title={cell.done !== null ? fmtStamp(t, cell.done) : undefined}>
              {challengeText(t, names, cell.challenge)}
            </li>
          ))}
        </ol>
        <p className="note">{t.bingoLines(c.bingo.lines)}</p>
      </section>

      <Milestones s={s} />
    </div>
  );
}
