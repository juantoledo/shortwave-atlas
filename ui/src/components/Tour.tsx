// The guided tour (tour/tour.ts): the window dimmed but for one part, its `data-tour` element,
// and a card beside it that says what it is. `onShow` brings that part into sight first (its
// view, unfolded); a part this layout doesn't show is passed over. Escape or Skip ends it, as
// Done does; the arrow keys step it; no other key reaches the window behind.

import { useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent } from 'react';
import { useT } from '../i18n';
import { holeOf, placeCard, stepTo, TOUR_STEPS, type Rect, type TourStep } from '../tour/tour';

interface Props {
  index: number;
  onShow: (step: TourStep) => void;
  onIndex: (i: number) => void;
  onClose: () => void;
}

const rectOf = (el: Element | null): Rect | null => {
  const r = el?.getBoundingClientRect();
  return r && r.width > 0 && r.height > 0 ? { left: r.left, top: r.top, width: r.width, height: r.height } : null;
};

export function Tour({ index, onShow, onIndex, onClose }: Props) {
  const t = useT();
  const id = useId();
  const n = TOUR_STEPS.length;
  const step = TOUR_STEPS[index];
  const [rect, setRect] = useState<Rect | null>(null);
  const [cardAt, setCardAt] = useState<CSSProperties>({ visibility: 'hidden' });
  const box = useRef<HTMLElement>(null);
  const next = useRef<HTMLButtonElement>(null);
  const dir = useRef<1 | -1>(1);

  const go = (d: 1 | -1) => {
    dir.current = d;
    const i = stepTo(index, n, d);
    if (i === null) onClose();
    else if (i !== index) onIndex(i);
  };

  // its part into sight; found once that has rendered, and followed while it moves
  useEffect(() => {
    onShow(step);
    let el: Element | null = null;
    const measure = () => setRect(rectOf(el));
    const ro = new ResizeObserver(measure);
    const raf = requestAnimationFrame(() => {
      el = document.querySelector(`[data-tour="${step.id}"]`);
      if (!rectOf(el)) {
        // not on this layout: the next one the same way (forward, from the first)
        return go(index === 0 ? 1 : dir.current);
      }
      measure();
      ro.observe(el!);
    });
    window.addEventListener('resize', measure);
    window.addEventListener('scroll', measure, true);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      window.removeEventListener('resize', measure);
      window.removeEventListener('scroll', measure, true);
    };
  }, [index]);

  // on Next once the card shows (a hidden one takes no focus)
  const placed = cardAt.visibility !== 'hidden';
  useEffect(() => { if (placed) next.current?.focus(); }, [index, placed]);

  // the card beside it, once its size is known
  useLayoutEffect(() => {
    if (!rect || !box.current) return setCardAt({ visibility: 'hidden' });
    const c = box.current.getBoundingClientRect();
    const p = placeCard(rect, { width: c.width, height: c.height }, window.innerWidth, window.innerHeight);
    setCardAt({ left: p.left, top: p.top });
  }, [rect, index, t]);

  // Tab stays on the card; Escape ends, the arrows step; nothing goes on to the window
  const onKeyDown = (e: KeyboardEvent) => {
    e.stopPropagation();
    if (e.key === 'Escape') { e.preventDefault(); return onClose(); }
    if (e.key === 'ArrowRight') { e.preventDefault(); return go(1); }
    if (e.key === 'ArrowLeft') { e.preventDefault(); return go(-1); }
    if (e.key !== 'Tab' || !box.current) return;
    const items = [...box.current.querySelectorAll<HTMLElement>('button')];
    const first = items[0], last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  };

  const hole = rect && holeOf(rect, window.innerWidth, window.innerHeight);
  const [title, body] = t.tour[step.id];
  const last = index === n - 1;
  return (
    <div className={'tour-layer' + (hole ? '' : ' dim')} onKeyDown={onKeyDown}>
      {hole && <div className="tour-hole" style={hole} />}
      <section className="tour-card" role="dialog" aria-modal="true" aria-labelledby={id} aria-describedby={id + 'b'} ref={box} style={cardAt}>
        <p className="tour-count" aria-live="polite">{t.tourStep(index + 1, n)}</p>
        <h2 id={id}>{title}</h2>
        <p id={id + 'b'}>{body}</p>
        <footer className="tour-foot">
          <button className="link" type="button" onClick={onClose}>{t.tourSkip}</button>
          <span className="tour-steps">
            {index > 0 && <button className="btn" type="button" onClick={() => go(-1)}>{t.tourBack}</button>}
            <button className="btn primary" type="button" ref={next} onClick={() => go(1)}>{last ? t.tourDone : t.tourNext}</button>
          </span>
        </footer>
      </section>
    </div>
  );
}
