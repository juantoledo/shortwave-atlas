// Listen on the phone remote: the same player as the atlas's audio panel, without the waterfall.
// `start` must run inside the tap that asks for it (phones only let a page play sound from a
// gesture). Listening stops when there is nothing left to hear: the rig's audio turned off, or the
// internet station stopped. Started before there was anything (a tap on an internet station whose
// tune is on its way), it waits for it.
// A phone is on Wi-Fi and in a pocket: the cushion starts at `PHONE_CUSHION_MS` at least (and grows
// after a dropout, cushion.ts), the output buffers more, and it plays through a media element so
// it goes on with the screen off.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { OpenAudio } from '../api/transport';
import { larger } from '../audio/cushion';
import { AudioPlayer, type PlayerBuffer, type PlayerOptions, type PlayerStatus } from '../audio/player';

/** The least cushion a phone starts with, in milliseconds. */
export const PHONE_CUSHION_MS = 250;
const PHONE: PlayerOptions = { latencyHint: 'playback', background: true };

export function useListen(open: OpenAudio | null, configured: PlayerBuffer, can: boolean) {
  // the same room above the cushion as configured
  const room = configured.maxAheadMs - configured.cushionMs;
  const buffer = useMemo(
    () => larger(configured, { cushionMs: PHONE_CUSHION_MS, maxAheadMs: PHONE_CUSHION_MS + room }),
    [configured, room],
  );
  const player = useRef<AudioPlayer | null>(null);
  const [playing, setPlaying] = useState(false);
  const [status, setStatus] = useState<PlayerStatus | null>(null);
  const [volume, setVolume] = useState(0.8);
  const vol = useRef(volume);
  vol.current = volume;

  const stop = useCallback(() => {
    player.current?.stop();
    player.current = null;
    setPlaying(false);
    setStatus(null);
  }, []);
  const start = useCallback(() => {
    if (player.current || !open) return;
    player.current = new AudioPlayer(open, vol.current, buffer, setStatus, PHONE);
    setPlaying(true);
  }, [open, buffer]);
  const toggle = () => (player.current ? stop() : start());

  useEffect(() => stop, [stop]);
  useEffect(() => player.current?.setVolume(volume), [volume, playing]);
  useEffect(() => player.current?.setBuffer(buffer), [buffer, playing]);
  // what there was to hear went away
  const could = useRef(can);
  useEffect(() => {
    if (could.current && !can) stop();
    could.current = can;
  }, [can, stop]);

  return { playing, status, volume, setVolume, start, stop, toggle };
}

export type Listen = ReturnType<typeof useListen>;
