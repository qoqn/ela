use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use ela_core::CredentialRef;
use ela_session::SecretString;
use ela_ssh::SavedKey;
use ela_vault::{
    CredentialState, IDLE_CHOICES_MINUTES, SavedKeyPassphrase, Vault, VaultError, VaultState,
};
use serde::Serialize;
use tauri::State;

use crate::error::{CommandError, CommandResult};
use crate::saved::{Saved, with_store};

const IDLE_CHECK: Duration = Duration::from_secs(20);

pub type VaultSlot = Result<Arc<Mutex<Vault>>, CommandError>;

pub fn open_default(saved: &Saved) -> VaultSlot {
    let mut vault = Vault::open_default()?;
    if let Some(minutes) = saved_idle_minutes(saved) {
        vault.set_idle_timeout(minutes_to_timeout(minutes));
    }
    Ok(Arc::new(Mutex::new(vault)))
}

fn minutes_to_timeout(minutes: u32) -> Duration {
    Duration::from_secs(u64::from(minutes) * 60)
}

/// Anything wrong here means the default, because a setting that can't be trusted must not be
/// what decides how long the Vault stays open.
fn saved_idle_minutes(saved: &Saved) -> Option<u32> {
    let settings = with_store(saved, |store| Ok(store.app_settings()?));
    let minutes = match settings {
        Ok(settings) => settings.vault_idle_minutes?,
        Err(error) => {
            tracing::warn!(%error, "couldn't read the settings");
            return None;
        }
    };
    if IDLE_CHOICES_MINUTES.contains(&minutes) {
        Some(minutes)
    } else {
        tracing::warn!(minutes, "the saved Vault idle time isn't one Ela offers");
        None
    }
}

/// Every use of the Vault checks the clock too. This is what clears the Secrets from memory when
/// nothing uses it again.
pub fn lock_when_idle(slot: &VaultSlot) {
    let Ok(vault) = slot.clone() else { return };
    thread::spawn(move || {
        loop {
            thread::sleep(IDLE_CHECK);
            if let Ok(mut vault) = vault.lock() {
                vault.lock_if_idle(SystemTime::now());
            }
        }
    });
}

/// Argon2 takes a noticeable moment and holds the lock meanwhile, so none of this may run on an
/// async worker or the main thread.
pub async fn with_vault<T: Send + 'static>(
    slot: &VaultSlot,
    use_vault: impl FnOnce(&mut Vault) -> Result<T, VaultError> + Send + 'static,
) -> CommandResult<T> {
    let vault = slot.clone()?;
    tauri::async_runtime::spawn_blocking(move || Ok(use_vault(&mut *vault.lock()?)?)).await?
}

/// Blocking, like everything that locks the Vault. `ela-ssh` calls it off the async workers.
pub struct VaultPassphrases(pub VaultSlot);

impl ela_ssh::Passphrases for VaultPassphrases {
    fn saved(&self, key: &SavedKey) -> Option<SecretString> {
        let vault = self.0.as_ref().ok()?;
        vault.lock().ok()?.key_passphrase(&labelled(key)).ok()
    }

    fn remember(&self, key: &SavedKey, passphrase: SecretString) -> Result<(), String> {
        let vault = self.0.as_ref().map_err(ToString::to_string)?;
        let mut vault = vault.lock().map_err(|e| e.to_string())?;
        vault
            .remember_key_passphrase(&labelled(key), passphrase)
            .map_err(|e| e.to_string())
    }
}

fn labelled(key: &SavedKey) -> SavedKeyPassphrase {
    SavedKeyPassphrase {
        credential: key.credential,
        path: key.path.clone(),
        fingerprint: key.fingerprint.clone(),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn vault_key_passphrases(
    vault: State<'_, VaultSlot>,
) -> CommandResult<Vec<SavedKeyPassphrase>> {
    with_vault(&vault, Vault::key_passphrases).await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_forget_key_passphrase(
    vault: State<'_, VaultSlot>,
    credential: CredentialRef,
) -> CommandResult<()> {
    with_vault(&vault, move |vault| vault.forget_key_passphrase(credential)).await
}

#[derive(Serialize, specta::Type)]
pub struct VaultInfo {
    state: VaultState,
    idle_minutes: u32,
    idle_choices: Vec<u32>,
}

#[tauri::command]
#[specta::specta]
pub async fn vault_info(vault: State<'_, VaultSlot>) -> CommandResult<VaultInfo> {
    with_vault(&vault, |vault| {
        Ok(VaultInfo {
            state: vault.state(),
            idle_minutes: (vault.idle_timeout().as_secs() / 60) as u32,
            idle_choices: IDLE_CHOICES_MINUTES.to_vec(),
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_create(vault: State<'_, VaultSlot>, master: String) -> CommandResult<()> {
    let master = SecretString::from(master);
    with_vault(&vault, move |vault| vault.create(&master)).await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_unlock(vault: State<'_, VaultSlot>, master: String) -> CommandResult<()> {
    let master = SecretString::from(master);
    with_vault(&vault, move |vault| vault.unlock(&master)).await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_change_master(
    vault: State<'_, VaultSlot>,
    current: String,
    replacement: String,
) -> CommandResult<()> {
    let current = SecretString::from(current);
    let replacement = SecretString::from(replacement);
    with_vault(&vault, move |vault| {
        vault.change_master(&current, &replacement)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_set_idle_minutes(
    saved: State<'_, Saved>,
    vault: State<'_, VaultSlot>,
    minutes: u32,
) -> CommandResult<()> {
    set_idle_minutes(&saved, &vault, minutes).await
}

async fn set_idle_minutes(saved: &Saved, slot: &VaultSlot, minutes: u32) -> CommandResult<()> {
    if !IDLE_CHOICES_MINUTES.contains(&minutes) {
        return Err(CommandError::new(format!(
            "Ela doesn't offer {minutes} minutes."
        )));
    }
    with_store(saved, |store| {
        Ok(store.update_app_settings(|settings| settings.vault_idle_minutes = Some(minutes))?)
    })?;
    with_vault(slot, move |vault| {
        vault.set_idle_timeout(minutes_to_timeout(minutes));
        Ok(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_lock(vault: State<'_, VaultSlot>) -> CommandResult<()> {
    with_vault(&vault, |vault| {
        vault.lock();
        Ok(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_store_secret(
    vault: State<'_, VaultSlot>,
    secret: String,
    replaces: Option<CredentialRef>,
) -> CommandResult<CredentialRef> {
    let secret = SecretString::from(secret);
    with_vault(&vault, move |vault| match replaces {
        Some(reference) => vault.replace(reference, secret).map(|()| reference),
        None => vault.store(secret),
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_remove_secret(
    vault: State<'_, VaultSlot>,
    reference: CredentialRef,
) -> CommandResult<()> {
    with_vault(&vault, move |vault| vault.remove(reference)).await
}

#[tauri::command]
#[specta::specta]
pub async fn vault_credential_state(
    vault: State<'_, VaultSlot>,
    reference: CredentialRef,
) -> CommandResult<CredentialState> {
    with_vault(&vault, move |vault| Ok(vault.credential_state(reference))).await
}

#[cfg(test)]
mod tests {
    use ela_core::Store;
    use ela_vault::DEFAULT_IDLE_TIMEOUT;
    use tauri::async_runtime::block_on;
    use tempfile::TempDir;

    use super::*;

    fn saved(dir: &TempDir) -> Saved {
        Ok(Arc::new(Mutex::new(Store::open(dir.path()).unwrap())))
    }

    fn slot(dir: &TempDir) -> VaultSlot {
        Ok(Arc::new(Mutex::new(Vault::new(
            dir.path().join("vault.bin"),
            DEFAULT_IDLE_TIMEOUT,
        ))))
    }

    fn timeout(slot: &VaultSlot) -> Duration {
        slot.as_ref().unwrap().lock().unwrap().idle_timeout()
    }

    #[test]
    fn a_chosen_idle_time_applies_now_and_after_a_restart() {
        let dir = TempDir::new().unwrap();
        let (saved_tree, vault) = (saved(&dir), slot(&dir));

        block_on(set_idle_minutes(&saved_tree, &vault, 30)).unwrap();

        assert_eq!(timeout(&vault), Duration::from_secs(30 * 60));
        assert_eq!(saved_idle_minutes(&saved(&dir)), Some(30));
    }

    #[test]
    fn an_idle_time_ela_does_not_offer_is_refused() {
        let dir = TempDir::new().unwrap();
        let (saved_tree, vault) = (saved(&dir), slot(&dir));

        assert!(block_on(set_idle_minutes(&saved_tree, &vault, 100_000)).is_err());

        assert_eq!(timeout(&vault), DEFAULT_IDLE_TIMEOUT);
        assert_eq!(saved_idle_minutes(&saved_tree), None);
    }

    #[test]
    fn settings_that_cannot_be_written_leave_the_vault_as_it_was() {
        let dir = TempDir::new().unwrap();
        let (saved_tree, vault) = (saved(&dir), slot(&dir));
        std::fs::write(dir.path().join("settings.toml"), "schema_version = 99\n").unwrap();

        assert!(block_on(set_idle_minutes(&saved_tree, &vault, 30)).is_err());
        assert_eq!(timeout(&vault), DEFAULT_IDLE_TIMEOUT);
    }

    #[test]
    fn a_key_passphrase_is_remembered_only_in_an_open_vault() {
        use ela_session::ExposeSecret;
        use ela_ssh::Passphrases;

        let dir = TempDir::new().unwrap();
        let vault = slot(&dir);
        let passphrases = VaultPassphrases(vault.clone());
        let key = saved_key("SHA256:key");
        let key = &key;
        let passphrase = || SecretString::from("key passphrase");

        assert!(passphrases.remember(key, passphrase()).is_err());
        let master = SecretString::from("master password");
        vault
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .create(&master)
            .unwrap();
        passphrases.remember(key, passphrase()).unwrap();
        assert_eq!(
            passphrases.saved(key).unwrap().expose_secret(),
            "key passphrase"
        );

        vault.as_ref().unwrap().lock().unwrap().lock();
        assert!(passphrases.saved(key).is_none());
    }

    fn saved_key(fingerprint: &str) -> SavedKey {
        SavedKey {
            credential: CredentialRef::for_key(fingerprint),
            path: "~/.ssh/id_ed25519".into(),
            fingerprint: Some(fingerprint.into()),
        }
    }

    #[test]
    fn a_hand_edited_idle_time_ela_does_not_offer_means_the_default() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("settings.toml"),
            "schema_version = 1\nvault_idle_minutes = 100000\n",
        )
        .unwrap();
        assert_eq!(saved_idle_minutes(&saved(&dir)), None);
    }
}
