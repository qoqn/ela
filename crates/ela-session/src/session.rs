use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use ela_core::CredentialRef;
use secrecy::SecretString;
use uuid::Uuid;

use crate::{FrameStore, FrontendEvent, RemoteFiles, SessionError};

#[async_trait]
pub trait Session: Send {
    async fn run(self: Box<Self>, io: SessionIo) -> Result<(), SessionError>;

    fn files(&self) -> Option<Arc<dyn RemoteFiles>> {
        None
    }
}

pub struct SessionIo {
    pub commands: mpsc::Receiver<SessionCommand>,
    pub sink: SessionSink,
    pub cancel: CancellationToken,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionCommand {
    Text {
        data: String,
    },
    Key {
        code: String,
        down: bool,
    },
    MouseMove {
        x: u16,
        y: u16,
    },
    MouseButton {
        button: u8,
        down: bool,
        x: u16,
        y: u16,
    },
    /// Wheel notches; positive means away from the user (scroll up).
    Wheel {
        vertical: i16,
        horizontal: i16,
    },
    Resize {
        width: u32,
        height: u32,
    },
    ReleaseAll,
    ClipboardOffer,
    ClipboardText {
        text: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SessionStatus {
    Connecting {
        jump_host: Option<String>,
    },
    Verifying {
        fingerprint: String,
        jump_host: Option<String>,
    },
    Asking {
        id: PromptId,
        jump_host: Option<String>,
        prompt: Prompt,
    },
    Authenticating {
        jump_host: Option<String>,
    },
    Connected,
    Reconnecting,
    Closed {
        error: Option<SessionError>,
    },
}

pub type PromptId = u32;

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Prompt {
    Trust {
        algorithm: String,
        fingerprint: String,
        known: Option<String>,
        // Other key types saved for this host: a new type, not a first connection.
        known_types: Vec<String>,
    },
    Passphrase {
        key: String,
        credential: CredentialRef,
        note: Option<PassphraseNote>,
    },
    Password {
        username: String,
        host: String,
        port: u16,
        note: Option<PasswordNote>,
        can_remember: bool,
        tried: Vec<Tried>,
    },
    // Unlocking the Vault for a saved password, with the master password as the write-only answer.
    Vault {
        wrong: bool,
        for_jump_host: Option<String>,
    },
    Questions {
        username: String,
        host: String,
        port: u16,
        // Both come from the server, so they are untrusted text.
        name: String,
        instructions: String,
        questions: Vec<Question>,
        refused: bool,
    },
}

/// What signed in before the Tab had to ask, so the prompt's copy needn't know the Protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Tried {
    Keys,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PassphraseNote {
    Wrong,
    SavedWrong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PasswordNote {
    Wrong,
    SavedWrong,
    // The Vault lost the Secret the saved Connection names.
    Missing,
}

#[derive(Debug)]
pub enum Answer {
    Trust(bool),
    Passphrase(PassphraseAnswer),
    Password(PasswordAnswer),
    Questions(Vec<SecretString>),
    // `None` goes on without the saved Secret.
    Vault(Option<SecretString>),
}

#[derive(Debug)]
pub enum PassphraseAnswer {
    Typed {
        passphrase: SecretString,
        remember: bool,
    },
    Saved,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct Question {
    pub text: String,
    pub echo: bool,
}

#[derive(Debug)]
pub struct PasswordAnswer {
    pub password: SecretString,
    pub remember: bool,
}

impl Answer {
    fn fits(&self, prompt: &Prompt) -> bool {
        matches!(
            (self, prompt),
            (Self::Trust(_), Prompt::Trust { .. })
                | (Self::Passphrase(_), Prompt::Passphrase { .. })
                | (Self::Password(_), Prompt::Password { .. })
                | (Self::Questions(_), Prompt::Questions { .. })
                | (Self::Vault(_), Prompt::Vault { .. })
        )
    }
}

pub(crate) struct Pending {
    id: PromptId,
    prompt: Prompt,
    answer: oneshot::Sender<Answer>,
}

#[derive(Clone, Default)]
pub(crate) struct Prompts {
    pending: Arc<Mutex<Option<Pending>>>,
    last: Arc<AtomicU32>,
}

impl Prompts {
    /// Only the open prompt with this ID takes the answer, and only an answer of its kind: a late
    /// or doubled submit must never land on the next host's prompt.
    pub(crate) fn answer(&self, id: PromptId, answer: Answer) -> bool {
        let mut pending = self.pending.lock().unwrap();
        match pending.take() {
            Some(open) if open.id == id && answer.fits(&open.prompt) => {
                let _ = open.answer.send(answer);
                true
            }
            other => {
                *pending = other;
                false
            }
        }
    }
}

/// Blocking: it is called off the async workers.
pub trait SavedPasswords: Send + Sync {
    fn remember(&self, connection: Uuid, password: SecretString) -> Result<(), String>;
}

#[derive(Clone)]
pub struct SessionSink {
    pub(crate) events: mpsc::UnboundedSender<FrontendEvent>,
    pub(crate) terminal: mpsc::Sender<Bytes>,
    pub(crate) frame: Arc<FrameStore>,
    pub(crate) prompts: Prompts,
    pub(crate) passwords: Option<Arc<dyn SavedPasswords>>,
}

impl SessionSink {
    pub fn status(&self, status: SessionStatus) {
        let _ = self.events.send(FrontendEvent::Status { status });
    }

    pub fn clipboard_write(&self, text: String) {
        let _ = self.events.send(FrontendEvent::ClipboardWrite { text });
    }

    pub fn clipboard_read(&self) {
        let _ = self.events.send(FrontendEvent::ClipboardRead);
    }

    /// Waits for the user as long as it takes. `None` once the Session is closed instead.
    pub async fn ask(&self, jump_host: Option<String>, prompt: Prompt) -> Option<Answer> {
        let (answer, answered) = oneshot::channel();
        let id = self.prompts.last.fetch_add(1, Ordering::Relaxed) + 1;
        *self.prompts.pending.lock().unwrap() = Some(Pending {
            id,
            prompt: prompt.clone(),
            answer,
        });
        self.status(SessionStatus::Asking {
            id,
            jump_host,
            prompt,
        });
        answered.await.ok()
    }

    pub async fn ask_trust(&self, jump_host: Option<String>, prompt: Prompt) -> bool {
        matches!(self.ask(jump_host, prompt).await, Some(Answer::Trust(true)))
    }

    pub async fn ask_passphrase(
        &self,
        jump_host: Option<String>,
        prompt: Prompt,
    ) -> PassphraseAnswer {
        match self.ask(jump_host, prompt).await {
            Some(Answer::Passphrase(answer)) => answer,
            _ => PassphraseAnswer::Skip,
        }
    }

    pub async fn ask_password(
        &self,
        jump_host: Option<String>,
        prompt: Prompt,
    ) -> Option<PasswordAnswer> {
        match self.ask(jump_host, prompt).await? {
            Answer::Password(answer) => Some(answer),
            _ => None,
        }
    }

    /// One answer per question, in order.
    pub async fn ask_answers(
        &self,
        jump_host: Option<String>,
        prompt: Prompt,
    ) -> Option<Vec<SecretString>> {
        match self.ask(jump_host, prompt).await? {
            Answer::Questions(answers) => Some(answers),
            _ => None,
        }
    }

    /// Keeps a password that signed in as the saved Connection's own. It signed in either way, so
    /// a failure is only logged.
    pub async fn keep_password(&self, connection: Uuid, password: SecretString) {
        let Some(passwords) = self.passwords.clone() else {
            return;
        };
        let kept =
            tokio::task::spawn_blocking(move || passwords.remember(connection, password)).await;
        match kept {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "couldn't remember the password"),
            Err(error) => tracing::warn!(%error, "couldn't remember the password"),
        }
    }

    pub fn can_keep_passwords(&self) -> bool {
        self.passwords.is_some()
    }

    /// Queues terminal output. Waits when the UI is behind, which in turn slows the
    /// remote side through the protocol's own flow control.
    pub async fn terminal(&self, data: impl Into<Bytes>) -> Result<(), SessionError> {
        self.terminal
            .send(data.into())
            .await
            .map_err(|_| SessionError::Cancelled)
    }

    pub fn frame(&self) -> &FrameStore {
        &self.frame
    }
}
