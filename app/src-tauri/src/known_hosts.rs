use std::path::PathBuf;
use std::sync::Arc;

use ela_session::{KnownHost, KnownHostList};
use ela_workspace::Workspace;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{CommandError, CommandResult};
use crate::saved;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum KnownHostStore {
    SshKeys,
    Certificates,
}

#[derive(Serialize, specta::Type)]
pub struct KnownHostSection {
    store: KnownHostStore,
    path: String,
    entries: Vec<KnownHost>,
    other_lines: u32,
    problem: Option<String>,
}

fn path_of(store: KnownHostStore) -> CommandResult<PathBuf> {
    match store {
        KnownHostStore::SshKeys => ela_ssh::known_hosts::default_path(),
        KnownHostStore::Certificates => ela_rdp::pinned_certs::default_path(),
    }
    .ok_or_else(saved::no_home)
}

fn section(store: KnownHostStore, saved_hosts: &[(String, u16)]) -> KnownHostSection {
    let path = path_of(store);
    let shown = path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let listed = path
        .map_err(|e| e.to_string())
        .and_then(|path| match store {
            KnownHostStore::SshKeys => ela_ssh::known_hosts::list(&path, saved_hosts),
            KnownHostStore::Certificates => ela_rdp::pinned_certs::list(&path),
        });
    let (
        KnownHostList {
            entries,
            other_lines,
        },
        problem,
    ) = match listed {
        Ok(list) => (list, None),
        Err(problem) => (KnownHostList::default(), Some(problem)),
    };
    KnownHostSection {
        store,
        path: shown,
        entries,
        other_lines,
        problem,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn known_hosts_list(
    workspace: State<'_, Arc<Workspace>>,
) -> CommandResult<Vec<KnownHostSection>> {
    // Without the saved tree hashed names stay unnamed, but the list still works.
    let saved_hosts = workspace.ssh_hosts_in_use();
    Ok(vec![
        section(KnownHostStore::SshKeys, &saved_hosts),
        section(KnownHostStore::Certificates, &saved_hosts),
    ])
}

/// Takes which store rather than a path, so the webview can't name a file to rewrite.
#[tauri::command]
#[specta::specta]
pub async fn known_hosts_remove(
    store: KnownHostStore,
    name: String,
    fingerprint: String,
) -> CommandResult<()> {
    let path = path_of(store)?;
    match store {
        KnownHostStore::SshKeys => ela_ssh::known_hosts::remove(&path, &name, &fingerprint),
        KnownHostStore::Certificates => ela_rdp::pinned_certs::remove(&path, &name, &fingerprint),
    }
    .map_err(CommandError::new)
}
