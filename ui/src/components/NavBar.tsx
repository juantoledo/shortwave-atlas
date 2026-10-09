// Switching views where the layout cannot show them all: tabs in the side panel's header
// (tablets, small windows) and a bottom tab bar on phones. CSS decides which one shows.

import { useT } from '../i18n';

export type View = 'globe' | 'station' | 'browse';

interface TabsProps {
  className: string;
  views: View[];
  current: View;
  onView: (v: View) => void;
  /** Shown small next to "Stations". */
  count?: string;
}

export function ViewTabs({ className, views, current, onView, count }: TabsProps) {
  const t = useT();
  const label = { globe: t.tabGlobe, station: t.station, browse: t.stations };
  return (
    <div className={'tabs ' + className} role="tablist" aria-label={t.views}>
      {views.map((v) => (
        <button key={v} type="button" role="tab" aria-selected={current === v} onClick={() => onView(v)}>
          {label[v]}
          {v === 'browse' && count && <small>{count}</small>}
        </button>
      ))}
    </div>
  );
}

interface HeadProps {
  own: 'station' | 'browse';
  onView: (v: View) => void;
  /** Right of the title (the result count). */
  aside?: string;
  count?: string;
}

/** A side panel's header: its title, or the Station/Stations tabs when the two share a column. */
export function PanelHead({ own, onView, aside, count }: HeadProps) {
  const t = useT();
  return (
    <header className="panel-head">
      <h2 className="h panel-title">
        <span>{own === 'station' ? t.station : t.stations}</span>
        {aside && <span aria-live="polite">{aside}</span>}
      </h2>
      <ViewTabs className="panel-tabs" views={['station', 'browse']} current={own} onView={onView} count={count} />
    </header>
  );
}
