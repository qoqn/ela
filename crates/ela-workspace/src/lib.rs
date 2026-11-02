mod outside;
mod remember;
mod resolving;

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use ela_core::{Recent, ResolveError, ResolvedConnection, Size, Store};
use ela_session::{Frontend, ProtocolRegistry, SessionError, SessionId, SessionManager};
use ela_transport::{Hop, JUMP_HOST_PROTOCOL, RouteError};
use ela_vault::Vault;
use serde_json::{Map, Value};
use uuid::Uuid;

pub use outside::{Outsides, SshConfigFile};
pub use remember::VaultPasswords;
use resolving::Pending;

pub type Shared<T> = Arc<Mutex<T>>;

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("{0}")]
    NoStore(String),
    #[error("That connection no longer exists.")]
    NotFound,
    #[error("Ela doesn't support the “{0}” protocol yet.")]
    Unsupported(String),
    #[error(transparent)]
    Route(#[from] RouteError),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("Another part of Ela failed while holding the saved connections.")]
    Poisoned,
}

/// Typed by the user, so nothing in it is saved, and the Session asks for what it needs.
#[derive(Debug, Clone)]
pub struct Address {
    pub protocol: String,
    pub host: String,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub options: Map<String, Value>,
}

/// What a Session is opened from: saved Connections, the Vault and Recent. The app and a future
/// `ela-cli` both open Sessions through it, so neither repeats how a Connection becomes one.
pub struct Workspace {
    store: Result<Shared<Store>, String>,
    vault: Result<Shared<Vault>, String>,
    recent: Option<Shared<Recent>>,
    ssh_config: Option<SshConfigFile>,
    manager: Arc<SessionManager>,
}

impl Workspace {
    pub fn new(
        store: Result<Shared<Store>, String>,
        vault: Result<Shared<Vault>, String>,
        recent: Option<Shared<Recent>>,
        registry: ProtocolRegistry,
    ) -> Self {
        let passwords = match (&store, &vault) {
            (Ok(store), Ok(vault)) => Some(Arc::new(VaultPasswords {
                store: store.clone(),
                vault: vault.clone(),
            }) as Arc<dyn ela_session::SavedPasswords>),
            _ => None,
        };
        Self {
            store,
            vault,
            recent,
            ssh_config: None,
            manager: SessionManager::keeping(registry, passwords),
        }
    }

    pub fn reading_ssh_config(mut self, file: SshConfigFile) -> Self {
        self.ssh_config = Some(file);
        self
    }

    /// Read again for every connect, so a change to the file counts at once.
    pub fn outsides(&self) -> Outsides {
        Outsides::read(self.ssh_config.as_ref())
    }

    pub fn outside(&self, protocol: &str, host: &str) -> Option<ela_core::Outside> {
        self.outsides().outside(protocol, host)
    }

    /// Every SSH host a saved Connection reaches, through the route too, so hashed Known Hosts
    /// entries can be named.
    pub fn ssh_hosts_in_use(&self) -> Vec<(String, u16)> {
        let Ok(store) = self.store() else {
            return Vec::new();
        };
        let outsides = self.outsides();
        let mut hosts = Vec::new();
        for connection in store.connections().values() {
            let Ok(resolved) = resolve(&store, self.registry(), &outsides, connection.id) else {
                continue;
            };
            if let Ok(hops) = plan(&store, self.registry(), &outsides, &resolved.options) {
                hosts.extend(hops.into_iter().map(|hop| (hop.host, hop.port)));
            }
            if resolved.protocol == JUMP_HOST_PROTOCOL {
                hosts.push((resolved.host, resolved.port));
            }
        }
        hosts.sort();
        hosts.dedup();
        hosts
    }

    pub fn manager(&self) -> &Arc<SessionManager> {
        &self.manager
    }

    pub fn registry(&self) -> &ProtocolRegistry {
        self.manager.registry()
    }

    fn store(&self) -> Result<MutexGuard<'_, Store>, WorkspaceError> {
        let store = self
            .store
            .as_ref()
            .map_err(|why| WorkspaceError::NoStore(why.clone()))?;
        store.lock().map_err(|_| WorkspaceError::Poisoned)
    }

    pub fn resolve(&self, id: Uuid) -> Result<ResolvedConnection, WorkspaceError> {
        resolve(&*self.store()?, self.registry(), &self.outsides(), id)
    }

    pub fn open_saved(
        &self,
        id: Uuid,
        size: Size,
        frontend: Arc<dyn Frontend>,
    ) -> Result<SessionId, WorkspaceError> {
        let outsides = self.outsides();
        let (target, hops) = {
            let store = self.store()?;
            let target = resolve(&store, self.registry(), &outsides, id)?;
            let hops = plan(&store, self.registry(), &outsides, &target.options)?;
            (target, hops)
        };
        let protocol = target.protocol.clone();
        let pending = Pending::saved(self.vault.clone(), target, hops, size);
        let session = self.manager.open_with(&protocol, pending, frontend)?;
        self.record(id);
        Ok(session)
    }

    pub fn open_address(
        &self,
        address: Address,
        size: Size,
        frontend: Arc<dyn Frontend>,
    ) -> Result<SessionId, WorkspaceError> {
        let protocol = self
            .registry()
            .get(&address.protocol)
            .ok_or_else(|| WorkspaceError::Unsupported(address.protocol.clone()))?;
        let outsides = self.outsides();
        let address = outsides.apply_to(address);
        let hops = match self.store() {
            Ok(store) => plan(&store, self.registry(), &outsides, &address.options)?,
            // Without the saved tree every hop is taken as written.
            Err(_) => ela_transport::plan(
                &address.options,
                outside::Unsaved(&outsides),
                &sign_in_options(self.registry()),
            )?,
        };
        let port = address.port.or(protocol.default_port()).unwrap_or(0);
        let protocol_id = address.protocol.clone();
        let pending = Pending::address(self.vault.clone(), address, port, hops, size);
        Ok(self.manager.open_with(&protocol_id, pending, frontend)?)
    }

    fn record(&self, id: Uuid) {
        let (Some(recent), Ok(store)) = (&self.recent, self.store()) else {
            return;
        };
        record(recent, &store, id);
    }
}

/// Failing to note it never stops a connection, so the error is only logged. What is in the
/// Trash stays in the file, so Undo brings it back to its place.
pub fn record(recent: &Mutex<Recent>, store: &Store, id: Uuid) {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let recorded = match recent.lock() {
        Ok(recent) => recent
            .opened(id, at, |other| {
                store.connections().contains_key(&other) || store.connection_in_trash(other)
            })
            .map_err(|e| e.to_string()),
        Err(_) => Err("Recent failed in another part of Ela.".to_owned()),
    };
    if let Err(error) = recorded {
        tracing::warn!("couldn't add to Recent: {error}");
    }
}

pub fn resolve(
    store: &Store,
    registry: &ProtocolRegistry,
    outsides: &Outsides,
    id: Uuid,
) -> Result<ResolvedConnection, WorkspaceError> {
    let connection = store
        .connections()
        .get(&id)
        .ok_or(WorkspaceError::NotFound)?;
    let default_port = registry
        .get(&connection.protocol)
        .and_then(|p| p.default_port());
    let outside = outsides.outside(&connection.protocol, &connection.host);
    Ok(ela_core::resolve_over(
        connection,
        store.folders(),
        outside.as_ref(),
        default_port,
    )?)
}

pub fn plan(
    store: &Store,
    registry: &ProtocolRegistry,
    outsides: &Outsides,
    options: &Map<String, Value>,
) -> Result<Vec<Hop>, WorkspaceError> {
    let directory = outside::Saved {
        store,
        registry,
        outsides,
    };
    Ok(ela_transport::plan(
        options,
        directory,
        &sign_in_options(registry),
    )?)
}

/// The options a Jump Host signs in with are the ones its Protocol's schema puts in "sign_in".
fn sign_in_options(registry: &ProtocolRegistry) -> Vec<String> {
    let Some(protocol) = registry.get(JUMP_HOST_PROTOCOL) else {
        return Vec::new();
    };
    let schema = protocol.settings_schema();
    let Some(fields) = schema.as_object() else {
        return Vec::new();
    };
    fields
        .iter()
        .filter(|(_, field)| field.get("group").and_then(Value::as_str) == Some("sign_in"))
        .map(|(key, _)| key.clone())
        .collect()
}

#[cfg(test)]
mod tests;
