// Data for the audio settings section: current choice, sound cards, and live capture
// status (polled only while the page is open).

import { useCallback, useEffect, useState } from 'react';
import type { Api } from '../api/client';
import type { AudioDiagnostics } from '../types/generated/AudioDiagnostics';
import type { AudioSettings } from '../types/generated/AudioSettings';
import type { SoundCard } from '../types/generated/SoundCard';

const DIAG_MS = 1000;

export function useAudioSettings(api: Api) {
  const [settings, setSettings] = useState<AudioSettings | null>(null);
  const [cards, setCards] = useState<SoundCard[]>([]);
  const [diag, setDiag] = useState<AudioDiagnostics | null>(null);

  const reload = useCallback(() => api.audioSettings().then(setSettings, () => {}), [api]);
  const refreshCards = useCallback(() => api.soundCards().then(setCards, () => {}), [api]);

  useEffect(() => {
    reload();
    refreshCards();
  }, [reload, refreshCards]);

  useEffect(() => {
    let alive = true;
    const tick = () => api.audioDiagnostics().then((d) => { if (alive) setDiag(d); }, () => {});
    tick();
    const id = setInterval(tick, DIAG_MS);
    return () => { alive = false; clearInterval(id); };
  }, [api]);

  return { settings, cards, diag, reload, refreshCards };
}
