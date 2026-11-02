use std::sync::Arc;

use ela_session::{
    Answer, Frontend, FrontendEvent, PassphraseAnswer, PasswordAnswer, PromptId, ProtocolInfo,
    QuickTarget, SecretString, SessionCommand, SessionId, Size,
};
use ela_workspace::{Address, Workspace};
use serde::Deserialize;
use tauri::State;
use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};

use crate::Manager_;
use crate::error::CommandResult;

/// One binary data message (`ela_session::wire`). The generated type says `number[]`, but
/// it is sent raw and reaches JavaScript as an `ArrayBuffer`.
#[derive(Clone, specta::Type)]
pub struct RawMessage(Vec<u8>);

impl IpcResponse for RawMessage {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        Ok(InvokeResponseBody::Raw(self.0))
    }
}

struct TauriFrontend {
    events: Channel<FrontendEvent>,
    data: Channel<RawMessage>,
}

impl Frontend for TauriFrontend {
    fn event(&self, event: FrontendEvent) {
        let _ = self.events.send(event);
    }

    fn data(&self, message: Vec<u8>) {
        let _ = self.data.send(RawMessage(message));
    }
}

fn frontend(events: Channel<FrontendEvent>, data: Channel<RawMessage>) -> Arc<dyn Frontend> {
    Arc::new(TauriFrontend { events, data })
}

#[tauri::command]
#[specta::specta]
pub fn list_protocols(manager: State<'_, Manager_>) -> Vec<ProtocolInfo> {
    manager.registry().list()
}

#[tauri::command]
#[specta::specta]
pub fn parse_quick_connect(input: String) -> CommandResult<QuickTarget> {
    Ok(QuickTarget::parse(&input, ela_ssh::PROTOCOL_ID)?)
}

#[derive(Deserialize, specta::Type)]
pub struct OpenSessionRequest {
    protocol: String,
    host: String,
    #[serde(default)]
    #[specta(optional)]
    port: Option<u16>,
    #[serde(default)]
    #[specta(optional)]
    username: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    options: serde_json::Map<String, serde_json::Value>,
    size: Size,
}

/// The Session asks on its Tab for any password it needs, so none is sent here.
#[tauri::command]
#[specta::specta]
pub async fn open_session(
    workspace: State<'_, Arc<Workspace>>,
    request: OpenSessionRequest,
    events: Channel<FrontendEvent>,
    data: Channel<RawMessage>,
) -> CommandResult<SessionId> {
    let address = Address {
        protocol: request.protocol,
        host: request.host,
        port: request.port,
        username: request.username,
        options: request.options,
    };
    Ok(workspace.open_address(address, request.size, frontend(events, data))?)
}

#[derive(Deserialize, specta::Type)]
pub struct OpenSavedSessionRequest {
    id: uuid::Uuid,
    size: Size,
}

/// Rust looks up where to connect, not the webview: a request naming a host and a Credential
/// Reference would let one XSS bug send any saved password to any server.
#[tauri::command]
#[specta::specta]
pub async fn open_saved_session(
    workspace: State<'_, Arc<Workspace>>,
    request: OpenSavedSessionRequest,
    events: Channel<FrontendEvent>,
    data: Channel<RawMessage>,
) -> CommandResult<SessionId> {
    Ok(workspace.open_saved(request.id, request.size, frontend(events, data))?)
}

#[tauri::command]
#[specta::specta]
pub fn session_input(
    manager: State<'_, Manager_>,
    id: SessionId,
    command: SessionCommand,
) -> CommandResult<()> {
    Ok(manager.send(id, command)?)
}

/// Every field holding text is write-only (ADR-0002): a passphrase, a password, answers that may
/// be one-time codes, or the master password.
#[derive(Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptReply {
    Trust { trust: bool },
    Passphrase { passphrase: String, remember: bool },
    SavedPassphrase,
    SkipPassphrase,
    Password { password: String, remember: bool },
    Answers { answers: Vec<String> },
    Vault { master: String },
    WithoutVault,
}

impl From<PromptReply> for Answer {
    fn from(reply: PromptReply) -> Self {
        match reply {
            PromptReply::Trust { trust } => Answer::Trust(trust),
            PromptReply::Passphrase {
                passphrase,
                remember,
            } => Answer::Passphrase(PassphraseAnswer::Typed {
                passphrase: SecretString::from(passphrase),
                remember,
            }),
            PromptReply::SavedPassphrase => Answer::Passphrase(PassphraseAnswer::Saved),
            PromptReply::SkipPassphrase => Answer::Passphrase(PassphraseAnswer::Skip),
            PromptReply::Password { password, remember } => Answer::Password(PasswordAnswer {
                password: SecretString::from(password),
                remember,
            }),
            PromptReply::Answers { answers } => {
                Answer::Questions(answers.into_iter().map(SecretString::from).collect())
            }
            PromptReply::Vault { master } => Answer::Vault(Some(SecretString::from(master))),
            PromptReply::WithoutVault => Answer::Vault(None),
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn session_answer(
    manager: State<'_, Manager_>,
    id: SessionId,
    prompt: PromptId,
    reply: PromptReply,
) -> CommandResult<()> {
    Ok(manager.answer(id, prompt, reply.into())?)
}

#[tauri::command]
#[specta::specta]
pub fn session_ack(manager: State<'_, Manager_>, id: SessionId) {
    manager.ack(id);
}

#[tauri::command]
#[specta::specta]
pub fn close_session(manager: State<'_, Manager_>, id: SessionId) {
    manager.close(id);
}
