// Where the station data comes from and how far to trust it: in the welcome splash and in
// Help › About.

import { useT } from '../i18n';

export function DataNotice() {
  const t = useT();
  return (
    <section className="data-notice">
      <h2 className="h"><span>{t.dataNoticeTitle}</span></h2>
      <p>{t.dataNoticeFrom}</p>
      <p>{t.dataNoticeAsIs}</p>
      <ul>
        {t.dataNoticeItems.map((s) => <li key={s}>{s}</li>)}
      </ul>
      <p>{t.dataNoticeId}</p>
      <p>{t.dataNoticeMore}</p>
    </section>
  );
}
