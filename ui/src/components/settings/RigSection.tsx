// Choose the rig and connect: backend, Hamlib model, serial port, baud; live status and hints.

import { useEffect, useId, useMemo, useState } from 'react';
import type { Api } from '../../api/client';
import { Combobox, Dropdown, type DropOpt } from '../Dropdown';
import { useRigSettings } from '../../hooks/useRigSettings';
import { fmtKhz, hintText, useT, type Messages } from '../../i18n';
import type { BackendKind } from '../../types/generated/BackendKind';
import type { Os } from '../../types/generated/Os';
import type { RigChoice } from '../../types/generated/RigChoice';
import type { RigDiagnostics } from '../../types/generated/RigDiagnostics';
import type { RigModel } from '../../types/generated/RigModel';
import type { RigState } from '../../types/generated/RigState';

const BAUDS = [0, 4800, 9600, 19200, 38400, 57600, 115200];
const OTHER = '__other';
/** How a serial port is called on each OS. */
const PORT_EXAMPLE: Record<Os, string> = { linux: '/dev/ttyUSB0', windows: 'COM5', macos: '/dev/cu.SLAB_USBtoUART' };

export const modelLabel = (m: RigModel) => `${m.id} · ${m.mfg} ${m.model}`;

/** "rigctl Hamlib 4.5.5 Apr 05 11:43:08Z 2023 SHA=6eecd3" -> "4.5.5" */
export const hamlibVersion = (line: string) => /Hamlib\s+([\w.~-]+)/.exec(line)?.[1] ?? line;

/** The model id typed or picked in the model field ("1042 · Yaesu FTDX-10" or "1042"). */
export function parseModel(text: string): number | null {
  const m = /^\s*(\d+)/.exec(text);
  return m ? Number(m[1]) : null;
}

function statusText(t: Messages, rig: RigState): string {
  switch (rig.link) {
    case 'sim': return t.stSim;
    case 'off': return t.stOff;
    case 'down': return t.stDown;
    case 'ok':
      if (rig.power === 'on') return t.stOn(`${rig.freq_hz === null ? '--' : fmtKhz(rig.freq_hz)} kHz ${rig.mode ?? ''}`.trim());
      return rig.power === 'off' ? t.stRadioOff : t.stNoAnswer;
  }
}

function Status({ t, os, rig, diag }: { t: Messages; os: Os; rig: RigState; diag: RigDiagnostics | null }) {
  const p = diag?.rigctld;
  const proc = p && [
    p.running ? t.rigctldUp(p.uptime_s ?? 0) : t.rigctldNotRunning,
    p.restarts ? t.restarts(p.restarts) : '',
    p.last_exit ? t.lastExit(p.last_exit) : '',
  ].filter(Boolean).join(' · ');
  return (
    <div className="st-box" aria-live="polite">
      <h3 className="h"><span>{t.status}</span></h3>
      <p className={'st-line st-' + rig.link + (rig.link === 'ok' && rig.power !== 'on' ? ' st-warn' : '')}>{statusText(t, rig)}</p>
      {proc && <p className="note">{proc}</p>}
      {diag?.last_error && <p className="note">{t.lastError(diag.last_error)}</p>}
      {diag?.hints.map((h) => (
        <div className="hint" key={h} role="alert">
          <b>{hintText(t, os, h)[0]}</b>
          <span>{hintText(t, os, h)[1]}</span>
        </div>
      ))}
      {p && p.log.length > 0 && (
        <details className="log">
          <summary>{t.rigctldLog}</summary>
          <pre>{p.log.slice(-20).join('\n')}</pre>
        </details>
      )}
    </div>
  );
}

interface Props {
  api: Api;
  os: Os;
  rig: RigState;
  onError: (e: unknown) => void;
  onApplied: () => void;
}

export function RigSection({ api, os, rig, onError, onApplied }: Props) {
  const t = useT();
  const { settings, models, ports, diag, reload, refreshPorts } = useRigSettings(api);
  const [draft, setDraft] = useState<RigChoice | null>(null);
  const [modelText, setModelText] = useState('');
  const uid = useId();
  const [otherPort, setOtherPort] = useState(false);
  const [busy, setBusy] = useState(false);

  // start from the saved choice; the simulated rig (the unconfigured default) is not offered
  useEffect(() => {
    if (!settings) return;
    const c = settings.choice;
    setDraft(c.backend === 'sim' ? { ...c, backend: 'spawn' } : c);
    setOtherPort(false);
  }, [settings]);
  useEffect(() => {
    if (!draft) return;
    const m = models.find((x) => x.id === draft.model);
    setModelText(m ? modelLabel(m) : String(draft.model));
    // only when the model list arrives or the saved choice changes
  }, [models, settings]);

  const model = useMemo(() => models.find((m) => m.id === draft?.model) ?? null, [models, draft?.model]);
  const modelOpts = useMemo<DropOpt[]>(() => models.map((m) => ({ value: String(m.id), label: `${m.mfg} ${m.model}`, hint: `${m.id} · ${m.status}` })), [models]);
  if (!settings || !draft) return <p className="note">{t.connecting}</p>;

  const set = (patch: Partial<RigChoice>) => setDraft({ ...draft, ...patch });
  const knownPort = ports.some((p) => p.path === draft.device);
  const portValue = otherPort || (draft.device && !knownPort && !ports.length) ? OTHER : draft.device;

  const connect = async () => {
    setBusy(true);
    try {
      await api.applyRig(draft);
      await reload();
      onApplied();
    } catch (e) {
      onError(e);
    }
    setBusy(false);
  };
  const disconnect = async () => {
    setBusy(true);
    try {
      await api.disconnectRig();
    } catch (e) {
      onError(e);
    }
    setBusy(false);
  };

  const kinds: [BackendKind, string, string][] = [
    ['spawn', t.backendSpawn, t.backendSpawnHelp],
    ['external', t.backendExternal, t.backendExternalHelp],
  ];

  return (
    <section className="sec form" aria-label={t.rigSection}>
      <h2 className="h"><span>{t.rigSection}</span>{settings.rigctld_version && <span>{t.hamlibVersion(hamlibVersion(settings.rigctld_version))}</span>}</h2>

      <div className="seg" role="radiogroup" aria-label={t.rigSection}>
        {kinds.map(([k, label]) => (
          <button key={k} type="button" role="radio" aria-checked={draft.backend === k} onClick={() => set({ backend: k })}>{label}</button>
        ))}
      </div>
      <p className="note">{kinds.find(([k]) => k === draft.backend)![2]}</p>
      <div className="hint">
        <b><span aria-hidden="true">⚠ </span>{t.rigRiskTitle}</b>
        <span>{t.rigRisk}</span>
      </div>
      {settings.locked.length > 0 && <p className="note warn">{t.lockedByEnv(settings.locked.join(', '))}</p>}
      {draft.backend === 'spawn' && !settings.rigctld_version && <p className="note warn">{t.hamlibMissing}</p>}

      {draft.backend === 'spawn' && (
        <>
          <div className="field">
            <span id={`${uid}-model`}>{t.model}</span>
            <Combobox
              labelledBy={`${uid}-model`}
              text={modelText}
              placeholder={t.modelPlaceholder}
              opts={modelOpts}
              selected={String(draft.model)}
              empty={t.noModel}
              onText={(text) => {
                setModelText(text);
                const id = parseModel(text);
                if (id !== null) set({ model: id });
              }}
              onPick={(o) => {
                const m = models.find((x) => String(x.id) === o.value);
                if (!m) return;
                setModelText(modelLabel(m));
                set({ model: m.id });
              }}
            />
            <small>{models.length === 0 ? t.modelsMissing : model ? `${model.mfg} ${model.model} · ${model.status}` : t.modelPick}</small>
          </div>

          <div className="field">
            <span id={`${uid}-port`}>{t.serialPort}</span>
            <span className="row-inline">
              <Dropdown
                labelledBy={`${uid}-port`}
                value={portValue}
                opts={[
                  ...(!draft.device ? [{ value: '', label: '—' }] : []),
                  ...(draft.device && !knownPort ? [{ value: draft.device, label: draft.device, hint: t.notFound }] : []),
                  ...ports.map((p) => ({ value: p.path, label: p.label, hint: p.accessible ? undefined : `⚠ ${t.noPermission}` })),
                  { value: OTHER, label: t.otherPort },
                ]}
                onChange={(v) => {
                  setOtherPort(v === OTHER);
                  if (v !== OTHER) set({ device: v });
                }}
              />
              <button className="btn" type="button" onClick={refreshPorts}>{t.refresh}</button>
            </span>
            {ports.length === 0 && <small>{t.noPorts}</small>}
          </div>
          {portValue === OTHER && (
            <label className="field">
              <span>{t.portPath}</span>
              <input value={draft.device} placeholder={PORT_EXAMPLE[os]} onChange={(e) => set({ device: e.currentTarget.value })} />
            </label>
          )}

          <div className="field">
            <span id={`${uid}-baud`}>{t.baud}</span>
            <Dropdown
              labelledBy={`${uid}-baud`}
              value={String(draft.baud)}
              opts={BAUDS.map((b) => ({ value: String(b), label: b === 0 ? t.baudDefault : String(b) }))}
              onChange={(v) => set({ baud: Number(v) })}
            />
            <small>{t.baudHelp}</small>
          </div>

          <details className="field">
            <summary>{t.advanced}</summary>
            <label className="field">
              <span>{t.tcpPort}</span>
              <input type="number" min={1024} max={65535} value={draft.port} onChange={(e) => set({ port: Number(e.currentTarget.value) })} />
            </label>
          </details>
        </>
      )}

      {draft.backend === 'external' && (
        <>
          <label className="field">
            <span>{t.host}</span>
            <input value={draft.host} onChange={(e) => set({ host: e.currentTarget.value })} />
          </label>
          <label className="field">
            <span>{t.tcpPort}</span>
            <input type="number" min={1024} max={65535} value={draft.port} onChange={(e) => set({ port: Number(e.currentTarget.value) })} />
          </label>
        </>
      )}

      <div className="actions">
        <button className="btn primary" type="button" disabled={busy} onClick={connect}>{t.connect}</button>
        <button className="btn" type="button" disabled={busy || rig.link === 'off'} onClick={disconnect}>{t.disconnect}</button>
      </div>
      {settings.config_path && <p className="note">{t.savedTo(settings.config_path)}</p>}

      <Status t={t} os={os} rig={rig} diag={diag} />
    </section>
  );
}
