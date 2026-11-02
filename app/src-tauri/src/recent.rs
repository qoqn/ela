use std::sync::{Arc, Mutex};

use ela_core::{Recent, RecentEntry, Store};
use tauri::State;

use crate::error::{CommandError, CommandResult};
use crate::saved::{Saved, with_store};

// The lock keeps two connects from reading the file before either has written it.
pub struct History(pub Option<Arc<Mutex<Recent>>>);

impl History {
    pub fn open_default() -> Self {
        Self(Recent::open_default().map(|recent| Arc::new(Mutex::new(recent))))
    }

    fn get(&self) -> CommandResult<std::sync::MutexGuard<'_, Recent>> {
        let recent = self
            .0
            .as_ref()
            .ok_or_else(|| CommandError::new("Can't find a folder to keep Recent in."))?;
        Ok(recent.lock()?)
    }
}

// What is in the Trash is left out but kept, so Undo brings it back to its place.
fn listed(history: &History, store: &Store) -> CommandResult<Vec<RecentEntry>> {
    let entries = history.get()?.entries()?;
    Ok(entries
        .into_iter()
        .filter(|entry| store.connections().contains_key(&entry.id))
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn list_recent(
    saved: State<'_, Saved>,
    history: State<'_, History>,
) -> CommandResult<Vec<RecentEntry>> {
    with_store(&saved, |store| listed(&history, store))
}

#[tauri::command]
#[specta::specta]
pub async fn forget_recent(history: State<'_, History>, id: uuid::Uuid) -> CommandResult<()> {
    Ok(history.get()?.forget(id)?)
}

#[tauri::command]
#[specta::specta]
pub async fn clear_recent(history: State<'_, History>) -> CommandResult<()> {
    Ok(history.get()?.clear()?)
}

#[cfg(test)]
mod tests {
    use ela_core::{Connection, ItemRef};

    use super::*;

    #[test]
    fn a_connection_in_the_trash_leaves_recent_and_undo_brings_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().join("config")).unwrap();
        let history = History(Some(Arc::new(Mutex::new(Recent::at(
            dir.path().join("recent.toml"),
        )))));
        let record = |store: &Store, id| {
            ela_workspace::record(history.0.as_ref().unwrap(), store, id);
        };
        let (db, web) = (
            Connection::new("db", "ssh", "db.lan"),
            Connection::new("web", "ssh", "web.lan"),
        );
        let (db_id, web_id) = (db.id, web.id);
        store.save_connection(db).unwrap();
        store.save_connection(web).unwrap();

        record(&store, db_id);
        record(&store, web_id);
        let ids = |store: &Store| -> Vec<uuid::Uuid> {
            listed(&history, store)
                .unwrap()
                .iter()
                .map(|e| e.id)
                .collect()
        };
        assert_eq!(ids(&store), [web_id, db_id]);

        let moved = store.trash(ItemRef::Connection(db_id)).unwrap();
        assert_eq!(ids(&store), [web_id]);
        record(&store, web_id);

        store.restore(&moved).unwrap();
        assert_eq!(ids(&store), [web_id, db_id]);
    }

    #[test]
    fn without_a_data_folder_listing_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let history = History(None);
        assert_eq!(
            listed(&history, &store).unwrap_err(),
            "Can't find a folder to keep Recent in."
        );
    }
}
