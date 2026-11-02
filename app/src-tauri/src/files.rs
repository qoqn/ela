use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ela_session::{
    CancellationToken, FileError, FolderOutcome, RemoteFiles, RemoteListing, SessionId, Transfer,
    local_file_name, remote_join,
};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, DragDropEvent, Emitter, State, Window};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use uuid::Uuid;

use crate::Manager_;
use crate::error::{CommandError, CommandResult};

#[derive(Default)]
pub struct Transfers(Mutex<HashMap<Uuid, CancellationToken>>);

impl Transfers {
    fn start(&self, id: Uuid) -> CancellationToken {
        let cancel = CancellationToken::new();
        self.0.lock().unwrap().insert(id, cancel.clone());
        cancel
    }

    fn finish(&self, id: Uuid) {
        self.0.lock().unwrap().remove(&id);
    }
}

#[derive(Clone, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransferEvent {
    Progress { name: String, done: u64, total: u64 },
    Finished { name: String, note: Option<String> },
    Cancelled { name: String },
    Failed { name: String, message: String },
}

fn files_of(manager: &Manager_, id: SessionId) -> CommandResult<Arc<dyn RemoteFiles>> {
    manager
        .files(id)
        .ok_or_else(|| CommandError::new("This session can't browse files."))
}

#[tauri::command]
#[specta::specta]
pub async fn files_list(
    manager: State<'_, Manager_>,
    id: SessionId,
    path: Option<String>,
) -> CommandResult<RemoteListing> {
    files_of(&manager, id)?
        .list(path)
        .await
        .map_err(CommandError::from)
}

fn progress_to(events: &Channel<TransferEvent>, name: &str) -> Box<dyn Fn(u64, u64) + Send + Sync> {
    let (events, name) = (events.clone(), name.to_owned());
    Box::new(move |done, total| {
        let _ = events.send(TransferEvent::Progress {
            name: name.clone(),
            done,
            total,
        });
    })
}

fn report(events: &Channel<TransferEvent>, name: String, result: Result<(), FileError>) -> bool {
    let (event, carry_on) = match result {
        Ok(()) => (TransferEvent::Finished { name, note: None }, true),
        Err(FileError::Cancelled) => (TransferEvent::Cancelled { name }, false),
        Err(e) => (
            TransferEvent::Failed {
                name,
                message: e.to_string(),
            },
            true,
        ),
    };
    let _ = events.send(event);
    carry_on
}

fn report_folder(
    events: &Channel<TransferEvent>,
    name: String,
    result: Result<FolderOutcome, FileError>,
) -> bool {
    match result {
        Ok(outcome) => {
            let event = match outcome.failure_text() {
                Some(message) => TransferEvent::Failed { name, message },
                None => TransferEvent::Finished {
                    name,
                    note: outcome.left_out_text(),
                },
            };
            let _ = events.send(event);
            true
        }
        Err(e) => report(events, name, Err(e)),
    }
}

/// The native dialog chooses the local path, in Rust. A webview that could name one would let an
/// XSS bug read or overwrite any file the user can.
#[tauri::command]
#[specta::specta]
pub async fn files_download(
    app: AppHandle,
    manager: State<'_, Manager_>,
    transfers: State<'_, Transfers>,
    id: SessionId,
    remote: String,
    transfer: Uuid,
    events: Channel<TransferEvent>,
) -> CommandResult<()> {
    let files = files_of(&manager, id)?;
    let name = local_file_name(&remote);
    let Some(local) = app
        .dialog()
        .file()
        .set_file_name(&name)
        .blocking_save_file()
        .and_then(|path| path.into_path().ok())
    else {
        return Ok(());
    };

    let cancel = transfers.start(transfer);
    let job = Transfer {
        cancel,
        progress: progress_to(&events, &name),
    };
    let result = files.download(&remote, &local, &job).await;
    transfers.finish(transfer);
    report(&events, name, result);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn files_download_folder(
    app: AppHandle,
    manager: State<'_, Manager_>,
    transfers: State<'_, Transfers>,
    id: SessionId,
    remote: String,
    transfer: Uuid,
    events: Channel<TransferEvent>,
) -> CommandResult<()> {
    let files = files_of(&manager, id)?;
    let name = local_file_name(&remote);
    let Some(chosen) = app
        .dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|path| path.into_path().ok())
    else {
        return Ok(());
    };
    let local = chosen.join(&name);
    if local.exists() && !confirm_merge(&app, &name, &chosen.display().to_string()) {
        return Ok(());
    }

    let cancel = transfers.start(transfer);
    let job = Transfer {
        cancel,
        progress: progress_to(&events, &name),
    };
    let result = files.download_folder(&remote, &local, &job).await;
    transfers.finish(transfer);
    report_folder(&events, name, result);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn files_upload(
    app: AppHandle,
    manager: State<'_, Manager_>,
    transfers: State<'_, Transfers>,
    id: SessionId,
    directory: String,
    transfer: Uuid,
    events: Channel<TransferEvent>,
) -> CommandResult<()> {
    let files = files_of(&manager, id)?;
    let Some(chosen) = app.dialog().file().blocking_pick_files() else {
        return Ok(());
    };
    let chosen = chosen
        .into_iter()
        .filter_map(|path| path.into_path().ok())
        .collect();
    upload_paths(
        &app, files, &transfers, chosen, &directory, transfer, &events,
    )
    .await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn files_upload_folder(
    app: AppHandle,
    manager: State<'_, Manager_>,
    transfers: State<'_, Transfers>,
    id: SessionId,
    directory: String,
    transfer: Uuid,
    events: Channel<TransferEvent>,
) -> CommandResult<()> {
    let files = files_of(&manager, id)?;
    let Some(chosen) = app.dialog().file().blocking_pick_folders() else {
        return Ok(());
    };
    let chosen = chosen
        .into_iter()
        .filter_map(|path| path.into_path().ok())
        .collect();
    upload_paths(
        &app, files, &transfers, chosen, &directory, transfer, &events,
    )
    .await;
    Ok(())
}

/// What was dropped on the window stays here, and the webview only gets a token for it. That keeps
/// local paths out of IPC as the dialogs do, and a token can only send what the user dragged in.
#[derive(Default)]
pub struct Drops(Mutex<Option<(Uuid, Vec<PathBuf>)>>);

impl Drops {
    fn hold(&self, paths: Vec<PathBuf>) -> Uuid {
        let token = Uuid::new_v4();
        *self.0.lock().unwrap() = Some((token, paths));
        token
    }

    fn take(&self, token: Uuid) -> Option<Vec<PathBuf>> {
        let mut held = self.0.lock().unwrap();
        match held.as_ref() {
            Some((current, _)) if *current == token => held.take().map(|(_, paths)| paths),
            _ => None,
        }
    }
}

#[derive(Clone, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileDrag {
    Over { x: i32, y: i32 },
    Left,
    Dropped { token: Uuid, x: i32, y: i32 },
}

pub const FILE_DRAG_EVENT: &str = "file-drag";

/// wry's positions are CSS pixels on WebKitGTK, whatever Tauri's `PhysicalPosition` type says.
pub fn on_drag_drop(window: &Window, drops: &Drops, event: &DragDropEvent) {
    let drag = match event {
        DragDropEvent::Enter { position, .. } | DragDropEvent::Over { position } => {
            FileDrag::Over {
                x: position.x as i32,
                y: position.y as i32,
            }
        }
        DragDropEvent::Drop { paths, position } => FileDrag::Dropped {
            token: drops.hold(paths.clone()),
            x: position.x as i32,
            y: position.y as i32,
        },
        DragDropEvent::Leave => FileDrag::Left,
        _ => return,
    };
    let _ = window.emit(FILE_DRAG_EVENT, drag);
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub async fn files_upload_dropped(
    app: AppHandle,
    manager: State<'_, Manager_>,
    transfers: State<'_, Transfers>,
    drops: State<'_, Drops>,
    id: SessionId,
    directory: String,
    token: Uuid,
    transfer: Uuid,
    events: Channel<TransferEvent>,
) -> CommandResult<()> {
    let files = files_of(&manager, id)?;
    let paths = drops
        .take(token)
        .ok_or_else(|| CommandError::new("That drop is no longer available."))?;
    upload_paths(
        &app, files, &transfers, paths, &directory, transfer, &events,
    )
    .await;
    Ok(())
}

fn shown_name(local: &Path) -> String {
    local
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

async fn upload_paths(
    app: &AppHandle,
    files: Arc<dyn RemoteFiles>,
    transfers: &Transfers,
    paths: Vec<PathBuf>,
    directory: &str,
    transfer: Uuid,
    events: &Channel<TransferEvent>,
) {
    let cancel = transfers.start(transfer);
    for local in paths {
        let name = shown_name(&local);
        let remote = remote_join(directory, &name);
        let folder = local.is_dir();
        let carry_on = match files.exists(&remote).await {
            Err(e) => report(events, name, Err(e)),
            Ok(true) if folder && !confirm_merge(app, &name, directory) => true,
            Ok(true) if !folder && !confirm_replace(app, &name, directory) => true,
            Ok(_) => {
                let job = Transfer {
                    cancel: cancel.clone(),
                    progress: progress_to(events, &name),
                };
                if folder {
                    let result = files.upload_folder(&local, &remote, &job).await;
                    report_folder(events, name, result)
                } else {
                    let result = files.upload(&local, &remote, &job).await;
                    report(events, name, result)
                }
            }
        };
        if !carry_on {
            break;
        }
    }
    transfers.finish(transfer);
}

fn confirm_replace(app: &AppHandle, name: &str, directory: &str) -> bool {
    app.dialog()
        .message(format!("{name} already exists in {directory}."))
        .title("Replace this file?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Replace".to_owned(),
            "Skip".to_owned(),
        ))
        .blocking_show()
}

fn confirm_merge(app: &AppHandle, name: &str, directory: &str) -> bool {
    app.dialog()
        .message(format!(
            "{name} already exists in {directory}. Files with the same name are replaced, and the others stay."
        ))
        .title("Merge into this folder?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Merge".to_owned(),
            "Skip".to_owned(),
        ))
        .blocking_show()
}

#[tauri::command]
#[specta::specta]
pub async fn files_rename(
    manager: State<'_, Manager_>,
    id: SessionId,
    path: String,
    name: String,
) -> CommandResult<()> {
    files_of(&manager, id)?
        .rename(&path, &name)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn files_make_folder(
    manager: State<'_, Manager_>,
    id: SessionId,
    directory: String,
    name: String,
) -> CommandResult<()> {
    files_of(&manager, id)?
        .make_folder(&directory, &name)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn files_delete(
    manager: State<'_, Manager_>,
    id: SessionId,
    path: String,
) -> CommandResult<()> {
    files_of(&manager, id)?
        .delete(&path)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn files_cancel(transfers: State<'_, Transfers>, transfer: Uuid) -> CommandResult<()> {
    if let Some(cancel) = transfers.0.lock().unwrap().get(&transfer) {
        cancel.cancel();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_token_works_once_and_only_for_the_latest_drop() {
        let drops = Drops::default();
        let first = drops.hold(vec![PathBuf::from("/a")]);
        let second = drops.hold(vec![PathBuf::from("/b")]);
        assert_eq!(drops.take(first), None);
        assert_eq!(drops.take(second), Some(vec![PathBuf::from("/b")]));
        assert_eq!(drops.take(second), None);
    }
}
