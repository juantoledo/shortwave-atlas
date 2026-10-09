// Help: an index of topics, each opening its own page. Topics are `HELP_TOPICS` (i18n/en.ts)
// with their texts in `helpText`; a topic can add more below its text here.

import { COARSE, useMedia } from '../hooks/useMedia';
import { useT } from '../i18n';
import { HELP_TOPICS, type HelpTopic } from '../i18n/en';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import { Sheet } from './Sheet';

interface Props {
  topic: HelpTopic | null;
  onTopic: (t: HelpTopic | null) => void;
  version: string | null;
  meta: CatalogMeta | null;
  onClose: () => void;
}

function Extra({ topic, version, meta }: { topic: HelpTopic; version: string | null; meta: CatalogMeta | null }) {
  const t = useT();
  const touch = useMedia(COARSE);
  if (topic === 'globe') {
    return (
      <>
        <ul className="help-list">
          <li>{touch ? t.dragToRotateTouch : t.dragToRotate}</li>
          <li>{touch ? t.tapForQth : t.clickForQth}</li>
          <li>{touch ? t.tapSite : t.clickSite}</li>
        </ul>
        <ul className="legend-list">
          <li><i className="lg on" />{t.legendOnAir}</li>
          <li><i className="lg quiet" />{t.legendQuiet}</li>
          <li><i className="lg qth" />{t.legendQth}</li>
        </ul>
      </>
    );
  }
  if (topic === 'about') {
    const season = meta?.source.season ?? '';
    return (
      <>
        <p className="help-app">{t.appName} {version && <span className="ver">{t.versionTag(version)}</span>}</p>
        <p className="help-by"><a className="link" href="https://cd3dxz.radio" target="_blank" rel="noreferrer">{t.byAuthor}</a></p>
        <p>{t.aboutBlurb}</p>
        <p>{t.receiveOnly}</p>
        {meta && <p>{t.listFoot(season)}</p>}
        <p><a className="link" href="https://github.com/juantoledo/shortwave-atlas" target="_blank" rel="noreferrer">{t.sourceCode}</a></p>
        <p className="note">{t.thirdParty}</p>
      </>
    );
  }
  return null;
}

export function Help({ topic, onTopic, version, meta, onClose }: Props) {
  const t = useT();
  return (
    <Sheet title={t.help} onClose={onClose}>
      {topic === null ? (
        <nav aria-label={t.helpTopics}>
          <h2 className="h"><span>{t.helpTopics}</span></h2>
          <ul className="help-index">
            {HELP_TOPICS.map((id) => (
              <li key={id}>
                <button type="button" onClick={() => onTopic(id)}>
                  <b>{t.helpText[id][0]}</b>
                  <span>{t.helpText[id][1]}</span>
                </button>
              </li>
            ))}
          </ul>
        </nav>
      ) : (
        <article className="help-page" key={topic}>
          <button className="link" type="button" onClick={() => onTopic(null)}>← {t.helpTopics}</button>
          <h2>{t.helpText[topic][0]}</h2>
          <p>{t.helpText[topic][1]}</p>
          <Extra topic={topic} version={version} meta={meta} />
        </article>
      )}
    </Sheet>
  );
}
