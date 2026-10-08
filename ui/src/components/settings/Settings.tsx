// The settings view, shown in the console column (the globe stays visible).
// Sections: Rig, Audio and Updates; QTH, language and server slot in here later.

import type { Api } from '../../api/client';
import type { UpdateApi } from '../../hooks/useUpdate';
import { useT } from '../../i18n';
import type { Os } from '../../types/generated/Os';
import type { RigState } from '../../types/generated/RigState';
import { AudioSection } from './AudioSection';
import { RigSection } from './RigSection';
import { UpdatesSection } from './UpdatesSection';

interface Props {
  api: Api;
  /** Where the core runs: ports, devices and hints are worded for it. */
  os: Os;
  rig: RigState;
  update: UpdateApi;
  onBack: () => void;
  onError: (e: unknown) => void;
  onApplied: () => void;
}

export function Settings({ api, os, rig, update, onBack, onError, onApplied }: Props) {
  const t = useT();
  return (
    <>
      <header className="brand">
        <div><h1>{t.settings}</h1></div>
        <button className="power" type="button" onClick={onBack}>← {t.back}</button>
      </header>
      <RigSection api={api} os={os} rig={rig} onError={onError} onApplied={onApplied} />
      <AudioSection api={api} os={os} onError={onError} onApplied={onApplied} />
      <UpdatesSection update={update} onError={onError} />
    </>
  );
}
