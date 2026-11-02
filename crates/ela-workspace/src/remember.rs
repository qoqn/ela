use ela_core::{Connection, Store};
use ela_session::{SavedPasswords, SecretString};
use ela_vault::Vault;
use uuid::Uuid;

use crate::{Shared, WorkspaceError};

pub struct VaultPasswords {
    pub store: Shared<Store>,
    pub vault: Shared<Vault>,
}

impl SavedPasswords for VaultPasswords {
    fn remember(&self, connection: Uuid, password: SecretString) -> Result<(), String> {
        self.keep(connection, password).map_err(|e| e.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
enum KeepError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Vault(#[from] ela_vault::VaultError),
    #[error(transparent)]
    Store(#[from] ela_core::StoreError),
}

impl VaultPasswords {
    /// The Store and the Vault are never locked together, so this can't deadlock with anything
    /// that takes them the other way round.
    fn keep(&self, connection: Uuid, password: SecretString) -> Result<(), KeepError> {
        let own = saved(&self.store, connection)?.settings.credential;
        if let Some(reference) = own {
            // As the Connection editor does, so whatever else names this Secret follows along.
            return Ok(self.vault()?.replace(reference, password)?);
        }
        let reference = self.vault()?.store(password)?;
        let linked = (|| -> Result<(), KeepError> {
            let mut connection = saved(&self.store, connection)?;
            connection.settings.credential = Some(reference);
            let mut store = self.store.lock().map_err(|_| WorkspaceError::Poisoned)?;
            Ok(store.save_connection(connection)?)
        })();
        if linked.is_err() {
            let _ = self.vault()?.remove(reference);
        }
        linked
    }

    fn vault(&self) -> Result<std::sync::MutexGuard<'_, Vault>, WorkspaceError> {
        self.vault.lock().map_err(|_| WorkspaceError::Poisoned)
    }
}

fn saved(store: &Shared<Store>, id: Uuid) -> Result<Connection, WorkspaceError> {
    let store = store.lock().map_err(|_| WorkspaceError::Poisoned)?;
    store
        .connections()
        .get(&id)
        .cloned()
        .ok_or(WorkspaceError::NotFound)
}
