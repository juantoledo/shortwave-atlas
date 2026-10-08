// Rig audio: enable it, pick the sound card and the rate; live capture status and hints.

import { useEffect, useState } from 'react';
import type { Api } from '../../api/client';
import { useAudioSettings } from '../../hooks/useAudioSettings';
import { audioHintText, useT, type Messages } from '../../i18n';
import type { AudioChoice } from '../../types/generated/AudioChoice';
import type { AudioDiagnostics } from '../../types/generated/AudioDiagnostics';
import type { Os } from '../../types/generated/Os';
import type { SoundCard } from '../../types/generated/SoundCard';

const RATES = [8000, 12000, 16000, 24000, 48000];
const OTHER = '__other';
/** How a capture device is called on each OS (ALSA name, DirectShow / AVFoundation name). */
const DEVICE_EXAMPLE: Record<Os, string> = {
  linux: 'plughw:CARD=CODEC,DEV=0',
  windows: 'Microphone (USB AUDIO  CODEC)',
  macos: 'USB AUDIO  CODEC',
};

/** "ffmpeg version 6.1.1-3ubuntu5 Copyright ..." -> "6.1.1-3ubuntu5" */
export const ffmpegVersion = (line: string) => /version\s+(\S+)/.exec(line)?.[1] ?? line;

/** The rig codec marker goes first, so a narrow select doesn't cut it off. */
export const cardLabel = (c: SoundCard, rigCodec: string) => (c.rig_codec ? `${rigCodec} · ${c.label}` : c.label);

function Status({ t, os, diag }: { t: Messages; os: Os; diag: AudioDiagnostics | null }) {
  if (!diag) return null;
  const s = diag.status;
  const line = !s.enabled ? t.audioOff : s.running ? t.audioCapturing(s.listeners) : t.audioIdle;
  return (
    <div className="st-box" aria-live="polite">
      <h3 className="h"><span>{t.status}</span></h3>
      <p className={'st-line' + (s.running ? ' st-ok' : s.last_error || s.spawn_error ? ' st-warn' : '')}>{line}</p>
      {(s.last_error || s.spawn_error) && <p className="note">{t.lastError(s.last_error ?? s.spawn_error ?? '')}</p>}
      {diag.hints.map((h) => (
        <div className="hint" key={h} role="alert">
          <b>{audioHintText(t, os, h)[0]}</b>
          <span>{audioHintText(t, os, h)[1]}</span>
        </div>
      ))}
      {s.log.length > 0 && (
        <details className="log">
          <summary>{t.ffmpegLog}</summary>
          <pre>{s.log.join('\n')}</pre>
        </details>
      )}
    </div>
  );
}

interface Props {
  api: Api;
  os: Os;
  onError: (e: unknown) => void;
  onApplied: () => void;
}

export function AudioSection({ api, os, onError, onApplied }: Props) {
  const t = useT();
  const { settings, cards, diag, reload, refreshCards } = useAudioSettings(api);
  const [draft, setDraft] = useState<AudioChoice | null>(null);
  const [other, setOther] = useState(false);
  const [busy, setBusy] = useState(false);

  // start from the saved choice
  useEffect(() => {
    if (!settings) return;
    setDraft(settings.choice);
    setOther(false);
  }, [settings]);

  if (!settings || !draft) return null;

  const set = (patch: Partial<AudioChoice>) => setDraft({ ...draft, ...patch });
  const known = cards.some((c) => c.device === draft.device);
  const cardValue = other ? OTHER : draft.device;
  const rates = RATES.includes(draft.rate) ? RATES : [...RATES, draft.rate].sort((a, b) => a - b);

  const apply = async () => {
    setBusy(true);
    try {
      await api.applyAudio(draft);
      await reload();
      onApplied();
    } catch (e) {
      onError(e);
    }
    setBusy(false);
  };

  return (
    <section className="sec form" aria-label={t.audioSection}>
      <h2 className="h"><span>{t.audioSection}</span>{settings.ffmpeg_version && <span>{t.ffmpegVersion(ffmpegVersion(settings.ffmpeg_version))}</span>}</h2>

      <div className="seg" role="radiogroup" aria-label={t.audioSection}>
        {[false, true].map((on) => (
          <button key={String(on)} type="button" role="radio" aria-checked={draft.enabled === on} onClick={() => set({ enabled: on })}>
            {on ? t.audioOn : t.audioOffLabel}
          </button>
        ))}
      </div>
      <p className="note">{t.audioHelp}</p>
      {settings.locked.length > 0 && <p className="note warn">{t.lockedByEnv(settings.locked.join(', '))}</p>}
      {!settings.ffmpeg_version && <p className="note warn">{t.ffmpegMissing}</p>}

      {draft.enabled && (
        <>
          <label className="field">
            <span>{t.soundCard}</span>
            <span className="row-inline">
              <select
                value={cardValue}
                onChange={(e) => {
                  const v = e.currentTarget.value;
                  setOther(v === OTHER);
                  if (v !== OTHER) set({ device: v });
                }}
              >
                {draft.device && !known && <option value={draft.device}>{draft.device} ({t.notFound})</option>}
                {cards.map((c) => <option key={c.device} value={c.device}>{cardLabel(c, t.rigCodec)}</option>)}
                <option value={OTHER}>{t.otherCard}</option>
              </select>
              <button className="btn" type="button" onClick={refreshCards}>{t.refresh}</button>
            </span>
            <small>{cards.length === 0 ? t.noCards : draft.device}</small>
          </label>
          {other && (
            <label className="field">
              <span>{t.alsaDevice}</span>
              <input value={draft.device} placeholder={DEVICE_EXAMPLE[os]} onChange={(e) => set({ device: e.currentTarget.value })} />
            </label>
          )}

          <label className="field">
            <span>{t.audioRate}</span>
            <select value={draft.rate} onChange={(e) => set({ rate: Number(e.currentTarget.value) })}>
              {rates.map((r) => <option key={r} value={r}>{r / 1000} kHz</option>)}
            </select>
            <small>{t.audioRateHelp(draft.rate / 2000, (draft.rate * 16) / 1000)}</small>
          </label>
        </>
      )}

      <div className="actions">
        <button className="btn primary" type="button" disabled={busy} onClick={apply}>{t.apply}</button>
      </div>
      {settings.config_path && <p className="note">{t.savedTo(settings.config_path)}</p>}

      <Status t={t} os={os} diag={diag} />
    </section>
  );
}
