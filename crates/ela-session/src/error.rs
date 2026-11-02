use serde::Serialize;

#[derive(Debug, Clone, thiserror::Error, Serialize, specta::Type)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum SessionError {
    #[error("host unreachable: {0}")]
    HostUnreachable(String),
    #[error("connection timed out")]
    Timeout,
    #[error("authentication failed: {0}")]
    AuthFailed(String),
    #[error("host key changed: {0}")]
    HostKeyChanged(String),
    #[error("host key revoked: {0}")]
    HostKeyRevoked(String),
    #[error("certificate untrusted: {0}")]
    CertificateUntrusted(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("Ela doesn't support the “{0}” protocol yet.")]
    UnknownProtocol(String),
    #[error("That session has already ended.")]
    UnknownSession,
    #[error("cancelled")]
    Cancelled,
    #[error("Jump Host {jump_host}: {error}")]
    JumpHost {
        jump_host: String,
        host: String,
        port: u16,
        error: Box<SessionError>,
    },
    #[error("{jump_host} couldn't open {address}: {failure:?}")]
    ForwardFailed {
        jump_host: String,
        address: String,
        failure: ForwardFailure,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ForwardFailure {
    Prohibited,
    Unreachable,
    Other,
}

impl SessionError {
    pub fn protocol(err: impl std::fmt::Display) -> Self {
        Self::Protocol(err.to_string())
    }

    pub fn from_io(err: &std::io::Error) -> Self {
        use std::io::ErrorKind::*;
        match err.kind() {
            TimedOut => Self::Timeout,
            ConnectionRefused | HostUnreachable | NetworkUnreachable | AddrNotAvailable
            | NotFound => Self::HostUnreachable(err.to_string()),
            _ => Self::Protocol(err.to_string()),
        }
    }
}
