import { describe, expect, it } from 'vitest';
import { cardLabel, ffmpegVersion } from './AudioSection';

describe('audio settings', () => {
  it('marks the rig codec', () => {
    const codec = { device: 'plughw:CARD=CODEC,DEV=0', label: 'USB AUDIO CODEC · USB Audio', card_id: 'CODEC', rig_codec: true };
    expect(cardLabel(codec, 'rig codec')).toBe('rig codec · USB AUDIO CODEC · USB Audio');
    expect(cardLabel({ ...codec, rig_codec: false }, 'rig codec')).toBe('USB AUDIO CODEC · USB Audio');
  });

  it('shortens the ffmpeg version line', () => {
    expect(ffmpegVersion('ffmpeg version 6.1.1-3ubuntu5 Copyright (c) 2000-2023 the FFmpeg developers')).toBe('6.1.1-3ubuntu5');
    expect(ffmpegVersion('something else')).toBe('something else');
  });
});
