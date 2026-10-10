// The tuning knob's geometry: pointer angle -> detents, wheel -> detents. Like a rig's dual
// concentric knob: the outer ring turns fast, the inner one slow; clockwise tunes up.

/** One detent every 10° (36 per turn). */
export const DETENT_DEG = 10;
/** What one detent tunes, outer ring and inner knob (as Shift+Arrow and Arrow). */
export const COARSE_HZ = 5000, FINE_HZ = 1000;
/** Trackpad scroll per detent; a wheel notch (a bigger delta) is one detent whatever its size. */
export const WHEEL_PX = 40;

/** Degrees clockwise from 12 o'clock of (x, y) around (cx, cy), in [0, 360). */
export function pointerDeg(cx: number, cy: number, x: number, y: number): number {
  const d = (Math.atan2(x - cx, cy - y) * 180) / Math.PI;
  return d < 0 ? d + 360 : d;
}

/** The shortest turn from `from` to `to`, in (-180, 180]. */
export function angleDelta(from: number, to: number): number {
  const d = (((to - from) % 360) + 360) % 360;
  return d > 180 ? d - 360 : d;
}

/** Whole detents in a turn so far (`acc` is what the last call left over), and what is left. */
export function detents(acc: number, deltaDeg: number): { clicks: number; rest: number } {
  const total = acc + deltaDeg;
  const clicks = Math.trunc(total / DETENT_DEG);
  return { clicks, rest: total - clicks * DETENT_DEG };
}

/** Where `clicks` detents of `step` Hz take `hz`. Off the step grid (the rig's own knob left
 *  odd hertz), the first detent lands on the grid line that way: 13 616 370 goes to 13 617 000
 *  up or 13 616 000 down in 1 kHz steps. */
export function stepHz(hz: number, clicks: number, step: number): number {
  if (clicks === 0) return hz;
  const from = clicks > 0 ? Math.floor(hz / step) : Math.ceil(hz / step);
  return (from + clicks) * step;
}

/** Detents for a wheel event (scrolling up tunes up): one per notch, trackpad pixels add up. */
export function wheelDetents(acc: number, deltaY: number, deltaMode: number): { clicks: number; rest: number } {
  if (deltaY === 0) return { clicks: 0, rest: acc };
  if (deltaMode !== 0 || Math.abs(deltaY) >= WHEEL_PX) return { clicks: deltaY < 0 ? 1 : -1, rest: 0 };
  const total = acc - deltaY;
  const clicks = Math.trunc(total / WHEEL_PX);
  return { clicks, rest: total - clicks * WHEEL_PX };
}
