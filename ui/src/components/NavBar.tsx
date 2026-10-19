// Switching views where the layout cannot show them all: tabs in the side panel's header
// (tablets, small windows) and a bottom tab bar on phones. CSS decides which one shows.
// A side panel's header also folds the panel to a rail (hooks/folds.ts); folded, the header is
// the rail, and its title or tabs open it again.

import { useT } from '../i18n';
import { FoldButton } from './FoldButton';

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
  /** Folded to a rail, and the button that folds it (none where the layout can't). */
  folded: boolean;
  onFold: () => void;
  /** The panel's id, which the fold button controls. */
  panelId: string;
}

/** A side panel's header: its title, or the Station/Stations tabs when the two share a column. */
export function PanelHead({ own, onView, aside, count, folded, onFold, panelId }: HeadProps) {
  const t = useT();
  const title = own === 'station' ? t.station : t.stations;
  const label = folded ? t.expandPanel(title) : t.collapsePanel(title);
  return (
    <header className="panel-head">
      <h2 className="h panel-title">
        {folded ? (
          <button className="panel-rail" type="button" aria-expanded={false} aria-controls={panelId} title={label} onClick={onFold}>
            <span>{title}</span>
            {count && <small>{count}</small>}
          </button>
        ) : (
          <span>{title}</span>
        )}
        {aside && <span className="panel-aside" aria-live="polite">{aside}</span>}
      </h2>
      <ViewTabs className="panel-tabs" views={['station', 'browse']} current={own} onView={onView} count={count} />
      {/* the list folds to the left, the station to the right (styles.css turns the list's below
          three columns, where it shares the right-hand column) */}
      <FoldButton className="panel-fold" edge={own === 'browse' ? 'left' : 'right'} expanded={!folded} label={label} onClick={onFold} controls={panelId} />
    </header>
  );
}
