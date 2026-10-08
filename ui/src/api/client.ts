// Typed wrapper over a Transport: one method per `Call`.

import type { AudioChoice } from '../types/generated/AudioChoice';
import type { AudioDiagnostics } from '../types/generated/AudioDiagnostics';
import type { AudioSettings } from '../types/generated/AudioSettings';
import type { Candidate } from '../types/generated/Candidate';
import type { Info } from '../types/generated/Info';
import type { Qth } from '../types/generated/Qth';
import type { RigChoice } from '../types/generated/RigChoice';
import type { RigCommand } from '../types/generated/RigCommand';
import type { RigDiagnostics } from '../types/generated/RigDiagnostics';
import type { RigModel } from '../types/generated/RigModel';
import type { RigSettings } from '../types/generated/RigSettings';
import type { RigState } from '../types/generated/RigState';
import type { SerialPortInfo } from '../types/generated/SerialPortInfo';
import type { SoundCard } from '../types/generated/SoundCard';
import type { UpdatePrefs } from '../types/generated/UpdatePrefs';
import type { UpdateStatus } from '../types/generated/UpdateStatus';
import type { Transport } from './transport';

export class Api {
  constructor(readonly transport: Transport) {}

  info = () => this.transport.call({ cmd: 'info' }) as Promise<Info>;
  state = () => this.transport.call({ cmd: 'state' }) as Promise<RigState>;
  rig = (args: RigCommand) => this.transport.call({ cmd: 'rig', args }) as Promise<null>;
  lookup = (freq_hz: number) => this.transport.call({ cmd: 'lookup', args: { freq_hz } }) as Promise<Candidate[]>;
  list = () => this.transport.call({ cmd: 'list' }) as Promise<Candidate[]>;
  setQth = (args: Qth) => this.transport.call({ cmd: 'set_qth', args }) as Promise<null>;

  rigSettings = () => this.transport.call({ cmd: 'rig_settings' }) as Promise<RigSettings>;
  rigModels = () => this.transport.call({ cmd: 'rig_models' }) as Promise<RigModel[]>;
  serialPorts = () => this.transport.call({ cmd: 'serial_ports' }) as Promise<SerialPortInfo[]>;
  applyRig = (args: RigChoice) => this.transport.call({ cmd: 'apply_rig', args }) as Promise<null>;
  disconnectRig = () => this.transport.call({ cmd: 'disconnect_rig' }) as Promise<null>;
  rigDiagnostics = () => this.transport.call({ cmd: 'rig_diagnostics' }) as Promise<RigDiagnostics>;

  audioSettings = () => this.transport.call({ cmd: 'audio_settings' }) as Promise<AudioSettings>;
  soundCards = () => this.transport.call({ cmd: 'sound_cards' }) as Promise<SoundCard[]>;
  applyAudio = (args: AudioChoice) => this.transport.call({ cmd: 'apply_audio', args }) as Promise<null>;
  audioDiagnostics = () => this.transport.call({ cmd: 'audio_diagnostics' }) as Promise<AudioDiagnostics>;

  updateStatus = () => this.transport.call({ cmd: 'update_status' }) as Promise<UpdateStatus>;
  checkUpdate = () => this.transport.call({ cmd: 'check_update' }) as Promise<UpdateStatus>;
  /** Desktop window only: a remote browser gets an `invalid` error. */
  installUpdate = () => this.transport.call({ cmd: 'install_update' }) as Promise<null>;
  setUpdatePrefs = (args: UpdatePrefs) => this.transport.call({ cmd: 'set_update_prefs', args }) as Promise<null>;
}
