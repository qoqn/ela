use std::sync::{Arc, Mutex};

use ela_core::{
    Connection, FieldChange, Folder, ItemRef, LoadProblem, Outside, ResolvedConnection, Store,
};
use ela_session::{ProtocolRegistry, check_options};
use ela_workspace::Workspace;
use serde::Serialize;
use tauri::State;

use crate::Manager_;
use crate::error::{CommandError, CommandResult};

// An Err keeps why the tree couldn't be opened, because Sessions still work without it.
pub type Saved = Result<Arc<Mutex<Store>>, CommandError>;

pub fn open_default() -> Saved {
    Ok(Arc::new(Mutex::new(Store::open_default()?)))
}

pub fn with_store<T>(
    saved: &Saved,
    use_store: impl FnOnce(&mut Store) -> CommandResult<T>,
) -> CommandResult<T> {
    let store = saved.as_ref().map_err(Clone::clone)?;
    let mut store = store.lock()?;
    use_store(&mut store)
}

#[derive(Serialize, specta::Type)]
pub struct SavedProblem {
    path: String,
    message: String,
}

impl From<&LoadProblem> for SavedProblem {
    fn from(problem: &LoadProblem) -> Self {
        Self {
            path: problem.path.display().to_string(),
            message: problem.message.clone(),
        }
    }
}

#[derive(Serialize, specta::Type)]
pub struct SavedTree {
    connections: Vec<Connection>,
    folders: Vec<Folder>,
    problems: Vec<SavedProblem>,
}

#[tauri::command]
#[specta::specta]
pub async fn list_tree(saved: State<'_, Saved>) -> CommandResult<SavedTree> {
    with_store(&saved, |store| {
        Ok(SavedTree {
            connections: store.connections().values().cloned().collect(),
            folders: store.folders().values().cloned().collect(),
            problems: store.problems().iter().map(SavedProblem::from).collect(),
        })
    })
}

#[tauri::command]
#[specta::specta]
pub async fn save_connection(
    saved: State<'_, Saved>,
    manager: State<'_, Manager_>,
    connection: Connection,
) -> CommandResult<()> {
    let protocol = manager
        .registry()
        .get(&connection.protocol)
        .ok_or_else(|| ela_workspace::WorkspaceError::Unsupported(connection.protocol.clone()))?;
    check_options(&protocol.settings_schema(), &connection.settings.options)
        .map_err(CommandError::new)?;
    with_store(&saved, |store| Ok(store.save_connection(connection)?))
}

#[tauri::command]
#[specta::specta]
pub async fn save_folder(
    saved: State<'_, Saved>,
    manager: State<'_, Manager_>,
    folder: Folder,
) -> CommandResult<()> {
    check_folder_options(manager.registry(), &folder)?;
    with_store(&saved, |store| Ok(store.save_folder(folder)?))
}

/// A Folder isn't tied to one Protocol, so what it holds has to suit whichever Protocol names it.
fn check_folder_options(registry: &ProtocolRegistry, folder: &Folder) -> CommandResult<()> {
    registry
        .list()
        .iter()
        .try_for_each(|protocol| check_options(&protocol.settings_schema, &folder.settings.options))
        .map_err(CommandError::new)
}

#[tauri::command]
#[specta::specta]
pub async fn trash_item(saved: State<'_, Saved>, item: ItemRef) -> CommandResult<Vec<ItemRef>> {
    with_store(&saved, |store| Ok(store.trash(item)?))
}

#[tauri::command]
#[specta::specta]
pub async fn restore_items(saved: State<'_, Saved>, items: Vec<ItemRef>) -> CommandResult<()> {
    with_store(&saved, |store| Ok(store.restore(&items)?))
}

#[tauri::command]
#[specta::specta]
pub async fn move_item(
    saved: State<'_, Saved>,
    item: ItemRef,
    to: Option<uuid::Uuid>,
) -> CommandResult<()> {
    with_store(&saved, |store| Ok(store.move_item(item, to)?))
}

#[tauri::command]
#[specta::specta]
pub async fn move_changes(
    saved: State<'_, Saved>,
    item: ItemRef,
    to: Option<uuid::Uuid>,
) -> CommandResult<Vec<FieldChange>> {
    with_store(&saved, |store| Ok(store.move_changes(item, to)?))
}

#[tauri::command]
#[specta::specta]
pub async fn resolve_connection(
    workspace: State<'_, Arc<Workspace>>,
    id: uuid::Uuid,
) -> CommandResult<ResolvedConnection> {
    Ok(workspace.resolve(id)?)
}

#[tauri::command]
#[specta::specta]
pub async fn outside_settings(
    workspace: State<'_, Arc<Workspace>>,
    protocol: String,
    host: String,
) -> CommandResult<Option<Outside>> {
    Ok(workspace.outside(&protocol, &host))
}

#[derive(Serialize, specta::Type)]
pub struct ImportSummary {
    added: u32,
    already_saved: u32,
    notes: Vec<String>,
}

fn import_hosts(store: &mut Store, imported: ela_import::Imported) -> CommandResult<ImportSummary> {
    let (connections, already_saved) =
        ela_import::new_connections(imported.hosts, store.connections());
    let mut added = 0;
    for connection in connections {
        store
            .save_connection(connection)
            .map_err(|e| CommandError::new(format!("Stopped after importing {added}: {e}")))?;
        added += 1;
    }
    Ok(ImportSummary {
        added,
        already_saved: already_saved as u32,
        notes: imported.notes,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn import_ssh_config(saved: State<'_, Saved>) -> CommandResult<ImportSummary> {
    let home = dirs::home_dir().ok_or_else(no_home)?;
    let context = ela_import::Context {
        local_user: std::env::var("USER").unwrap_or_default(),
        home: home.clone(),
    };
    let imported = ela_import::read_ssh_config(&home.join(".ssh/config"), &context)?;
    with_store(&saved, |store| import_hosts(store, imported))
}

pub fn no_home() -> CommandError {
    CommandError::new("Can't find your home folder.")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn importing_twice_saves_each_host_once_and_passes_notes_on() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let imported = || ela_import::Imported {
            hosts: vec![ela_import::SshHost {
                alias: "db".into(),
                hostname: Some("db.internal".into()),
                ..Default::default()
            }],
            notes: vec!["a note".into()],
        };

        let first = import_hosts(&mut store, imported()).unwrap();
        assert_eq!((first.added, first.already_saved), (1, 0));
        assert_eq!(first.notes, ["a note"]);

        let second = import_hosts(&mut store, imported()).unwrap();
        assert_eq!((second.added, second.already_saved), (0, 1));

        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(reopened.connections().len(), 1);
    }

    fn check(options: serde_json::Value) -> CommandResult<()> {
        let mut registry = ProtocolRegistry::new();
        registry
            .register(ela_ssh::SshProtocol::default())
            .register(ela_rdp::RdpProtocol::default());
        let mut folder = Folder::new("folder");
        folder.settings.options = options.as_object().unwrap().clone();
        check_folder_options(&registry, &folder)
    }

    #[test]
    fn a_folder_can_hold_options_for_every_protocol_and_keys_none_describes() {
        check(json!({ "term": "screen", "image_quality": "lossless", "custom": 1 })).unwrap();
    }

    #[test]
    fn a_folder_option_any_protocol_would_refuse_is_refused() {
        assert_eq!(
            check(json!({ "image_quality": "blurry" })).unwrap_err(),
            "Image quality can't be “blurry”."
        );
        assert!(check(json!({ "term": 5 })).is_err());
    }
}
