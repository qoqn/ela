use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct CredentialRef(pub Uuid);

const KEY_PASSPHRASES: Uuid = Uuid::from_u128(0x7e052629_7243_4a11_a940_c8ffe8be7f21);

impl CredentialRef {
    // Derived rather than saved on a Connection, so every Connection and Jump Host naming the
    // key finds the same passphrase, and nothing in the synced configuration points at it.
    pub fn for_key(identity: &str) -> Self {
        Self(Uuid::new_v5(&KEY_PASSPHRASES, identity.as_bytes()))
    }
}

/// The kind of place a Connection belongs to. `None` is a value of its own, so a Connection
/// can opt out of the Environment of its Folder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    #[default]
    None,
    Dev,
    Staging,
    Prod,
    Lab,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub credential: Option<CredentialRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub environment: Option<Environment>,
    /// Validated against the Protocol's `settings_schema`, so new Protocols need no migration.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub options: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Connection {
    pub id: Uuid,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub folder: Option<Uuid>,
    pub protocol: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub settings: Settings,
}

impl Connection {
    pub fn new(
        name: impl Into<String>,
        protocol: impl Into<String>,
        host: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            folder: None,
            protocol: protocol.into(),
            host: host.into(),
            tags: Vec::new(),
            settings: Settings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Folder {
    pub id: Uuid,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub parent: Option<Uuid>,
    #[serde(flatten)]
    pub settings: Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ItemRef {
    Connection(Uuid),
    Folder(Uuid),
}

impl Folder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            parent: None,
            settings: Settings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_has_one_reference_for_its_passphrase() {
        let key = "SHA256:9fQ2k1c2";
        assert_eq!(CredentialRef::for_key(key), CredentialRef::for_key(key));
        assert_ne!(
            CredentialRef::for_key(key),
            CredentialRef::for_key("SHA256:other")
        );
    }
}
