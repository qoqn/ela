mod encode;
mod error;
mod files;
mod frame;
mod known_host;
mod manager;
mod protocol;
mod quick_connect;
mod session;
pub mod wire;

pub use ela_core::{CredentialRef, JumpHost, ResolvedConfig, Size};
pub use error::{ForwardFailure, SessionError};
pub use files::{
    FileError, FolderOutcome, RemoteEntry, RemoteFiles, RemoteKind, RemoteListing, Transfer,
    check_remote_name, local_file_name, remote_join, remote_parent, sort_entries,
};
pub use frame::{FrameStore, Rect};
pub use known_host::{KnownHost, KnownHostList, rewrite_keeping_backup};
pub use manager::{Frontend, FrontendEvent, Resolve, SessionId, SessionManager, SessionStats};
pub use protocol::{
    Capabilities, Protocol, ProtocolInfo, ProtocolRegistry, RendererKind, check_options,
};
pub use quick_connect::{QuickConnectError, QuickTarget};
pub use secrecy::{ExposeSecret, SecretString};
pub use session::{
    Answer, PassphraseAnswer, PassphraseNote, PasswordAnswer, PasswordNote, Prompt, PromptId,
    Question, SavedPasswords, Session, SessionCommand, SessionIo, SessionSink, SessionStatus,
    Tried,
};
pub use tokio_util::sync::CancellationToken;
