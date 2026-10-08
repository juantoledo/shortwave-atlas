// Rig audio and the audio waterfall, in the desktop app and the remote browser.

import { useEffect, useRef, useState } from 'react';
import type { OpenAudio } from '../api/transport';
import { AudioPlayer, type PlayerStatus } from '../audio/player';
import { cwDelta, SP_H, Waterfall, WF_H, WF_W } from '../audio/waterfall';
import { useT, type Messages } from '../i18n';

const ROW_MS = 50;

function statusText(t: Messages, s: PlayerStatus | null): string {
  if (!s) return '';
  switch (s.kind) {
    case 'connecting': return t.connecting;
    case 'playing': return t.audioPlaying(s.bufferMs);
    case 'ended': return t.audioEnded;
    case 'error': return t.audioError(s.message);
  }
}

interface Props {
  openAudio: OpenAudio;
  rate: number;
  mode: string | null;
  cwPitch: number | null;
  freqHz: number | null;
  onTune: (hz: number) => void;
}

export function AudioPanel({ openAudio, rate, mode, cwPitch, freqHz, onTune }: Props) {
  const t = useT();
  const spans = [...new Set([3000, 4000, rate / 2])].filter((s) => s <= rate / 2);
  const [span, setSpan] = useState(spans[0]);
  const [volume, setVolume] = useState(0.8);
  const [status, setStatus] = useState<PlayerStatus | null>(null);
  const [player, setPlayer] = useState<AudioPlayer | null>(null);
  const [hover, setHover] = useState('');
  const wfRef = useRef<HTMLCanvasElement>(null), spRef = useRef<HTMLCanvasElement>(null), wrapRef = useRef<HTMLDivElement>(null);
  const spanRef = useRef(span);
  spanRef.current = span;

  useEffect(() => {
    if (!player) return;
    const wf = new Waterfall(wfRef.current!, spRef.current!, player.analyser);
    const id = setInterval(() => wf.tick(spanRef.current), ROW_MS);
    return () => clearInterval(id);
  }, [player]);
  useEffect(() => () => player?.stop(), [player]);
  useEffect(() => player?.setVolume(volume), [player, volume]);

  const start = () => setPlayer(new AudioPlayer(openAudio, volume, setStatus));
  const stop = () => { setPlayer(null); setStatus(null); };

  const isCW = (mode === 'CW' || mode === 'CWR') && !!cwPitch;
  const toneAt = (clientX: number) => {
    const r = wrapRef.current!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * span;
  };
  const deltaAt = (clientX: number) => cwDelta(toneAt(clientX), cwPitch!, mode!);

  return (
    <section className="sec" aria-label={t.audio}>
      <h2 className="h"><span>{t.audio}</span><span>{statusText(t, status)}</span></h2>
      <div className="audio-row">
        <button className="btn" type="button" disabled={!!player} onClick={start}>▶ {t.listen}</button>
        <button className="btn" type="button" disabled={!player} onClick={stop}>■ {t.stop}</button>
        <label className="vol">{t.volume}
          <input type="range" min={0} max={1} step={0.01} value={volume} onChange={(e) => setVolume(Number(e.currentTarget.value))} />
        </label>
        <label className="vol">{t.span}
          <select value={span} onChange={(e) => setSpan(Number(e.currentTarget.value))}>
            {spans.map((s) => <option key={s} value={s}>{s / 1000} kHz</option>)}
          </select>
        </label>
      </div>
      {player && (
        <>
          <div
            className="wf-wrap"
            ref={wrapRef}
            aria-label={t.waterfall}
            onMouseMove={(e) => {
              const f = Math.round(toneAt(e.clientX));
              const d = isCW ? deltaAt(e.clientX) : 0;
              setHover(`${f} Hz` + (isCW ? ' · ' + t.cwTuneHint(cwPitch!, (d >= 0 ? '+' : '') + d) : ''));
            }}
            onMouseLeave={() => setHover('')}
            onClick={(e) => {
              const d = isCW ? deltaAt(e.clientX) : 0;
              if (d && freqHz !== null) onTune(freqHz + d);
            }}
          >
            <canvas className="wf-sp" ref={spRef} width={WF_W} height={SP_H} />
            <canvas className="wf-wf" ref={wfRef} width={WF_W} height={WF_H} />
            {isCW && cwPitch! <= span && <div className="wf-mk" style={{ left: (cwPitch! / span) * 100 + '%' }} />}
          </div>
          <div className="axis">{[0, 1, 2, 3, 4].map((i) => <span key={i}>{Math.round((span * i) / 4)}{i === 4 ? ' Hz' : ''}</span>)}</div>
          <p className="note">{hover || t.waterfallNote}</p>
        </>
      )}
    </section>
  );
}
