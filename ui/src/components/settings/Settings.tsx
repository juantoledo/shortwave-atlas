// The settings sheet: Rig, Audio, Sources, Remote access, Display and Updates, one tab each, so
// each fits without scrolling.
// QTH and language slot in here later.

import type { Api } from '../../api/client';
import type { DisplayApi } from '../../hooks/useDisplay';
import type { UpdateApi } from '../../hooks/useUpdate';
import type { UserStationsApi } from '../../hooks/useUserStations';
import { useT } from '../../i18n';
import type { CatalogMeta } from '../../types/generated/CatalogMeta';
import type { Os } from '../../types/generated/Os';
import type { RigState } from '../../types/generated/RigState';
import { Sheet } from '../Sheet';
import { AudioSection } from './AudioSection';
import { DisplaySection } from './DisplaySection';
import { RemoteSection } from './RemoteSection';
import { RigSection } from './RigSection';
import { SourcesSection } from './SourcesSection';
import { UpdatesSection } from './UpdatesSection';

export type SettingsTab = 'rig' | 'audio' | 'sources' | 'remote' | 'display' | 'updates';
const TABS: SettingsTab[] = ['rig', 'audio', 'sources', 'remote', 'display', 'updates'];

interface Props {
  api: Api;
  /** Where the core runs: ports, devices and hints are worded for it. */
  os: Os;
  rig: RigState;
  update: UpdateApi;
  display: DisplayApi;
  meta: CatalogMeta | null;
  mine: UserStationsApi;
  tab: SettingsTab;
  onTab: (t: SettingsTab) => void;
  onClose: () => void;
  onError: (e: unknown) => void;
  onApplied: () => void;
  /** Internet radio (the NET band) is on. */
  net: boolean;
  /** Internet radio was just turned on: the station list goes to the NET band. */
  onNetOn: () => void;
  /** The rig's audio is on (remote browsers can listen). */
  listen: boolean;
}

export function Settings({ api, os, rig, update, display, meta, mine, tab, onTab, onClose, onError, onApplied, net, onNetOn, listen }: Props) {
  const t = useT();
  const label = { rig: t.rigSection, audio: t.audioSection, sources: t.sourcesSection, remote: t.remoteSection, display: t.displaySection, updates: t.updatesSection };
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
      <div hidden={tab !== 'sources'}><SourcesSection api={api} meta={meta} mine={mine} net={net} onError={onError} onApplied={onApplied} onNetOn={onNetOn} /></div>
      {/* only while shown: it polls the server */}
      <div hidden={tab !== 'remote'}>{tab === 'remote' && <RemoteSection api={api} os={os} listen={listen} onAudio={() => onTab('audio')} onError={onError} />}</div>
      <div hidden={tab !== 'display'}><DisplaySection display={display} /></div>
      <div hidden={tab !== 'updates'}><UpdatesSection update={update} onError={onError} /></div>
    </Sheet>
  );
}
