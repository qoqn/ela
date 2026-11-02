use ela_core::CredentialRef;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::{Vault, VaultError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct SavedKeyPassphrase {
    pub credential: CredentialRef,
    pub path: String,
    pub fingerprint: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Label {
    KeyPassphrase {
        path: String,
        fingerprint: Option<String>,
    },
}

impl Label {
    fn of(key: &SavedKeyPassphrase) -> String {
        let label = Label::KeyPassphrase {
            path: key.path.clone(),
            fingerprint: key.fingerprint.clone(),
        };
        serde_json::to_string(&label).expect("a label always encodes")
    }

    fn parse(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }
}

impl Vault {
    pub fn remember_key_passphrase(
        &mut self,
        key: &SavedKeyPassphrase,
        passphrase: SecretString,
    ) -> Result<(), VaultError> {
        self.replace_labelled(key.credential, passphrase, Label::of(key))
    }

    pub fn key_passphrase(&mut self, key: &SavedKeyPassphrase) -> Result<SecretString, VaultError> {
        let passphrase = self.read(key.credential)?;
        // Saved before Secrets had labels: labelled now, so the Vault panel can list it.
        if let Ok(None) = self.label(key.credential)
            && let Err(error) = self.set_label(key.credential, Label::of(key))
        {
            tracing::warn!(%error, "couldn't label a saved passphrase");
        }
        Ok(passphrase)
    }

    pub fn key_passphrases(&mut self) -> Result<Vec<SavedKeyPassphrase>, VaultError> {
        let mut keys: Vec<SavedKeyPassphrase> = self
            .labels()?
            .into_iter()
            .filter_map(|(credential, label)| match Label::parse(&label)? {
                Label::KeyPassphrase { path, fingerprint } => Some(SavedKeyPassphrase {
                    credential,
                    path,
                    fingerprint,
                }),
            })
            .collect();
        keys.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(keys)
    }

    // A Connection's password goes with its Connection, never from here.
    pub fn forget_key_passphrase(&mut self, credential: CredentialRef) -> Result<(), VaultError> {
        match self.label(credential)?.as_deref().and_then(Label::parse) {
            Some(Label::KeyPassphrase { .. }) => self.remove(credential),
            None => Err(VaultError::NotAKeyPassphrase),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use secrecy::ExposeSecret;
    use tempfile::TempDir;

    use super::*;

    fn open_vault(dir: &TempDir) -> Vault {
        let mut vault = Vault::new(dir.path().join("vault.bin"), Duration::from_secs(60));
        vault
            .create(&SecretString::from("master password"))
            .unwrap();
        vault
    }

    fn key(fingerprint: &str) -> SavedKeyPassphrase {
        SavedKeyPassphrase {
            credential: CredentialRef::for_key(fingerprint),
            path: "~/.ssh/id_ed25519".into(),
            fingerprint: Some(fingerprint.into()),
        }
    }

    #[test]
    fn remembered_key_passphrases_are_listed_and_only_they_can_be_forgotten() {
        let dir = TempDir::new().unwrap();
        let mut vault = open_vault(&dir);
        let key = key("SHA256:key");
        vault
            .remember_key_passphrase(&key, SecretString::from("key passphrase"))
            .unwrap();
        let password = vault.store(SecretString::from("a connection's")).unwrap();

        assert_eq!(vault.key_passphrases().unwrap(), std::slice::from_ref(&key));
        assert!(matches!(
            vault.forget_key_passphrase(password),
            Err(VaultError::NotAKeyPassphrase)
        ));
        assert!(vault.read(password).is_ok(), "a password is not a key's");

        vault.forget_key_passphrase(key.credential).unwrap();
        assert!(vault.key_passphrases().unwrap().is_empty());
        assert!(vault.read(key.credential).is_err());
    }

    #[test]
    fn a_passphrase_saved_before_labels_is_labelled_when_it_is_used() {
        let dir = TempDir::new().unwrap();
        let mut vault = open_vault(&dir);
        let key = key("SHA256:older");
        vault
            .replace(key.credential, SecretString::from("older"))
            .unwrap();
        assert!(vault.key_passphrases().unwrap().is_empty());

        let read = vault.key_passphrase(&key).unwrap();

        assert_eq!(read.expose_secret(), "older");
        assert_eq!(
            vault.key_passphrases().unwrap()[0].path,
            "~/.ssh/id_ed25519"
        );
    }
}
