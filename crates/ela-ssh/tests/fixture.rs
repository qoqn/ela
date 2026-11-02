use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ela_session::{
    Answer, CancellationToken, ExposeSecret, FileError, FolderOutcome, ForwardFailure, Frontend,
    FrontendEvent, JumpHost, PassphraseAnswer, PasswordAnswer, PasswordNote, Prompt,
    ProtocolRegistry, RemoteFiles, RemoteKind, ResolvedConfig, SecretString, SessionCommand,
    SessionError, SessionId, SessionManager, SessionStatus, Size, Transfer, remote_join,
};

/// Not the fixture's key: a valid ed25519 key that stands in for a previous identity.
const STALE_KEY: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
const STALE_FINGERPRINT: &str = "SHA256:";
const HOST_ENTRY: &str = "[127.0.0.1]:2222";

#[derive(Default)]
struct Recorder {
    output: Mutex<Vec<u8>>,
    statuses: Mutex<Vec<SessionStatus>>,
}

impl Recorder {
    fn prompt_id(&self) -> ela_session::PromptId {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|s| match s {
                SessionStatus::Asking { id, .. } => Some(*id),
                _ => None,
            })
            .expect("Ela asked nothing")
    }
    fn asked(&self) -> Option<(String, Option<String>)> {
        self.statuses.lock().unwrap().iter().find_map(|s| match s {
            SessionStatus::Asking {
                prompt: Prompt::Trust {
                    fingerprint, known, ..
                },
                ..
            } => Some((fingerprint.clone(), known.clone())),
            _ => None,
        })
    }
    fn prompts(&self) -> Vec<Option<String>> {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .filter_map(|s| match s {
                SessionStatus::Asking {
                    jump_host,
                    prompt: Prompt::Trust { .. },
                    ..
                } => Some(jump_host.clone()),
                _ => None,
            })
            .collect()
    }
    fn passphrase_prompts(&self) -> usize {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    SessionStatus::Asking {
                        prompt: Prompt::Passphrase { .. },
                        ..
                    }
                )
            })
            .count()
    }
    fn password_prompts(&self) -> Vec<(Option<String>, Option<PasswordNote>)> {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .filter_map(|s| match s {
                SessionStatus::Asking {
                    jump_host,
                    prompt: Prompt::Password { note, .. },
                    ..
                } => Some((jump_host.clone(), *note)),
                _ => None,
            })
            .collect()
    }
    fn offered_remembering(&self) -> Vec<bool> {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .filter_map(|s| match s {
                SessionStatus::Asking {
                    prompt: Prompt::Password { can_remember, .. },
                    ..
                } => Some(*can_remember),
                _ => None,
            })
            .collect()
    }
    fn saw_connected(&self) -> bool {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .any(|s| matches!(s, SessionStatus::Connected))
    }
    fn closed(&self) -> Option<Option<SessionError>> {
        self.statuses.lock().unwrap().iter().find_map(|s| match s {
            SessionStatus::Closed { error } => Some(error.clone()),
            _ => None,
        })
    }
}

impl Frontend for Recorder {
    fn event(&self, event: FrontendEvent) {
        if let FrontendEvent::Status { status } = event {
            self.statuses.lock().unwrap().push(status);
        }
    }
    fn data(&self, message: Vec<u8>) {
        self.output.lock().unwrap().extend_from_slice(&message[1..]);
    }
}

fn config(password: &str) -> ResolvedConfig {
    ResolvedConfig {
        connection: None,
        host: "127.0.0.1".into(),
        port: 2222,
        username: Some("ela".into()),
        password: Some(SecretString::from(password.to_owned())),
        options: Default::default(),
        initial_size: Size {
            width: 100,
            height: 30,
        },
        jump_hosts: Vec::new(),
        saved_password_lost: false,
    }
}

// The keys and agent of whoever runs the tests stay out of them.
fn ssh(known_hosts: &Path) -> ela_ssh::SshProtocol {
    ela_ssh::SshProtocol::with_known_hosts(known_hosts.to_owned())
        .finding_keys(ela_ssh::KeySources::default())
}

fn manager(known_hosts: &Path) -> Arc<SessionManager> {
    let mut registry = ProtocolRegistry::new();
    registry.register(ssh(known_hosts));
    SessionManager::new(registry)
}

fn known_hosts() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("known_hosts");
    (dir, path)
}

fn open(mgr: &Arc<SessionManager>) -> (Arc<Recorder>, SessionId) {
    let rec = Arc::new(Recorder::default());
    let id = mgr.open("ssh", config("ela"), rec.clone()).unwrap();
    (rec, id)
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..100 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting until {what}");
}

async fn answer(mgr: &SessionManager, rec: &Recorder, id: SessionId, trust: bool) {
    until("Ela asks", || rec.asked().is_some()).await;
    mgr.answer(id, rec.prompt_id(), Answer::Trust(trust))
        .unwrap();
}

async fn wait_for(rec: &Recorder, mgr: &SessionManager, id: SessionId, needle: &str) -> bool {
    for _ in 0..100 {
        mgr.ack(id);
        if String::from_utf8_lossy(&rec.output.lock().unwrap()).contains(needle) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_saved_connection_signs_in_with_the_password_from_the_vault() {
    let (dir, path) = known_hosts();
    let mut vault = ela_vault::Vault::new(
        dir.path().join("vault.bin"),
        ela_vault::DEFAULT_IDLE_TIMEOUT,
    );
    vault
        .create(&SecretString::from("fixture master password"))
        .unwrap();
    let reference = vault.store(SecretString::from("ela")).unwrap();

    let mut connection = ela_core::Connection::new("fixture", "ssh", "127.0.0.1");
    connection.settings.port = Some(2222);
    connection.settings.username = Some("ela".into());
    connection.settings.credential = Some(reference);
    let resolved = ela_core::resolve(&connection, &Default::default(), Some(22)).unwrap();

    let password = vault.read(resolved.credential.unwrap()).unwrap();
    let size = Size {
        width: 100,
        height: 30,
    };
    let mgr = manager(&path);
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open(
            "ssh",
            resolved.into_config(Some(password), size),
            rec.clone(),
        )
        .unwrap();
    answer(&mgr, &rec, id, true).await;
    until("connected", || rec.saw_connected()).await;
    assert!(rec.closed().is_none());
    mgr.close(id);
}

fn generate_key(dir: &Path, name: &str, passphrase: &str) -> (PathBuf, String) {
    let key = dir.join(name);
    let generated = std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", passphrase, "-f"])
        .arg(&key)
        .status()
        .unwrap();
    assert!(generated.success());
    let public = std::fs::read_to_string(key.with_extension("pub")).unwrap();
    (key, public)
}

async fn authorize(mgr: &Arc<SessionManager>, public: &str) {
    let (rec, id) = open(mgr);
    answer(mgr, &rec, id, true).await;
    until("connected", || rec.saw_connected()).await;
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let install = format!(
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && echo '{}' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && echo installed-$((40+2))\r",
        public.trim()
    );
    mgr.send(id, SessionCommand::Text { data: install })
        .unwrap();
    assert!(wait_for(&rec, mgr, id, "installed-42").await);
    mgr.close(id);
}

fn with_identity_file(key: &Path) -> ResolvedConfig {
    let mut with_key = config("unused");
    with_key.password = None;
    with_key
        .options
        .insert("identity_file".into(), key.display().to_string().into());
    with_key
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn an_identity_file_signs_in_without_a_password() {
    let (dir, path) = known_hosts();
    let (key, public) = generate_key(dir.path(), "id_ed25519", "");
    let mgr = manager(&path);
    authorize(&mgr, &public).await;

    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", with_identity_file(&key), rec.clone())
        .unwrap();
    until("connected", || rec.saw_connected()).await;
    assert!(rec.closed().is_none());
    mgr.close(id);
}

#[derive(Default)]
struct Remembered(Mutex<Option<String>>);

impl ela_ssh::Passphrases for Remembered {
    fn saved(&self, _: &ela_ssh::SavedKey) -> Option<SecretString> {
        self.0.lock().unwrap().clone().map(SecretString::from)
    }
    fn remember(&self, _: &ela_ssh::SavedKey, passphrase: SecretString) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(passphrase.expose_secret().to_owned());
        Ok(())
    }
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn an_encrypted_identity_file_signs_in_with_its_passphrase_and_then_from_the_vault() {
    let (dir, path) = known_hosts();
    let (key, public) = generate_key(dir.path(), "id_locked", "fixture passphrase");
    authorize(&manager(&path), &public).await;

    let remembered = Arc::new(Remembered::default());
    let mut registry = ProtocolRegistry::new();
    registry.register(ssh(&path).remembering_passphrases(remembered.clone()));
    let mgr = SessionManager::new(registry);

    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", with_identity_file(&key), rec.clone())
        .unwrap();
    until("Ela asks for the passphrase", || {
        rec.passphrase_prompts() == 1
    })
    .await;
    let typed = PassphraseAnswer::Typed {
        passphrase: SecretString::from("fixture passphrase"),
        remember: true,
    };
    mgr.answer(id, rec.prompt_id(), Answer::Passphrase(typed))
        .unwrap();
    until("connected", || rec.saw_connected()).await;
    assert!(rec.closed().is_none());
    assert_eq!(
        remembered.0.lock().unwrap().as_deref(),
        Some("fixture passphrase")
    );
    mgr.close(id);

    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", with_identity_file(&key), rec.clone())
        .unwrap();
    until("connected", || rec.saw_connected()).await;
    assert_eq!(rec.passphrase_prompts(), 0);
    mgr.close(id);
}

fn home_with_key(passphrase: &str) -> (tempfile::TempDir, PathBuf, String) {
    let home = tempfile::tempdir().unwrap();
    let ssh = home.path().join(".ssh");
    std::fs::create_dir(&ssh).unwrap();
    let (key, public) = generate_key(&ssh, "id_ed25519", passphrase);
    (home, key, public)
}

struct TestAgent {
    process: std::process::Child,
    socket: PathBuf,
    _dir: tempfile::TempDir,
}

impl TestAgent {
    fn holding(key: &Path) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("agent.sock");
        let process = std::process::Command::new("ssh-agent")
            .arg("-D")
            .arg("-a")
            .arg(&socket)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        for _ in 0..50 {
            if socket.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let added = std::process::Command::new("ssh-add")
            .arg("-q")
            .arg(key)
            .env("SSH_AUTH_SOCK", &socket)
            .status()
            .unwrap();
        assert!(added.success());
        Self {
            process,
            socket,
            _dir: dir,
        }
    }
}

impl Drop for TestAgent {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

fn finding_keys(
    known_hosts: &Path,
    sources: ela_ssh::KeySources,
    passwords: Option<Arc<KeptPasswords>>,
) -> Arc<SessionManager> {
    let mut registry = ProtocolRegistry::new();
    registry.register(ssh(known_hosts).finding_keys(sources));
    let passwords = passwords.map(|kept| kept as Arc<dyn ela_session::SavedPasswords>);
    SessionManager::keeping(registry, passwords)
}

/// The fixture's sshd sets `ExposeAuthInfo`, so the shell can tell which method signed in.
async fn signed_in_with(mgr: &SessionManager, rec: &Recorder, id: SessionId) -> String {
    let ask = "echo method-$(cut -d' ' -f1 \"$SSH_USER_AUTH\")-done\r";
    mgr.send(id, SessionCommand::Text { data: ask.into() })
        .unwrap();
    for method in ["publickey", "password", "keyboard-interactive"] {
        if wait_for(rec, mgr, id, &format!("method-{method}-done")).await {
            return method.into();
        }
    }
    panic!(
        "no sign-in method in {:?}",
        String::from_utf8_lossy(&rec.output.lock().unwrap())
    );
}

fn without_password(connection: Option<uuid::Uuid>) -> ResolvedConfig {
    let mut target = config("unused");
    target.password = None;
    target.connection = connection;
    target
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_default_key_file_signs_in_with_nothing_asked() {
    let (_dir, path) = known_hosts();
    let (home, _key, public) = home_with_key("");
    authorize(&manager(&path), &public).await;

    let sources = ela_ssh::KeySources::new(Some(home.path().into()), Vec::new());
    let mgr = finding_keys(&path, sources, None);
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", without_password(None), rec.clone())
        .unwrap();
    until("connected", || rec.saw_connected()).await;
    assert_eq!(signed_in_with(&mgr, &rec, id).await, "publickey");
    assert!(rec.password_prompts().is_empty());
    assert_eq!(rec.passphrase_prompts(), 0);
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_saved_connection_no_key_signs_in_to_asks_on_the_tab_and_remembers() {
    let (_dir, path) = known_hosts();
    let (home, _refused, _) = home_with_key("");
    let kept = Arc::new(KeptPasswords::default());
    let sources = ela_ssh::KeySources::new(Some(home.path().into()), Vec::new());
    let mgr = finding_keys(&path, sources, Some(kept.clone()));
    let connection = uuid::Uuid::new_v4();
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", without_password(Some(connection)), rec.clone())
        .unwrap();
    answer_passwords(&mgr, &rec, id, &["wrong", "ela"]).await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    assert_eq!(
        rec.password_prompts(),
        [(None, None), (None, Some(PasswordNote::Wrong))]
    );
    assert_eq!(rec.offered_remembering(), [true, true]);
    assert_eq!(*kept.0.lock().unwrap(), [(connection, "ela".to_owned())]);
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_saved_password_goes_before_ssh_agent_and_default_key_files() {
    let (_dir, path) = known_hosts();
    let (home, key, public) = home_with_key("");
    authorize(&manager(&path), &public).await;
    let agent = TestAgent::holding(&key);

    let sources = ela_ssh::KeySources::new(Some(home.path().into()), vec![agent.socket.clone()]);
    let mgr = finding_keys(&path, sources, None);
    let mut saved = config("ela");
    saved.connection = Some(uuid::Uuid::new_v4());
    let rec = Arc::new(Recorder::default());
    let id = mgr.open("ssh", saved, rec.clone()).unwrap();
    until("connected", || rec.saw_connected()).await;
    assert_eq!(signed_in_with(&mgr, &rec, id).await, "password");
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_named_identity_file_goes_before_a_saved_password() {
    let (dir, path) = known_hosts();
    let (key, public) = generate_key(dir.path(), "id_named", "");
    authorize(&manager(&path), &public).await;

    let mgr = manager(&path);
    let mut saved = with_identity_file(&key);
    saved.password = Some(SecretString::from("ela"));
    let rec = Arc::new(Recorder::default());
    let id = mgr.open("ssh", saved, rec.clone()).unwrap();
    until("connected", || rec.saw_connected()).await;
    assert_eq!(signed_in_with(&mgr, &rec, id).await, "publickey");
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn shell_round_trip() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let (rec, id) = open(&mgr);
    answer(&mgr, &rec, id, true).await;
    until("connected", || rec.saw_connected()).await;

    mgr.send(
        id,
        SessionCommand::Resize {
            width: 120,
            height: 40,
        },
    )
    .ok();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    mgr.send(
        id,
        SessionCommand::Text {
            data: "echo ela-$((40+2)); stty size\r".into(),
        },
    )
    .unwrap();
    // `stty size` prints "rows cols": proves the Resize reached the remote PTY.
    let ok = wait_for(&rec, &mgr, id, "ela-42\r\n40 120").await;
    assert!(
        ok,
        "output: {:?}",
        String::from_utf8_lossy(&rec.output.lock().unwrap())
    );

    let statuses = rec.statuses.lock().unwrap().clone();
    assert!(statuses.iter().any(|s| matches!(s, SessionStatus::Verifying { fingerprint, .. } if fingerprint.starts_with("SHA256:"))));
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_wrong_saved_password_asks_three_times_and_then_fails() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let rec = Arc::new(Recorder::default());
    let id = mgr.open("ssh", config("wrong"), rec.clone()).unwrap();
    answer_passwords(&mgr, &rec, id, &["wrong", "wrong", "wrong"]).await;
    until("closed", || rec.closed().is_some()).await;
    assert!(
        matches!(rec.closed(), Some(Some(SessionError::AuthFailed(_)))),
        "{:?}",
        rec.closed()
    );
    let wrong = Some(PasswordNote::Wrong);
    assert_eq!(
        rec.password_prompts(),
        [
            (None, Some(PasswordNote::SavedWrong)),
            (None, wrong),
            (None, wrong)
        ],
        "a password that comes with the configuration is the Vault's"
    );
    assert_eq!(rec.offered_remembering(), [false, false, false]);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_password_the_vault_lost_says_so_when_the_tab_asks() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let rec = Arc::new(Recorder::default());
    let mut lost = config("unused");
    lost.password = None;
    lost.saved_password_lost = true;
    let id = mgr.open("ssh", lost, rec.clone()).unwrap();
    answer_passwords(&mgr, &rec, id, &["ela"]).await;
    until("connected", || rec.saw_connected()).await;
    assert_eq!(
        rec.password_prompts(),
        [(None, Some(PasswordNote::Missing))]
    );
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn trusting_a_new_host_records_it_and_the_next_connect_asks_nothing() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);

    let (rec, id) = open(&mgr);
    until("Ela asks", || rec.asked().is_some()).await;
    let (fingerprint, known) = rec.asked().unwrap();
    assert!(fingerprint.starts_with("SHA256:"));
    assert_eq!(known, None);
    assert!(!rec.saw_connected(), "must wait for the answer");
    assert!(!path.exists(), "nothing is recorded before the answer");

    mgr.answer(id, rec.prompt_id(), Answer::Trust(true))
        .unwrap();
    until("connected", || rec.saw_connected()).await;
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .trim_start()
            .starts_with(HOST_ENTRY)
    );
    mgr.close(id);

    let (again, id) = open(&mgr);
    until("connected again", || again.saw_connected()).await;
    assert!(again.asked().is_none(), "a known host must not ask");
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn the_connection_waits_past_its_timeout_for_an_answer() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let (rec, id) = open(&mgr);
    until("Ela asks", || rec.asked().is_some()).await;

    // Longer than CONNECT_TIMEOUT.
    tokio::time::sleep(Duration::from_secs(17)).await;
    assert!(
        rec.closed().is_none(),
        "closed while waiting: {:?}",
        rec.closed()
    );

    mgr.answer(id, rec.prompt_id(), Answer::Trust(true))
        .unwrap();
    until("connected", || rec.saw_connected()).await;
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn cancelling_ends_the_session_and_records_nothing() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let (rec, id) = open(&mgr);
    answer(&mgr, &rec, id, false).await;
    until("closed", || rec.closed().is_some()).await;
    assert!(matches!(rec.closed(), Some(None)), "{:?}", rec.closed());
    assert!(!rec.saw_connected());
    assert!(!path.exists());
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_changed_key_shows_both_and_replacing_needs_an_answer() {
    let (_dir, path) = known_hosts();
    let stale = format!(
        "# keep me\nother.example ssh-ed25519 {STALE_KEY}\n{HOST_ENTRY} ssh-ed25519 {STALE_KEY}\n"
    );
    std::fs::write(&path, &stale).unwrap();
    let mgr = manager(&path);

    let (rec, id) = open(&mgr);
    until("Ela asks", || rec.asked().is_some()).await;
    let (fingerprint, known) = rec.asked().unwrap();
    let known = known.expect("the saved key is shown");
    assert!(known.starts_with(STALE_FINGERPRINT));
    assert_ne!(known, fingerprint);
    assert!(!rec.saw_connected());
    mgr.answer(id, rec.prompt_id(), Answer::Trust(false))
        .unwrap();
    until("closed", || rec.closed().is_some()).await;
    assert!(
        matches!(rec.closed(), Some(Some(SessionError::HostKeyChanged(_)))),
        "{:?}",
        rec.closed()
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), stale);

    let (rec, id) = open(&mgr);
    answer(&mgr, &rec, id, true).await;
    until("connected", || rec.saw_connected()).await;
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.starts_with(&format!(
            "# keep me\nother.example ssh-ed25519 {STALE_KEY}\n"
        )),
        "{text}"
    );
    assert!(
        !text.contains(&format!("{HOST_ENTRY} ssh-ed25519 {STALE_KEY}")),
        "{text}"
    );
    assert_eq!(
        std::fs::read_to_string(path.with_extension("old")).unwrap(),
        stale
    );
    mgr.close(id);

    let (again, id) = open(&mgr);
    until("connected again", || again.saw_connected()).await;
    assert!(again.asked().is_none());
    mgr.close(id);
}

struct FilesFixture {
    _known: tempfile::TempDir,
    local: tempfile::TempDir,
    mgr: Arc<SessionManager>,
    session: SessionId,
    rec: Arc<Recorder>,
    files: Arc<dyn RemoteFiles>,
    folder: String,
}

impl FilesFixture {
    async fn open() -> Self {
        let (known, path) = known_hosts();
        let mgr = manager(&path);
        let (rec, session) = open(&mgr);
        answer(&mgr, &rec, session, true).await;
        until("connected", || rec.saw_connected()).await;
        let files = mgr.files(session).expect("SSH Sessions offer files");
        let home = files.list(None).await.unwrap().path;
        let folder = remote_join(&home, &format!("ela-test-{}", uuid_like()));
        let fixture = Self {
            _known: known,
            local: tempfile::tempdir().unwrap(),
            mgr,
            session,
            rec,
            files,
            folder,
        };
        fixture
            .shell(&format!("mkdir -p '{}'", fixture.folder))
            .await;
        fixture
    }

    async fn shell(&self, command: &str) {
        let marker = format!("done-{}", uuid_like());
        let (left, right) = marker.split_at(4);
        self.mgr
            .send(
                self.session,
                SessionCommand::Text {
                    data: format!("{command}; echo {left}''{right}\r"),
                },
            )
            .unwrap();
        assert!(wait_for(&self.rec, &self.mgr, self.session, &marker).await);
    }

    async fn finish(self) {
        self.shell(&format!("rm -rf '{}'", self.folder)).await;
        self.mgr.close(self.session);
    }

    fn local_file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.local.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn remote(&self, name: &str) -> String {
        remote_join(&self.folder, name)
    }

    async fn names(&self) -> Vec<String> {
        let listing = self.files.list(Some(self.folder.clone())).await.unwrap();
        listing.entries.into_iter().map(|e| e.name).collect()
    }
}

fn uuid_like() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_string()
}

fn quiet() -> Transfer {
    Transfer {
        cancel: CancellationToken::new(),
        progress: Box::new(|_, _| {}),
    }
}

fn varied_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 251) as u8).collect()
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_file_survives_upload_and_download_byte_for_byte() {
    let f = FilesFixture::open().await;
    let bytes = varied_bytes(3 * 1024 * 1024 + 17);
    let local = f.local_file("up.bin", &bytes);
    let remote = f.remote("up.bin");

    let seen = Arc::new(Mutex::new(Vec::new()));
    let progress = seen.clone();
    let transfer = Transfer {
        cancel: CancellationToken::new(),
        progress: Box::new(move |done, total| progress.lock().unwrap().push((done, total))),
    };
    f.files.upload(&local, &remote, &transfer).await.unwrap();
    assert_eq!(f.names().await, ["up.bin"], "no part file is left behind");

    let reports = seen.lock().unwrap().clone();
    let total = bytes.len() as u64;
    assert_eq!(reports.first(), Some(&(0, total)));
    assert_eq!(reports.last(), Some(&(total, total)));
    assert!(reports.windows(2).all(|pair| pair[0].0 <= pair[1].0));

    let back = f.local.path().join("back.bin");
    f.files.download(&remote, &back, &quiet()).await.unwrap();
    assert_eq!(std::fs::read(&back).unwrap(), bytes);
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn the_listing_sorts_folders_first_and_follows_links() {
    let f = FilesFixture::open().await;
    f.shell(&format!(
        "cd '{0}' && mkdir sub && echo hi > b.txt && echo yo > A.txt && ln -s sub to-sub && ln -s b.txt to-b && ln -s nowhere broken; cd",
        f.folder
    ))
    .await;
    let listing = f.files.list(Some(f.folder.clone())).await.unwrap();
    assert_eq!(listing.path, f.folder);
    let summary: Vec<(&str, RemoteKind, bool)> = listing
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.kind, e.link))
        .collect();
    assert_eq!(
        summary,
        [
            ("sub", RemoteKind::Folder, false),
            ("to-sub", RemoteKind::Folder, true),
            ("A.txt", RemoteKind::File, false),
            ("b.txt", RemoteKind::File, false),
            ("broken", RemoteKind::Other, true),
            ("to-b", RemoteKind::File, true),
        ]
    );
    let text = listing.entries.iter().find(|e| e.name == "b.txt").unwrap();
    assert_eq!(text.size, 3);
    assert!(text.modified.is_some());

    let inner = f.files.list(Some(f.remote("to-sub"))).await.unwrap();
    assert!(inner.entries.is_empty());
    assert!(inner.parent.is_some());
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_cancelled_upload_leaves_the_old_file_and_no_part() {
    let f = FilesFixture::open().await;
    let remote = f.remote("kept.txt");
    let old = f.local_file("old.txt", b"the original");
    f.files.upload(&old, &remote, &quiet()).await.unwrap();

    let big = f.local_file("big.bin", &varied_bytes(8 * 1024 * 1024));
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let transfer = Transfer {
        cancel,
        progress: Box::new(move |_, _| trigger.cancel()),
    };
    assert_eq!(
        f.files.upload(&big, &remote, &transfer).await,
        Err(FileError::Cancelled)
    );
    assert_eq!(f.names().await, ["kept.txt"]);

    let back = f.local.path().join("kept.txt");
    f.files.download(&remote, &back, &quiet()).await.unwrap();
    assert_eq!(std::fs::read(&back).unwrap(), b"the original");
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_cancelled_download_leaves_the_local_folder_as_it_was() {
    let f = FilesFixture::open().await;
    let remote = f.remote("big.bin");
    let big = f.local_file("big.bin", &varied_bytes(8 * 1024 * 1024));
    f.files.upload(&big, &remote, &quiet()).await.unwrap();

    let target = f.local_file("target.bin", b"mine");
    let before = std::fs::read_dir(f.local.path()).unwrap().count();
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let transfer = Transfer {
        cancel,
        progress: Box::new(move |_, _| trigger.cancel()),
    };
    assert_eq!(
        f.files.download(&remote, &target, &transfer).await,
        Err(FileError::Cancelled)
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"mine");
    assert_eq!(std::fs::read_dir(f.local.path()).unwrap().count(), before);
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn replacing_a_file_keeps_its_permissions() {
    let f = FilesFixture::open().await;
    let remote = f.remote("run.sh");
    let first = f.local_file("first.sh", b"one");
    f.files.upload(&first, &remote, &quiet()).await.unwrap();
    f.shell(&format!("chmod 750 '{remote}'")).await;
    assert!(f.files.exists(&remote).await.unwrap());

    let second = f.local_file("second.sh", b"two");
    f.files.upload(&second, &remote, &quiet()).await.unwrap();

    let back = f.local.path().join("run.sh");
    f.files.download(&remote, &back, &quiet()).await.unwrap();
    assert_eq!(std::fs::read(&back).unwrap(), b"two");
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&back).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o640, "750 on the server, and never executable here");
    f.shell(&format!(
        "test \"$(stat -c %a '{remote}')\" = 750 && echo mode-is-''kept"
    ))
    .await;
    assert!(wait_for(&f.rec, &f.mgr, f.session, "mode-is-kept").await);
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn errors_say_what_is_missing_or_forbidden() {
    let f = FilesFixture::open().await;
    let missing = f.remote("nope");
    assert_eq!(
        f.files.list(Some(missing.clone())).await,
        Err(FileError::NotFound(missing.clone()))
    );
    let back = f.local.path().join("nope");
    assert_eq!(
        f.files.download(&missing, &back, &quiet()).await,
        Err(FileError::NotFound(missing.clone()))
    );
    assert!(!back.exists());
    assert!(matches!(
        f.files.list(Some("/root".into())).await,
        Err(FileError::Denied(_))
    ));
    assert!(!f.files.exists(&missing).await.unwrap());
    f.finish().await;
}

fn local_tree(f: &FilesFixture) -> PathBuf {
    let root = f.local.path().join("site");
    std::fs::create_dir_all(root.join("deep/er")).unwrap();
    std::fs::create_dir(root.join("empty")).unwrap();
    std::fs::write(root.join("index.html"), varied_bytes(200_000)).unwrap();
    std::fs::write(root.join("deep/er/data.bin"), varied_bytes(1234)).unwrap();
    root
}

fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_folder_survives_upload_and_download() {
    let f = FilesFixture::open().await;
    let root = local_tree(&f);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let transfer = Transfer {
        cancel: CancellationToken::new(),
        progress: Box::new(move |done, total| log.lock().unwrap().push((done, total))),
    };

    let remote = f.remote("site");
    let up = f.files.upload_folder(&root, &remote, &transfer).await;
    assert_eq!(
        up,
        Ok(FolderOutcome {
            files: 2,
            ..Default::default()
        })
    );
    {
        let seen = seen.lock().unwrap();
        assert_eq!(*seen.last().unwrap(), (201_234, 201_234));
        assert!(seen.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    }

    let back = f.local.path().join("back");
    let down = f.files.download_folder(&remote, &back, &quiet()).await;
    assert_eq!(down.unwrap().files, 2);
    assert_eq!(
        std::fs::read(back.join("index.html")).unwrap(),
        varied_bytes(200_000)
    );
    assert_eq!(
        std::fs::read(back.join("deep/er/data.bin")).unwrap(),
        varied_bytes(1234)
    );
    assert!(back.join("empty").is_dir());
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn links_in_a_folder_are_left_out_both_ways() {
    let f = FilesFixture::open().await;
    let root = local_tree(&f);
    let secret = f.local_file("outside.txt", b"outside the folder");
    std::os::unix::fs::symlink(&secret, root.join("to-outside")).unwrap();
    std::os::unix::fs::symlink(&root, root.join("deep/loop")).unwrap();

    let remote = f.remote("site");
    let up = f
        .files
        .upload_folder(&root, &remote, &quiet())
        .await
        .unwrap();
    assert_eq!((up.files, up.left_out), (2, 2));
    assert!(
        !f.files
            .exists(&remote_join(&remote, "to-outside"))
            .await
            .unwrap()
    );

    f.shell(&format!(
        "ln -s /etc/passwd '{remote}/passwd'; ln -s '{remote}' '{remote}/deep/loop'"
    ))
    .await;
    let back = f.local.path().join("back");
    let down = f
        .files
        .download_folder(&remote, &back, &quiet())
        .await
        .unwrap();
    assert_eq!((down.files, down.left_out), (2, 2));
    assert!(std::fs::symlink_metadata(back.join("passwd")).is_err());
    assert!(std::fs::symlink_metadata(back.join("deep/loop")).is_err());
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_folder_merges_and_private_things_stay_private() {
    let f = FilesFixture::open().await;
    let root = local_tree(&f);
    set_mode(&root.join("deep"), 0o700);
    set_mode(&root.join("index.html"), 0o600);
    std::fs::write(root.join("run.sh"), b"#!/bin/sh").unwrap();
    set_mode(&root.join("run.sh"), 0o755);

    let remote = f.remote("site");
    f.shell(&format!(
        "mkdir '{remote}'; echo keep > '{remote}/already-here'; echo old > '{remote}/run.sh'"
    ))
    .await;
    let up = f
        .files
        .upload_folder(&root, &remote, &quiet())
        .await
        .unwrap();
    assert_eq!(up.files, 3);

    f.shell(&format!(
        "test \"$(stat -c %a '{remote}/deep' '{remote}/index.html' | xargs)\" = '700 600' \
         && test \"$(cat '{remote}/already-here')\" = keep \
         && test \"$(cat '{remote}/run.sh')\" = '#!/bin/sh' \
         && echo merged-''well"
    ))
    .await;
    assert!(wait_for(&f.rec, &f.mgr, f.session, "merged-well").await);

    let back = f.local.path().join("back");
    f.files
        .download_folder(&remote, &back, &quiet())
        .await
        .unwrap();
    assert_eq!(mode_of(&back.join("deep")), 0o700);
    assert_eq!(mode_of(&back.join("index.html")), 0o600);
    assert_eq!(
        mode_of(&back.join("run.sh")) & 0o111,
        0,
        "never executable here"
    );
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_file_that_fails_does_not_stop_the_folder() {
    let f = FilesFixture::open().await;
    let remote = f.remote("site");
    f.shell(&format!(
        "mkdir '{remote}'; echo a > '{remote}/a'; echo b > '{remote}/b'; chmod 000 '{remote}/a'"
    ))
    .await;
    let back = f.local.path().join("back");
    let down = f
        .files
        .download_folder(&remote, &back, &quiet())
        .await
        .unwrap();
    assert_eq!(down.files, 1);
    assert_eq!(down.failed.len(), 1);
    assert!(down.failed[0].contains("/a"), "{:?}", down.failed);
    assert!(back.join("b").exists());
    assert!(!back.join("a").exists());
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn cancelling_a_folder_stops_it_and_leaves_no_part() {
    let f = FilesFixture::open().await;
    let root = local_tree(&f);
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let transfer = Transfer {
        cancel,
        progress: Box::new(move |done, _| {
            if done > 0 {
                stop.cancel();
            }
        }),
    };
    let remote = f.remote("site");
    let up = f.files.upload_folder(&root, &remote, &transfer).await;
    assert_eq!(up, Err(FileError::Cancelled));
    f.shell(&format!(
        "test -z \"$(find '{remote}' -name '*.ela-part')\" && echo no-''parts"
    ))
    .await;
    assert!(wait_for(&f.rec, &f.mgr, f.session, "no-parts").await);
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn renaming_never_overwrites() {
    let f = FilesFixture::open().await;
    let (a, b) = (f.remote("a.txt"), f.remote("b.txt"));
    f.shell(&format!(
        "echo a > '{a}'; echo b > '{b}'; ln -s /nowhere '{}'",
        f.remote("dangling")
    ))
    .await;

    assert_eq!(
        f.files.rename(&a, "b.txt").await,
        Err(FileError::Exists(b.clone()))
    );
    assert_eq!(
        f.files.rename(&a, "dangling").await,
        Err(FileError::Exists(f.remote("dangling")))
    );
    assert_eq!(
        f.files.rename(&a, "../out.txt").await,
        Err(FileError::BadName("../out.txt".into()))
    );
    assert_eq!(f.files.rename(&a, "a.txt").await, Ok(()));
    assert_eq!(f.files.rename(&a, "c.txt").await, Ok(()));
    assert_eq!(f.names().await, ["b.txt", "c.txt", "dangling"]);
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_new_folder_appears_once() {
    let f = FilesFixture::open().await;
    assert_eq!(f.files.make_folder(&f.folder, "fresh").await, Ok(()));
    assert_eq!(f.names().await, ["fresh"]);
    assert_eq!(
        f.files.make_folder(&f.folder, "fresh").await,
        Err(FileError::Exists(f.remote("fresh")))
    );
    assert_eq!(
        f.files.make_folder(&f.folder, "a/b").await,
        Err(FileError::BadName("a/b".into()))
    );
    assert!(matches!(
        f.files.make_folder("/root", "nope").await,
        Err(FileError::Denied(_))
    ));
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn deleting_takes_a_whole_folder_but_never_what_a_link_points_to() {
    let f = FilesFixture::open().await;
    let (kept, doomed, top_link) = (f.remote("kept"), f.remote("doomed"), f.remote("top-link"));
    f.shell(&format!(
        "mkdir -p '{kept}' '{doomed}/deep/er'; echo safe > '{kept}/safe'; \
         echo x > '{doomed}/deep/er/x'; ln -s '{kept}' '{doomed}/deep/link'; \
         ln -s '{kept}' '{top_link}'; echo y > '{}'",
        f.remote("single")
    ))
    .await;

    assert_eq!(f.files.delete(&f.remote("single")).await, Ok(()));
    assert_eq!(f.files.delete(&top_link).await, Ok(()));
    assert_eq!(f.files.delete(&doomed).await, Ok(()));
    assert_eq!(f.names().await, ["kept"]);
    assert!(f.files.exists(&remote_join(&kept, "safe")).await.unwrap());

    assert_eq!(
        f.files.delete(&f.remote("nope")).await,
        Err(FileError::NotFound(f.remote("nope")))
    );
    assert_eq!(
        f.files.delete("/").await,
        Err(FileError::Denied("/".into()))
    );
    f.finish().await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_folder_with_an_unreadable_part_is_not_deleted_at_all() {
    let f = FilesFixture::open().await;
    let doomed = f.remote("doomed");
    f.shell(&format!(
        "mkdir -p '{doomed}/closed'; echo a > '{doomed}/a'; chmod 000 '{doomed}/closed'"
    ))
    .await;
    let deleted = f.files.delete(&doomed).await;
    assert!(deleted.is_err(), "{deleted:?}");
    assert!(f.files.exists(&remote_join(&doomed, "a")).await.unwrap());
    f.shell(&format!("chmod 700 '{doomed}/closed'")).await;
    f.finish().await;
}

fn leg(host: &str, password: &str) -> ResolvedConfig {
    let mut leg = config(password);
    leg.host = host.into();
    leg
}

// The fixture's Jump Host is reached on the published port, the rest by name inside Docker.
fn through(route: &[(&str, &str)], target: ResolvedConfig) -> ResolvedConfig {
    let mut target = target;
    let names: Vec<&str> = route.iter().map(|(name, _)| *name).collect();
    target
        .options
        .insert("proxy_jump".into(), names.join(",").into());
    target.jump_hosts = route
        .iter()
        .map(|(name, password)| {
            let host = if *name == "gate" { "127.0.0.1" } else { name };
            JumpHost {
                name: (*name).into(),
                config: leg(host, password),
            }
        })
        .collect();
    target
}

async fn trust_every_prompt(mgr: &SessionManager, rec: &Recorder, id: SessionId) {
    let mut answered = 0;
    for _ in 0..200 {
        if rec.saw_connected() || rec.closed().is_some() {
            return;
        }
        let asked = rec.prompts().len();
        if asked > answered {
            mgr.answer(id, rec.prompt_id(), Answer::Trust(true))
                .unwrap();
            answered = asked;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out answering prompts");
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_chain_of_jump_hosts_signs_in_and_checks_every_hop() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let route = [("gate", "ela"), ("ssh-inner", "ela")];

    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", through(&route, leg("ssh", "ela")), rec.clone())
        .unwrap();
    trust_every_prompt(&mgr, &rec, id).await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    assert_eq!(
        rec.prompts(),
        [Some("gate".into()), Some("ssh-inner".into()), None]
    );
    let saved = std::fs::read_to_string(&path).unwrap();
    for entry in ["[127.0.0.1]:2222", "[ssh-inner]:2222", "[ssh]:2222"] {
        assert!(
            saved.lines().any(|line| line.starts_with(entry)),
            "{entry} in {saved}"
        );
    }
    mgr.send(
        id,
        SessionCommand::Text {
            data: "echo jumped-$((40+2))\r".into(),
        },
    )
    .unwrap();
    assert!(wait_for(&rec, &mgr, id, "jumped-42").await);
    mgr.close(id);

    let again = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", through(&route, leg("ssh", "ela")), again.clone())
        .unwrap();
    until("connected again", || again.saw_connected()).await;
    assert!(again.prompts().is_empty(), "known hops must not ask");
    mgr.close(id);
}

fn unsaved(route: &[&str], target: ResolvedConfig) -> ResolvedConfig {
    let route: Vec<(&str, &str)> = route.iter().map(|name| (*name, "")).collect();
    let mut target = through(&route, target);
    for hop in &mut target.jump_hosts {
        hop.config.password = None;
    }
    target
}

async fn answer_passwords(mgr: &SessionManager, rec: &Recorder, id: SessionId, typed: &[&str]) {
    let (mut trusted, mut typed) = (0, typed.iter());
    for _ in 0..300 {
        if rec.saw_connected() || rec.closed().is_some() {
            return;
        }
        let asked = rec.prompts().len();
        if asked > trusted {
            mgr.answer(id, rec.prompt_id(), Answer::Trust(true))
                .unwrap();
            trusted = asked;
        }
        let waiting = matches!(
            rec.statuses.lock().unwrap().last(),
            Some(SessionStatus::Asking {
                prompt: Prompt::Password { .. },
                ..
            })
        );
        if waiting && let Some(password) = typed.next() {
            let answer = PasswordAnswer {
                password: SecretString::from(*password),
                remember: true,
            };
            mgr.answer(id, rec.prompt_id(), Answer::Password(answer))
                .unwrap();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out answering prompts");
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_jump_host_without_a_password_asks_for_it_and_signs_in() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open(
            "ssh",
            unsaved(&["gate"], leg("ssh-inner", "ela")),
            rec.clone(),
        )
        .unwrap();
    answer_passwords(&mgr, &rec, id, &["wrong", "ela"]).await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    assert_eq!(
        rec.password_prompts(),
        [
            (Some("gate".into()), None),
            (Some("gate".into()), Some(PasswordNote::Wrong))
        ]
    );
    assert_eq!(
        rec.offered_remembering(),
        [false, false],
        "a hop that isn't a saved Connection has nowhere to keep it"
    );
    mgr.close(id);
}

#[derive(Default)]
struct KeptPasswords(Mutex<Vec<(uuid::Uuid, String)>>);

impl ela_session::SavedPasswords for KeptPasswords {
    fn remember(&self, connection: uuid::Uuid, password: SecretString) -> Result<(), String> {
        let password = password.expose_secret().to_owned();
        self.0.lock().unwrap().push((connection, password));
        Ok(())
    }
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_saved_jump_host_keeps_only_the_password_that_signed_in() {
    let (_dir, path) = known_hosts();
    let kept = Arc::new(KeptPasswords::default());
    let mut registry = ProtocolRegistry::new();
    registry.register(ssh(&path));
    let mgr = SessionManager::keeping(registry, Some(kept.clone()));
    let gate = uuid::Uuid::new_v4();
    let mut target = unsaved(&["gate"], leg("ssh-inner", "ela"));
    target.jump_hosts[0].config.connection = Some(gate);
    let rec = Arc::new(Recorder::default());
    let id = mgr.open("ssh", target, rec.clone()).unwrap();
    answer_passwords(&mgr, &rec, id, &["wrong", "ela"]).await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    assert_eq!(rec.offered_remembering(), [true, true]);
    assert_eq!(*kept.0.lock().unwrap(), [(gate, "ela".to_owned())]);
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_typed_jump_host_password_goes_to_that_jump_host_only() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let mut target = leg("ssh-inner", "unused");
    target.password = None;
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", unsaved(&["gate"], target), rec.clone())
        .unwrap();
    answer_passwords(&mgr, &rec, id, &["ela", "wrong", "wrong", "wrong"]).await;
    until("closed", || rec.closed().is_some()).await;
    assert!(
        matches!(rec.closed(), Some(Some(SessionError::AuthFailed(_)))),
        "the target must fail on its own, not with gate's password: {:?}",
        rec.closed()
    );
    let wrong = Some(PasswordNote::Wrong);
    assert_eq!(
        rec.password_prompts(),
        [
            (Some("gate".into()), None),
            (None, None),
            (None, wrong),
            (None, wrong)
        ]
    );
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_jump_host_that_rejects_the_sign_in_is_named() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open(
            "ssh",
            through(&[("gate", "wrong")], leg("ssh-inner", "ela")),
            rec.clone(),
        )
        .unwrap();
    answer_passwords(&mgr, &rec, id, &["wrong", "wrong", "wrong"]).await;
    until("closed", || rec.closed().is_some()).await;
    assert!(
        matches!(
            rec.closed(),
            Some(Some(SessionError::JumpHost { ref jump_host, ref error, .. }))
                if jump_host == "gate" && matches!(**error, SessionError::AuthFailed(_))
        ),
        "{:?}",
        rec.closed()
    );
    let wrong = Some(PasswordNote::Wrong);
    assert_eq!(
        rec.password_prompts(),
        [
            (Some("gate".into()), Some(PasswordNote::SavedWrong)),
            (Some("gate".into()), wrong),
            (Some("gate".into()), wrong)
        ]
    );
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_target_the_jump_host_cannot_reach_says_so() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let mut target = leg("ssh-inner", "ela");
    target.port = 1;
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", through(&[("gate", "ela")], target), rec.clone())
        .unwrap();
    trust_every_prompt(&mgr, &rec, id).await;
    until("closed", || rec.closed().is_some()).await;
    assert!(
        matches!(
            rec.closed(),
            Some(Some(SessionError::ForwardFailed { ref jump_host, ref address, failure: ForwardFailure::Unreachable }))
                if jump_host == "gate" && address == "ssh-inner:1"
        ),
        "{:?}",
        rec.closed()
    );
}

// The `ssh-otp` service offers only keyboard-interactive. ela / ela needs the password alone,
// otp / otp a TOTP code from the secret in its Dockerfile as well.
fn keyboard_only(user: &str, password: Option<&str>) -> ResolvedConfig {
    let mut config = config(password.unwrap_or_default());
    config.port = 2223;
    config.username = Some(user.into());
    config.password = password.map(|p| SecretString::from(p.to_owned()));
    config
}

fn totp_now() -> String {
    use hmac::{Hmac, KeyInit, Mac};
    let secret = data_encoding::BASE32_NOPAD
        .decode(b"JBSWY3DPEHPK3PXP")
        .unwrap();
    let step = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 30;
    let mut mac = Hmac::<sha1::Sha1>::new_from_slice(&secret).unwrap();
    mac.update(&step.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[19] & 0x0f);
    let code = u32::from_be_bytes(digest[offset..offset + 4].try_into().unwrap()) & 0x7fff_ffff;
    format!("{:06}", code % 1_000_000)
}

impl Recorder {
    fn questions(&self) -> Vec<Vec<ela_session::Question>> {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .filter_map(|s| match s {
                SessionStatus::Asking {
                    prompt: Prompt::Questions { questions, .. },
                    ..
                } => Some(questions.clone()),
                _ => None,
            })
            .collect()
    }
}

async fn answer_questions(mgr: &SessionManager, rec: &Recorder, id: SessionId, rounds: &[&[&str]]) {
    let (mut trusted, mut answered, mut rounds) = (0, 0, rounds.iter());
    for _ in 0..300 {
        if rec.saw_connected() || rec.closed().is_some() {
            return;
        }
        let asked = rec.prompts().len();
        if asked > trusted {
            mgr.answer(id, rec.prompt_id(), Answer::Trust(true))
                .unwrap();
            trusted = asked;
        }
        let questions = rec.questions().len();
        let waiting = matches!(
            rec.statuses.lock().unwrap().last(),
            Some(SessionStatus::Asking {
                prompt: Prompt::Questions { .. },
                ..
            })
        );
        if waiting && questions > answered {
            answered = questions;
            let answers = rounds.next().expect("more questions than expected");
            let answers = answers.iter().map(|a| SecretString::from(*a)).collect();
            mgr.answer(id, rec.prompt_id(), Answer::Questions(answers))
                .unwrap();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out answering questions");
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_saved_password_signs_in_where_only_keyboard_interactive_is_offered() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", keyboard_only("ela", Some("ela")), rec.clone())
        .unwrap();
    answer_questions(&mgr, &rec, id, &[]).await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    assert!(rec.questions().is_empty(), "{:?}", rec.questions());
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_question_besides_the_password_is_asked_on_the_tab() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", keyboard_only("otp", Some("otp")), rec.clone())
        .unwrap();
    let code = totp_now();
    answer_questions(&mgr, &rec, id, &[&[&code]]).await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    let questions = rec.questions();
    assert_eq!(questions.len(), 1, "{questions:?}");
    assert!(!questions[0][0].echo);
    assert!(
        questions[0][0].text.contains("Verification code"),
        "the password must not be asked again: {questions:?}"
    );
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn without_a_password_every_question_is_asked_and_a_wrong_answer_asks_again() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", keyboard_only("ela", None), rec.clone())
        .unwrap();
    answer_questions(&mgr, &rec, id, &[&["wrong"], &["ela"]]).await;
    until("connected", || {
        rec.saw_connected() || rec.closed().is_some()
    })
    .await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    let questions = rec.questions();
    assert_eq!(questions.len(), 2, "{questions:?}");
    assert!(questions[0][0].text.contains("Password"), "{questions:?}");
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_jump_host_with_only_keyboard_interactive_asks_for_its_password() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let mut target = leg("ssh-inner", "ela");
    target
        .options
        .insert("proxy_jump".into(), "otp-gate".into());
    target.jump_hosts = vec![JumpHost {
        name: "otp-gate".into(),
        config: keyboard_only("ela", None),
    }];
    let rec = Arc::new(Recorder::default());
    let id = mgr.open("ssh", target, rec.clone()).unwrap();
    answer_passwords(&mgr, &rec, id, &["wrong", "ela"]).await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    assert_eq!(
        rec.password_prompts(),
        [
            (Some("otp-gate".into()), None),
            (Some("otp-gate".into()), Some(PasswordNote::Wrong))
        ]
    );
    assert!(rec.questions().is_empty(), "{:?}", rec.questions());
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_mistyped_code_is_asked_again_with_the_saved_password_kept() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", keyboard_only("otp", Some("otp")), rec.clone())
        .unwrap();
    let code = totp_now();
    let mistyped = if code == "000000" { "111111" } else { "000000" };
    answer_questions(&mgr, &rec, id, &[&[mistyped], &[&code]]).await;
    until("connected", || {
        rec.saw_connected() || rec.closed().is_some()
    })
    .await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    let refused: Vec<bool> = rec
        .statuses
        .lock()
        .unwrap()
        .iter()
        .filter_map(|s| match s {
            SessionStatus::Asking {
                prompt: Prompt::Questions { refused, .. },
                ..
            } => Some(*refused),
            _ => None,
        })
        .collect();
    assert_eq!(refused, [false, true]);
    assert!(
        rec.questions()
            .iter()
            .all(|round| round.len() == 1 && round[0].text.contains("Verification code")),
        "the saved password answers every round: {:?}",
        rec.questions()
    );
    mgr.close(id);
}

fn host_key_of_type(algorithm: &str) -> String {
    let scanned = std::process::Command::new("ssh-keyscan")
        .args(["-t", algorithm, "-p", "2222", "127.0.0.1"])
        .output()
        .unwrap();
    let text = String::from_utf8(scanned.stdout).unwrap();
    let line = text.lines().find(|l| !l.starts_with('#')).unwrap();
    line.split_once(' ').unwrap().1.to_owned()
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_host_certificate_from_a_trusted_authority_connects_without_asking() {
    let (_dir, path) = known_hosts();
    let ca = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/ssh/host-ca.pub"),
    )
    .expect("dev/gen-keys.sh makes the test CAs");
    std::fs::write(&path, format!("@cert-authority {HOST_ENTRY} {ca}")).unwrap();
    let mgr = manager(&path);
    let (rec, id) = open(&mgr);
    until("connected", || {
        rec.saw_connected() || rec.closed().is_some()
    })
    .await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    assert!(rec.asked().is_none());
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_host_known_by_another_key_type_is_asked_for_that_type() {
    let (_dir, path) = known_hosts();
    let ecdsa = host_key_of_type("ecdsa");
    std::fs::write(&path, format!("{HOST_ENTRY} {ecdsa}\n")).unwrap();
    let mgr = manager(&path);
    let (rec, id) = open(&mgr);
    until("connected", || {
        rec.saw_connected() || rec.closed().is_some()
    })
    .await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    assert!(rec.asked().is_none());
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_revoked_host_key_is_refused_without_asking() {
    let (_dir, path) = known_hosts();
    let ed25519 = host_key_of_type("ed25519");
    std::fs::write(
        &path,
        format!("{HOST_ENTRY} {ed25519}\n@revoked * {ed25519}\n"),
    )
    .unwrap();
    let mgr = manager(&path);
    let (rec, _id) = open(&mgr);
    until("closed", || rec.closed().is_some()).await;
    assert!(matches!(
        rec.closed(),
        Some(Some(SessionError::HostKeyRevoked(_)))
    ));
    assert!(rec.asked().is_none());
    assert!(!rec.saw_connected());
}

fn certify(key: &Path) {
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("user-ca");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/ssh/user-ca"),
        &ca,
    )
    .expect("dev/gen-keys.sh makes the test CAs");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&ca, std::fs::Permissions::from_mode(0o600)).unwrap();
    let signed = std::process::Command::new("ssh-keygen")
        .args(["-q", "-s"])
        .arg(&ca)
        .args(["-I", "ela-fixture", "-n", "ela"])
        .arg(key.with_extension("pub"))
        .status()
        .unwrap();
    assert!(signed.success());
}

fn certified_key(dir: &Path, name: &str, passphrase: &str) -> PathBuf {
    let (key, _) = generate_key(dir, name, passphrase);
    // `with_extension` would replace a dot in the name, so keys here have none.
    certify(&key);
    assert!(dir.join(format!("{name}-cert.pub")).exists());
    key
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_certificate_beside_an_unauthorized_key_signs_in() {
    let (dir, path) = known_hosts();
    let key = certified_key(dir.path(), "certified", "");
    let mgr = manager(&path);
    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("ssh", with_identity_file(&key), rec.clone())
        .unwrap();
    answer(&mgr, &rec, id, true).await;
    until("connected", || {
        rec.saw_connected() || rec.closed().is_some()
    })
    .await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    assert!(rec.password_prompts().is_empty());
    mgr.send(
        id,
        SessionCommand::Text {
            data: "cut -d' ' -f2 $SSH_USER_AUTH; echo method-$((40+2))\r".into(),
        },
    )
    .unwrap();
    assert!(wait_for(&rec, &mgr, id, "method-42").await);
    let output = String::from_utf8_lossy(&rec.output.lock().unwrap()).into_owned();
    assert!(
        output.contains("ssh-ed25519-cert-v01@openssh.com\r\n"),
        "{output:?}"
    );
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_certificate_the_agent_holds_signs_in() {
    let (dir, path) = known_hosts();
    let key = certified_key(dir.path(), "agentcert", "");
    let agent = TestAgent::holding(&key);
    let sources = ela_ssh::KeySources::new(None, vec![agent.socket.clone()]);
    let mgr = finding_keys(&path, sources, None);
    let mut config = config("unused");
    config.password = None;
    let rec = Arc::new(Recorder::default());
    let id = mgr.open("ssh", config, rec.clone()).unwrap();
    answer(&mgr, &rec, id, true).await;
    until("connected", || {
        rec.saw_connected() || rec.closed().is_some()
    })
    .await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    assert!(rec.password_prompts().is_empty());
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn the_banner_shows_first_and_the_terminal_is_utf_8() {
    let (_dir, path) = known_hosts();
    let mgr = manager(&path);
    let (rec, id) = open(&mgr);
    answer(&mgr, &rec, id, true).await;
    until("connected", || rec.saw_connected()).await;
    mgr.send(
        id,
        SessionCommand::Text {
            data: "stty -a | tr ' ' '\\n' | grep -x -e iutf8 -e -iutf8; echo modes-$((40+2))\r"
                .into(),
        },
    )
    .unwrap();
    assert!(wait_for(&rec, &mgr, id, "modes-42").await);
    let output = String::from_utf8_lossy(&rec.output.lock().unwrap()).into_owned();
    assert!(
        output.starts_with("Ela fixture banner: authorized use only.\r\n"),
        "{output:?}"
    );
    assert!(output.contains("iutf8\r\nmodes-42"), "{output:?}");
    assert!(!output.contains("-iutf8\r\n"), "{output:?}");
    mgr.close(id);
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn identities_only_keeps_to_the_named_key_and_leaves_the_agent_alone() {
    let (dir, path) = known_hosts();
    let in_agent = certified_key(dir.path(), "inagent", "");
    let agent = TestAgent::holding(&in_agent);
    let (named, _) = generate_key(dir.path(), "notauthorized", "");
    let open_with = |only: bool| {
        let sources = ela_ssh::KeySources::new(None, vec![agent.socket.clone()]);
        let mgr = finding_keys(&path, sources, None);
        let mut config = with_identity_file(&named);
        if only {
            config
                .options
                .insert("identities_only".into(), "yes".into());
        }
        let rec = Arc::new(Recorder::default());
        let id = mgr.open("ssh", config, rec.clone()).unwrap();
        (mgr, rec, id)
    };

    let (mgr, rec, id) = open_with(true);
    answer(&mgr, &rec, id, true).await;
    until("asked for a password", || {
        !rec.password_prompts().is_empty()
    })
    .await;
    let tried = rec
        .statuses
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find_map(|s| match s {
            SessionStatus::Asking {
                prompt: Prompt::Password { tried, .. },
                ..
            } => Some(tried.clone()),
            _ => None,
        });
    assert_eq!(tried, Some(vec![ela_session::Tried::Keys]));
    assert!(!rec.saw_connected());
    mgr.close(id);

    let (mgr, rec, id) = open_with(false);
    until("connected", || {
        rec.saw_connected() || rec.closed().is_some()
    })
    .await;
    assert!(rec.saw_connected(), "{:?}", rec.closed());
    mgr.close(id);
}
