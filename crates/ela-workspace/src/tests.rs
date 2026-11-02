use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use ela_core::{Connection, CredentialRef, Recent, ResolvedConfig, Size, Store};
use ela_session::{
    Answer, ExposeSecret, Frontend, FrontendEvent, Prompt, PromptId, Protocol, ProtocolRegistry,
    RendererKind, SavedPasswords, SecretString, Session, SessionError, SessionIo, SessionSink,
    SessionStatus,
};
use ela_vault::{DEFAULT_IDLE_TIMEOUT, Vault};
use serde_json::json;
use tempfile::TempDir;

use crate::{Address, VaultPasswords, Workspace};

const MASTER: &str = "correct horse battery";
const SIZE: Size = Size {
    width: 80,
    height: 24,
};

#[derive(Debug, Clone, PartialEq)]
struct Seen {
    address: (String, u16, Option<String>),
    options: serde_json::Map<String, serde_json::Value>,
    password: Option<String>,
    lost: bool,
    jump_hosts: Vec<(String, Option<String>)>,
}

#[derive(Clone, Default)]
struct Probe(Arc<Mutex<Option<Seen>>>);

#[async_trait]
impl Protocol for Probe {
    fn id(&self) -> &'static str {
        "ssh"
    }
    fn display_name(&self) -> &'static str {
        "Probe"
    }
    fn default_port(&self) -> Option<u16> {
        Some(22)
    }
    fn renderer(&self) -> RendererKind {
        RendererKind::Terminal
    }
    async fn connect(
        &self,
        config: ResolvedConfig,
        _: &SessionSink,
    ) -> Result<Box<dyn Session>, SessionError> {
        let reveal = |p: &Option<SecretString>| p.as_ref().map(|p| p.expose_secret().to_owned());
        *self.0.lock().unwrap() = Some(Seen {
            address: (config.host.clone(), config.port, config.username.clone()),
            options: config.options.clone(),
            password: reveal(&config.password),
            lost: config.saved_password_lost,
            jump_hosts: config
                .jump_hosts
                .iter()
                .map(|hop| (hop.config.host.clone(), reveal(&hop.config.password)))
                .collect(),
        });
        Ok(Box::new(Idle))
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
    fn vault_prompts(&self) -> Vec<(PromptId, bool, Option<String>)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|status| match status {
                SessionStatus::Asking {
                    id,
                    prompt:
                        Prompt::Vault {
                            wrong,
                            for_jump_host,
                        },
                    ..
                } => Some((*id, *wrong, for_jump_host.clone())),
                _ => None,
            })
            .collect()
    }
}

struct Fixture {
    _dir: TempDir,
    store: Arc<Mutex<Store>>,
    vault: Arc<Mutex<Vault>>,
    recent: Arc<Mutex<Recent>>,
    probe: Probe,
    workspace: Workspace,
}

impl Fixture {
    fn new() -> Self {
        Self::with_ssh_config("")
    }

    fn with_ssh_config(text: &str) -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("ssh_config"), text).unwrap();
        let store = Arc::new(Mutex::new(Store::open(dir.path().join("config")).unwrap()));
        let vault = Arc::new(Mutex::new(Vault::new(
            dir.path().join("vault.bin"),
            DEFAULT_IDLE_TIMEOUT,
        )));
        vault
            .lock()
            .unwrap()
            .create(&SecretString::from(MASTER))
            .unwrap();
        let recent = Arc::new(Mutex::new(Recent::at(dir.path().join("recent.toml"))));
        let probe = Probe::default();
        let mut registry = ProtocolRegistry::new();
        registry.register(probe.clone());
        let workspace = Workspace::new(
            Ok(store.clone()),
            Ok(vault.clone()),
            Some(recent.clone()),
            registry,
        )
        .reading_ssh_config(crate::SshConfigFile {
            path: dir.path().join("ssh_config"),
            context: ela_import::Context {
                home: dir.path().to_owned(),
                local_user: "me".into(),
            },
        });
        Self {
            _dir: dir,
            store,
            vault,
            recent,
            probe,
            workspace,
        }
    }

    fn secret(&self, text: &str) -> CredentialRef {
        self.vault
            .lock()
            .unwrap()
            .store(SecretString::from(text))
            .unwrap()
    }

    fn add(&self, name: &str, credential: Option<CredentialRef>, proxy_jump: &str) -> Connection {
        let mut connection = Connection::new(name, "ssh", format!("{name}.example"));
        connection.settings.credential = credential;
        if !proxy_jump.is_empty() {
            connection
                .settings
                .options
                .insert("proxy_jump".into(), json!(proxy_jump));
        }
        self.store
            .lock()
            .unwrap()
            .save_connection(connection.clone())
            .unwrap();
        connection
    }

    fn open(&self, connection: &Connection) -> (Arc<Statuses>, ela_session::SessionId) {
        let statuses = Arc::new(Statuses::default());
        let id = self
            .workspace
            .open_saved(connection.id, SIZE, statuses.clone())
            .unwrap();
        (statuses, id)
    }

    async fn seen(&self) -> Seen {
        for _ in 0..200 {
            if let Some(seen) = self.probe.0.lock().unwrap().clone() {
                return seen;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("the Protocol was never reached");
    }

    async fn asked(&self, statuses: &Statuses, count: usize) -> (PromptId, bool, Option<String>) {
        for _ in 0..200 {
            if let Some(prompt) = statuses.vault_prompts().get(count - 1) {
                return prompt.clone();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("the Vault prompt {count} never came");
    }
}

fn master(text: &str) -> Answer {
    Answer::Vault(Some(SecretString::from(text)))
}

#[tokio::test]
async fn a_saved_connection_gets_its_password_from_an_open_vault() {
    let f = Fixture::new();
    let db = f.add("db", Some(f.secret("db password")), "");
    let (statuses, _) = f.open(&db);
    let seen = f.seen().await;
    assert_eq!(seen.password.as_deref(), Some("db password"));
    assert!(!seen.lost);
    assert!(statuses.vault_prompts().is_empty());
}

#[tokio::test]
async fn a_locked_vault_is_unlocked_on_the_tab_and_a_wrong_master_asks_again() {
    let f = Fixture::new();
    let db = f.add("db", Some(f.secret("db password")), "");
    f.vault.lock().unwrap().lock();
    let (statuses, id) = f.open(&db);

    let (first, wrong, about) = f.asked(&statuses, 1).await;
    assert!(!wrong);
    assert_eq!(about, None);
    let manager = f.workspace.manager();
    manager.answer(id, first, master("not it")).unwrap();
    let (second, wrong, _) = f.asked(&statuses, 2).await;
    assert!(wrong);
    manager.answer(id, second, master(MASTER)).unwrap();

    assert_eq!(f.seen().await.password.as_deref(), Some("db password"));
}

#[tokio::test]
async fn going_on_without_the_vault_asks_once_for_the_whole_route() {
    let f = Fixture::new();
    f.add("gate", Some(f.secret("gate password")), "");
    let db = f.add("db", Some(f.secret("db password")), "gate");
    f.vault.lock().unwrap().lock();
    let (statuses, id) = f.open(&db);

    let (prompt, _, about) = f.asked(&statuses, 1).await;
    assert_eq!(about.as_deref(), Some("gate"));
    f.workspace
        .manager()
        .answer(id, prompt, Answer::Vault(None))
        .unwrap();

    let seen = f.seen().await;
    assert_eq!(seen.password, None);
    assert!(!seen.lost, "skipped, not lost");
    assert_eq!(seen.jump_hosts, [("gate.example".to_owned(), None)]);
    assert_eq!(statuses.vault_prompts().len(), 1);
}

#[tokio::test]
async fn a_jump_host_gets_its_own_saved_password() {
    let f = Fixture::new();
    f.add("gate", Some(f.secret("gate password")), "");
    let db = f.add("db", Some(f.secret("db password")), "gate");
    f.open(&db);
    let seen = f.seen().await;
    assert_eq!(seen.password.as_deref(), Some("db password"));
    assert_eq!(
        seen.jump_hosts,
        [("gate.example".to_owned(), Some("gate password".to_owned()))]
    );
}

#[tokio::test]
async fn a_secret_the_vault_lost_reaches_the_protocol_as_lost() {
    let f = Fixture::new();
    let db = f.add("db", Some(CredentialRef(uuid::Uuid::new_v4())), "");
    f.open(&db);
    let seen = f.seen().await;
    assert_eq!(seen.password, None);
    assert!(seen.lost);
}

#[tokio::test]
async fn an_address_goes_through_a_saved_jump_host_and_brings_no_password_of_its_own() {
    let f = Fixture::new();
    f.add("gate", Some(f.secret("gate password")), "");
    let address = Address {
        protocol: "ssh".into(),
        host: "10.0.0.7".into(),
        port: None,
        username: Some(String::new()),
        options: json!({ "proxy_jump": "gate" }).as_object().unwrap().clone(),
    };
    f.workspace
        .open_address(address, SIZE, Arc::new(Statuses::default()))
        .unwrap();
    let seen = f.seen().await;
    assert_eq!(seen.password, None);
    assert_eq!(
        seen.jump_hosts,
        [("gate.example".to_owned(), Some("gate password".to_owned()))]
    );
}

#[tokio::test]
async fn opening_a_saved_connection_adds_it_to_recent() {
    let f = Fixture::new();
    let db = f.add("db", None, "");
    f.open(&db);
    let entries = f.recent.lock().unwrap().entries().unwrap();
    assert_eq!(entries[0].id, db.id);
}

#[test]
fn an_unknown_protocol_or_connection_is_refused_before_a_tab_opens() {
    let f = Fixture::new();
    let address = Address {
        protocol: "gopher".into(),
        host: "h".into(),
        port: None,
        username: None,
        options: Default::default(),
    };
    let frontend = Arc::new(Statuses::default());
    assert!(
        f.workspace
            .open_address(address, SIZE, frontend.clone())
            .is_err()
    );
    assert!(
        f.workspace
            .open_saved(uuid::Uuid::new_v4(), SIZE, frontend)
            .is_err()
    );
}

fn passwords(f: &Fixture) -> VaultPasswords {
    VaultPasswords {
        store: f.store.clone(),
        vault: f.vault.clone(),
    }
}

fn credential(f: &Fixture, id: uuid::Uuid) -> Option<CredentialRef> {
    f.store.lock().unwrap().connections()[&id]
        .settings
        .credential
}

#[test]
fn a_remembered_password_becomes_the_connections_credential() {
    let f = Fixture::new();
    let gate = f.add("gate", None, "");
    passwords(&f)
        .remember(gate.id, SecretString::from("typed"))
        .unwrap();
    let reference = credential(&f, gate.id).expect("linked to the Connection");
    let read = f.vault.lock().unwrap().read(reference).unwrap();
    assert_eq!(read.expose_secret(), "typed");
}

#[test]
fn a_password_that_replaces_a_wrong_one_keeps_its_reference() {
    let f = Fixture::new();
    let reference = f.secret("old");
    let gate = f.add("gate", Some(reference), "");
    passwords(&f)
        .remember(gate.id, SecretString::from("new"))
        .unwrap();
    assert_eq!(credential(&f, gate.id), Some(reference));
    let read = f.vault.lock().unwrap().read(reference).unwrap();
    assert_eq!(read.expose_secret(), "new");
}

#[test]
fn a_locked_vault_keeps_nothing_and_changes_no_connection() {
    let f = Fixture::new();
    let gate = f.add("gate", None, "");
    f.vault.lock().unwrap().lock();
    assert!(
        passwords(&f)
            .remember(gate.id, SecretString::from("typed"))
            .is_err()
    );
    assert_eq!(credential(&f, gate.id), None);
}

#[test]
fn a_hop_written_as_an_address_has_nothing_to_keep_it_on() {
    let f = Fixture::new();
    f.add("gate", None, "");
    let store = f.store.lock().unwrap();
    let hops = |route: &str| {
        let options = json!({ "proxy_jump": route });
        crate::plan(
            &store,
            f.workspace.registry(),
            &crate::Outsides::none(),
            options.as_object().unwrap(),
        )
        .unwrap()
    };
    assert!(hops("gate")[0].connection.is_some());
    assert_eq!(hops("ops@10.0.0.9")[0].connection, None);
    assert_eq!(hops("root@gate")[0].connection, None);
}

const SSH_CONFIG: &str = "Host db\n  HostName db.internal\n  Port 2222\n  User pg\n  IdentityFile ~/.ssh/%n_key\n  IdentitiesOnly yes\n  ProxyJump gate\nHost gate\n  HostName gate.internal\n  User ops\n";

#[tokio::test]
async fn a_quick_connect_to_an_alias_goes_where_ssh_would() {
    let f = Fixture::with_ssh_config(SSH_CONFIG);
    let address = Address {
        protocol: "ssh".into(),
        host: "db".into(),
        port: None,
        username: None,
        options: Default::default(),
    };
    f.workspace
        .open_address(address, SIZE, Arc::new(Statuses::default()))
        .unwrap();
    let seen = f.seen().await;
    assert_eq!(
        seen.address,
        ("db.internal".into(), 2222, Some("pg".into()))
    );
    assert_eq!(seen.options["identity_file"], json!(["~/.ssh/db_key"]));
    assert_eq!(seen.options["identities_only"], json!("yes"));
    assert_eq!(seen.jump_hosts, [("gate.internal".to_owned(), None)]);
}

#[tokio::test]
async fn a_saved_connection_takes_from_the_file_only_what_it_and_its_folders_leave() {
    let f = Fixture::with_ssh_config(SSH_CONFIG);
    let mut db = Connection::new("db", "ssh", "db");
    db.settings.username = Some("admin".into());
    f.store.lock().unwrap().save_connection(db.clone()).unwrap();
    f.open(&db);
    let seen = f.seen().await;
    assert_eq!(
        seen.address,
        ("db.internal".into(), 2222, Some("admin".into()))
    );
    assert_eq!(seen.options["identity_file"], json!(["~/.ssh/db_key"]));
}

#[test]
fn the_editor_learns_what_the_file_says_and_where_it_came_from() {
    let f = Fixture::with_ssh_config(SSH_CONFIG);
    let outside = f.workspace.outside("ssh", "db").unwrap();
    assert_eq!(outside.source, "~/.ssh/config");
    assert_eq!(outside.hostname.as_deref(), Some("db.internal"));
    assert_eq!(outside.settings.port, Some(2222));
    assert!(f.workspace.outside("rdp", "db").is_none());
    assert!(f.workspace.outside("ssh", "unknown").is_none());
}

#[test]
fn hashed_names_are_tried_against_every_ssh_host_in_use_and_their_jump_hosts() {
    let f = Fixture::with_ssh_config(SSH_CONFIG);
    let mut store = f.store.lock().unwrap();
    store
        .save_connection(Connection::new("db", "ssh", "db"))
        .unwrap();
    let mut build = Connection::new("build", "ssh", "build.lan");
    build.settings.port = Some(2222);
    store.save_connection(build).unwrap();
    let mut desktop = Connection::new("desktop", "rdp", "desktop.lan");
    desktop.settings.port = Some(3389);
    desktop
        .settings
        .options
        .insert("proxy_jump".into(), json!(["admin@hop.lan:2200"]));
    store.save_connection(desktop).unwrap();
    drop(store);
    assert_eq!(
        f.workspace.ssh_hosts_in_use(),
        [
            ("build.lan".to_owned(), 2222),
            ("db.internal".to_owned(), 2222),
            ("gate.internal".to_owned(), 22),
            ("hop.lan".to_owned(), 2200),
        ]
    );
}
