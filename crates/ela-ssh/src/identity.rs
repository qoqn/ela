use std::path::{Path, PathBuf};
use std::sync::Arc;

use ela_session::{
    CredentialRef, ExposeSecret, PassphraseAnswer, PassphraseNote, Prompt, SecretString,
    SessionSink, SessionStatus,
};
use russh::keys::agent::AgentIdentity;
use russh::keys::ssh_key::encoding::Encode;
use russh::keys::ssh_key::private::KeypairData;
use russh::keys::{Certificate, HashAlg, PrivateKey, PublicKey, decode_secret_key, ssh_key};
use signature::Signer as _;

pub trait Passphrases: Send + Sync {
    fn saved(&self, key: &SavedKey) -> Option<SecretString>;
    fn remember(&self, key: &SavedKey, passphrase: SecretString) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedKey {
    pub credential: CredentialRef,
    pub path: String,
    pub fingerprint: Option<String>,
}

pub(crate) enum IdentityFile {
    Open(PrivateKey),
    Locked(LockedKey),
}

pub(crate) struct LockedKey {
    pub label: String,
    text: String,
    pub public: Option<PublicKey>,
    pub credential: CredentialRef,
}

pub(crate) fn read(path: &Path, home: Option<&Path>) -> Result<IdentityFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let error = match decode_secret_key(&text, None) {
        Ok(key) => return Ok(IdentityFile::Open(key)),
        Err(error) => error,
    };
    // Encrypted PKCS#8 fails without a passphrase as if it were damaged.
    let locked = matches!(error, russh::keys::Error::KeyIsEncrypted)
        || text.contains("-----BEGIN ENCRYPTED PRIVATE KEY-----");
    if !locked {
        return Err(error.to_string());
    }
    let public = public_part(path, &text);
    let identity = match &public {
        Some(public) => public.fingerprint(HashAlg::Sha256).to_string(),
        None => format!(
            "file:{}",
            path.canonicalize()
                .unwrap_or_else(|_| path.to_owned())
                .display()
        ),
    };
    Ok(IdentityFile::Locked(LockedKey {
        label: label(path, home),
        text,
        public,
        credential: CredentialRef::for_key(&identity),
    }))
}

/// OpenSSH loads `<key>-cert.pub` beside every identity file without being told.
pub(crate) fn certificate_beside(path: &Path) -> Option<Certificate> {
    let mut beside = path.as_os_str().to_owned();
    beside.push("-cert.pub");
    let text = std::fs::read_to_string(PathBuf::from(beside)).ok()?;
    match Certificate::from_openssh(text.trim()) {
        Ok(certificate) => Some(certificate),
        Err(error) => {
            tracing::debug!(
                "skipping the certificate beside {}: {error}",
                path.display()
            );
            None
        }
    }
}

/// OpenSSH keeps the public key readable inside an encrypted file. Older formats don't, and
/// OpenSSH then looks for the `.pub` beside it, as here.
fn public_part(path: &Path, text: &str) -> Option<PublicKey> {
    if let Ok(key) = PrivateKey::from_openssh(text) {
        return Some(key.public_key().clone());
    }
    let mut beside = path.as_os_str().to_owned();
    beside.push(".pub");
    let text = std::fs::read_to_string(PathBuf::from(beside)).ok()?;
    PublicKey::from_openssh(text.trim()).ok()
}

fn label(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

impl LockedKey {
    pub(crate) fn saved_key(&self) -> SavedKey {
        SavedKey {
            credential: self.credential,
            path: self.label.clone(),
            fingerprint: self
                .public
                .as_ref()
                .map(|public| public.fingerprint(HashAlg::Sha256).to_string()),
        }
    }

    async fn open(&self, passphrase: &SecretString) -> Option<PrivateKey> {
        let (text, passphrase) = (self.text.clone(), passphrase.clone());
        // OpenSSH's bcrypt KDF takes a noticeable moment on purpose.
        tokio::task::spawn_blocking(move || {
            decode_secret_key(&text, Some(passphrase.expose_secret())).ok()
        })
        .await
        .ok()
        .flatten()
    }
}

pub(crate) struct Unlocker<'a> {
    pub sink: &'a SessionSink,
    pub jump_host: Option<&'a str>,
    pub passphrases: Option<&'a Arc<dyn Passphrases>>,
}

impl Unlocker<'_> {
    pub async fn unlock(&self, key: &LockedKey) -> Option<PrivateKey> {
        let mut note = None;
        if let Some(saved) = self.saved(key.saved_key()).await {
            if let Some(open) = key.open(&saved).await {
                return Some(open);
            }
            note = Some(PassphraseNote::SavedWrong);
        }
        loop {
            let answer = self
                .sink
                .ask_passphrase(
                    self.jump_host.map(str::to_owned),
                    Prompt::Passphrase {
                        key: key.label.clone(),
                        credential: key.credential,
                        note,
                    },
                )
                .await;
            self.sink.status(SessionStatus::Authenticating {
                jump_host: self.jump_host.map(str::to_owned),
            });
            match answer {
                PassphraseAnswer::Skip => return None,
                PassphraseAnswer::Saved => {
                    let Some(saved) = self.saved(key.saved_key()).await else {
                        continue;
                    };
                    match key.open(&saved).await {
                        Some(open) => return Some(open),
                        None => note = Some(PassphraseNote::SavedWrong),
                    }
                }
                PassphraseAnswer::Typed {
                    passphrase,
                    remember,
                } => match key.open(&passphrase).await {
                    Some(open) => {
                        if remember {
                            self.remember(key.saved_key(), passphrase).await;
                        }
                        return Some(open);
                    }
                    None => note = Some(PassphraseNote::Wrong),
                },
            }
        }
    }

    async fn saved(&self, key: SavedKey) -> Option<SecretString> {
        let passphrases = self.passphrases?.clone();
        tokio::task::spawn_blocking(move || passphrases.saved(&key))
            .await
            .ok()
            .flatten()
    }

    async fn remember(&self, key: SavedKey, passphrase: SecretString) {
        let Some(passphrases) = self.passphrases.cloned() else {
            return;
        };
        let remembered =
            tokio::task::spawn_blocking(move || passphrases.remember(&key, passphrase)).await;
        // The key is open either way, so signing in goes on.
        match remembered {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "couldn't remember the passphrase"),
            Err(error) => tracing::warn!(%error, "couldn't remember the passphrase"),
        }
    }
}

/// Asks for the passphrase only once the server has accepted the public key, as OpenSSH does,
/// so a key the server would refuse never interrupts.
pub(crate) struct LockedSigner<'a> {
    key: &'a LockedKey,
    unlocker: &'a Unlocker<'a>,
    // Kept for the plain key after its certificate, so the passphrase is asked once.
    unlocked: Option<Box<PrivateKey>>,
}

impl<'a> LockedSigner<'a> {
    pub(crate) fn new(key: &'a LockedKey, unlocker: &'a Unlocker<'a>) -> Self {
        Self {
            key,
            unlocker,
            unlocked: None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum SignError {
    Send,
    Skipped,
    Sign(String),
}

impl std::fmt::Display for SignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Send => f.write_str("the connection went away while signing"),
            Self::Skipped => f.write_str("the key was skipped"),
            Self::Sign(error) => write!(f, "couldn't sign: {error}"),
        }
    }
}

impl From<russh::SendError> for SignError {
    fn from(_: russh::SendError) -> Self {
        Self::Send
    }
}

impl russh::Signer for LockedSigner<'_> {
    type Error = SignError;

    async fn auth_sign(
        &mut self,
        _: &AgentIdentity,
        hash_alg: Option<HashAlg>,
        mut to_sign: Vec<u8>,
    ) -> Result<Vec<u8>, SignError> {
        if self.unlocked.is_none() {
            self.unlocked = self.unlocker.unlock(self.key).await.map(Box::new);
        }
        let key = self.unlocked.as_ref().ok_or(SignError::Skipped)?;
        let signature = sign(key, hash_alg, &to_sign).map_err(SignError::Sign)?;
        signature
            .as_slice()
            .encode(&mut to_sign)
            .map_err(|e| SignError::Sign(e.to_string()))?;
        Ok(to_sign)
    }
}

fn sign(key: &PrivateKey, hash_alg: Option<HashAlg>, data: &[u8]) -> Result<Vec<u8>, String> {
    let signature: ssh_key::Signature = match key.key_data() {
        KeypairData::Rsa(pair) => (pair, hash_alg).try_sign(data),
        pair => pair.try_sign(data),
    }
    .map_err(|e| e.to_string())?;
    let mut encoded = Vec::new();
    signature.encode(&mut encoded).map_err(|e| e.to_string())?;
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use ela_session::Answer;
    use std::collections::HashMap;
    use std::process::Command;
    use std::sync::{LazyLock, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use ela_session::{
        Frontend, FrontendEvent, Protocol, ProtocolRegistry, RendererKind, ResolvedConfig, Session,
        SessionError, SessionIo, SessionManager, Size,
    };
    use russh::keys::ssh_key::LineEnding;

    use super::*;

    const PASSPHRASE: &str = "fixture passphrase";

    fn generate_key(dir: &Path, name: &str, format: &[&str]) {
        let generated = Command::new("ssh-keygen")
            .args(["-q", "-C", "", "-N", PASSPHRASE])
            .args(format)
            .arg("-f")
            .arg(dir.join(name))
            .status()
            .unwrap();
        assert!(generated.success());
    }

    // One set for the whole run: a passphrase is remembered under its key's fingerprint, and a
    // test may read the same key again to open it.
    fn locked(name: &str) -> &'static LockedKey {
        static KEYS: LazyLock<HashMap<&str, LockedKey>> = LazyLock::new(|| {
            const OLDER_FORMAT: &[&str] = &["-t", "rsa", "-b", "2048", "-m", "PEM"];
            let dir = tempfile::tempdir().unwrap();
            generate_key(dir.path(), "locked_ed25519", &["-t", "ed25519"]);
            generate_key(dir.path(), "locked_rsa.pem", OLDER_FORMAT);
            generate_key(dir.path(), "alone_rsa.pem", OLDER_FORMAT);
            std::fs::remove_file(dir.path().join("alone_rsa.pem.pub")).unwrap();
            ["locked_ed25519", "locked_rsa.pem", "alone_rsa.pem"]
                .map(|name| match read(&dir.path().join(name), None).unwrap() {
                    IdentityFile::Locked(key) => (name, key),
                    IdentityFile::Open(_) => panic!("{name} opened without a passphrase"),
                })
                .into()
        });
        &KEYS[name]
    }

    #[test]
    fn an_openssh_key_names_its_passphrase_by_its_fingerprint_without_a_pub_file() {
        let key = locked("locked_ed25519");
        let public = key.public.clone().unwrap();
        let fingerprint = public.fingerprint(HashAlg::Sha256).to_string();
        assert_eq!(key.credential, CredentialRef::for_key(&fingerprint));
    }

    #[test]
    fn an_older_format_takes_its_public_key_from_the_pub_file_beside_it() {
        let with_pub = locked("locked_rsa.pem");
        let alone = locked("alone_rsa.pem");
        assert!(with_pub.public.is_some());
        assert!(alone.public.is_none());
        assert_ne!(with_pub.credential, alone.credential);
    }

    #[tokio::test]
    async fn only_the_right_passphrase_opens_a_key() {
        for name in ["locked_ed25519", "locked_rsa.pem", "alone_rsa.pem"] {
            let key = locked(name);
            assert!(key.open(&SecretString::from(PASSPHRASE)).await.is_some());
            assert!(key.open(&SecretString::from("wrong")).await.is_none());
        }
    }

    #[tokio::test]
    async fn a_key_without_a_passphrase_opens_at_once() {
        let opened = locked("locked_ed25519")
            .open(&SecretString::from(PASSPHRASE))
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("open");
        std::fs::write(&path, opened.to_openssh(LineEnding::LF).unwrap()).unwrap();
        assert!(matches!(read(&path, None).unwrap(), IdentityFile::Open(_)));
    }

    #[test]
    fn a_key_under_home_is_shown_from_the_tilde() {
        let home = Path::new("/home/me");
        assert_eq!(
            label(Path::new("/home/me/.ssh/id_ed25519"), Some(home)),
            "~/.ssh/id_ed25519"
        );
        assert_eq!(label(Path::new("/keys/id"), Some(home)), "/keys/id");
    }

    #[derive(Default)]
    struct Remembered(Mutex<Vec<(SavedKey, String)>>);

    impl Passphrases for Remembered {
        fn saved(&self, key: &SavedKey) -> Option<SecretString> {
            let saved = self.0.lock().unwrap();
            let (_, passphrase) = saved
                .iter()
                .find(|(saved, _)| saved.credential == key.credential)?;
            Some(SecretString::from(passphrase.clone()))
        }
        fn remember(&self, key: &SavedKey, passphrase: SecretString) -> Result<(), String> {
            let mut saved = self.0.lock().unwrap();
            saved.retain(|(saved, _)| saved.credential != key.credential);
            saved.push((key.clone(), passphrase.expose_secret().to_owned()));
            Ok(())
        }
    }

    struct Unlocking(Arc<dyn Passphrases>);

    #[async_trait]
    impl Protocol for Unlocking {
        fn id(&self) -> &'static str {
            "unlocking"
        }
        fn display_name(&self) -> &'static str {
            "Unlocking"
        }
        fn default_port(&self) -> Option<u16> {
            None
        }
        fn renderer(&self) -> RendererKind {
            RendererKind::Terminal
        }
        async fn connect(
            &self,
            _: ResolvedConfig,
            sink: &SessionSink,
        ) -> Result<Box<dyn Session>, SessionError> {
            let unlocker = Unlocker {
                sink,
                jump_host: None,
                passphrases: Some(&self.0),
            };
            match unlocker.unlock(locked("locked_ed25519")).await {
                Some(_) => Ok(Box::new(Idle)),
                None => Err(SessionError::AuthFailed("skipped".into())),
            }
        }
    }

    struct Idle;

    #[async_trait]
    impl Session for Idle {
        async fn run(self: Box<Self>, io: SessionIo) -> Result<(), SessionError> {
            io.cancel.cancelled().await;
            Ok(())
        }
    }

    #[derive(Default)]
    struct Statuses(Mutex<Vec<SessionStatus>>);

    impl Frontend for Statuses {
        fn event(&self, event: FrontendEvent) {
            if let FrontendEvent::Status { status } = event {
                self.0.lock().unwrap().push(status);
            }
        }
        fn data(&self, _: Vec<u8>) {}
    }

    impl Statuses {
        fn notes(&self) -> Vec<Option<PassphraseNote>> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .filter_map(|status| match status {
                    SessionStatus::Asking {
                        prompt: Prompt::Passphrase { note, .. },
                        ..
                    } => Some(*note),
                    _ => None,
                })
                .collect()
        }
        fn prompt(&self) -> ela_session::PromptId {
            self.0
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find_map(|status| match status {
                    SessionStatus::Asking { id, .. } => Some(*id),
                    _ => None,
                })
                .unwrap()
        }
        fn connected(&self) -> bool {
            self.0
                .lock()
                .unwrap()
                .iter()
                .any(|status| matches!(status, SessionStatus::Connected))
        }
        fn failed(&self) -> bool {
            self.0.lock().unwrap().iter().any(|status| {
                matches!(
                    status,
                    SessionStatus::Closed {
                        error: Some(SessionError::AuthFailed(_))
                    }
                )
            })
        }
    }

    fn open(
        passphrases: Arc<Remembered>,
    ) -> (Arc<SessionManager>, Arc<Statuses>, ela_session::SessionId) {
        let mut registry = ProtocolRegistry::new();
        registry.register(Unlocking(passphrases));
        let manager = SessionManager::new(registry);
        let statuses = Arc::new(Statuses::default());
        let config = ResolvedConfig {
            connection: None,
            host: "test".into(),
            port: 22,
            username: None,
            password: None,
            options: Default::default(),
            initial_size: Size {
                width: 80,
                height: 24,
            },
            jump_hosts: Vec::new(),
            saved_password_lost: false,
        };
        let id = manager.open("unlocking", config, statuses.clone()).unwrap();
        (manager, statuses, id)
    }

    async fn until(what: &str, done: impl Fn() -> bool) {
        for _ in 0..200 {
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting until {what}");
    }

    fn typed(passphrase: &str, remember: bool) -> PassphraseAnswer {
        PassphraseAnswer::Typed {
            passphrase: SecretString::from(passphrase),
            remember,
        }
    }

    #[tokio::test]
    async fn a_remembered_passphrase_opens_the_key_without_asking() {
        let passphrases = Arc::new(Remembered::default());
        let key = locked("locked_ed25519").saved_key();
        passphrases
            .remember(&key, SecretString::from(PASSPHRASE))
            .unwrap();

        let (manager, statuses, id) = open(passphrases);
        until("connected", || statuses.connected()).await;
        assert!(statuses.notes().is_empty());
        manager.close(id);
    }

    #[tokio::test]
    async fn a_wrong_passphrase_asks_again_and_a_right_one_is_remembered() {
        let passphrases = Arc::new(Remembered::default());
        let key = locked("locked_ed25519").saved_key();
        passphrases
            .remember(&key, SecretString::from("stale"))
            .unwrap();

        let (manager, statuses, id) = open(passphrases.clone());
        until("asked", || statuses.notes().len() == 1).await;
        assert_eq!(statuses.notes(), [Some(PassphraseNote::SavedWrong)]);

        manager
            .answer(
                id,
                statuses.prompt(),
                Answer::Passphrase(typed("wrong", true)),
            )
            .unwrap();
        until("asked again", || statuses.notes().len() == 2).await;
        assert_eq!(statuses.notes()[1], Some(PassphraseNote::Wrong));
        assert_eq!(passphrases.saved(&key).unwrap().expose_secret(), "stale");

        manager
            .answer(
                id,
                statuses.prompt(),
                Answer::Passphrase(typed(PASSPHRASE, true)),
            )
            .unwrap();
        until("connected", || statuses.connected()).await;
        assert_eq!(passphrases.saved(&key).unwrap().expose_secret(), PASSPHRASE);
        let remembered = passphrases.0.lock().unwrap()[0].0.clone();
        assert_eq!(remembered.path, key.path);
        assert!(
            remembered
                .fingerprint
                .as_deref()
                .is_some_and(|f| f.starts_with("SHA256:")),
            "{remembered:?}"
        );
        manager.close(id);
    }

    #[tokio::test]
    async fn a_passphrase_not_to_remember_is_not_kept() {
        let passphrases = Arc::new(Remembered::default());
        let (manager, statuses, id) = open(passphrases.clone());
        until("asked", || statuses.notes() == [None]).await;
        manager
            .answer(
                id,
                statuses.prompt(),
                Answer::Passphrase(typed(PASSPHRASE, false)),
            )
            .unwrap();
        until("connected", || statuses.connected()).await;
        assert!(passphrases.0.lock().unwrap().is_empty());
        manager.close(id);
    }

    #[tokio::test]
    async fn skipping_leaves_the_key_locked() {
        let (manager, statuses, id) = open(Arc::new(Remembered::default()));
        until("asked", || statuses.notes().len() == 1).await;
        manager
            .answer(
                id,
                statuses.prompt(),
                Answer::Passphrase(PassphraseAnswer::Skip),
            )
            .unwrap();
        until("failed", || statuses.failed()).await;
        manager.close(id);
    }
}
