import { describe, expect, it } from 'vitest';
import { en } from './en';
import { es } from './es';
import { audioHintText, compass, fmtDur, hintText, pickMessages, sLabel } from './index';

/** Keys of nested plain objects (hints, per-OS hints), as sorted dotted paths. */
function shape(o: object, at = ''): string[] {
  return Object.entries(o)
    .flatMap(([k, v]) => (v && typeof v === 'object' && !Array.isArray(v) ? shape(v, `${at}${k}.`) : [`${at}${k}`]))
    .sort();
}

describe('i18n', () => {
  it('every catalog has the same keys and value kinds as English', () => {
    for (const cat of [es]) {
      expect(Object.keys(cat).sort()).toEqual(Object.keys(en).sort());
      for (const k of Object.keys(en) as (keyof typeof en)[]) {
        expect(typeof cat[k], k).toBe(typeof en[k]);
        // same number of placeholders for message functions
        if (typeof en[k] === 'function') expect((cat[k] as () => string).length, k).toBe((en[k] as () => string).length);
      }
      expect(cat.compass).toHaveLength(8);
      for (const k of ['hints', 'audioHints', 'hintsByOs', 'audioHintsByOs'] as const) expect(shape(cat[k]), k).toEqual(shape(en[k]));
    }
  });

  it('words hints for the OS, falling back to the shared text', () => {
    expect(hintText(en, 'linux', 'permission_denied')[1]).toContain('dialout');
    expect(hintText(en, 'windows', 'device_busy')[0]).toBe('The COM port is in use');
    expect(hintText(en, 'windows', 'tcp_port_in_use')).toEqual(en.hints.tcp_port_in_use);
    expect(audioHintText(es, 'macos', 'microphone_denied')[1]).toContain('Micrófono');
    expect(audioHintText(en, 'windows', 'ffmpeg_missing')[1]).toContain('reinstall');
  });

  it('picks the override, then the browser language, then English', () => {
    expect(pickMessages('es', ['en-US']).locale).toBe('es');
    expect(pickMessages(null, ['es-CL', 'en']).locale).toBe('es');
    expect(pickMessages(undefined, ['fr-FR', 'de']).locale).toBe('en');
    expect(pickMessages('xx', []).locale).toBe('en');
  });

  it('formats durations and compass points per locale', () => {
    expect(fmtDur(en, 45)).toBe('45 min');
    expect(fmtDur(en, 120)).toBe('2 h');
    expect(fmtDur(es, 135)).toBe('2 h 15 min');
    expect(compass(en, 225)).toBe('SW');
    expect(compass(es, 225)).toBe('SO');
    expect(compass(en, 359)).toBe('N');
  });

  it('S-meter labels', () => {
    expect(sLabel(null)).toBe('--');
    expect(sLabel(-54)).toBe('S0');
    expect(sLabel(-12)).toBe('S7');
    expect(sLabel(0)).toBe('S9');
    expect(sLabel(20)).toBe('S9+20');
  });
});
