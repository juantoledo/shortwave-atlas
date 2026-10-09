// The settings sheet: Rig, Audio and Updates, one tab each, so each fits without scrolling.
// QTH, language and server slot in here later.

import type { Api } from '../../api/client';
import type { UpdateApi } from '../../hooks/useUpdate';
import { useT } from '../../i18n';
import type { Os } from '../../types/generated/Os';
import type { RigState } from '../../types/generated/RigState';
import { Sheet } from '../Sheet';
import { AudioSection } from './AudioSection';
import { RigSection } from './RigSection';
import { UpdatesSection } from './UpdatesSection';

export type SettingsTab = 'rig' | 'audio' | 'updates';
const TABS: SettingsTab[] = ['rig', 'audio', 'updates'];

interface Props {
  api: Api;
  /** Where the core runs: ports, devices and hints are worded for it. */
  os: Os;
  rig: RigState;
  update: UpdateApi;
  tab: SettingsTab;
  onTab: (t: SettingsTab) => void;
  onClose: () => void;
  onError: (e: unknown) => void;
  onApplied: () => void;
}

export function Settings({ api, os, rig, update, tab, onTab, onClose, onError, onApplied }: Props) {
  const t = useT();
  const label = { rig: t.rigSection, audio: t.audioSection, updates: t.updatesSection };
  const tabs = (
    <div className="tabs sheet-tabs" role="tablist" aria-label={t.settings}>
      {TABS.map((k) => (
        <button key={k} type="button" role="tab" aria-selected={tab === k} onClick={() => onTab(k)}>{label[k]}</button>
      ))}
    </div>
  );
  return (
    <Sheet title={t.settings} onClose={onClose} head={tabs}>
      {/* all mounted, so a half-filled form survives a tab switch */}
      <div hidden={tab !== 'rig'}><RigSection api={api} os={os} rig={rig} onError={onError} onApplied={onApplied} /></div>
      <div hidden={tab !== 'audio'}><AudioSection api={api} os={os} onError={onError} onApplied={onApplied} /></div>
      <div hidden={tab !== 'updates'}><UpdatesSection update={update} onError={onError} /></div>
    </Sheet>
  );
}
