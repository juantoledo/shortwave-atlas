// Typed wrapper over a Transport: one method per `Call`.

import type { AudioChoice } from '../types/generated/AudioChoice';
import type { AudioDiagnostics } from '../types/generated/AudioDiagnostics';
import type { AudioSettings } from '../types/generated/AudioSettings';
import type { Candidate } from '../types/generated/Candidate';
import type { CatalogMeta } from '../types/generated/CatalogMeta';
import type { CodeTables } from '../types/generated/CodeTables';
import type { Dial } from '../types/generated/Dial';
import type { Favorite } from '../types/generated/Favorite';
import type { FavoriteRequest } from '../types/generated/FavoriteRequest';
import type { FavoriteView } from '../types/generated/FavoriteView';
import type { Hunt } from '../types/generated/Hunt';
import type { ImportReport } from '../types/generated/ImportReport';
import type { Info } from '../types/generated/Info';
import type { ListeningNow } from '../types/generated/ListeningNow';
import type { Logged } from '../types/generated/Logged';
import type { Overview } from '../types/generated/Overview';
import type { Qth } from '../types/generated/Qth';
import type { RigChoice } from '../types/generated/RigChoice';
import type { RigCommand } from '../types/generated/RigCommand';
import type { RigDiagnostics } from '../types/generated/RigDiagnostics';
import type { RigModel } from '../types/generated/RigModel';
import type { RigSettings } from '../types/generated/RigSettings';
import type { RigState } from '../types/generated/RigState';
import type { Scan } from '../types/generated/Scan';
import type { ScanRequest } from '../types/generated/ScanRequest';
import type { ScanStatus } from '../types/generated/ScanStatus';
import type { ScanSummary } from '../types/generated/ScanSummary';
import type { SerialPortInfo } from '../types/generated/SerialPortInfo';
import type { SoundCard } from '../types/generated/SoundCard';
import type { StationKey } from '../types/generated/StationKey';
import type { StationPage } from '../types/generated/StationPage';
import type { StationQuery } from '../types/generated/StationQuery';
import type { Stats } from '../types/generated/Stats';
import type { StatsQuery } from '../types/generated/StatsQuery';
import type { TrendPoint } from '../types/generated/TrendPoint';
import type { UpdatePrefs } from '../types/generated/UpdatePrefs';
import type { UpdateStatus } from '../types/generated/UpdateStatus';
import type { UserStationDraft } from '../types/generated/UserStationDraft';
import type { UserStationView } from '../types/generated/UserStationView';
import type { Transport } from './transport';

export class Api {
  constructor(readonly transport: Transport) {}

  info = () => this.transport.call({ cmd: 'info' }) as Promise<Info>;
  state = () => this.transport.call({ cmd: 'state' }) as Promise<RigState>;
  rig = (args: RigCommand) => this.transport.call({ cmd: 'rig', args }) as Promise<null>;
  lookup = (freq_hz: number) => this.transport.call({ cmd: 'lookup', args: { freq_hz } }) as Promise<Candidate[]>;
  search = (args: StationQuery) => this.transport.call({ cmd: 'search', args }) as Promise<StationPage>;
  overview = () => this.transport.call({ cmd: 'overview' }) as Promise<Overview>;
  stationsMeta = () => this.transport.call({ cmd: 'stations_meta' }) as Promise<CatalogMeta>;
  setQth = (args: Qth) => this.transport.call({ cmd: 'set_qth', args }) as Promise<null>;
  dial = () => this.transport.call({ cmd: 'dial' }) as Promise<Dial>;

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

  startScan = (args: ScanRequest) => this.transport.call({ cmd: 'start_scan', args }) as Promise<ScanStatus>;
  stopScan = () => this.transport.call({ cmd: 'stop_scan' }) as Promise<null>;
  scanStatus = () => this.transport.call({ cmd: 'scan_status' }) as Promise<ScanStatus>;
  scans = () => this.transport.call({ cmd: 'scans' }) as Promise<ScanSummary[]>;
  scan = (id: string) => this.transport.call({ cmd: 'scan', args: { id } }) as Promise<Scan>;
  deleteScan = (id: string) => this.transport.call({ cmd: 'delete_scan', args: { id } }) as Promise<null>;
  deleteScans = () => this.transport.call({ cmd: 'delete_scans' }) as Promise<number>;
  logbook = () => this.transport.call({ cmd: 'logbook' }) as Promise<Logged[]>;

  favorites = () => this.transport.call({ cmd: 'favorites' }) as Promise<FavoriteView[]>;
  addFavorite = (args: FavoriteRequest) => this.transport.call({ cmd: 'add_favorite', args }) as Promise<Favorite>;
  renameFavorite = (id: string, label: string) => this.transport.call({ cmd: 'rename_favorite', args: { id, label } }) as Promise<null>;
  deleteFavorite = (id: string) => this.transport.call({ cmd: 'delete_favorite', args: { id } }) as Promise<null>;

  userStations = () => this.transport.call({ cmd: 'user_stations' }) as Promise<UserStationView[]>;
  addUserStation = (args: UserStationDraft) => this.transport.call({ cmd: 'add_user_station', args }) as Promise<UserStationView>;
  updateUserStation = (id: string, station: UserStationDraft) =>
    this.transport.call({ cmd: 'update_user_station', args: { id, station } }) as Promise<UserStationView>;
  deleteUserStation = (id: string) => this.transport.call({ cmd: 'delete_user_station', args: { id } }) as Promise<null>;
  codeTables = () => this.transport.call({ cmd: 'code_tables' }) as Promise<CodeTables>;
  /** The listener's stations as JSON text, to save as a file. */
  exportUserStations = () => this.transport.call({ cmd: 'export_user_stations' }) as Promise<string>;
  importUserStations = (json: string, replace: boolean) =>
    this.transport.call({ cmd: 'import_user_stations', args: { json, replace } }) as Promise<ImportReport>;

  stats = (args: StatsQuery) => this.transport.call({ cmd: 'stats', args }) as Promise<Stats>;
  listening = () => this.transport.call({ cmd: 'listening' }) as Promise<ListeningNow>;
  hunt = () => this.transport.call({ cmd: 'hunt' }) as Promise<Hunt>;
  stationTrend = (args: StationKey) => this.transport.call({ cmd: 'station_trend', args }) as Promise<TrendPoint[]>;
  hearingStation = (args: StationKey) => this.transport.call({ cmd: 'hearing_station', args }) as Promise<null>;
  /** Unix seconds, or `null` for everything; resolves to how many sessions went. */
  clearListening = (before: number | null) => this.transport.call({ cmd: 'clear_listening', args: { before } }) as Promise<number>;
}
