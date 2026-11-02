use std::path::Path;
use std::sync::{Arc, Mutex};

use ela_core::{Connection, Store};
use ela_session::{ProtocolRegistry, SessionManager};
use ela_vault::{DEFAULT_IDLE_TIMEOUT, Vault};
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};
use tempfile::TempDir;

use crate::Manager_;
use crate::saved::Saved;
use crate::vault::{self, VaultSlot};

const MASTER: &str = "master-sentinel-do-not-leak";
const SECRET: &str = "secret-sentinel-do-not-leak";
const NEW_MASTER: &str = "new-master-sentinel-do-not-leak";

/// Everything that handles the Vault or a saved Connection. `open_saved_session` is here by name
/// only: it needs live Channels, and `ela-workspace`'s tests cover what it does with a Secret.
const DRIVEN: &[&str] = &[
    "vault_info",
    "vault_create",
    "vault_unlock",
    "vault_change_master",
    "vault_set_idle_minutes",
    "vault_lock",
    "vault_store_secret",
    "vault_remove_secret",
    "vault_credential_state",
    "vault_key_passphrases",
    "vault_forget_key_passphrase",
    "list_tree",
    "resolve_connection",
    "open_saved_session",
    "session_answer",
    "list_trash",
    "empty_trash",
];

struct Ipc {
    webview: WebviewWindow<MockRuntime>,
    _app: tauri::App<MockRuntime>,
    responses: Vec<String>,
}

impl Ipc {
    fn call(&mut self, command: &str, body: Value) -> Result<Value, Value> {
        let result = tauri::test::get_ipc_response(
            &self.webview,
            InvokeRequest {
                cmd: command.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        );
        let result = result.map(|body| match body {
            InvokeResponseBody::Json(json) => serde_json::from_str(&json).unwrap(),
            InvokeResponseBody::Raw(raw) => Value::String(String::from_utf8_lossy(&raw).into()),
        });
        let seen = match &result {
            Ok(value) | Err(value) => value.to_string(),
        };
        self.responses.push(seen);
        result
    }
}

fn vault_slot(dir: &Path) -> VaultSlot {
    Ok(Arc::new(Mutex::new(Vault::new(
        dir.join("vault.bin"),
        DEFAULT_IDLE_TIMEOUT,
    ))))
}

fn ipc(dir: &Path, connections: impl IntoIterator<Item = Connection>) -> Ipc {
    let mut store = Store::open(dir.join("config")).unwrap();
    for connection in connections {
        store.save_connection(connection).unwrap();
    }
    let store = Arc::new(Mutex::new(store));
    let saved: Saved = Ok(store.clone());
    let vault = vault_slot(dir);
    let workspace = Arc::new(ela_workspace::Workspace::new(
        Ok(store),
        vault.clone().map_err(|e| e.to_string()),
        None,
        ProtocolRegistry::new(),
    ));
    let manager: Manager_ = SessionManager::new(ProtocolRegistry::new());
    let app = mock_builder()
        .manage(manager)
        .manage(workspace)
        .manage(saved)
        .manage(vault)
        .invoke_handler(tauri::generate_handler![
            crate::saved::list_tree,
            crate::saved::resolve_connection,
            crate::session::session_answer,
            crate::trash::list_trash,
            crate::trash::empty_trash,
            vault::vault_info,
            vault::vault_create,
            vault::vault_unlock,
            vault::vault_change_master,
            vault::vault_set_idle_minutes,
            vault::vault_lock,
            vault::vault_store_secret,
            vault::vault_remove_secret,
            vault::vault_credential_state,
            vault::vault_key_passphrases,
            vault::vault_forget_key_passphrase,
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    Ipc {
        webview,
        _app: app,
        responses: Vec::new(),
    }
}

#[test]
fn no_response_carries_a_secret_through_a_whole_vault_lifetime() {
    let dir = TempDir::new().unwrap();
    let mut connection = Connection::new("db-01", "ssh", "db-01.internal");
    let id = connection.id;
    // A store with no Protocols still resolves a Connection that names its own port.
    connection.settings.port = Some(22);
    connection
        .settings
        .options
        .insert("proxy_jump".into(), json!("gate"));
    let mut gate = Connection::new("gate", "ssh", "gate.internal");
    gate.settings.port = Some(22);
    let mut ipc = ipc(dir.path(), [connection.clone(), gate.clone()]);

    assert_eq!(
        ipc.call("vault_info", json!({})).unwrap()["state"],
        "absent"
    );
    ipc.call("vault_create", json!({ "master": MASTER }))
        .unwrap();
    assert_eq!(
        ipc.call("vault_info", json!({})).unwrap()["state"],
        "unlocked"
    );

    let reference = ipc
        .call(
            "vault_store_secret",
            json!({ "secret": SECRET, "replaces": null }),
        )
        .unwrap();
    assert!(reference.is_string(), "only a reference comes back");
    assert_eq!(
        ipc.call("vault_credential_state", json!({ "reference": reference }))
            .unwrap(),
        "available"
    );

    let same = ipc
        .call(
            "vault_store_secret",
            json!({ "secret": SECRET, "replaces": reference }),
        )
        .unwrap();
    assert_eq!(same, reference);

    ipc.call("vault_lock", json!({})).unwrap();
    assert_eq!(
        ipc.call("vault_credential_state", json!({ "reference": reference }))
            .unwrap(),
        "locked"
    );
    let refused = ipc
        .call("vault_unlock", json!({ "master": SECRET }))
        .unwrap_err();
    assert_eq!(refused, "That password doesn't open the Vault.");
    ipc.call("vault_unlock", json!({ "master": MASTER }))
        .unwrap();

    let refused = ipc
        .call(
            "vault_change_master",
            json!({ "current": SECRET, "replacement": NEW_MASTER }),
        )
        .unwrap_err();
    assert_eq!(refused, "That password doesn't open the Vault.");
    ipc.call(
        "vault_change_master",
        json!({ "current": MASTER, "replacement": NEW_MASTER }),
    )
    .unwrap();
    ipc.call("vault_set_idle_minutes", json!({ "minutes": 30 }))
        .unwrap();

    let tree = ipc.call("list_tree", json!({})).unwrap();
    let listed = tree["connections"].as_array().unwrap();
    assert!(listed.iter().any(|c| c["id"] == json!(id)));
    let resolved = ipc.call("resolve_connection", json!({ "id": id })).unwrap();
    assert_eq!(resolved["host"], "db-01.internal");

    ipc.call("vault_remove_secret", json!({ "reference": reference }))
        .unwrap();
    assert_eq!(
        ipc.call("vault_credential_state", json!({ "reference": reference }))
            .unwrap(),
        "missing"
    );

    assert_eq!(
        ipc.call("vault_key_passphrases", json!({})).unwrap(),
        json!([])
    );
    ipc.call(
        "vault_forget_key_passphrase",
        json!({ "credential": reference }),
    )
    .unwrap_err();

    ipc.call("list_trash", json!({})).unwrap();
    ipc.call("empty_trash", json!({})).unwrap();

    let no_session = uuid::Uuid::new_v4();
    for reply in [
        json!({ "type": "passphrase", "passphrase": SECRET, "remember": true }),
        json!({ "type": "password", "password": SECRET, "remember": true }),
        json!({ "type": "answers", "answers": [SECRET] }),
    ] {
        ipc.call(
            "session_answer",
            json!({ "id": no_session, "prompt": 1, "reply": reply }),
        )
        .unwrap_err();
    }

    assert!(ipc.responses.len() > 10);
    for response in &ipc.responses {
        assert!(!response.contains(SECRET), "leaked in {response}");
        assert!(!response.contains(MASTER), "leaked in {response}");
        assert!(!response.contains(NEW_MASTER), "leaked in {response}");
    }
}

#[test]
fn every_command_that_touches_the_vault_is_driven_above() {
    let bindings = include_str!("../../src/lib/bindings.ts");
    let commands: Vec<&str> = bindings
        .split("__TAURI_INVOKE<")
        .skip(1)
        .filter_map(|rest| rest.split("(\"").nth(1)?.split('"').next())
        .collect();
    assert!(commands.contains(&"list_protocols"), "found {commands:?}");
    for command in commands {
        let touches_secrets = command.starts_with("vault_")
            || command.contains("saved")
            || command.contains("passphrase")
            || command.contains("password")
            || command.contains("answer");
        assert!(
            !touches_secrets || DRIVEN.contains(&command),
            "{command} isn't covered by no_response_carries_a_secret…"
        );
    }
}

#[test]
fn the_only_ipc_types_with_a_password_field_are_requests() {
    let bindings = include_str!("../../src/lib/bindings.ts");
    for line in bindings.lines() {
        let line_lower = line.to_lowercase();
        let mentions_secret = ["secret", "master"]
            .iter()
            .any(|word| line_lower.contains(word))
            || ["password", "passphrase"].iter().any(|field| {
                line_lower.contains(&format!("{field}:"))
                    || line_lower.contains(&format!("{field}?:"))
            });
        if !mentions_secret || !line.starts_with("export type") {
            continue;
        }
        assert!(
            line.contains("_Deserialize")
                || line.starts_with("export type OpenSessionRequest")
                || line.starts_with("export type OpenSavedSessionRequest")
                || line.starts_with("export type PromptReply"),
            "a type sent to the webview may hold a Secret: {line}"
        );
    }
}
