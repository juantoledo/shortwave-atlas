// The listener's cushion grows after a dropout, as the NET band's automatic jitter buffer does
// (`JitterBuffer` in atlas-core's net.rs): up a step each time the audio runs dry, never down, and
// back to the configured one on the next Listen. A steady link keeps the configured, low delay;
// a bursty one (a phone on Wi-Fi) settles where it stops cutting out.

import type { PlayerBuffer } from './player';

/** The cushion's steps above the configured one, in milliseconds. */
export const CUSHION_STEPS_MS = [250, 400, 600, 900, 1200] as const;

/** The buffer after a dropout: the next step above `live`'s cushion (the last stays), keeping the
 *  configured room above it (`base`), so a burst after a stall is not dropped. */
export function grown(base: PlayerBuffer, live: PlayerBuffer): PlayerBuffer {
  const cushionMs = CUSHION_STEPS_MS.find((s) => s > live.cushionMs) ?? Math.max(live.cushionMs, CUSHION_STEPS_MS[CUSHION_STEPS_MS.length - 1]);
  return { cushionMs, maxAheadMs: Math.max(live.maxAheadMs, cushionMs + (base.maxAheadMs - base.cushionMs)) };
}

/** `a` or `b`, whichever holds more. */
export const larger = (a: PlayerBuffer, b: PlayerBuffer): PlayerBuffer => ({
  cushionMs: Math.max(a.cushionMs, b.cushionMs),
  maxAheadMs: Math.max(a.maxAheadMs, b.maxAheadMs),
});
