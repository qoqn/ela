use std::sync::{Arc, Mutex};
use std::time::Duration;

use ela_core::{Connection, Recent, Size, Store};
use ela_session::{
    Answer, Frontend, FrontendEvent, Prompt, ProtocolRegistry, SecretString, SessionStatus,
};
use ela_vault::{DEFAULT_IDLE_TIMEOUT, Vault};
use ela_workspace::Workspace;
use serde_json::json;

const MASTER: &str = "fixture master password";

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

async fn answer_until_connected(workspace: &Workspace, statuses: &Statuses, id: uuid::Uuid) {
    let mut answered = Vec::new();
    for _ in 0..300 {
        let last = statuses.0.lock().unwrap().last().cloned();
        match last {
            Some(SessionStatus::Connected) => return,
            Some(SessionStatus::Closed { error }) => panic!("closed: {error:?}"),
            Some(SessionStatus::Asking {
                id: prompt,
                prompt: kind,
                ..
            }) if !answered.contains(&prompt) => {
                answered.push(prompt);
                let answer = match kind {
                    Prompt::Trust { .. } => Answer::Trust(true),
                    Prompt::Vault { .. } => Answer::Vault(Some(SecretString::from(MASTER))),
                    other => panic!("asked for {other:?}, which the Vault should have answered"),
                };
                workspace.manager().answer(id, prompt, answer).unwrap();
            }
            _ => {}
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out");
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml SSH fixture"]
async fn a_saved_connection_opens_through_a_jump_host_with_the_vault_unlocked_on_the_tab() {
    let dir = tempfile::tempdir().unwrap();
    let mut vault = Vault::new(dir.path().join("vault.bin"), DEFAULT_IDLE_TIMEOUT);
    vault.create(&SecretString::from(MASTER)).unwrap();
    let password = vault.store(SecretString::from("ela")).unwrap();
    vault.lock();

    let mut store = Store::open(dir.path().join("config")).unwrap();
    let mut gate = Connection::new("gate", "ssh", "127.0.0.1");
    gate.settings.port = Some(2222);
    gate.settings.username = Some("ela".into());
    gate.settings.credential = Some(password);
    let mut inner = Connection::new("inner", "ssh", "ssh-inner");
    inner.settings.port = Some(2222);
    inner.settings.username = Some("ela".into());
    inner.settings.credential = Some(password);
    inner
        .settings
        .options
        .insert("proxy_jump".into(), json!("gate"));
    store.save_connection(gate).unwrap();
    store.save_connection(inner.clone()).unwrap();

    let mut registry = ProtocolRegistry::new();
    registry.register(
        ela_ssh::SshProtocol::with_known_hosts(dir.path().join("known_hosts"))
            .finding_keys(ela_ssh::KeySources::default()),
    );
    let workspace = Workspace::new(
        Ok(Arc::new(Mutex::new(store))),
        Ok(Arc::new(Mutex::new(vault))),
        Some(Arc::new(Mutex::new(Recent::at(
            dir.path().join("recent.toml"),
        )))),
        registry,
    );
    let statuses = Arc::new(Statuses::default());
    let size = Size {
        width: 80,
        height: 24,
    };
    let id = workspace
        .open_saved(inner.id, size, statuses.clone())
        .unwrap();
    answer_until_connected(&workspace, &statuses, id).await;
    let vault_prompts = statuses
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|s| {
            matches!(
                s,
                SessionStatus::Asking {
                    prompt: Prompt::Vault { .. },
                    ..
                }
            )
        })
        .count();
    assert_eq!(vault_prompts, 1, "one unlock serves the whole route");
    workspace.manager().close(id);
}
