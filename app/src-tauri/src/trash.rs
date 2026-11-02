use ela_core::TrashEntry;
use ela_vault::VaultState;
use serde::Serialize;
use tauri::State;

use crate::error::CommandResult;
use crate::saved::{Saved, SavedProblem, with_store};
use crate::vault::{VaultSlot, with_vault};

#[derive(Serialize, specta::Type)]
pub struct TrashView {
    items: Vec<TrashEntry>,
    saved_credentials: u32,
    problems: Vec<SavedProblem>,
}

#[tauri::command]
#[specta::specta]
pub async fn list_trash(saved: State<'_, Saved>) -> CommandResult<TrashView> {
    with_store(&saved, |store| {
        let trash = store.trash_contents()?;
        Ok(TrashView {
            items: trash.entries,
            saved_credentials: trash.unused_credentials.len() as u32,
            problems: trash.problems.iter().map(SavedProblem::from).collect(),
        })
    })
}

#[tauri::command]
#[specta::specta]
pub async fn empty_trash(
    saved: State<'_, Saved>,
    vault: State<'_, VaultSlot>,
) -> CommandResult<u32> {
    empty(&saved, &vault).await
}

/// Secrets go first. If the files then can't go, an item that remains asks for its password, which
/// beats a password left in the Vault with nothing able to reach or remove it.
async fn empty(saved: &Saved, vault: &VaultSlot) -> CommandResult<u32> {
    let credentials = with_store(saved, |store| {
        Ok(store.trash_contents()?.unused_credentials)
    })?;
    if !credentials.is_empty() {
        with_vault(vault, move |vault| {
            // A synced Connection can name a Secret on a machine with no Vault at all.
            if vault.state() == VaultState::Absent {
                return Ok(());
            }
            credentials
                .into_iter()
                .try_for_each(|reference| vault.remove(reference))
        })
        .await?;
    }
    with_store(saved, |store| Ok(store.empty_trash()? as u32))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use ela_core::{Connection, CredentialRef, ItemRef, Store};
    use ela_session::SecretString;
    use ela_vault::{CredentialState, DEFAULT_IDLE_TIMEOUT, Vault};
    use tauri::async_runtime::block_on;
    use tempfile::TempDir;

    use super::*;

    const MASTER: &str = "correct horse battery";

    fn saved(dir: &Path) -> Saved {
        Ok(Arc::new(Mutex::new(
            Store::open(dir.join("config")).unwrap(),
        )))
    }

    fn vault_slot(dir: &Path) -> VaultSlot {
        Ok(Arc::new(Mutex::new(Vault::new(
            dir.join("vault.bin"),
            DEFAULT_IDLE_TIMEOUT,
        ))))
    }

    fn open_vault(slot: &VaultSlot) -> (CredentialRef, CredentialRef) {
        let mut vault = slot.as_ref().unwrap().lock().unwrap();
        vault.create(&SecretString::from(MASTER)).unwrap();
        let store = |vault: &mut Vault| vault.store(SecretString::from("secret")).unwrap();
        (store(&mut vault), store(&mut vault))
    }

    fn add(saved: &Saved, name: &str, credential: CredentialRef, trashed: bool) {
        let mut connection = Connection::new(name, "ssh", "10.0.0.1");
        connection.settings.credential = Some(credential);
        with_store(saved, |store| {
            store.save_connection(connection.clone()).unwrap();
            if trashed {
                store.trash(ItemRef::Connection(connection.id)).unwrap();
            }
            Ok(())
        })
        .unwrap();
    }

    fn state(slot: &VaultSlot, reference: CredentialRef) -> CredentialState {
        let mut vault = slot.as_ref().unwrap().lock().unwrap();
        vault.credential_state(reference)
    }

    fn trash_size(saved: &Saved) -> usize {
        with_store(saved, |store| {
            Ok(store.trash_contents().unwrap().entries.len())
        })
        .unwrap()
    }

    #[test]
    fn emptying_removes_the_secrets_only_the_trash_used() {
        let dir = TempDir::new().unwrap();
        let (saved, slot) = (saved(dir.path()), vault_slot(dir.path()));
        let (only_trash, shared) = open_vault(&slot);
        add(&saved, "gone", only_trash, true);
        add(&saved, "gone too", shared, true);
        add(&saved, "live", shared, false);

        assert_eq!(block_on(empty(&saved, &slot)).unwrap(), 2);

        assert_eq!(trash_size(&saved), 0);
        assert_eq!(state(&slot, only_trash), CredentialState::Missing);
        assert_eq!(state(&slot, shared), CredentialState::Available);
    }

    #[test]
    fn a_locked_vault_stops_emptying_before_anything_is_removed() {
        let dir = TempDir::new().unwrap();
        let (saved, slot) = (saved(dir.path()), vault_slot(dir.path()));
        let (reference, _) = open_vault(&slot);
        add(&saved, "gone", reference, true);
        slot.as_ref().unwrap().lock().unwrap().lock();

        let refused = block_on(empty(&saved, &slot)).unwrap_err();

        assert_eq!(refused, "The Vault is locked.");
        assert_eq!(trash_size(&saved), 1);
    }

    #[test]
    fn a_machine_with_no_vault_can_still_empty_the_trash() {
        let dir = TempDir::new().unwrap();
        let (saved, slot) = (saved(dir.path()), vault_slot(dir.path()));
        add(&saved, "synced", CredentialRef(uuid::Uuid::new_v4()), true);

        assert_eq!(block_on(empty(&saved, &slot)).unwrap(), 1);
        assert_eq!(trash_size(&saved), 0);
        assert!(!dir.path().join("vault.bin").exists());
    }

    #[test]
    fn nothing_to_remove_from_the_vault_needs_no_unlocking() {
        let dir = TempDir::new().unwrap();
        let (saved, slot) = (saved(dir.path()), vault_slot(dir.path()));
        let (reference, _) = open_vault(&slot);
        add(&saved, "live", reference, false);
        add(&saved, "gone", reference, true);
        slot.as_ref().unwrap().lock().unwrap().lock();

        assert_eq!(block_on(empty(&saved, &slot)).unwrap(), 1);
    }
}
