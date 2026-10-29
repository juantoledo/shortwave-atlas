// Rig audio and the audio waterfall, in the desktop app and the remote browser: the dock's
// right-hand cell. The waterfall box is always there (empty until you listen), so the dock
// does not jump; `audio-x` parts fold away while the cell is folded (hooks/folds.ts; its chevron unfolds it).
// The same stream carries an internet station instead of the rig while the receiver is tuned to
// one (the NET band): Listen plays whatever it is tuned to. With the rig's audio off it offers
// Listen while an internet station is tuned, and invites to set the rig's audio up otherwise.

import { useEffect, useRef, useState, type RefObject } from 'react';
import type { OpenAudio } from '../api/transport';
import { AudioPlayer, type PlayerBuffer, type PlayerStatus } from '../audio/player';
import { cwDelta, SP_H, Waterfall, WF_H, WF_W } from '../audio/waterfall';
import { useT, type Messages } from '../i18n';
import type { NetNow } from '../types/generated/NetNow';
import { FoldButton } from './FoldButton';
import { bufferPct, canListen } from './net';

const ROW_MS = 50;

/** What the keyboard can do to the audio (App's shortcuts). */
export interface AudioKeys {
  toggle: () => void;
  /** Start listening if it isn't and there is something to hear; true if it is now. */
  listen: () => boolean;
  mute: () => void;
  volume: (d: number) => void;
}

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
  buffer: PlayerBuffer;
  mode: string | null;
  cwPitch: number | null;
  freqHz: number | null;
  onTune: (hz: number) => void;
  keys?: RefObject<AudioKeys | null>;
  /** The rig's audio is set up. */
  rigAudio: boolean;
  /** The internet station tuned instead of the rig, if any. */
  net: NetNow | null;
  onSetUp: () => void;
  /** Listening started or stopped (the map's point beats meanwhile). */
  onListening: (on: boolean) => void;
  /** Unfolded: the volume, the span and the waterfall show. */
  expanded: boolean;
  onExpand: () => void;
}

/** What the panel says about the internet station: connecting, or why it stopped. */
function netLine(t: Messages, n: NetNow): string {
  switch (n.state.state) {
    case 'tuned':
    case 'playing': return '';
    case 'connecting': return t.netConnecting;
    case 'buffering': return t.netBuffering(bufferPct(n));
    case 'failed': return t.netFailed(n.state.message);
  }
}

export function AudioPanel({ openAudio, rate, buffer, mode, cwPitch, freqHz, onTune, keys, rigAudio, net, onSetUp, onListening, expanded, onExpand }: Props) {
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
    // drawn only while it shows (it folds away, or scrolls out of sight)
    let shown = true;
    const io = typeof IntersectionObserver === 'undefined' ? null
      : new IntersectionObserver(([e]) => { shown = e.isIntersecting; });
    io?.observe(wfRef.current!);
    const id = setInterval(() => { if (shown) wf.tick(spanRef.current); }, ROW_MS);
    return () => {
      clearInterval(id);
      io?.disconnect();
      for (const c of [wfRef.current, spRef.current]) c?.getContext('2d')?.clearRect(0, 0, c.width, c.height);
    };
  }, [player]);
  useEffect(() => () => player?.stop(), [player]);
  useEffect(() => {
    onListening(player !== null);
    return () => onListening(false);
  }, [player, onListening]);
  useEffect(() => player?.setVolume(volume), [player, volume]);
  const { cushionMs, maxAheadMs } = buffer;
  useEffect(() => player?.setBuffer({ cushionMs, maxAheadMs }), [player, cushionMs, maxAheadMs]);

  const listenable = canListen(rigAudio, net);
  const toggle = () => {
    if (player) {
      setPlayer(null);
      setStatus(null);
      setHover('');
    } else if (listenable) {
      setPlayer(new AudioPlayer(openAudio, volume, buffer, setStatus));
    }
  };
  // nothing left to hear (the internet station stopped, the rig's audio is off): stop listening
  useEffect(() => {
    if (!listenable && player) {
      setPlayer(null);
      setStatus(null);
    }
  }, [listenable, player]);
  // mute remembers the level it left
  const unmuted = useRef(volume);
  const mute = () => {
    if (volume > 0) {
      unmuted.current = volume;
      setVolume(0);
    } else setVolume(unmuted.current || 0.8);
  };
  useEffect(() => () => { if (keys) keys.current = null; }, [keys]);
  const listen = () => {
    if (player) return true;
    if (!listenable) return false;
    toggle();
    return true;
  };
  if (keys) keys.current = { toggle, listen, mute, volume: (d) => setVolume((v) => Math.round(Math.min(1, Math.max(0, v + d)) * 100) / 100) };

  const isCW = !net && (mode === 'CW' || mode === 'CWR') && !!cwPitch;
  const toneAt = (clientX: number) => {
    const r = wrapRef.current!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * span;
  };
  const deltaAt = (clientX: number) => cwDelta(toneAt(clientX), cwPitch!, mode!);
  const describe = (clientX: number) => {
    const d = isCW ? deltaAt(clientX) : 0;
    setHover(`${Math.round(toneAt(clientX))} Hz` + (isCW ? ' · ' + t.cwTuneHint(cwPitch!, (d >= 0 ? '+' : '') + d) : ''));
  };

  if (!listenable && !player) return <AudioOff onSetUp={onSetUp} />;
  const netNote = net && netLine(t, net);
  return (
    <section className={'audio' + (net ? ' net' : '')} data-tour="audio" aria-label={t.audio}>
      <div className="audio-row">
        <button className={'btn' + (player ? ' primary' : '')} type="button" aria-pressed={!!player} title={t.keyHint(player ? t.stop : t.listen, t.keySpace)} onClick={toggle}>
          {player ? `■ ${t.stop}` : `▶ ${t.listen}`}
        </button>
        <label className="vol audio-x">{t.volume}
          <input type="range" min={0} max={1} step={0.01} value={volume} onChange={(e) => setVolume(Number(e.currentTarget.value))} />
        </label>
        <label className="vol audio-x">{t.span}
          <select value={span} onChange={(e) => setSpan(Number(e.currentTarget.value))}>
            {spans.map((s) => <option key={s} value={s}>{s / 1000} kHz</option>)}
          </select>
        </label>
        <FoldButton className="dock-more" edge="bottom" expanded={expanded} label={expanded ? t.lessAudio : t.moreAudio} onClick={onExpand} />
        <span className="audio-st">{netNote || statusText(t, status)}</span>
      </div>
      <div
        className={'wf-wrap audio-x' + (player ? '' : ' idle')}
        ref={wrapRef}
        aria-label={t.waterfall}
        onMouseMove={(e) => player && describe(e.clientX)}
        onMouseLeave={() => setHover('')}
        onClick={(e) => {
          if (!player) return;
          describe(e.clientX);
          const d = isCW ? deltaAt(e.clientX) : 0;
          if (d && freqHz !== null) onTune(freqHz + d);
        }}
      >
        <canvas className="wf-sp" ref={spRef} width={WF_W} height={SP_H} />
        <canvas className="wf-wf" ref={wfRef} width={WF_W} height={WF_H} />
        {player && isCW && cwPitch! <= span && <div className="wf-mk" style={{ left: (cwPitch! / span) * 100 + '%' }} />}
        <p className="wf-note">{player ? hover : t.waterfallNote}</p>
      </div>
      <div className="axis audio-x" aria-hidden="true">{[0, 1, 2, 3, 4].map((i) => <span key={i}>{Math.round((span * i) / 4)}{i === 4 ? ' Hz' : ''}</span>)}</div>
    </section>
  );
}

/** The audio cell when the rig audio is not set up. */
export function AudioOff({ onSetUp }: { onSetUp: () => void }) {
  const t = useT();
  return (
    <section className="audio off" data-tour="audio" aria-label={t.audio}>
      <h2 className="h"><span>{t.audio}</span></h2>
      <p className="note">{t.audioOff}</p>
      <p><button className="link" type="button" onClick={onSetUp}>{t.audioSetUp} →</button></p>
    </section>
  );
}
