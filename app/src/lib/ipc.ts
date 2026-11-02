// Names for the generated IPC types (bindings.ts, generated from the Rust side), plus
// what generation can't cover: wire constants. The generator makes a type per direction,
// and these pick the one the UI sees.
import type {
  Connection_Serialize,
  FieldChange_Serialize,
  Folder_Serialize,
  FrontendEvent_Serialize,
  KnownHostSection_Serialize,
  KnownHost_Serialize,
  Outside_Serialize,
  OpenSessionRequest_Deserialize,
  ProtocolInfo_Serialize,
  ResolvedConnection_Serialize,
  SessionCommand_Deserialize,
  SessionError,
  SavedKeyPassphrase_Serialize,
  SessionStatus_Serialize,
  Settings_Serialize,
  TrashEntry_Serialize,
  TrashView_Serialize,
} from './bindings';

export type {
  CredentialRef,
  FileDrag,
  HostKeyCode,
  ItemRef,
  KnownHostStore,
  PassphraseNote,
  PasswordNote,
  PromptReply,
  Tried,
  RecentEntry,
  RemoteEntry,
  RemoteListing,
  RemoteCursor,
  RendererKind,
  SavedProblem,
  SessionStats,
  TransferEvent,
  VaultInfo,
} from './bindings';
export type KnownHost = KnownHost_Serialize;
export type KnownHostSection = KnownHostSection_Serialize;
export type SavedConnection = Connection_Serialize;
export type SavedKeyPassphrase = SavedKeyPassphrase_Serialize;
export type SavedFolder = Folder_Serialize;
export type SavedSettings = Settings_Serialize;
export type TrashEntry = TrashEntry_Serialize;
export type TrashView = TrashView_Serialize;
export type FieldChange = FieldChange_Serialize;
export type Outside = Outside_Serialize;
export type ResolvedConnection = ResolvedConnection_Serialize;
export type ProtocolInfo = ProtocolInfo_Serialize;
export type SessionErrorInfo = SessionError;
export type FrontendEvent = FrontendEvent_Serialize;
export type SessionCommand = SessionCommand_Deserialize;
/** `idle` exists only in the UI, before a Session has been asked to open. */
export type SessionStatus = SessionStatus_Serialize | { state: 'idle' };
export type OpenRequest = Omit<OpenSessionRequest_Deserialize, 'size'>;

type Asking = Extract<SessionStatus_Serialize, { state: 'asking' }>;
export type Prompt = Asking['prompt'];
export type PromptOf<K extends Prompt['kind']> = Extract<Prompt, { kind: K }> & {
  id: number;
  jump_host: string | null;
};

export function promptOf<K extends Prompt['kind']>(
  status: SessionStatus,
  kind: K,
): PromptOf<K> | null {
  if (status.state !== 'asking' || status.prompt.kind !== kind) return null;
  return {
    ...(status.prompt as Extract<Prompt, { kind: K }>),
    id: status.id,
    jump_host: status.jump_host,
  };
}

export const MSG_TERMINAL = 1;
export const MSG_FRAME = 2;
export const FRAME_HEADER_LEN = 7;
export const FRAME_RECT_LEN = 13;
export const ENCODING_JPEG = 1;
