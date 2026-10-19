// The NET faceplate's stream buffer drawn as a funnel lying on its side: the stream pours in at the
// wide mouth (left) and leaves for the speaker through the spout (right). Seconds held run on a
// log scale from 0.5 s at the outlet to 32 s at the mouth, so every doubling is as wide as the
// last: the spout (0.5–1 s), the sections of the sizes the listener can pick (1, 2, 4, 8, 16 s)
// and the mouth (16–32 s, a 16 s buffer's headroom). Paths are in a 100 × 1 viewBox, drawn
// stretched (`preserveAspectRatio="none"`), like the dial's marks.

import { NET_BUFFER_STEPS_MS } from './net';

/** Seconds held at the outlet and at the mouth. */
export const FUNNEL_LO_MS = 500;
export const FUNNEL_HI_MS = 32_000;
/** The viewBox's width. */
export const FUNNEL_W = 100;
/** Half the spout's height (the mouth's is 0.5). */
const SPOUT = 0.09;
/** Where the cone narrows into the spout: at 1 s. */
const THROAT = 1 - Math.log2(1000 / FUNNEL_LO_MS) / Math.log2(FUNNEL_HI_MS / FUNNEL_LO_MS);

const r = (v: number) => Math.round(v * 1000) / 1000;

/** Where `ms` held sits: 0 at the mouth (32 s and more), 1 at the outlet (0.5 s and less, or
 *  nothing). */
export function funnelX(ms: number): number {
  if (!(ms > 0)) return 1;
  const x = 1 - Math.log2(ms / FUNNEL_LO_MS) / Math.log2(FUNNEL_HI_MS / FUNNEL_LO_MS);
  return Math.min(1, Math.max(0, x));
}

/** The funnel's half-height at `x` (0..1): the cone from the mouth to the throat, then the spout. */
export function halfHeight(x: number): number {
  const c = Math.min(1, Math.max(0, x));
  return c >= THROAT ? SPOUT : 0.5 - ((0.5 - SPOUT) * c) / THROAT;
}

/** The funnel's outline. */
export function funnelOutline(): string {
  const t = r(THROAT * FUNNEL_W);
  return `M0 0 L${t} ${r(0.5 - SPOUT)} L${FUNNEL_W} ${r(0.5 - SPOUT)} L${FUNNEL_W} ${r(0.5 + SPOUT)} L${t} ${r(0.5 + SPOUT)} L0 1 Z`;
}

/** The audio held, filling the funnel from the outlet; '' when it is empty. */
export function funnelFill(heldMs: number): string {
  const x0 = funnelX(heldMs);
  if (x0 >= 1) return '';
  const xs = x0 < THROAT ? [x0, THROAT, 1] : [x0, 1];
  const top = xs.map((x) => `${r(x * FUNNEL_W)} ${r(0.5 - halfHeight(x))}`);
  const bottom = xs.reverse().map((x) => `${r(x * FUNNEL_W)} ${r(0.5 + halfHeight(x))}`);
  return `M${[...top, ...bottom].join(' L')} Z`;
}

/** A line across the funnel where `ms` held sits (a section's edge, the buffer's size, the most
 *  it holds), in viewBox units. */
export function funnelLine(ms: number): { x: number; y1: number; y2: number } {
  const x = funnelX(ms), h = halfHeight(x);
  return { x: r(x * FUNNEL_W), y1: r(0.5 - h), y2: r(0.5 + h) };
}

/** The sizes the listener can pick, where each one's section ends (a percentage of the width). */
export function funnelTiers(): { ms: number; pct: number }[] {
  return NET_BUFFER_STEPS_MS.map((ms) => ({ ms, pct: r(funnelX(ms) * 100) }));
}
