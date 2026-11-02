use serde::Serialize;

// The webview shows it as written, so it must read as a sentence for a person.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(transparent)]
pub struct CommandError(String);

pub type CommandResult<T> = Result<T, CommandError>;

impl CommandError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl<E: std::error::Error> From<E> for CommandError {
    fn from(error: E) -> Self {
        Self(error.to_string())
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<&str> for CommandError {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}
