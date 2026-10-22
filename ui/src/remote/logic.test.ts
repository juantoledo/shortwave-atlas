import { describe, expect, it } from 'vitest';
import { makeNames } from '../catalog/names';
import { en } from '../i18n/en';
import { openPage, type PageFacts } from '../page';
import type { Candidate } from '../types/generated/Candidate';
import type { StationRow } from '../types/generated/StationRow';
import { parseReadout } from '../components/Readout';
import { candLines, dragSteps, fineStepHz, keypadPress, listBand, onAirFirst, scanPct, utcNow } from './logic';
import type { ScanStatus } from '../types/generated/ScanStatus';

describe('which page opens', () => {
  const phone: PageFacts = { transport: 'ws', phone: true, coarse: true, keptFull: false };
  it('a phone browser gets the remote', () => {
    expect(openPage(phone)).toBe('remote');
  });
  it('the atlas everywhere else, or once the listener chose it', () => {
    expect(openPage({ ...phone, keptFull: true })).toBe('atlas');
    expect(openPage({ ...phone, transport: 'tauri' })).toBe('atlas');
    expect(openPage({ ...phone, phone: false })).toBe('atlas');
    // a narrow desktop window with a mouse
    expect(openPage({ ...phone, coarse: false })).toBe('atlas');
  });
});

describe('tuning', () => {
  it('steps finely in the sidebands and CW', () => {
    expect(fineStepHz('USB')).toBe(100);
    expect(fineStepHz('CW')).toBe(100);
    expect(fineStepHz('AM')).toBe(1000);
    expect(fineStepHz(null)).toBe(1000);
  });

  it('a drag tunes by whole steps, a wobble by none', () => {
    expect(dragSteps(5)).toBe(0);
    expect(dragSteps(-5)).toBe(0);
    expect(dragSteps(30)).toBe(2);
    expect(dragSteps(-30)).toBe(-2);
  });

  it('the keypad builds kHz with one decimal', () => {
    const typed = (keys: string[]) => keys.reduce(keypadPress, '');
    expect(typed(['1', '1', '8', '6', '0'])).toBe('11860');
    expect(typed(['9', '.', '5', '5'])).toBe('9.5');
    expect(typed(['.', '5'])).toBe('0.5');
    expect(typed(['0', '7'])).toBe('7');
    expect(typed(['1', '2', 'back'])).toBe('1');
    expect(typed(['1', '2', '3', '4', '5', '6', '7'])).toBe('123456');
    expect(parseReadout(typed(['9', '7', '5', '0', '.', '5']))).toBe(9_750_500);
  });
});

describe('lists', () => {
  const row = (freq_hz: number, on: boolean) => ({ freq_hz, on }) as StationRow;
  it('On air lists who is on the air first, each part by frequency', () => {
    const rows = [row(9500, false), row(9600, true), row(9700, false), row(9800, true)];
    expect(onAirFirst(rows).map((r) => r.freq_hz)).toEqual([9600, 9800, 9500, 9700]);
  });
  it('On air only follows a broadcast band', () => {
    expect(listBand({ id: '31m', label: '31', kind: 'broadcast', lo_khz: 9400, hi_khz: 9900, scan_step_hz: 5000 })).toBe('31m');
    expect(listBand({ id: 'ham-40m', label: '40', kind: 'ham', lo_khz: 7000, hi_khz: 7200, scan_step_hz: 1000 })).toBeNull();
    expect(listBand(null)).toBeNull();
  });
});

describe('scan', () => {
  it('says how far a running scan has got', () => {
    const running = { state: 'running', done: 21, total: 50 } as ScanStatus;
    expect(scanPct(running)).toBe(42);
    expect(scanPct({ ...running, total: 0 } as ScanStatus)).toBe(0);
    expect(scanPct({ state: 'idle' })).toBe(0);
  });
});

describe('the now-playing card', () => {
  const names = makeNames(null, en);
  const cand = {
    station: { slots: [{ lang: 'J' }] },
    site: { name: 'Yamata', itu: 'J' },
    route: { km: 8412.4, bearing: 312.4 },
    air: { on: true, minutes: 28, edge: 900 },
    slot: 0,
  } as unknown as Candidate;

  it('says what, where from, how far and until when', () => {
    const { what, where } = candLines(en, names, cand);
    expect(what).toBe('J · Yamata · J');
    expect(where).toBe('8,412 km · 312° NW · ends in 28 min (15:00 UTC)');
  });
  it('leaves out an air time that never changes', () => {
    const { where } = candLines(en, names, { ...cand, air: { on: true, minutes: null, edge: 0 } });
    expect(where).toBe('8,412 km · 312° NW');
  });
  it('the clock is UTC', () => {
    expect(utcNow(new Date(Date.UTC(2026, 9, 21, 7, 5)))).toBe('07:05');
  });
});
