import { describe, expect, it } from 'vitest';
import { cardLabel, TUNING } from './AudioSection';
import { en } from '../../i18n/en';

describe('audio settings', () => {
  it('marks the rig codec', () => {
    const codec = { device: 'plughw:CARD=CODEC,DEV=0', label: 'USB AUDIO CODEC · USB Audio', card_id: 'CODEC', rig_codec: true, usable: true };
    expect(cardLabel(codec, 'rig codec')).toBe('rig codec · USB AUDIO CODEC · USB Audio');
    expect(cardLabel({ ...codec, rig_codec: false }, 'rig codec')).toBe('USB AUDIO CODEC · USB Audio');
  });

  it('has a field and a text for every tuning value', () => {
    expect(TUNING.map((f) => f.key).sort()).toEqual(Object.keys(en.audioTuning).sort());
    for (const f of TUNING) expect(f.min).toBeLessThan(f.max);
  });
});
