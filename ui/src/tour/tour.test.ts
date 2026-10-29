import { describe, expect, it } from 'vitest';
import { holeOf, placeCard, readSeen, stepTo, STEP_IDS, TOUR_STEPS } from './tour';

// every step's target, as the sources mark it (same reading as components/noNativeDialogs.test.ts)
const SOURCES = import.meta.glob(['../**/*.tsx'], { eager: true, query: '?raw', import: 'default' }) as Record<string, string>;

const CARD = { width: 300, height: 160 };
const VW = 1280, VH = 800;

describe('tour', () => {
  it('has one step per id, in order', () => {
    expect(TOUR_STEPS.map((s) => s.id)).toEqual([...STEP_IDS]);
  });

  it('every step has an element to show', () => {
    const all = Object.values(SOURCES).join('\n');
    const missing = STEP_IDS.filter((id) => !all.includes(`data-tour="${id}"`));
    expect(missing).toEqual([]);
  });

  it('steps forward to the end, and back to the first', () => {
    expect(stepTo(0, 8, 1)).toBe(1);
    expect(stepTo(6, 8, 1)).toBe(7);
    expect(stepTo(7, 8, 1)).toBeNull();
    expect(stepTo(3, 8, -1)).toBe(2);
    expect(stepTo(0, 8, -1)).toBe(0);
  });

  it('puts the card under the top bar, over the dock, beside a tall panel', () => {
    const top = placeCard({ left: 900, top: 0, width: 300, height: 50 }, CARD, VW, VH);
    expect(top.side).toBe('below');
    expect(top.top).toBe(62);
    const dock = placeCard({ left: 0, top: 600, width: 1280, height: 200 }, CARD, VW, VH);
    expect(dock.side).toBe('above');
    expect(dock.top).toBe(600 - 12 - 160);
    const side = placeCard({ left: 980, top: 60, width: 300, height: 700 }, CARD, VW, VH);
    expect(side).toEqual({ side: 'left', left: 980 - 12 - 300, top: 60 + 350 - 80 });
    const leftPanel = placeCard({ left: 0, top: 60, width: 300, height: 700 }, CARD, VW, VH);
    expect(leftPanel.side).toBe('right');
  });

  it('keeps the card on screen', () => {
    // under a button at the right edge: pulled back in
    const edge = placeCard({ left: 1250, top: 10, width: 30, height: 30 }, CARD, VW, VH);
    expect(edge.left).toBe(VW - 12 - 300);
    // a phone narrower than the card: at the margin
    const phone = placeCard({ left: 10, top: 10, width: 30, height: 30 }, CARD, 280, 600);
    expect(phone.left).toBe(12);
  });

  it('lays the card over the bottom of a target that fills the window', () => {
    const p = placeCard({ left: 0, top: 0, width: 400, height: 700 }, CARD, 400, 700);
    expect(p).toEqual({ side: 'inside', left: 50, top: 700 - 12 - 160 });
  });

  it('pads the spotlight, inside the window', () => {
    expect(holeOf({ left: 100, top: 100, width: 50, height: 20 }, VW, VH)).toEqual({ left: 94, top: 94, width: 62, height: 32 });
    expect(holeOf({ left: 0, top: 0, width: VW, height: VH }, VW, VH)).toEqual({ left: 0, top: 0, width: VW, height: VH });
  });

  it('reads the seen tips, dropping what it does not know', () => {
    expect(readSeen(['stats', 'nope', 'net'])).toEqual(['net', 'stats']);
    expect(readSeen([])).toEqual([]);
    expect(readSeen('stats')).toBeNull();
    expect(readSeen(null)).toBeNull();
  });
});
