// The favorite star: ☆ keeps the station or frequency; ★ opens a popover under it to rename the
// favorite or remove it (no question asked: ☆ brings it back).

import { useEffect, useId, useRef, useState, type RefObject } from 'react';
import { useDismiss } from '../hooks/useDismiss';
import { useT } from '../i18n';
import type { Favorite } from '../types/generated/Favorite';

const MAX_LABEL = 60;

interface Props {
  /** The favorite already kept here, if any. */
  fav: Favorite | null;
  disabled?: boolean;
  onAdd: () => void;
  onRename: (label: string) => void;
  onRemove: () => void;
}

export function FavStar({ fav, disabled, onAdd, onRename, onRemove }: Props) {
  const t = useT();
  const wrap = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  useEffect(() => { if (!fav) setOpen(false); }, [fav]);
  return (
    <span className="fav-wrap" ref={wrap}>
      <button
        className="fav-star"
        type="button"
        aria-pressed={fav !== null}
        aria-expanded={fav ? open : undefined}
        aria-label={fav ? t.favEdit : t.favAdd}
        title={fav ? `${t.favEdit}${fav.label && ` · ${fav.label}`}` : t.favAdd}
        disabled={disabled}
        onClick={() => (fav ? setOpen((o) => !o) : onAdd())}
      >
        {fav ? '★' : '☆'}
      </button>
      {open && fav && (
        <Edit
          key={fav.id}
          fav={fav}
          anchor={wrap}
          onSave={(label) => { setOpen(false); if (label !== fav.label) onRename(label); }}
          onRemove={() => { setOpen(false); onRemove(); }}
          onCancel={() => setOpen(false)}
        />
      )}
    </span>
  );
}

interface EditProps {
  fav: Favorite;
  anchor: RefObject<HTMLElement | null>;
  onSave: (label: string) => void;
  onRemove: () => void;
  onCancel: () => void;
}

function Edit({ fav, anchor, onSave, onRemove, onCancel }: EditProps) {
  const t = useT();
  const id = useId();
  const [label, setLabel] = useState(fav.label);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    input.current?.select();
    return () => opener?.focus();
  }, []);
  useDismiss(anchor, onCancel);

  return (
    <section className="confirm-pop fav-pop" role="dialog" aria-labelledby={id}>
      <form onSubmit={(e) => { e.preventDefault(); onSave(label.trim()); }}>
        <label className="fav-label">
          <span id={id}>{t.favLabel}</span>
          <input ref={input} type="text" value={label} maxLength={MAX_LABEL} onChange={(e) => setLabel(e.currentTarget.value)} />
        </label>
        <div className="actions">
          <button className="btn primary" type="submit">{t.favSave}</button>
          <button className="btn" type="button" onClick={onRemove}>{t.favRemove}</button>
          <button className="btn" type="button" onClick={onCancel}>{t.cancel}</button>
        </div>
      </form>
    </section>
  );
}
