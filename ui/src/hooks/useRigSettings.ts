// Data for the rig settings page: current choice, Hamlib models, serial ports, and
// live diagnostics (polled only while the page is open).

import { useCallback, useEffect, useState } from 'react';
import type { Api } from '../api/client';
import type { RigDiagnostics } from '../types/generated/RigDiagnostics';
import type { RigModel } from '../types/generated/RigModel';
import type { RigSettings } from '../types/generated/RigSettings';
import type { SerialPortInfo } from '../types/generated/SerialPortInfo';

const DIAG_MS = 1000;

export function useRigSettings(api: Api) {
  const [settings, setSettings] = useState<RigSettings | null>(null);
  const [models, setModels] = useState<RigModel[]>([]);
  const [ports, setPorts] = useState<SerialPortInfo[]>([]);
  const [diag, setDiag] = useState<RigDiagnostics | null>(null);

  const reload = useCallback(() => api.rigSettings().then(setSettings, () => {}), [api]);
  const refreshPorts = useCallback(() => api.serialPorts().then(setPorts, () => {}), [api]);

  useEffect(() => {
    reload();
    refreshPorts();
    api.rigModels().then(setModels, () => {});
  }, [api, reload, refreshPorts]);

  useEffect(() => {
    let alive = true;
    const tick = () => api.rigDiagnostics().then((d) => { if (alive) setDiag(d); }, () => {});
    tick();
    const id = setInterval(tick, DIAG_MS);
    return () => { alive = false; clearInterval(id); };
  }, [api]);

  return { settings, models, ports, diag, reload, refreshPorts };
}
