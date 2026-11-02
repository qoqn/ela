use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Connection, CredentialRef, FieldChange, Folder, ItemRef, Settings, inheritance_changes,
};

type Migration = fn(&mut toml::Table);

const TRASH_DIR: &str = "trash";
const SETTINGS_FILE: &str = "settings.toml";

trait Item: Serialize + DeserializeOwned {
    const DIR: &'static str;
    const MIGRATIONS: &'static [Migration];
    fn id(&self) -> Uuid;
}

impl Item for Connection {
    const DIR: &'static str = "connections";
    const MIGRATIONS: &'static [Migration] = &[];
    fn id(&self) -> Uuid {
        self.id
    }
}

impl Item for Folder {
    const DIR: &'static str = "folders";
    const MIGRATIONS: &'static [Migration] = &[];
    fn id(&self) -> Uuid {
        self.id
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Can't find the config folder.")]
    NoConfigDir,
    #[error("Can't use {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("Can't write this as TOML: {0}")]
    Encode(#[from] toml::ser::Error),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("That item no longer exists.")]
    NotFound,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DecodeError {
    #[error("not valid TOML: {0}")]
    Syntax(#[from] toml::de::Error),
    #[error("schema_version is missing or isn't a positive whole number")]
    BadVersion,
    #[error(
        "schema_version {found} comes from a newer Ela, which is needed to open it (this one reads up to {supported})"
    )]
    TooNew { found: i64, supported: i64 },
    #[error("the file is named for a different id than the {0} it contains")]
    WrongName(Uuid),
}

#[derive(Debug)]
pub struct LoadProblem {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_idle_minutes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_font: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_font_size: Option<u16>,
    // What another version of Ela or a hand edit put here isn't this version's to drop.
    #[serde(flatten)]
    other: toml::Table,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct TrashEntry {
    pub item: ItemRef,
    pub name: String,
    pub host: Option<String>,
    pub inside: Vec<ItemRef>,
}

#[derive(Debug)]
pub struct Trash {
    pub entries: Vec<TrashEntry>,
    pub problems: Vec<LoadProblem>,
    pub unused_credentials: Vec<CredentialRef>,
}

#[derive(Serialize)]
pub(crate) struct Versioned<'a, T> {
    pub schema_version: i64,
    #[serde(flatten)]
    pub item: &'a T,
}

pub(crate) fn current_version(migrations: &[Migration]) -> i64 {
    migrations.len() as i64 + 1
}

fn encode<T: Item>(item: &T) -> Result<String, toml::ser::Error> {
    toml::to_string(&Versioned {
        schema_version: current_version(T::MIGRATIONS),
        item,
    })
}

fn decode<T: Item>(text: &str) -> Result<T, DecodeError> {
    decode_with(text, T::MIGRATIONS)
}

/// Migrated files are not written back here: that happens when the item is next saved, so
/// opening Ela doesn't touch every file in a synced folder.
pub(crate) fn decode_with<T: DeserializeOwned>(
    text: &str,
    migrations: &[Migration],
) -> Result<T, DecodeError> {
    let mut table: toml::Table = text.parse()?;
    let found = match table.remove("schema_version") {
        Some(toml::Value::Integer(version)) if version >= 1 => version,
        _ => return Err(DecodeError::BadVersion),
    };
    let supported = current_version(migrations);
    if found > supported {
        return Err(DecodeError::TooNew { found, supported });
    }
    for migrate in &migrations[found as usize - 1..] {
        migrate(&mut table);
    }
    Ok(table.try_into()?)
}

fn load_all<T: Item>(
    root: &Path,
    problems: &mut Vec<LoadProblem>,
) -> Result<BTreeMap<Uuid, T>, StoreError> {
    let dir = root.join(T::DIR);
    let mut items = BTreeMap::new();
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(items),
        Err(source) => return Err(StoreError::Io { path: dir, source }),
    };
    for entry in entries {
        let path = entry
            .map_err(|source| StoreError::Io {
                path: dir.clone(),
                source,
            })?
            .path();
        if path.extension().is_none_or(|extension| extension != "toml") {
            continue;
        }
        match load_one::<T>(&path) {
            Ok(item) => {
                items.insert(item.id(), item);
            }
            Err(message) => {
                tracing::warn!(path = %path.display(), %message, "skipped a saved file");
                problems.push(LoadProblem { path, message });
            }
        }
    }
    Ok(items)
}

fn load_one<T: Item>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let item: T = decode(&text).map_err(|error| error.to_string())?;
    // Sync tools leave copies such as `<uuid>.sync-conflict-1.toml`, which must not win over the original.
    if path.file_stem().and_then(|stem| stem.to_str()) != Some(item.id().to_string().as_str()) {
        return Err(DecodeError::WrongName(item.id()).to_string());
    }
    Ok(item)
}

fn write_item<T: Item>(root: &Path, item: &T) -> Result<(), StoreError> {
    let path = root.join(T::DIR).join(format!("{}.toml", item.id()));
    write_atomically(&path, &encode(item)?)
}

/// Replaces the file in one step, so a crash leaves either the old version or the new one.
pub(crate) fn write_atomically(path: &Path, text: &str) -> Result<(), StoreError> {
    let io_error = |source| StoreError::Io {
        path: path.to_owned(),
        source,
    };
    let dir = path.parent().unwrap_or(path);
    fs::create_dir_all(dir).map_err(io_error)?;
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(io_error)?;
    file.write_all(text.as_bytes()).map_err(io_error)?;
    file.as_file().sync_all().map_err(io_error)?;
    file.persist(path).map_err(|error| io_error(error.error))?;
    Ok(())
}

fn item_path<T: Item>(root: &Path, id: Uuid) -> PathBuf {
    root.join(T::DIR).join(format!("{id}.toml"))
}

fn trash_path<T: Item>(root: &Path, id: Uuid) -> PathBuf {
    root.join(TRASH_DIR).join(T::DIR).join(format!("{id}.toml"))
}

fn move_file(from: &Path, to: &Path) -> Result<(), StoreError> {
    let io_error = |source| StoreError::Io {
        path: from.to_owned(),
        source,
    };
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir).map_err(io_error)?;
    }
    fs::rename(from, to).map_err(io_error)
}

fn require_text(value: &str, message: &'static str) -> Result<(), StoreError> {
    if value.trim().is_empty() {
        Err(StoreError::Invalid(message))
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub struct Store {
    root: PathBuf,
    connections: BTreeMap<Uuid, Connection>,
    folders: BTreeMap<Uuid, Folder>,
    problems: Vec<LoadProblem>,
}

impl Store {
    pub fn open_default() -> Result<Self, StoreError> {
        let root = dirs::config_dir()
            .ok_or(StoreError::NoConfigDir)?
            .join("ela");
        Self::open(root)
    }

    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        let mut problems = Vec::new();
        let connections = load_all(&root, &mut problems)?;
        let folders = load_all(&root, &mut problems)?;
        Ok(Self {
            root,
            connections,
            folders,
            problems,
        })
    }

    pub fn connections(&self) -> &BTreeMap<Uuid, Connection> {
        &self.connections
    }

    pub fn connection_in_trash(&self, id: Uuid) -> bool {
        trash_path::<Connection>(&self.root, id).exists()
    }

    pub fn folders(&self) -> &BTreeMap<Uuid, Folder> {
        &self.folders
    }

    pub fn problems(&self) -> &[LoadProblem] {
        &self.problems
    }

    pub fn save_connection(&mut self, connection: Connection) -> Result<(), StoreError> {
        require_text(&connection.name, "A connection needs a name.")?;
        require_text(&connection.host, "A connection needs a host.")?;
        require_text(&connection.protocol, "A connection needs a protocol.")?;
        write_item(&self.root, &connection)?;
        self.connections.insert(connection.id, connection);
        Ok(())
    }

    pub fn save_folder(&mut self, folder: Folder) -> Result<(), StoreError> {
        require_text(&folder.name, "A folder needs a name.")?;
        self.check_parent(folder.id, folder.parent)?;
        write_item(&self.root, &folder)?;
        self.folders.insert(folder.id, folder);
        Ok(())
    }

    fn check_parent(&self, id: Uuid, parent: Option<Uuid>) -> Result<(), StoreError> {
        let mut seen = BTreeSet::new();
        let mut next = parent;
        while let Some(current) = next {
            if current == id {
                return Err(StoreError::Invalid("A folder can't be moved into itself."));
            }
            if !seen.insert(current) {
                break;
            }
            next = self.folders.get(&current).and_then(|folder| folder.parent);
        }
        Ok(())
    }

    pub fn move_item(&mut self, item: ItemRef, to: Option<Uuid>) -> Result<(), StoreError> {
        match item {
            ItemRef::Connection(id) => {
                let connection = self.connections.get(&id).ok_or(StoreError::NotFound)?;
                self.save_connection(Connection {
                    folder: to,
                    ..connection.clone()
                })
            }
            ItemRef::Folder(id) => {
                let folder = self.folders.get(&id).ok_or(StoreError::NotFound)?;
                self.save_folder(Folder {
                    parent: to,
                    ..folder.clone()
                })
            }
        }
    }

    pub fn move_changes(
        &self,
        item: ItemRef,
        to: Option<Uuid>,
    ) -> Result<Vec<FieldChange>, StoreError> {
        let (settings, from) = match item {
            ItemRef::Connection(id) => {
                let connection = self.connections.get(&id).ok_or(StoreError::NotFound)?;
                (&connection.settings, connection.folder)
            }
            ItemRef::Folder(id) => {
                let folder = self.folders.get(&id).ok_or(StoreError::NotFound)?;
                self.check_parent(id, to)?;
                (&folder.settings, folder.parent)
            }
        };
        Ok(inheritance_changes(settings, from, to, &self.folders))
    }

    pub fn trash(&mut self, item: ItemRef) -> Result<Vec<ItemRef>, StoreError> {
        let items = with_contents(item, &self.connections, &self.folders);
        for (index, &each) in items.iter().enumerate() {
            let (from, to) = match each {
                ItemRef::Connection(id) => (
                    item_path::<Connection>(&self.root, id),
                    trash_path::<Connection>(&self.root, id),
                ),
                ItemRef::Folder(id) => (
                    item_path::<Folder>(&self.root, id),
                    trash_path::<Folder>(&self.root, id),
                ),
            };
            if let Err(error) = move_file(&from, &to) {
                // All or nothing, so a half-trashed Folder can't scatter its contents.
                self.put_back(&items[..index]);
                return Err(error);
            }
        }
        for each in &items {
            match each {
                ItemRef::Connection(id) => drop(self.connections.remove(id)),
                ItemRef::Folder(id) => drop(self.folders.remove(id)),
            }
        }
        Ok(items)
    }

    fn put_back(&self, items: &[ItemRef]) {
        for &item in items {
            let (from, to) = match item {
                ItemRef::Connection(id) => (
                    trash_path::<Connection>(&self.root, id),
                    item_path::<Connection>(&self.root, id),
                ),
                ItemRef::Folder(id) => (
                    trash_path::<Folder>(&self.root, id),
                    item_path::<Folder>(&self.root, id),
                ),
            };
            if let Err(error) = move_file(&from, &to) {
                tracing::error!(%error, "couldn't take an item back out of the Trash");
            }
        }
    }

    pub fn restore(&mut self, items: &[ItemRef]) -> Result<(), StoreError> {
        for &item in items {
            match item {
                ItemRef::Connection(id) => {
                    let connection = restore_one::<Connection>(&self.root, id)?;
                    self.connections.insert(id, connection);
                }
                ItemRef::Folder(id) => {
                    let folder = restore_one::<Folder>(&self.root, id)?;
                    self.folders.insert(id, folder);
                }
            }
        }
        Ok(())
    }

    pub fn app_settings(&self) -> Result<AppSettings, StoreError> {
        let path = self.root.join(SETTINGS_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(AppSettings::default());
            }
            Err(source) => return Err(StoreError::Io { path, source }),
        };
        decode_with(&text, &[]).map_err(|error| StoreError::Io {
            path,
            source: io::Error::other(error.to_string()),
        })
    }

    /// Fails rather than starting afresh when the file can't be read, which would overwrite it.
    pub fn update_app_settings(
        &self,
        change: impl FnOnce(&mut AppSettings),
    ) -> Result<(), StoreError> {
        let mut settings = self.app_settings()?;
        change(&mut settings);
        let text = toml::to_string(&Versioned {
            schema_version: current_version(&[]),
            item: &settings,
        })?;
        write_atomically(&self.root.join(SETTINGS_FILE), &text)
    }

    /// Read from disk each time, because `trash/` isn't loaded and sync tools can change any of it.
    pub fn trash_contents(&self) -> Result<Trash, StoreError> {
        let trash = self.root.join(TRASH_DIR);
        let mut problems = Vec::new();
        let connections: BTreeMap<Uuid, Connection> = load_all(&trash, &mut problems)?;
        let folders: BTreeMap<Uuid, Folder> = load_all(&trash, &mut problems)?;
        let unused_credentials = self.unused_credentials(&connections, &folders, &problems)?;
        Ok(Trash {
            entries: trash_entries(&connections, &folders),
            problems,
            unused_credentials,
        })
    }

    fn unused_credentials(
        &self,
        connections: &BTreeMap<Uuid, Connection>,
        folders: &BTreeMap<Uuid, Folder>,
        trash_problems: &[LoadProblem],
    ) -> Result<Vec<CredentialRef>, StoreError> {
        let mut live_problems = Vec::new();
        let live_connections: BTreeMap<Uuid, Connection> =
            load_all(&self.root, &mut live_problems)?;
        let live_folders: BTreeMap<Uuid, Folder> = load_all(&self.root, &mut live_problems)?;
        let in_use = credentials_of(&live_connections, &live_folders);
        let unreadable: Vec<&Path> = live_problems
            .iter()
            .chain(trash_problems)
            .map(|problem| problem.path.as_path())
            .collect();
        Ok(credentials_of(connections, folders)
            .difference(&in_use)
            .filter(|&&id| !unreadable.iter().any(|path| may_use_credential(path, id)))
            .map(|&id| CredentialRef(id))
            .collect())
    }

    /// Files that can't be read stay, like everywhere else: they may come from a newer Ela.
    pub fn empty_trash(&mut self) -> Result<usize, StoreError> {
        let trash = self.root.join(TRASH_DIR);
        let mut problems = Vec::new();
        let connections: BTreeMap<Uuid, Connection> = load_all(&trash, &mut problems)?;
        let folders: BTreeMap<Uuid, Folder> = load_all(&trash, &mut problems)?;
        let paths = connections
            .keys()
            .map(|&id| trash_path::<Connection>(&self.root, id))
            .chain(
                folders
                    .keys()
                    .map(|&id| trash_path::<Folder>(&self.root, id)),
            );
        let mut removed = 0;
        for path in paths {
            fs::remove_file(&path).map_err(|source| StoreError::Io { path, source })?;
            removed += 1;
        }
        Ok(removed)
    }
}

fn with_contents(
    item: ItemRef,
    connections: &BTreeMap<Uuid, Connection>,
    all_folders: &BTreeMap<Uuid, Folder>,
) -> Vec<ItemRef> {
    let ItemRef::Folder(top) = item else {
        return vec![item];
    };
    let mut folders = BTreeSet::from([top]);
    let mut order = vec![top];
    let mut index = 0;
    while let Some(&current) = order.get(index) {
        index += 1;
        for folder in all_folders.values() {
            if folder.parent == Some(current) && folders.insert(folder.id) {
                order.push(folder.id);
            }
        }
    }
    let inside = connections
        .values()
        .filter(|c| c.folder.is_some_and(|f| folders.contains(&f)));
    let mut items: Vec<ItemRef> = inside.map(|c| ItemRef::Connection(c.id)).collect();
    items.extend(order.into_iter().rev().map(ItemRef::Folder));
    items
}

fn id_of(item: ItemRef) -> Uuid {
    match item {
        ItemRef::Connection(id) | ItemRef::Folder(id) => id,
    }
}

fn trash_entries(
    connections: &BTreeMap<Uuid, Connection>,
    folders: &BTreeMap<Uuid, Folder>,
) -> Vec<TrashEntry> {
    let in_trash = |parent: Option<Uuid>| parent.is_some_and(|id| folders.contains_key(&id));
    let top_folders = folders
        .values()
        .filter(|f| !in_trash(f.parent))
        .map(|f| ItemRef::Folder(f.id));
    let top_connections = connections
        .values()
        .filter(|c| !in_trash(c.folder))
        .map(|c| ItemRef::Connection(c.id));
    // Folders caught in a loop have no top, so going through all of them last keeps them listed.
    let every_folder = folders.keys().map(|&id| ItemRef::Folder(id));

    let mut covered = BTreeSet::new();
    let mut entries = Vec::new();
    for item in top_folders.chain(top_connections).chain(every_folder) {
        if covered.contains(&id_of(item)) {
            continue;
        }
        let mut inside = with_contents(item, connections, folders);
        covered.extend(inside.iter().map(|&each| id_of(each)));
        inside.pop();
        let (name, host) = match item {
            ItemRef::Connection(id) => (
                connections[&id].name.clone(),
                Some(connections[&id].host.clone()),
            ),
            ItemRef::Folder(id) => (folders[&id].name.clone(), None),
        };
        entries.push(TrashEntry {
            item,
            name,
            host,
            inside,
        });
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries
}

fn credentials_of(
    connections: &BTreeMap<Uuid, Connection>,
    folders: &BTreeMap<Uuid, Folder>,
) -> BTreeSet<Uuid> {
    let settings: Vec<&Settings> = connections
        .values()
        .map(|c| &c.settings)
        .chain(folders.values().map(|f| &f.settings))
        .collect();
    settings
        .iter()
        .filter_map(|s| s.credential.map(|reference| reference.0))
        .collect()
}

/// Removing a Secret can't be undone, so a file that can't be parsed still counts if it names it,
/// and one that can't be read at all counts whatever it says.
fn may_use_credential(path: &Path, id: Uuid) -> bool {
    match fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).contains(&id.to_string()),
        Err(error) => error.kind() != io::ErrorKind::NotFound,
    }
}

fn restore_one<T: Item>(root: &Path, id: Uuid) -> Result<T, StoreError> {
    let from = trash_path::<T>(root, id);
    let to = item_path::<T>(root, id);
    move_file(&from, &to)?;
    load_one(&to).map_err(|message| StoreError::Io {
        path: to,
        source: io::Error::other(message),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{CredentialRef, Environment, Settings};

    fn full_connection() -> Connection {
        Connection {
            folder: Some(Uuid::new_v4()),
            tags: vec!["web".into(), "eu".into()],
            settings: Settings {
                port: Some(2222),
                username: Some("root".into()),
                credential: Some(CredentialRef(Uuid::new_v4())),
                environment: Some(Environment::Prod),
                options:
                    json!({ "term": "xterm", "retries": 3, "nested": { "ratio": 1.5, "on": true, "list": [1, 2] } })
                        .as_object()
                        .unwrap()
                        .clone(),
            },
            ..Connection::new("web-1", "ssh", "10.0.0.1")
        }
    }

    fn connection_file(root: &Path, id: Uuid) -> PathBuf {
        root.join("connections").join(format!("{id}.toml"))
    }

    #[test]
    fn items_survive_a_restart() {
        let root = tempfile::tempdir().unwrap();
        let connection = full_connection();
        let bare = Connection::new("bare", "rdp", "10.0.0.2");
        let parent = Folder::new("parent");
        let child = Folder {
            parent: Some(parent.id),
            settings: Settings {
                username: Some("admin".into()),
                ..Default::default()
            },
            ..Folder::new("child")
        };

        let mut store = Store::open(root.path()).unwrap();
        store.save_connection(connection.clone()).unwrap();
        store.save_connection(bare.clone()).unwrap();
        store.save_folder(parent.clone()).unwrap();
        store.save_folder(child.clone()).unwrap();
        drop(store);

        let store = Store::open(root.path()).unwrap();
        assert!(store.problems().is_empty(), "{:?}", store.problems());
        assert_eq!(store.connections()[&connection.id], connection);
        assert_eq!(store.connections()[&bare.id], bare);
        assert_eq!(store.folders()[&parent.id], parent);
        assert_eq!(store.folders()[&child.id], child);
    }

    #[test]
    fn a_file_is_readable_and_starts_with_its_version() {
        let root = tempfile::tempdir().unwrap();
        let connection = full_connection();
        Store::open(root.path())
            .unwrap()
            .save_connection(connection.clone())
            .unwrap();

        let text = fs::read_to_string(connection_file(root.path(), connection.id)).unwrap();

        assert!(text.starts_with("schema_version = 1\n"), "{text}");
        assert!(text.contains(r#"host = "10.0.0.1""#), "{text}");
        assert!(text.contains("[options"), "{text}");
    }

    #[test]
    fn an_absent_folder_is_an_empty_tree() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().join("not-yet")).unwrap();
        assert!(
            store.connections().is_empty()
                && store.folders().is_empty()
                && store.problems().is_empty()
        );
    }

    #[test]
    fn saving_again_replaces_the_item_and_leaves_no_temporary_files() {
        let root = tempfile::tempdir().unwrap();
        let mut connection = full_connection();
        let mut store = Store::open(root.path()).unwrap();
        store.save_connection(connection.clone()).unwrap();
        connection.host = "10.9.9.9".into();
        store.save_connection(connection.clone()).unwrap();

        assert_eq!(
            fs::read_dir(root.path().join("connections"))
                .unwrap()
                .count(),
            1
        );
        assert_eq!(
            Store::open(root.path()).unwrap().connections()[&connection.id].host,
            "10.9.9.9"
        );
    }

    #[test]
    fn a_value_toml_cannot_hold_is_refused_and_the_old_file_stays() {
        let root = tempfile::tempdir().unwrap();
        let good = full_connection();
        let mut store = Store::open(root.path()).unwrap();
        store.save_connection(good.clone()).unwrap();

        let mut bad = good.clone();
        bad.settings
            .options
            .insert("nothing".into(), serde_json::Value::Null);
        assert!(matches!(
            store.save_connection(bad),
            Err(StoreError::Encode(_))
        ));

        assert_eq!(store.connections()[&good.id], good);
        assert_eq!(
            Store::open(root.path()).unwrap().connections()[&good.id],
            good
        );
    }

    fn nested(store: &mut Store) -> (Folder, Folder, Connection, Connection) {
        let parent = Folder::new("parent");
        let child = Folder {
            parent: Some(parent.id),
            ..Folder::new("child")
        };
        let inner = Connection {
            folder: Some(child.id),
            ..Connection::new("inner", "ssh", "10.0.0.3")
        };
        let direct = Connection {
            folder: Some(parent.id),
            ..Connection::new("direct", "ssh", "10.0.0.4")
        };
        store.save_folder(parent.clone()).unwrap();
        store.save_folder(child.clone()).unwrap();
        store.save_connection(inner.clone()).unwrap();
        store.save_connection(direct.clone()).unwrap();
        (parent, child, inner, direct)
    }

    #[test]
    fn a_trashed_folder_takes_its_contents_and_survives_a_restart_in_the_trash() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (parent, _, inner, _) = nested(&mut store);
        let outside = Connection::new("outside", "ssh", "10.0.0.5");
        store.save_connection(outside.clone()).unwrap();

        let moved = store.trash(ItemRef::Folder(parent.id)).unwrap();

        assert_eq!(moved.len(), 4);
        assert_eq!(store.connections().len(), 1);
        assert!(store.folders().is_empty());
        let reopened = Store::open(root.path()).unwrap();
        assert_eq!(
            reopened.connections().keys().collect::<Vec<_>>(),
            [&outside.id]
        );
        assert!(
            root.path()
                .join("trash/connections")
                .join(format!("{}.toml", inner.id))
                .exists()
        );
    }

    #[test]
    fn undo_brings_back_exactly_what_was_trashed() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (parent, child, inner, direct) = nested(&mut store);

        let moved = store.trash(ItemRef::Folder(parent.id)).unwrap();
        store.restore(&moved).unwrap();

        for store in [store, Store::open(root.path()).unwrap()] {
            assert_eq!(store.folders()[&parent.id], parent);
            assert_eq!(store.folders()[&child.id], child);
            assert_eq!(store.connections()[&inner.id], inner);
            assert_eq!(store.connections()[&direct.id], direct);
        }
    }

    #[test]
    fn trashing_a_connection_leaves_its_folder() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (parent, _, inner, _) = nested(&mut store);

        assert_eq!(
            store.trash(ItemRef::Connection(inner.id)).unwrap(),
            [ItemRef::Connection(inner.id)]
        );
        assert!(store.folders().contains_key(&parent.id));
    }

    #[test]
    fn a_failed_trash_changes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (parent, child, inner, direct) = nested(&mut store);
        fs::remove_file(item_path::<Folder>(root.path(), child.id)).unwrap();

        assert!(store.trash(ItemRef::Folder(parent.id)).is_err());

        assert!(item_path::<Connection>(root.path(), inner.id).exists());
        assert!(item_path::<Connection>(root.path(), direct.id).exists());
        assert_eq!(store.connections().len(), 2);
    }

    fn with_credential(mut connection: Connection, id: Uuid) -> Connection {
        connection.settings.credential = Some(CredentialRef(id));
        connection
    }

    fn unused(store: &Store) -> Vec<Uuid> {
        let found = store.trash_contents().unwrap().unused_credentials;
        found.into_iter().map(|reference| reference.0).collect()
    }

    #[test]
    fn an_absent_trash_is_empty() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let trash = store.trash_contents().unwrap();
        assert!(trash.entries.is_empty() && trash.problems.is_empty());
        assert_eq!(store.empty_trash().unwrap(), 0);
    }

    #[test]
    fn the_trash_lists_a_folder_once_with_its_contents_after_a_restart() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (parent, child, inner, direct) = nested(&mut store);
        let alone = Connection::new("alone", "ssh", "10.0.0.5");
        store.save_connection(alone.clone()).unwrap();
        store.trash(ItemRef::Folder(parent.id)).unwrap();
        store.trash(ItemRef::Connection(alone.id)).unwrap();
        drop(store);

        let trash = Store::open(root.path()).unwrap().trash_contents().unwrap();

        assert_eq!(trash.entries.len(), 2);
        assert_eq!(trash.entries[0].name, "alone");
        assert_eq!(trash.entries[0].host.as_deref(), Some("10.0.0.5"));
        assert!(trash.entries[0].inside.is_empty());
        let folder = &trash.entries[1];
        assert_eq!(folder.item, ItemRef::Folder(parent.id));
        assert_eq!(folder.host, None);
        for each in [
            ItemRef::Folder(child.id),
            ItemRef::Connection(inner.id),
            ItemRef::Connection(direct.id),
        ] {
            assert!(folder.inside.contains(&each), "{each:?}");
        }
        assert_eq!(folder.inside.len(), 3);
    }

    #[test]
    fn something_trashed_before_its_folder_is_listed_inside_it() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (parent, _, inner, _) = nested(&mut store);
        store.trash(ItemRef::Connection(inner.id)).unwrap();
        store.trash(ItemRef::Folder(parent.id)).unwrap();

        let trash = store.trash_contents().unwrap();

        assert_eq!(trash.entries.len(), 1);
        assert!(
            trash.entries[0]
                .inside
                .contains(&ItemRef::Connection(inner.id))
        );
    }

    #[test]
    fn restoring_an_entry_and_its_contents_takes_them_out_of_the_trash() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (parent, child, inner, direct) = nested(&mut store);
        store.trash(ItemRef::Folder(parent.id)).unwrap();

        let entry = store.trash_contents().unwrap().entries.remove(0);
        let mut items = entry.inside;
        items.push(entry.item);
        store.restore(&items).unwrap();

        assert!(store.trash_contents().unwrap().entries.is_empty());
        let store = Store::open(root.path()).unwrap();
        assert_eq!(store.folders()[&child.id], child);
        assert_eq!(store.connections()[&inner.id], inner);
        assert_eq!(store.connections()[&direct.id], direct);
    }

    #[test]
    fn folders_in_a_loop_are_still_listed_and_emptied() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
        for (id, parent) in [(first, second), (second, first)] {
            let folder = Folder {
                id,
                parent: Some(parent),
                ..Folder::new("loop")
            };
            let path = trash_path::<Folder>(root.path(), id);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, encode(&folder).unwrap()).unwrap();
        }
        let mut store = store;

        let trash = store.trash_contents().unwrap();
        assert_eq!(trash.entries.len(), 1);
        assert_eq!(trash.entries[0].inside.len(), 1);
        assert_eq!(store.empty_trash().unwrap(), 2);
    }

    #[test]
    fn emptying_removes_only_the_trash_and_leaves_files_it_cannot_read() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (parent, _, inner, _) = nested(&mut store);
        let kept = Connection::new("kept", "ssh", "10.0.0.5");
        store.save_connection(kept.clone()).unwrap();
        store.trash(ItemRef::Folder(parent.id)).unwrap();
        let newer = root
            .path()
            .join("trash/connections")
            .join(format!("{}.toml", Uuid::new_v4()));
        fs::write(&newer, "schema_version = 99\n").unwrap();

        assert_eq!(store.trash_contents().unwrap().problems.len(), 1);
        assert_eq!(store.empty_trash().unwrap(), 4);

        let trash = store.trash_contents().unwrap();
        assert!(trash.entries.is_empty());
        assert_eq!(trash.problems[0].path, newer);
        assert!(newer.exists());
        assert!(!trash_path::<Connection>(root.path(), inner.id).exists());
        let store = Store::open(root.path()).unwrap();
        assert_eq!(store.connections()[&kept.id], kept);
    }

    #[test]
    fn a_credential_only_the_trash_uses_is_unused_and_counted_once() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let shared = Uuid::new_v4();
        for name in ["one", "two"] {
            let connection = with_credential(Connection::new(name, "ssh", "10.0.0.1"), shared);
            store.save_connection(connection.clone()).unwrap();
            store.trash(ItemRef::Connection(connection.id)).unwrap();
        }
        assert_eq!(unused(&store), [shared]);
    }

    #[test]
    fn a_folder_in_the_trash_counts_for_its_credential() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let id = Uuid::new_v4();
        let mut folder = Folder::new("folder");
        folder.settings.credential = Some(CredentialRef(id));
        store.save_folder(folder.clone()).unwrap();
        store.trash(ItemRef::Folder(folder.id)).unwrap();
        assert_eq!(unused(&store), [id]);
    }

    #[test]
    fn a_credential_something_saved_still_uses_stays() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (by_connection, by_folder) = (Uuid::new_v4(), Uuid::new_v4());
        store
            .save_connection(with_credential(
                Connection::new("live", "ssh", "10.0.0.1"),
                by_connection,
            ))
            .unwrap();
        let mut folder = Folder::new("live folder");
        folder.settings.credential = Some(CredentialRef(by_folder));
        store.save_folder(folder).unwrap();
        for id in [by_connection, by_folder] {
            let gone = with_credential(Connection::new("gone", "ssh", "10.0.0.2"), id);
            store.save_connection(gone.clone()).unwrap();
            store.trash(ItemRef::Connection(gone.id)).unwrap();
        }
        assert!(unused(&store).is_empty());
    }

    #[test]
    fn a_credential_named_by_a_file_that_cannot_be_parsed_stays() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (named, other) = (Uuid::new_v4(), Uuid::new_v4());
        for id in [named, other] {
            let gone = with_credential(Connection::new("gone", "ssh", "10.0.0.2"), id);
            store.save_connection(gone.clone()).unwrap();
            store.trash(ItemRef::Connection(gone.id)).unwrap();
        }
        let live = connection_file(root.path(), Uuid::new_v4());
        fs::write(
            &live,
            format!("schema_version = 99\ncredential = \"{named}\"\n"),
        )
        .unwrap();

        assert_eq!(unused(&store), [other]);
    }

    #[test]
    fn a_credential_named_only_by_an_unreadable_file_in_the_trash_stays() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let id = Uuid::new_v4();
        let gone = with_credential(Connection::new("gone", "ssh", "10.0.0.2"), id);
        store.save_connection(gone.clone()).unwrap();
        store.trash(ItemRef::Connection(gone.id)).unwrap();
        let conflict = root
            .path()
            .join("trash/connections")
            .join(format!("{}.sync-conflict-1.toml", gone.id));
        fs::write(
            &conflict,
            format!("schema_version = 99\ncredential = \"{id}\"\n"),
        )
        .unwrap();

        assert!(unused(&store).is_empty());
    }

    #[test]
    fn settings_start_empty_and_survive_a_restart() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().join("not-yet")).unwrap();
        assert_eq!(store.app_settings().unwrap().vault_idle_minutes, None);

        store
            .update_app_settings(|s| s.vault_idle_minutes = Some(30))
            .unwrap();

        let store = Store::open(root.path().join("not-yet")).unwrap();
        assert_eq!(store.app_settings().unwrap().vault_idle_minutes, Some(30));
        let text = fs::read_to_string(root.path().join("not-yet/settings.toml")).unwrap();
        assert!(text.starts_with("schema_version = 1\n"), "{text}");
    }

    #[test]
    fn saving_a_setting_keeps_what_this_version_does_not_know() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("settings.toml");
        fs::write(
            &path,
            "schema_version = 1\nsomething_new = \"kept\"\n\n[table]\nalso = 1\n",
        )
        .unwrap();
        let store = Store::open(root.path()).unwrap();

        store
            .update_app_settings(|s| s.vault_idle_minutes = Some(5))
            .unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("something_new = \"kept\""), "{text}");
        assert!(text.contains("also = 1"), "{text}");
        assert_eq!(store.app_settings().unwrap().vault_idle_minutes, Some(5));
    }

    #[test]
    fn each_setting_is_saved_without_touching_the_others() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();

        store
            .update_app_settings(|s| s.vault_idle_minutes = Some(30))
            .unwrap();
        store
            .update_app_settings(|s| s.host_key = Some("AltRight".to_owned()))
            .unwrap();

        let settings = Store::open(root.path()).unwrap().app_settings().unwrap();
        assert_eq!(settings.vault_idle_minutes, Some(30));
        assert_eq!(settings.host_key.as_deref(), Some("AltRight"));
    }

    #[test]
    fn settings_that_cannot_be_read_are_reported_and_never_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("settings.toml");
        let store = Store::open(root.path()).unwrap();
        for text in ["schema_version = 99\nnew = true\n", "this is = = not toml"] {
            fs::write(&path, text).unwrap();
            assert!(store.app_settings().is_err(), "{text}");
            assert!(
                store
                    .update_app_settings(|s| s.vault_idle_minutes = Some(5))
                    .is_err(),
                "{text}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
    }

    #[test]
    fn moving_changes_the_parent_on_disk() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (parent, child, inner, _) = nested(&mut store);

        store
            .move_item(ItemRef::Connection(inner.id), Some(parent.id))
            .unwrap();
        store.move_item(ItemRef::Folder(child.id), None).unwrap();

        let store = Store::open(root.path()).unwrap();
        assert_eq!(store.connections()[&inner.id].folder, Some(parent.id));
        assert_eq!(store.folders()[&child.id].parent, None);
    }

    #[test]
    fn a_folder_cannot_move_into_itself_or_its_contents() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let (parent, child, _, _) = nested(&mut store);

        for target in [parent.id, child.id] {
            let item = ItemRef::Folder(parent.id);
            assert!(matches!(
                store.move_item(item, Some(target)),
                Err(StoreError::Invalid(_))
            ));
            assert!(matches!(
                store.move_changes(item, Some(target)),
                Err(StoreError::Invalid(_))
            ));
        }
        assert_eq!(store.folders()[&parent.id].parent, None);
    }

    #[test]
    fn move_changes_come_from_the_target_folder() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();
        let connection = Connection::new("web", "ssh", "10.0.0.1");
        let target = Folder {
            settings: Settings {
                username: Some("deploy".into()),
                ..Default::default()
            },
            ..Folder::new("target")
        };
        store.save_connection(connection.clone()).unwrap();
        store.save_folder(target.clone()).unwrap();

        let changes = store
            .move_changes(ItemRef::Connection(connection.id), Some(target.id))
            .unwrap();

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "username");
        assert_eq!(changes[0].after.as_deref(), Some("deploy"));
    }

    #[test]
    fn items_need_their_required_text() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::open(root.path()).unwrap();

        let no_host = Connection::new("web", "ssh", "  ");
        assert!(matches!(
            store.save_connection(no_host),
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            store.save_folder(Folder::new("")),
            Err(StoreError::Invalid(_))
        ));
        assert!(store.connections().is_empty() && store.folders().is_empty());
    }

    #[test]
    fn a_bad_file_is_reported_and_the_rest_loads() {
        let root = tempfile::tempdir().unwrap();
        let good = full_connection();
        Store::open(root.path())
            .unwrap()
            .save_connection(good.clone())
            .unwrap();
        let broken = root
            .path()
            .join("connections")
            .join(format!("{}.toml", Uuid::new_v4()));
        fs::write(&broken, "this is = = not toml").unwrap();

        let store = Store::open(root.path()).unwrap();

        assert_eq!(store.connections().len(), 1);
        assert_eq!(store.problems().len(), 1);
        assert_eq!(store.problems()[0].path, broken);
        assert!(broken.exists());
    }

    #[test]
    fn a_sync_conflict_copy_does_not_replace_the_original() {
        let root = tempfile::tempdir().unwrap();
        let original = full_connection();
        Store::open(root.path())
            .unwrap()
            .save_connection(original.clone())
            .unwrap();
        let conflict = root
            .path()
            .join("connections")
            .join(format!("{}.sync-conflict-1.toml", original.id));
        let mut other = original.clone();
        other.host = "conflict".into();
        fs::write(&conflict, encode(&other).unwrap()).unwrap();

        let store = Store::open(root.path()).unwrap();

        assert_eq!(store.connections()[&original.id], original);
        assert_eq!(store.problems()[0].path, conflict);
    }

    #[test]
    fn a_file_from_a_newer_ela_is_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let path = connection_file(root.path(), Uuid::new_v4());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "schema_version = 99\nsomething_new = true\n").unwrap();

        let store = Store::open(root.path()).unwrap();

        assert!(
            store.problems()[0].message.contains("newer Ela"),
            "{}",
            store.problems()[0].message
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "schema_version = 99\nsomething_new = true\n"
        );
    }

    #[test]
    fn the_version_must_be_there_and_make_sense() {
        for text in [
            "name = \"x\"",
            "schema_version = 0",
            "schema_version = -1",
            "schema_version = \"1\"",
        ] {
            assert!(
                matches!(decode::<Connection>(text), Err(DecodeError::BadVersion)),
                "{text}"
            );
        }
    }

    /// No shipped format has needed a migration yet, so these steps stand in for real ones.
    const RENAME_ADDRESS_TO_HOST: Migration = |table| {
        if let Some(address) = table.remove("address") {
            table.insert("host".into(), address);
        }
    };
    const RENAME_LOGIN_TO_USERNAME: Migration = |table| {
        if let Some(login) = table.remove("login") {
            table.insert("username".into(), login);
        }
    };
    const STEPS: &[Migration] = &[RENAME_ADDRESS_TO_HOST, RENAME_LOGIN_TO_USERNAME];

    fn old_file(version: u32, host_key: &str) -> String {
        let id = Uuid::new_v4();
        format!(
            "schema_version = {version}\nid = \"{id}\"\nname = \"old\"\nprotocol = \"ssh\"\n{host_key} = \"10.0.0.1\"\nlogin = \"root\"\n"
        )
    }

    #[test]
    fn a_file_is_migrated_through_every_step_it_is_behind() {
        let connection: Connection = decode_with(&old_file(1, "address"), STEPS).unwrap();
        assert_eq!(connection.host, "10.0.0.1");
        assert_eq!(connection.settings.username.as_deref(), Some("root"));
    }

    #[test]
    fn a_migration_starts_at_the_version_of_the_file() {
        let connection: Connection = decode_with(&old_file(2, "host"), STEPS).unwrap();
        assert_eq!(connection.settings.username.as_deref(), Some("root"));

        let skipped: Result<Connection, _> = decode_with(&old_file(2, "address"), STEPS);
        assert!(
            skipped.is_err(),
            "a version 2 file must not run the step from version 1"
        );
    }

    #[test]
    fn a_migrated_file_is_saved_at_the_current_version() {
        let connection: Connection = decode_with(&old_file(1, "address"), STEPS).unwrap();
        let text = encode(&connection).unwrap();
        assert!(text.starts_with(&format!(
            "schema_version = {}\n",
            current_version(Connection::MIGRATIONS)
        )));
        assert_eq!(decode::<Connection>(&text).unwrap(), connection);
    }
}
