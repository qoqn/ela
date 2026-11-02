use std::collections::{BTreeMap, BTreeSet};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{Connection, CredentialRef, Environment, Folder, Settings};

/// The names carry no unit because terminals count columns and rows, framebuffers pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct ResolvedConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<SecretString>,
    pub options: serde_json::Map<String, serde_json::Value>,
    pub initial_size: Size,
    pub jump_hosts: Vec<JumpHost>,
    // The Credential Reference names a Secret the Vault no longer holds, so the prompt can say so.
    pub saved_password_lost: bool,
    /// Set only when the saved Connection's own account is the one signing in, since a password
    /// typed for another account or port must not land on it.
    pub connection: Option<Uuid>,
}

#[derive(Debug)]
pub struct JumpHost {
    pub name: String,
    pub config: ResolvedConfig,
}

impl ResolvedConfig {
    pub fn option_str(&self, key: &str) -> Option<&str> {
        self.options.get(key).and_then(|v| v.as_str())
    }

    pub fn option_u64(&self, key: &str) -> Option<u64> {
        self.options.get(key).and_then(|v| v.as_u64())
    }

    pub fn option_list(&self, key: &str) -> Vec<String> {
        option_list(self.options.get(key))
    }
}

/// A `list` option is an array of strings. Ela wrote lists as comma-separated text before, which
/// is still read, though it can't hold an entry with a comma.
pub fn option_list(value: Option<&Value>) -> Vec<String> {
    let entries: Vec<String> = match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::String(text)) => text.split(',').map(str::to_owned).collect(),
        _ => Vec::new(),
    };
    entries
        .into_iter()
        .map(|entry| entry.trim().to_owned())
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// A Connection with Inheritance applied. It holds no Secret, so the UI can have it.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct ResolvedConnection {
    pub id: Uuid,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub credential: Option<CredentialRef>,
    pub environment: Environment,
    pub options: Map<String, Value>,
}

impl ResolvedConnection {
    pub fn into_config(self, password: Option<SecretString>, initial_size: Size) -> ResolvedConfig {
        ResolvedConfig {
            connection: Some(self.id),
            host: self.host,
            port: self.port,
            username: self.username,
            password,
            options: self.options,
            initial_size,
            jump_hosts: Vec::new(),
            saved_password_lost: false,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResolveError {
    #[error("“{name}” has no port, and neither do its folders or its protocol, {protocol}.")]
    NoPort { name: String, protocol: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct FieldChange {
    pub field: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Files can be synced in from elsewhere, so the tree isn't assumed to be well formed. A parent
/// that doesn't exist, or a loop of Folders, ends the walk there and the item stays usable.
fn inherit(own: &Settings, folder: Option<Uuid>, folders: &BTreeMap<Uuid, Folder>) -> Settings {
    let mut settings = own.clone();
    let mut seen = BTreeSet::new();
    let mut next = folder;

    while let Some(id) = next {
        if !seen.insert(id) {
            tracing::warn!(folder = %id, "Folders form a loop");
            break;
        }
        let Some(folder) = folders.get(&id) else {
            tracing::warn!(folder = %id, "parent Folder doesn't exist");
            break;
        };
        let inherited = &folder.settings;
        settings.port = settings.port.or(inherited.port);
        settings.username = settings.username.or_else(|| inherited.username.clone());
        settings.credential = settings.credential.or(inherited.credential);
        settings.environment = settings.environment.or(inherited.environment);
        for (key, value) in &inherited.options {
            settings
                .options
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
        next = folder.parent;
    }
    settings
}

/// What a source outside Ela says about a host, such as `~/.ssh/config`: the lowest layer of
/// Inheritance, below every Folder. `source` names it for the editor.
#[derive(Debug, Clone, Default, PartialEq, Serialize, specta::Type)]
pub struct Outside {
    pub source: String,
    pub hostname: Option<String>,
    pub settings: Settings,
}

pub fn resolve(
    connection: &Connection,
    folders: &BTreeMap<Uuid, Folder>,
    default_port: Option<u16>,
) -> Result<ResolvedConnection, ResolveError> {
    resolve_over(connection, folders, None, default_port)
}

pub fn resolve_over(
    connection: &Connection,
    folders: &BTreeMap<Uuid, Folder>,
    outside: Option<&Outside>,
    default_port: Option<u16>,
) -> Result<ResolvedConnection, ResolveError> {
    let mut settings = inherit(&connection.settings, connection.folder, folders);
    if let Some(outside) = outside {
        let below = &outside.settings;
        settings.port = settings.port.or(below.port);
        settings.username = settings.username.or_else(|| below.username.clone());
        for (key, value) in &below.options {
            settings
                .options
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
    }
    let host = outside
        .and_then(|outside| outside.hostname.clone())
        .unwrap_or_else(|| connection.host.clone());
    let port = settings
        .port
        .or(default_port)
        .ok_or_else(|| ResolveError::NoPort {
            name: connection.name.clone(),
            protocol: connection.protocol.clone(),
        })?;

    Ok(ResolvedConnection {
        id: connection.id,
        name: connection.name.clone(),
        protocol: connection.protocol.clone(),
        host,
        port,
        username: settings.username,
        credential: settings.credential,
        environment: settings.environment.unwrap_or_default(),
        options: settings.options,
    })
}

pub fn inheritance_changes(
    own: &Settings,
    from: Option<Uuid>,
    to: Option<Uuid>,
    folders: &BTreeMap<Uuid, Folder>,
) -> Vec<FieldChange> {
    let before = inherit(own, from, folders);
    let after = inherit(own, to, folders);
    let credential = |value: Option<CredentialRef>| value.map(|reference| reference.0.to_string());
    let environment = |settings: &Settings| {
        let value = settings.environment.unwrap_or_default();
        serde_json::to_value(value)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
    };
    let option = |value: Option<&Value>| {
        value.map(|value| {
            value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned)
        })
    };

    let mut fields = vec![
        (
            "port".to_owned(),
            before.port.map(|port| port.to_string()),
            after.port.map(|port| port.to_string()),
        ),
        (
            "username".to_owned(),
            before.username.clone(),
            after.username.clone(),
        ),
        (
            "credential".to_owned(),
            credential(before.credential),
            credential(after.credential),
        ),
        (
            "environment".to_owned(),
            environment(&before),
            environment(&after),
        ),
    ];
    let keys: BTreeSet<&String> = before.options.keys().chain(after.options.keys()).collect();
    for key in keys {
        fields.push((
            key.clone(),
            option(before.options.get(key)),
            option(after.options.get(key)),
        ));
    }

    fields
        .into_iter()
        .filter(|(_, before, after)| before != after)
        .map(|(field, before, after)| FieldChange {
            field,
            before,
            after,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn folder(name: &str, parent: Option<&Folder>, settings: Settings) -> Folder {
        Folder {
            parent: parent.map(|p| p.id),
            settings,
            ..Folder::new(name)
        }
    }

    fn connection_in(folder: &Folder, settings: Settings) -> Connection {
        Connection {
            folder: Some(folder.id),
            settings,
            ..Connection::new("web", "ssh", "10.0.0.1")
        }
    }

    fn tree(folders: &[&Folder]) -> BTreeMap<Uuid, Folder> {
        folders.iter().map(|f| (f.id, (*f).clone())).collect()
    }

    #[test]
    fn the_nearest_folder_that_sets_a_field_wins() {
        let top = folder(
            "top",
            None,
            Settings {
                port: Some(2200),
                username: Some("top".into()),
                ..Default::default()
            },
        );
        let near = folder(
            "near",
            Some(&top),
            Settings {
                username: Some("near".into()),
                ..Default::default()
            },
        );
        let connection = connection_in(&near, Settings::default());

        let resolved = resolve(&connection, &tree(&[&top, &near]), Some(22)).unwrap();

        assert_eq!(resolved.username.as_deref(), Some("near"));
        assert_eq!(
            resolved.port, 2200,
            "near sets no port, so it comes from top"
        );
    }

    #[test]
    fn an_override_stops_inheritance_for_that_field_only() {
        let parent = folder(
            "parent",
            None,
            Settings {
                port: Some(2200),
                username: Some("parent".into()),
                ..Default::default()
            },
        );
        let connection = connection_in(
            &parent,
            Settings {
                port: Some(2222),
                ..Default::default()
            },
        );

        let resolved = resolve(&connection, &tree(&[&parent]), Some(22)).unwrap();

        assert_eq!(resolved.port, 2222);
        assert_eq!(resolved.username.as_deref(), Some("parent"));
    }

    #[test]
    fn options_inherit_key_by_key() {
        let options = |value: serde_json::Value| value.as_object().unwrap().clone();
        let parent = folder(
            "parent",
            None,
            Settings {
                options: options(json!({ "term": "vt100", "domain": "CORP" })),
                ..Default::default()
            },
        );
        let connection = connection_in(
            &parent,
            Settings {
                options: options(json!({ "term": "xterm" })),
                ..Default::default()
            },
        );

        let resolved = resolve(&connection, &tree(&[&parent]), Some(22)).unwrap();

        assert_eq!(resolved.options["term"], "xterm");
        assert_eq!(resolved.options["domain"], "CORP");
    }

    #[test]
    fn a_connection_can_opt_out_of_its_folders_environment() {
        let production = folder(
            "production",
            None,
            Settings {
                environment: Some(Environment::Prod),
                ..Default::default()
            },
        );
        let inside = connection_in(&production, Settings::default());
        let opted_out = connection_in(
            &production,
            Settings {
                environment: Some(Environment::None),
                ..Default::default()
            },
        );
        let folders = tree(&[&production]);

        assert_eq!(
            resolve(&inside, &folders, Some(22)).unwrap().environment,
            Environment::Prod
        );
        assert_eq!(
            resolve(&opted_out, &folders, Some(22)).unwrap().environment,
            Environment::None
        );
    }

    #[test]
    fn the_credential_is_inherited_and_the_password_is_added_last() {
        let reference = CredentialRef(Uuid::new_v4());
        let parent = folder(
            "parent",
            None,
            Settings {
                credential: Some(reference),
                ..Default::default()
            },
        );
        let connection = connection_in(&parent, Settings::default());

        let resolved = resolve(&connection, &tree(&[&parent]), Some(22)).unwrap();

        assert_eq!(resolved.credential, Some(reference));
        let size = Size {
            width: 80,
            height: 24,
        };
        assert!(resolved.into_config(None, size).password.is_none());
    }

    #[test]
    fn the_protocol_port_is_the_last_resort() {
        let connection = Connection::new("web", "ssh", "10.0.0.1");
        let none = BTreeMap::new();

        assert_eq!(resolve(&connection, &none, Some(22)).unwrap().port, 22);
        assert_eq!(
            resolve(&connection, &none, None).unwrap_err(),
            ResolveError::NoPort {
                name: "web".into(),
                protocol: "ssh".into()
            }
        );
    }

    #[test]
    fn a_missing_parent_ends_the_walk() {
        let orphan = Folder {
            parent: Some(Uuid::new_v4()),
            ..folder(
                "orphan",
                None,
                Settings {
                    username: Some("orphan".into()),
                    ..Default::default()
                },
            )
        };
        let connection = connection_in(&orphan, Settings::default());

        let resolved = resolve(&connection, &tree(&[&orphan]), Some(22)).unwrap();

        assert_eq!(resolved.username.as_deref(), Some("orphan"));
    }

    #[test]
    fn a_loop_of_folders_ends_the_walk() {
        let mut first = folder(
            "first",
            None,
            Settings {
                username: Some("first".into()),
                ..Default::default()
            },
        );
        let second = folder(
            "second",
            Some(&first),
            Settings {
                port: Some(2200),
                ..Default::default()
            },
        );
        first.parent = Some(second.id);
        let connection = connection_in(&first, Settings::default());

        let resolved = resolve(&connection, &tree(&[&first, &second]), Some(22)).unwrap();

        assert_eq!(resolved.username.as_deref(), Some("first"));
        assert_eq!(resolved.port, 2200);
    }

    #[test]
    fn moving_reports_only_the_fields_that_change() {
        let options = json!({ "term": "vt100" }).as_object().unwrap().clone();
        let from = folder(
            "from",
            None,
            Settings {
                username: Some("old".into()),
                port: Some(2200),
                environment: Some(Environment::Dev),
                ..Default::default()
            },
        );
        let to = folder(
            "to",
            None,
            Settings {
                username: Some("new".into()),
                port: Some(2200),
                options,
                ..Default::default()
            },
        );
        let folders = tree(&[&from, &to]);

        let changes =
            inheritance_changes(&Settings::default(), Some(from.id), Some(to.id), &folders);

        let change = |field: &str, before: Option<&str>, after: Option<&str>| FieldChange {
            field: field.into(),
            before: before.map(Into::into),
            after: after.map(Into::into),
        };
        assert_eq!(
            changes,
            [
                change("username", Some("old"), Some("new")),
                change("environment", Some("dev"), Some("none")),
                change("term", None, Some("vt100")),
            ]
        );
    }

    #[test]
    fn an_override_is_not_changed_by_moving() {
        let to = folder(
            "to",
            None,
            Settings {
                username: Some("new".into()),
                ..Default::default()
            },
        );
        let own = Settings {
            username: Some("mine".into()),
            ..Default::default()
        };

        assert!(inheritance_changes(&own, None, Some(to.id), &tree(&[&to])).is_empty());
    }

    #[test]
    fn a_list_reads_from_an_array_and_from_the_older_comma_text() {
        use serde_json::json;
        assert_eq!(
            option_list(Some(&json!(["~/.ssh/a,b", " /c ", ""]))),
            ["~/.ssh/a,b", "/c"]
        );
        assert_eq!(
            option_list(Some(&json!("~/.ssh/a, /b ,,"))),
            ["~/.ssh/a", "/b"]
        );
        assert!(option_list(Some(&json!(5))).is_empty());
        assert!(option_list(None).is_empty());
    }

    #[test]
    fn the_outside_layer_fills_only_what_the_connection_and_folders_leave() {
        use serde_json::json;
        let mut folder = Folder::new("Databases");
        folder.settings.username = Some("postgres".into());
        let folders = BTreeMap::from([(folder.id, folder.clone())]);
        let mut connection = Connection::new("db", "ssh", "db");
        connection.folder = Some(folder.id);
        connection
            .settings
            .options
            .insert("identity_file".into(), json!(["~/.ssh/own"]));
        let outside = Outside {
            source: "~/.ssh/config".into(),
            hostname: Some("db.internal".into()),
            settings: Settings {
                port: Some(2222),
                username: Some("from-file".into()),
                options: json!({ "identity_file": ["~/.ssh/file"], "identities_only": "yes" })
                    .as_object()
                    .unwrap()
                    .clone(),
                ..Settings::default()
            },
        };
        let resolved = resolve_over(&connection, &folders, Some(&outside), Some(22)).unwrap();
        assert_eq!(resolved.host, "db.internal");
        assert_eq!(resolved.port, 2222);
        assert_eq!(resolved.username.as_deref(), Some("postgres"));
        assert_eq!(resolved.options["identity_file"], json!(["~/.ssh/own"]));
        assert_eq!(resolved.options["identities_only"], json!("yes"));
    }
}
