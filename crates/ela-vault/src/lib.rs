use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use argon2::{Algorithm, Argon2, Params, Version};
use ela_core::CredentialRef;
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use uuid::Uuid;
use zeroize::Zeroizing;

mod key_passphrases;

pub use key_passphrases::SavedKeyPassphrase;

const MAGIC: &[u8; 8] = b"ELAVAULT";
const FORMAT_VERSION: u8 = 2;
// Format 1 had no labels. It is still read, and the next write turns it into format 2.
const UNLABELLED_FORMAT: u8 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const HEADER_LEN: usize = MAGIC.len() + 1 + 3 * 4 + SALT_LEN + NONCE_LEN;
const MIN_MASTER_CHARS: usize = 8;

/// RFC 9106's choice for machines that can't spare 2 GiB.
const KDF: KdfCost = KdfCost {
    memory_kib: 64 * 1024,
    iterations: 3,
    parallelism: 4,
};
/// The header isn't authenticated until the key is derived, so a damaged or hostile file must not
/// be able to ask for unbounded memory first.
const MAX_MEMORY_KIB: u32 = 4 * 1024 * 1024;

pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// A list rather than a range, so nothing can ask for a Vault that effectively never locks.
pub const IDLE_CHOICES_MINUTES: &[u32] = &[1, 5, 15, 30, 60, 240];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum VaultState {
    Absent,
    Locked,
    Unlocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CredentialState {
    Available,
    Locked,
    Missing,
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("Can't find a folder for the Vault.")]
    NoDataDir,
    #[error("Can't use {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("A Vault already exists at {0}.")]
    AlreadyExists(PathBuf),
    #[error("There is no Vault yet.")]
    Absent,
    #[error("The Vault is locked.")]
    Locked,
    #[error("That password doesn't open the Vault.")]
    WrongPassword,
    #[error("The master password needs at least {MIN_MASTER_CHARS} characters.")]
    MasterTooShort,
    #[error("A password can't be empty.")]
    EmptySecret,
    #[error("The Vault no longer holds this password.")]
    Missing,
    #[error("{0} isn't a Vault file Ela can read.")]
    Unreadable(PathBuf),
    #[error("{0} comes from a newer Ela, which is needed to open it.")]
    TooNew(PathBuf),
    #[error("The Vault holds no key passphrase under that reference.")]
    NotAKeyPassphrase,
    #[error("Encryption failed: {0}")]
    Crypto(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KdfCost {
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
}

#[derive(Clone, Copy)]
struct Header {
    cost: KdfCost,
    salt: [u8; SALT_LEN],
}

impl Header {
    /// All of it is authenticated data, so changing a cost or the salt makes decryption fail.
    fn encode(&self, nonce: &[u8; NONCE_LEN]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_LEN);
        bytes.extend_from_slice(MAGIC);
        bytes.push(FORMAT_VERSION);
        bytes.extend_from_slice(&self.cost.memory_kib.to_le_bytes());
        bytes.extend_from_slice(&self.cost.iterations.to_le_bytes());
        bytes.extend_from_slice(&self.cost.parallelism.to_le_bytes());
        bytes.extend_from_slice(&self.salt);
        bytes.extend_from_slice(nonce);
        bytes
    }
}

struct Unlocked {
    key: Zeroizing<[u8; KEY_LEN]>,
    header: Header,
    secrets: BTreeMap<Uuid, SecretString>,
    // Says what a Secret belongs to when no Connection names it, such as a key's passphrase.
    labels: BTreeMap<Uuid, String>,
    last_used: SystemTime,
}

pub struct Vault {
    path: PathBuf,
    idle_timeout: Duration,
    cost: KdfCost,
    unlocked: Option<Unlocked>,
}

impl Vault {
    pub fn new(path: impl Into<PathBuf>, idle_timeout: Duration) -> Self {
        Self {
            path: path.into(),
            idle_timeout,
            cost: KDF,
            unlocked: None,
        }
    }

    pub fn open_default() -> Result<Self, VaultError> {
        let path = dirs::data_dir()
            .ok_or(VaultError::NoDataDir)?
            .join("ela")
            .join("vault.bin");
        Ok(Self::new(path, DEFAULT_IDLE_TIMEOUT))
    }

    pub fn idle_timeout(&self) -> Duration {
        self.idle_timeout
    }

    /// Choosing counts as using the Vault, so shortening the time doesn't lock it on the spot.
    pub fn set_idle_timeout(&mut self, idle_timeout: Duration) {
        self.idle_timeout = idle_timeout;
        if let Some(unlocked) = &mut self.unlocked {
            unlocked.last_used = SystemTime::now();
        }
    }

    pub fn state(&mut self) -> VaultState {
        self.lock_if_idle(SystemTime::now());
        if self.unlocked.is_some() {
            VaultState::Unlocked
        } else if self.path.exists() {
            VaultState::Locked
        } else {
            VaultState::Absent
        }
    }

    pub fn create(&mut self, master: &SecretString) -> Result<(), VaultError> {
        check_master(master)?;
        if self.path.exists() {
            return Err(VaultError::AlreadyExists(self.path.clone()));
        }
        let header = new_header(self.cost)?;
        let unlocked = Unlocked {
            key: derive_key(master, &header)?,
            header,
            secrets: BTreeMap::new(),
            labels: BTreeMap::new(),
            last_used: SystemTime::now(),
        };
        write_file(&self.path, &unlocked, Replace::Never)?;
        self.unlocked = Some(unlocked);
        Ok(())
    }

    pub fn unlock(&mut self, master: &SecretString) -> Result<(), VaultError> {
        self.unlocked = Some(self.open_file(master)?);
        Ok(())
    }

    fn open_file(&self, master: &SecretString) -> Result<Unlocked, VaultError> {
        let file = fs::read(&self.path).map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => VaultError::Absent,
            _ => VaultError::Io {
                path: self.path.clone(),
                source,
            },
        })?;
        let Parsed {
            header,
            version,
            nonce,
            ciphertext,
        } = parse_file(&self.path, &file)?;
        let key = derive_key(master, &header)?;
        let plaintext = cipher(&key)
            .decrypt(
                &(*nonce).into(),
                Payload {
                    msg: ciphertext,
                    aad: &file[..HEADER_LEN],
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| VaultError::WrongPassword)?;
        let (secrets, labels) = decode_secrets(&plaintext, version)
            .ok_or_else(|| VaultError::Unreadable(self.path.clone()))?;
        Ok(Unlocked {
            key,
            header,
            secrets,
            labels,
            last_used: SystemTime::now(),
        })
    }

    /// Asks for the current password even when unlocked, so a Vault left open on an unattended
    /// machine can't be taken over. The file is one atomic replace: a crash leaves the old password
    /// working or the new one, never a Vault that neither opens.
    pub fn change_master(
        &mut self,
        current: &SecretString,
        new: &SecretString,
    ) -> Result<(), VaultError> {
        let rekeyed = self.rekeyed(current, new)?;
        write_file(&self.path, &rekeyed, Replace::Always)?;
        self.unlocked = Some(rekeyed);
        Ok(())
    }

    fn rekeyed(&self, current: &SecretString, new: &SecretString) -> Result<Unlocked, VaultError> {
        check_master(new)?;
        let opened = self.open_file(current)?;
        let header = new_header(self.cost)?;
        Ok(Unlocked {
            key: derive_key(new, &header)?,
            header,
            ..opened
        })
    }

    pub fn lock(&mut self) {
        self.unlocked = None;
    }

    /// Wall-clock time on purpose: `Instant` stops during suspend on Linux, which would leave the
    /// Vault open after a night with the lid closed. A clock set backwards also locks.
    pub fn lock_if_idle(&mut self, now: SystemTime) {
        let expired = self.unlocked.as_ref().is_some_and(|unlocked| {
            now.duration_since(unlocked.last_used)
                .map_or(true, |idle| idle >= self.idle_timeout)
        });
        if expired {
            self.lock();
        }
    }

    pub fn store(&mut self, secret: SecretString) -> Result<CredentialRef, VaultError> {
        let reference = CredentialRef(Uuid::new_v4());
        self.replace(reference, secret)?;
        Ok(reference)
    }

    pub fn replace(
        &mut self,
        reference: CredentialRef,
        secret: SecretString,
    ) -> Result<(), VaultError> {
        if secret.expose_secret().is_empty() {
            return Err(VaultError::EmptySecret);
        }
        let path = self.path.clone();
        let unlocked = self.in_use()?;
        let previous = unlocked.secrets.insert(reference.0, secret);
        write_file(&path, unlocked, Replace::Always).inspect_err(|_| {
            match previous {
                Some(previous) => unlocked.secrets.insert(reference.0, previous),
                None => unlocked.secrets.remove(&reference.0),
            };
        })
    }

    fn replace_labelled(
        &mut self,
        reference: CredentialRef,
        secret: SecretString,
        label: String,
    ) -> Result<(), VaultError> {
        let previous = self.in_use()?.labels.insert(reference.0, label);
        self.replace(reference, secret).inspect_err(|_| {
            if let Some(unlocked) = self.unlocked.as_mut() {
                restore(&mut unlocked.labels, reference.0, previous);
            }
        })
    }

    fn set_label(&mut self, reference: CredentialRef, label: String) -> Result<(), VaultError> {
        let path = self.path.clone();
        let unlocked = self.in_use()?;
        if !unlocked.secrets.contains_key(&reference.0) {
            return Err(VaultError::Missing);
        }
        let previous = unlocked.labels.insert(reference.0, label);
        write_file(&path, unlocked, Replace::Always)
            .inspect_err(|_| restore(&mut unlocked.labels, reference.0, previous))
    }

    fn labels(&mut self) -> Result<Vec<(CredentialRef, String)>, VaultError> {
        let unlocked = self.in_use()?;
        Ok(unlocked
            .labels
            .iter()
            .filter(|(id, _)| unlocked.secrets.contains_key(id))
            .map(|(id, label)| (CredentialRef(*id), label.clone()))
            .collect())
    }

    fn label(&mut self, reference: CredentialRef) -> Result<Option<String>, VaultError> {
        Ok(self.in_use()?.labels.get(&reference.0).cloned())
    }

    pub fn remove(&mut self, reference: CredentialRef) -> Result<(), VaultError> {
        let path = self.path.clone();
        let unlocked = self.in_use()?;
        let Some(previous) = unlocked.secrets.remove(&reference.0) else {
            return Ok(());
        };
        let label = unlocked.labels.remove(&reference.0);
        write_file(&path, unlocked, Replace::Always).inspect_err(|_| {
            unlocked.secrets.insert(reference.0, previous);
            restore(&mut unlocked.labels, reference.0, label);
        })
    }

    pub fn read(&mut self, reference: CredentialRef) -> Result<SecretString, VaultError> {
        self.in_use()?
            .secrets
            .get(&reference.0)
            .cloned()
            .ok_or(VaultError::Missing)
    }

    pub fn credential_state(&mut self, reference: CredentialRef) -> CredentialState {
        self.lock_if_idle(SystemTime::now());
        match &self.unlocked {
            // A synced Connection can name a Secret on a machine with no Vault at all.
            None if !self.path.exists() => CredentialState::Missing,
            None => CredentialState::Locked,
            Some(unlocked) if unlocked.secrets.contains_key(&reference.0) => {
                CredentialState::Available
            }
            Some(_) => CredentialState::Missing,
        }
    }

    fn in_use(&mut self) -> Result<&mut Unlocked, VaultError> {
        let now = SystemTime::now();
        self.lock_if_idle(now);
        let locked_or_absent = if self.path.exists() {
            VaultError::Locked
        } else {
            VaultError::Absent
        };
        let unlocked = self.unlocked.as_mut().ok_or(locked_or_absent)?;
        unlocked.last_used = now;
        Ok(unlocked)
    }
}

fn restore(labels: &mut BTreeMap<Uuid, String>, id: Uuid, previous: Option<String>) {
    match previous {
        Some(label) => labels.insert(id, label),
        None => labels.remove(&id),
    };
}

fn random(bytes: &mut [u8]) -> Result<(), VaultError> {
    getrandom::fill(bytes).map_err(|e| VaultError::Crypto(e.to_string()))
}

fn check_master(master: &SecretString) -> Result<(), VaultError> {
    if master.expose_secret().chars().count() < MIN_MASTER_CHARS {
        return Err(VaultError::MasterTooShort);
    }
    Ok(())
}

fn new_header(cost: KdfCost) -> Result<Header, VaultError> {
    let mut salt = [0; SALT_LEN];
    random(&mut salt)?;
    Ok(Header { cost, salt })
}

fn cipher(key: &[u8; KEY_LEN]) -> Aes256Gcm {
    Aes256Gcm::new_from_slice(key).expect("the array has the key's length")
}

fn derive_key(
    master: &SecretString,
    header: &Header,
) -> Result<Zeroizing<[u8; KEY_LEN]>, VaultError> {
    let crypto = |e: argon2::Error| VaultError::Crypto(e.to_string());
    let params = Params::new(
        header.cost.memory_kib,
        header.cost.iterations,
        header.cost.parallelism,
        Some(KEY_LEN),
    )
    .map_err(crypto)?;
    let mut key = Zeroizing::new([0; KEY_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(
            master.expose_secret().as_bytes(),
            &header.salt,
            key.as_mut(),
        )
        .map_err(crypto)?;
    Ok(key)
}

struct Parsed<'a> {
    header: Header,
    version: u8,
    nonce: &'a [u8; NONCE_LEN],
    ciphertext: &'a [u8],
}

fn parse_file<'a>(path: &Path, file: &'a [u8]) -> Result<Parsed<'a>, VaultError> {
    let unreadable = || VaultError::Unreadable(path.to_owned());
    if !file.starts_with(MAGIC) {
        return Err(unreadable());
    }
    let version = match file.get(MAGIC.len()) {
        Some(&version @ (UNLABELLED_FORMAT | FORMAT_VERSION)) => version,
        Some(&newer) if newer > FORMAT_VERSION => return Err(VaultError::TooNew(path.to_owned())),
        _ => return Err(unreadable()),
    };
    if file.len() < HEADER_LEN {
        return Err(unreadable());
    }
    let word = |at: usize| u32::from_le_bytes(file[at..at + 4].try_into().unwrap());
    let cost_at = MAGIC.len() + 1;
    let cost = KdfCost {
        memory_kib: word(cost_at),
        iterations: word(cost_at + 4),
        parallelism: word(cost_at + 8),
    };
    if cost.memory_kib > MAX_MEMORY_KIB {
        return Err(unreadable());
    }
    let salt_at = cost_at + 12;
    let nonce_at = salt_at + SALT_LEN;
    let header = Header {
        cost,
        salt: file[salt_at..nonce_at].try_into().unwrap(),
    };
    let nonce = file[nonce_at..HEADER_LEN].try_into().unwrap();
    Ok(Parsed {
        header,
        version,
        nonce,
        ciphertext: &file[HEADER_LEN..],
    })
}

type Labels = BTreeMap<Uuid, String>;

fn encode_secrets(secrets: &BTreeMap<Uuid, SecretString>, labels: &Labels) -> Zeroizing<Vec<u8>> {
    fn put(plain: &mut Vec<u8>, text: &[u8]) {
        plain.extend_from_slice(&(text.len() as u32).to_le_bytes());
        plain.extend_from_slice(text);
    }
    let mut plain = Zeroizing::new(Vec::new());
    for (id, secret) in secrets {
        plain.extend_from_slice(id.as_bytes());
        put(&mut plain, secret.expose_secret().as_bytes());
        put(
            &mut plain,
            labels.get(id).map_or(&[][..], |label| label.as_bytes()),
        );
    }
    plain
}

fn decode_secrets(mut plain: &[u8], version: u8) -> Option<(BTreeMap<Uuid, SecretString>, Labels)> {
    fn text(plain: &[u8]) -> Option<(&str, &[u8])> {
        let (len, rest) = plain.split_first_chunk::<4>()?;
        let (text, rest) = rest.split_at_checked(u32::from_le_bytes(*len) as usize)?;
        Some((std::str::from_utf8(text).ok()?, rest))
    }
    let mut secrets = BTreeMap::new();
    let mut labels = BTreeMap::new();
    while !plain.is_empty() {
        let (id, rest) = plain.split_first_chunk::<16>()?;
        let id = Uuid::from_bytes(*id);
        let (secret, rest) = text(rest)?;
        secrets.insert(id, SecretString::from(secret));
        plain = rest;
        if version >= FORMAT_VERSION {
            let (label, rest) = text(plain)?;
            if !label.is_empty() {
                labels.insert(id, label.to_owned());
            }
            plain = rest;
        }
    }
    Some((secrets, labels))
}

#[derive(Clone, Copy)]
enum Replace {
    Always,
    Never,
}

fn write_file(path: &Path, unlocked: &Unlocked, replace: Replace) -> Result<(), VaultError> {
    persist_file(path, prepare_file(path, unlocked)?, replace)
}

/// A fresh nonce for every write: one key encrypts every version of this file.
fn prepare_file(path: &Path, unlocked: &Unlocked) -> Result<tempfile::NamedTempFile, VaultError> {
    let io_error = |source| VaultError::Io {
        path: path.to_owned(),
        source,
    };
    let mut nonce = [0; NONCE_LEN];
    random(&mut nonce)?;
    let header = unlocked.header.encode(&nonce);
    let plaintext = encode_secrets(&unlocked.secrets, &unlocked.labels);
    let ciphertext = cipher(&unlocked.key)
        .encrypt(
            &nonce.into(),
            Payload {
                msg: &plaintext,
                aad: &header,
            },
        )
        .map_err(|e| VaultError::Crypto(e.to_string()))?;

    let dir = path
        .parent()
        .ok_or_else(|| VaultError::Unreadable(path.to_owned()))?;
    fs::create_dir_all(dir).map_err(io_error)?;
    // tempfile creates it readable only by the owner, and the rename keeps that. A crash midway
    // leaves the previous Vault whole.
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(io_error)?;
    file.write_all(&header).map_err(io_error)?;
    file.write_all(&ciphertext).map_err(io_error)?;
    file.as_file().sync_all().map_err(io_error)?;
    Ok(file)
}

fn persist_file(
    path: &Path,
    file: tempfile::NamedTempFile,
    replace: Replace,
) -> Result<(), VaultError> {
    match replace {
        Replace::Always => file.persist(path).map(drop),
        // Never overwrite on create: losing a Vault loses every password in it.
        Replace::Never => file.persist_noclobber(path).map(drop),
    }
    .map_err(|e| match (replace, e.error.kind()) {
        (Replace::Never, io::ErrorKind::AlreadyExists) => {
            VaultError::AlreadyExists(path.to_owned())
        }
        _ => VaultError::Io {
            path: path.to_owned(),
            source: e.error,
        },
    })?;
    // Without this a power cut could undo the rename, and a changed master password with it. The
    // rename has happened by now, so a failure here can't be reported as the write failing.
    if let Some(dir) = path.parent().and_then(|dir| fs::File::open(dir).ok()) {
        let _ = dir.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::*;

    const SENTINEL: &str = "sentinel-password-do-not-leak";

    fn master() -> SecretString {
        SecretString::from("correct horse battery")
    }

    fn cheap_vault(dir: &TempDir) -> Vault {
        let mut vault = Vault::new(dir.path().join("sub/vault.bin"), DEFAULT_IDLE_TIMEOUT);
        vault.cost = KdfCost {
            memory_kib: 64,
            iterations: 1,
            parallelism: 1,
        };
        vault
    }

    fn created(dir: &TempDir) -> Vault {
        let mut vault = cheap_vault(dir);
        vault.create(&master()).unwrap();
        vault
    }

    fn file_of(dir: &TempDir) -> PathBuf {
        dir.path().join("sub/vault.bin")
    }

    fn exposed(secret: SecretString) -> String {
        secret.expose_secret().to_owned()
    }

    #[test]
    fn the_default_costs_are_ones_argon2_accepts() {
        Params::new(KDF.memory_kib, KDF.iterations, KDF.parallelism, Some(32)).unwrap();
    }

    #[test]
    fn a_secret_survives_locking_and_reopening() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let reference = vault.store(SecretString::from(SENTINEL)).unwrap();
        vault.lock();
        assert!(matches!(vault.read(reference), Err(VaultError::Locked)));

        let mut reopened = cheap_vault(&dir);
        assert_eq!(reopened.state(), VaultState::Locked);
        reopened.unlock(&master()).unwrap();
        assert_eq!(exposed(reopened.read(reference).unwrap()), SENTINEL);
    }

    #[test]
    fn the_file_never_holds_the_secret_in_the_clear() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        vault.store(SecretString::from(SENTINEL)).unwrap();
        let bytes = fs::read(file_of(&dir)).unwrap();
        assert!(
            !bytes
                .windows(SENTINEL.len())
                .any(|w| w == SENTINEL.as_bytes())
        );
    }

    #[test]
    fn the_file_is_readable_only_by_its_owner() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        vault.store(SecretString::from(SENTINEL)).unwrap();
        let mode = fs::metadata(file_of(&dir)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn the_wrong_master_password_opens_nothing() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        vault.lock();
        let wrong = SecretString::from("incorrect horse battery");
        assert!(matches!(
            vault.unlock(&wrong),
            Err(VaultError::WrongPassword)
        ));
        assert_eq!(vault.state(), VaultState::Locked);
    }

    #[test]
    fn any_changed_byte_is_refused() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        vault.store(SecretString::from(SENTINEL)).unwrap();
        vault.lock();
        let good = fs::read(file_of(&dir)).unwrap();
        // The salt, the nonce and the ciphertext. Costs are covered by the next test.
        for at in [
            HEADER_LEN - NONCE_LEN - 1,
            HEADER_LEN - 1,
            HEADER_LEN,
            good.len() - 1,
        ] {
            let mut bad = good.clone();
            bad[at] ^= 1;
            fs::write(file_of(&dir), &bad).unwrap();
            assert!(
                matches!(vault.unlock(&master()), Err(VaultError::WrongPassword)),
                "byte {at}"
            );
        }
    }

    #[test]
    fn a_changed_cost_is_refused_even_if_the_key_comes_out_the_same_size() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        vault.lock();
        let mut bad = fs::read(file_of(&dir)).unwrap();
        bad[MAGIC.len() + 1 + 4] ^= 2;
        fs::write(file_of(&dir), &bad).unwrap();
        assert!(matches!(
            vault.unlock(&master()),
            Err(VaultError::WrongPassword)
        ));
    }

    #[test]
    fn files_that_are_not_vaults_are_named_as_such() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        vault.lock();
        let good = fs::read(file_of(&dir)).unwrap();

        fs::write(file_of(&dir), b"something else").unwrap();
        assert!(matches!(
            vault.unlock(&master()),
            Err(VaultError::Unreadable(_))
        ));

        fs::write(file_of(&dir), &good[..HEADER_LEN - 1]).unwrap();
        assert!(matches!(
            vault.unlock(&master()),
            Err(VaultError::Unreadable(_))
        ));

        let mut newer = good.clone();
        newer[MAGIC.len()] = FORMAT_VERSION + 1;
        fs::write(file_of(&dir), &newer).unwrap();
        assert!(matches!(
            vault.unlock(&master()),
            Err(VaultError::TooNew(_))
        ));

        let mut greedy = good;
        greedy[MAGIC.len() + 1..MAGIC.len() + 5].copy_from_slice(&u32::MAX.to_le_bytes());
        fs::write(file_of(&dir), &greedy).unwrap();
        assert!(matches!(
            vault.unlock(&master()),
            Err(VaultError::Unreadable(_))
        ));
    }

    #[test]
    fn creating_never_overwrites_a_vault() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let reference = vault.store(SecretString::from(SENTINEL)).unwrap();

        let mut second = cheap_vault(&dir);
        assert!(matches!(
            second.create(&SecretString::from("another master password")),
            Err(VaultError::AlreadyExists(_))
        ));
        second.unlock(&master()).unwrap();
        assert_eq!(exposed(second.read(reference).unwrap()), SENTINEL);
    }

    #[test]
    fn a_short_master_password_is_refused() {
        let dir = TempDir::new().unwrap();
        let mut vault = cheap_vault(&dir);
        assert!(matches!(
            vault.create(&SecretString::from("short")),
            Err(VaultError::MasterTooShort)
        ));
        assert_eq!(vault.state(), VaultState::Absent);
    }

    #[test]
    fn an_idle_vault_locks_and_forgets_its_secrets() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let reference = vault.store(SecretString::from(SENTINEL)).unwrap();
        let start = SystemTime::now();

        vault.lock_if_idle(start + DEFAULT_IDLE_TIMEOUT - Duration::from_secs(5));
        assert!(vault.unlocked.is_some());

        vault.lock_if_idle(start + DEFAULT_IDLE_TIMEOUT + Duration::from_secs(5));
        assert!(vault.unlocked.is_none());
        assert_eq!(vault.credential_state(reference), CredentialState::Locked);
    }

    #[test]
    fn a_clock_set_backwards_locks() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        vault.lock_if_idle(SystemTime::now() - Duration::from_secs(3600));
        assert_eq!(vault.state(), VaultState::Locked);
    }

    #[test]
    fn using_the_vault_restarts_the_idle_clock() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let reference = vault.store(SecretString::from(SENTINEL)).unwrap();
        let long_ago = SystemTime::now() - Duration::from_secs(60);
        vault.unlocked.as_mut().unwrap().last_used = long_ago;
        vault.read(reference).unwrap();
        assert!(vault.unlocked.as_ref().unwrap().last_used > long_ago);
    }

    #[test]
    fn replacing_and_removing_are_saved() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let reference = vault.store(SecretString::from("first")).unwrap();
        let other = vault.store(SecretString::from("other")).unwrap();
        vault
            .replace(reference, SecretString::from("second"))
            .unwrap();

        let mut reopened = cheap_vault(&dir);
        reopened.unlock(&master()).unwrap();
        assert_eq!(exposed(reopened.read(reference).unwrap()), "second");

        reopened.remove(reference).unwrap();
        let mut again = cheap_vault(&dir);
        again.unlock(&master()).unwrap();
        assert_eq!(again.credential_state(reference), CredentialState::Missing);
        assert!(matches!(again.read(reference), Err(VaultError::Missing)));
        assert_eq!(exposed(again.read(other).unwrap()), "other");
    }

    #[test]
    fn labels_are_kept_with_their_secret_and_go_with_it() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let key = CredentialRef(Uuid::new_v4());
        let plain = vault.store(SecretString::from("unlabelled")).unwrap();
        vault
            .replace_labelled(key, SecretString::from("passphrase"), "key label".into())
            .unwrap();
        vault.replace(key, SecretString::from("newer")).unwrap();

        let mut reopened = cheap_vault(&dir);
        reopened.unlock(&master()).unwrap();
        assert_eq!(reopened.labels().unwrap(), [(key, "key label".to_owned())]);
        assert_eq!(reopened.label(plain).unwrap(), None);
        assert_eq!(exposed(reopened.read(key).unwrap()), "newer");

        reopened.set_label(plain, "now labelled".into()).unwrap();
        reopened.remove(key).unwrap();
        let mut again = cheap_vault(&dir);
        again.unlock(&master()).unwrap();
        assert_eq!(
            again.labels().unwrap(),
            [(plain, "now labelled".to_owned())]
        );
    }

    #[test]
    fn only_a_secret_that_exists_can_be_labelled_and_a_locked_vault_lists_nothing() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let nothing = CredentialRef(Uuid::new_v4());
        assert!(matches!(
            vault.set_label(nothing, "x".into()),
            Err(VaultError::Missing)
        ));
        vault.lock();
        assert!(matches!(vault.labels(), Err(VaultError::Locked)));
    }

    #[test]
    fn a_failed_write_leaves_the_label_as_it_was() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let key = CredentialRef(Uuid::new_v4());
        vault
            .replace_labelled(key, SecretString::from("kept"), "old".into())
            .unwrap();
        vault.path = dir.path().join("sub/vault.bin/not-a-directory");
        assert!(
            vault
                .replace_labelled(key, SecretString::from("lost"), "new".into())
                .is_err()
        );
        assert!(vault.set_label(key, "new".into()).is_err());
        assert!(vault.remove(key).is_err());
        assert_eq!(vault.label(key).unwrap().as_deref(), Some("old"));
        assert_eq!(exposed(vault.read(key).unwrap()), "kept");
    }

    #[test]
    fn a_format_1_vault_opens_and_is_written_as_format_2() {
        let dir = TempDir::new().unwrap();
        let vault = created(&dir);
        let id = Uuid::new_v4();
        let unlocked = vault.unlocked.as_ref().unwrap();
        let mut nonce = [0; NONCE_LEN];
        random(&mut nonce).unwrap();
        let mut header = unlocked.header.encode(&nonce);
        header[MAGIC.len()] = UNLABELLED_FORMAT;
        let mut plain = id.as_bytes().to_vec();
        plain.extend_from_slice(&6u32.to_le_bytes());
        plain.extend_from_slice(b"old v1");
        let ciphertext = cipher(&unlocked.key)
            .encrypt(
                &nonce.into(),
                Payload {
                    msg: &plain,
                    aad: &header,
                },
            )
            .unwrap();
        fs::write(file_of(&dir), [header, ciphertext].concat()).unwrap();

        let mut old = cheap_vault(&dir);
        old.unlock(&master()).unwrap();
        assert_eq!(exposed(old.read(CredentialRef(id)).unwrap()), "old v1");
        assert!(old.labels().unwrap().is_empty());
        old.set_label(CredentialRef(id), "labelled".into()).unwrap();
        assert_eq!(
            fs::read(file_of(&dir)).unwrap()[MAGIC.len()],
            FORMAT_VERSION
        );
        let mut reopened = cheap_vault(&dir);
        reopened.unlock(&master()).unwrap();
        assert_eq!(
            reopened.labels().unwrap(),
            [(CredentialRef(id), "labelled".to_owned())]
        );
    }

    #[test]
    fn without_a_vault_a_reference_is_missing_rather_than_locked() {
        let dir = TempDir::new().unwrap();
        let mut vault = cheap_vault(&dir);
        let reference = CredentialRef(Uuid::new_v4());
        assert_eq!(vault.credential_state(reference), CredentialState::Missing);
        assert!(matches!(vault.read(reference), Err(VaultError::Absent)));
    }

    #[test]
    fn an_empty_secret_is_refused() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        assert!(matches!(
            vault.store(SecretString::from("")),
            Err(VaultError::EmptySecret)
        ));
    }

    #[test]
    fn a_failed_write_leaves_memory_matching_the_file() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let reference = vault.store(SecretString::from("kept")).unwrap();
        vault.path = dir.path().join("sub/vault.bin/not-a-directory");
        assert!(
            vault
                .replace(reference, SecretString::from("lost"))
                .is_err()
        );
        assert_eq!(exposed(vault.read(reference).unwrap()), "kept");
    }

    #[test]
    fn secrets_with_any_length_or_text_round_trip() {
        let mut secrets = BTreeMap::new();
        for text in ["a", "pässwörd 🔑", &"x".repeat(70_000)] {
            secrets.insert(Uuid::new_v4(), SecretString::from(text));
        }
        let (decoded, _) =
            decode_secrets(&encode_secrets(&secrets, &Labels::new()), FORMAT_VERSION).unwrap();
        assert_eq!(decoded.len(), 3);
        for (id, secret) in &secrets {
            assert_eq!(decoded[id].expose_secret(), secret.expose_secret());
        }
    }

    #[test]
    fn a_truncated_plaintext_is_not_read_as_fewer_secrets() {
        let mut secrets = BTreeMap::new();
        secrets.insert(Uuid::new_v4(), SecretString::from("secret"));
        let plain = encode_secrets(&secrets, &Labels::new());
        assert!(decode_secrets(&plain[..plain.len() - 1], FORMAT_VERSION).is_none());
    }

    fn new_master() -> SecretString {
        SecretString::from("a different master password")
    }

    #[test]
    fn changing_the_master_password_keeps_the_secrets_and_only_the_new_one_opens() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let reference = vault.store(SecretString::from(SENTINEL)).unwrap();
        let before = fs::read(file_of(&dir)).unwrap();

        vault.change_master(&master(), &new_master()).unwrap();

        assert_eq!(exposed(vault.read(reference).unwrap()), SENTINEL);
        let after = fs::read(file_of(&dir)).unwrap();
        let salt = HEADER_LEN - NONCE_LEN - SALT_LEN..HEADER_LEN - NONCE_LEN;
        assert_ne!(before[salt.clone()], after[salt], "the salt must be new");
        assert_eq!(
            fs::metadata(file_of(&dir)).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let mut reopened = cheap_vault(&dir);
        assert!(matches!(
            reopened.unlock(&master()),
            Err(VaultError::WrongPassword)
        ));
        reopened.unlock(&new_master()).unwrap();
        assert_eq!(exposed(reopened.read(reference).unwrap()), SENTINEL);
    }

    #[test]
    fn later_writes_use_the_new_master_password() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        vault.change_master(&master(), &new_master()).unwrap();
        let reference = vault.store(SecretString::from(SENTINEL)).unwrap();

        let mut reopened = cheap_vault(&dir);
        reopened.unlock(&new_master()).unwrap();
        assert_eq!(exposed(reopened.read(reference).unwrap()), SENTINEL);
    }

    #[test]
    fn a_crash_before_the_rename_leaves_the_old_password_working() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let reference = vault.store(SecretString::from(SENTINEL)).unwrap();
        let rekeyed = vault.rekeyed(&master(), &new_master()).unwrap();
        // Kept on disk the way a process dying here would leave it.
        let left_behind = prepare_file(&file_of(&dir), &rekeyed)
            .unwrap()
            .into_temp_path()
            .keep()
            .unwrap();
        drop(vault);

        let mut after_crash = cheap_vault(&dir);
        assert!(matches!(
            after_crash.unlock(&new_master()),
            Err(VaultError::WrongPassword)
        ));
        after_crash.unlock(&master()).unwrap();
        assert_eq!(exposed(after_crash.read(reference).unwrap()), SENTINEL);

        // What was about to replace it is whole too, so no moment exists with neither.
        fs::rename(left_behind, file_of(&dir)).unwrap();
        let mut after_rename = cheap_vault(&dir);
        after_rename.unlock(&new_master()).unwrap();
        assert_eq!(exposed(after_rename.read(reference).unwrap()), SENTINEL);
    }

    #[test]
    fn a_failed_change_leaves_the_file_and_memory_on_the_old_password() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let reference = vault.store(SecretString::from(SENTINEL)).unwrap();
        let real_path = vault.path.clone();
        // The file can be read, but the replacement can't be put next to it.
        let folder = real_path.parent().unwrap().to_owned();
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o500)).unwrap();

        let refused = vault.change_master(&master(), &new_master());
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).unwrap();

        assert!(matches!(refused, Err(VaultError::Io { .. })), "{refused:?}");
        vault.store(SecretString::from("written later")).unwrap();
        let mut reopened = cheap_vault(&dir);
        reopened.unlock(&master()).unwrap();
        assert_eq!(exposed(reopened.read(reference).unwrap()), SENTINEL);
    }

    #[test]
    fn the_current_password_is_required_even_when_unlocked() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let before = fs::read(file_of(&dir)).unwrap();
        let wrong = SecretString::from("incorrect horse battery");

        assert!(matches!(
            vault.change_master(&wrong, &new_master()),
            Err(VaultError::WrongPassword)
        ));

        assert_eq!(fs::read(file_of(&dir)).unwrap(), before);
        assert_eq!(vault.state(), VaultState::Unlocked);
    }

    #[test]
    fn a_short_new_password_is_refused_and_nothing_changes() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let before = fs::read(file_of(&dir)).unwrap();

        assert!(matches!(
            vault.change_master(&master(), &SecretString::from("short")),
            Err(VaultError::MasterTooShort)
        ));
        assert_eq!(fs::read(file_of(&dir)).unwrap(), before);
    }

    #[test]
    fn a_locked_vault_can_change_its_password_and_ends_up_open() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        let reference = vault.store(SecretString::from(SENTINEL)).unwrap();
        vault.lock();

        vault.change_master(&master(), &new_master()).unwrap();

        assert_eq!(vault.state(), VaultState::Unlocked);
        assert_eq!(exposed(vault.read(reference).unwrap()), SENTINEL);
    }

    #[test]
    fn without_a_vault_there_is_nothing_to_change() {
        let dir = TempDir::new().unwrap();
        let mut vault = cheap_vault(&dir);
        assert!(matches!(
            vault.change_master(&master(), &new_master()),
            Err(VaultError::Absent)
        ));
        assert_eq!(vault.state(), VaultState::Absent);
    }

    #[test]
    fn the_idle_timeout_can_be_chosen_and_choosing_counts_as_use() {
        let dir = TempDir::new().unwrap();
        let mut vault = created(&dir);
        vault.unlocked.as_mut().unwrap().last_used =
            SystemTime::now() - Duration::from_secs(10 * 60);

        vault.set_idle_timeout(Duration::from_secs(5 * 60));

        assert_eq!(vault.idle_timeout(), Duration::from_secs(5 * 60));
        assert_eq!(vault.state(), VaultState::Unlocked);
        vault.lock_if_idle(SystemTime::now() + Duration::from_secs(6 * 60));
        assert_eq!(vault.state(), VaultState::Locked);
    }

    #[test]
    fn the_default_idle_timeout_is_one_of_the_choices() {
        assert!(IDLE_CHOICES_MINUTES.contains(&((DEFAULT_IDLE_TIMEOUT.as_secs() / 60) as u32)));
    }

    #[test]
    fn debug_output_shows_no_secret() {
        let secret = SecretString::from(SENTINEL);
        assert!(!format!("{secret:?}").contains(SENTINEL));
    }
}
