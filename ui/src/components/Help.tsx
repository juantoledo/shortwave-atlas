// Help: an index of topics, each opening its own page. Topics are `HELP_TOPICS` (i18n/en.ts)
// with their texts in `helpText`; a topic can add more below its text here (Getting started:
// the tour and the tips).

import { useEffect, useState } from 'react';
import type { Api } from '../api/client';
import { COARSE, useMedia } from '../hooks/useMedia';
import { useT } from '../i18n';
import { HELP_TOPICS, type HelpTopic } from '../i18n/en';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { NoteBlock } from '../types/generated/NoteBlock';
import type { Release } from '../types/generated/Release';
import { DataNotice } from './DataNotice';
import { ReleaseNotes } from './ReleaseNotes';
import { Sheet } from './Sheet';
import { scheduleOf } from './sources';

interface Props {
  topic: HelpTopic | null;
  onTopic: (t: HelpTopic | null) => void;
  version: string | null;
  meta: CatalogMeta | null;
  api: Api | null;
  /** The release an update offers, if any. */
  offered: Release | null;
  /** Getting started: take the tour (closing Help), show the tips again. */
  onTour: () => void;
  onTipsReset: () => void;
  onClose: () => void;
}

type ExtraProps = Pick<Props, 'version' | 'meta' | 'api' | 'offered' | 'onTour' | 'onTipsReset'> & { topic: HelpTopic };

/** This build's notes, then the offered update's. */
function Notes({ api, version, offered }: Pick<Props, 'api' | 'version' | 'offered'>) {
  const t = useT();
  const [blocks, setBlocks] = useState<NoteBlock[] | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (!api) return;
    let live = true;
    api.releaseNotes().then((b) => live && setBlocks(b), () => live && setFailed(true));
    return () => { live = false; };
  }, [api]);
  return (
    <>
      <section className="help-notes">
        {version && <h2 className="h"><span>{t.notesThisVersion(version)}</span></h2>}
        {blocks && blocks.length > 0 ? <ReleaseNotes blocks={blocks} full />
          : <p className="note">{blocks ? t.notesNone : failed ? t.notesUnavailable : t.notesLoading}</p>}
      </section>
      {offered && (
        <section className="help-notes">
          <h2 className="h"><span>{t.updateTitle(offered.version)}</span></h2>
          {offered.notes.length > 0 ? <ReleaseNotes blocks={offered.notes} full /> : <p className="note">{t.notesNone}</p>}
        </section>
      )}
    </>
  );
}

function Extra({ topic, version, meta, api, offered, onTour, onTipsReset }: ExtraProps) {
  const t = useT();
  const touch = useMedia(COARSE);
  const [reset, setReset] = useState(false);
  if (topic === 'notes') return <Notes api={api} version={version} offered={offered} />;
  if (topic === 'start') {
    return (
      <>
      <div className="actions">
        <button className="btn primary" type="button" onClick={onTour}>{t.helpTour}</button>
        <button className="btn" type="button" disabled={reset} onClick={() => { onTipsReset(); setReset(true); }}>{reset ? t.helpTipsBack : t.helpTipsReset}</button>
      </div>
      <ul className="legend-list">
        <li><i className="lg band k-broadcast" />{t.legendBroadcast}</li>
        <li><i className="lg band k-ham" />{t.legendHam}</li>
        <li><i className="lg band k-utility" />{t.legendCb}</li>
        <li><i className="lg band gap" />{t.legendGap}</li>
      </ul>
      </>
    );
  }
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
          <li><i className="lg logged" />{t.legendLogged}</li>
          <li><i className="lg mine" />{t.legendMine}</li>
        </ul>
      </>
    );
  }
  if (topic === 'about') {
    const season = scheduleOf(meta)?.season ?? '';
    return (
      <>
        <p className="help-app">{t.appName} {version && <span className="ver">{t.versionTag(version)}</span>}</p>
        <p className="help-by"><a className="link" href="https://cd3dxz.radio" target="_blank" rel="noreferrer">{t.byAuthor}</a></p>
        <p>{t.aboutBlurb}</p>
        <p>{t.receiveOnly}</p>
        {meta && <p>{t.listFoot(season)}</p>}
        <DataNotice />
        <section className="data-notice net-notice">
          <h2 className="h"><span>{t.netNoticeTitle}</span></h2>
          <p>
            {t.netNoticeSource}{' '}
            <a className="link" href="https://www.radio-browser.info" target="_blank" rel="noreferrer">radio-browser.info</a>
          </p>
          <p>{t.netNoticeAsIs}</p>
          <h2 className="h"><span>{t.netNoticeLegalTitle}</span></h2>
          <p>{t.netNoticeLegal}</p>
        </section>
        <p><a className="link" href="https://github.com/juantoledo/shortwave-atlas" target="_blank" rel="noreferrer">{t.sourceCode}</a></p>
        <p className="note">{t.thirdParty}</p>
      </>
    );
  }
  return null;
}

export function Help({ topic, onTopic, version, meta, api, offered, onTour, onTipsReset, onClose }: Props) {
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
          <Extra topic={topic} version={version} meta={meta} api={api} offered={offered} onTour={onTour} onTipsReset={onTipsReset} />
        </article>
      )}
    </Sheet>
  );
}
