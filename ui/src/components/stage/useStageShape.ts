// The part of the canvas left visible: the canvas (`stage`) may cover the whole window, behind the
// panels (styles.css, `data-globe="full"`), while the view (`view`, the stage's box) is what shows
// between them. Each view centres itself on `view` (`offset`) and sizes itself for it.

import { useEffect, useRef, useState } from 'react';

export interface StageShape {
  viewW: number;
  viewH: number;
  canvasW: number;
  canvasH: number;
}

export function useStageShape() {
  const stage = useRef<HTMLDivElement>(null);
  const view = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const shape = useRef<StageShape>({ viewW: 1, viewH: 1, canvasW: 1, canvasH: 1 });
  /** Moves the view's centre from the canvas's to the stage's. */
  const [offset, setOffset] = useState<[number, number]>([0, 0]);

  useEffect(() => {
    const el = stage.current!, v = view.current!;
    const ro = new ResizeObserver(() => {
      if (!el.clientWidth || !el.clientHeight || !v.clientWidth || !v.clientHeight) return;
      const c = el.getBoundingClientRect(), r = v.getBoundingClientRect();
      shape.current = { viewW: r.width, viewH: r.height, canvasW: c.width, canvasH: c.height };
      const dx = Math.round(r.left + r.width / 2 - (c.left + c.width / 2));
      const dy = Math.round(r.top + r.height / 2 - (c.top + c.height / 2));
      setOffset((o) => (o[0] === dx && o[1] === dy ? o : [dx, dy]));
      setSize({ w: el.clientWidth, h: el.clientHeight });
    });
    ro.observe(el);
    ro.observe(v);
    return () => ro.disconnect();
  }, []);

  return { stage, view, size, shape, offset };
}
