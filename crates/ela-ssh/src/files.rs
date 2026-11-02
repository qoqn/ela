use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use ela_session::{
    CancellationToken, FileError, FolderOutcome, RemoteEntry, RemoteFiles, RemoteKind,
    RemoteListing, Transfer, check_remote_name, local_file_name, remote_join, remote_parent,
    sort_entries,
};
use russh::client;
use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::OnceCell;

use crate::Handler;

const CHUNK: usize = 64 * 1024;
const PROGRESS_EVERY: Duration = Duration::from_millis(100);
/// The library's 10 seconds is for one request, which a busy server's disk can outlast.
const REQUEST_TIMEOUT_SECS: u64 = 60;

pub(crate) struct SshFiles {
    handle: Arc<client::Handle<Handler>>,
    sftp: OnceCell<SftpSession>,
}

impl SshFiles {
    pub(crate) fn new(handle: Arc<client::Handle<Handler>>) -> Self {
        Self {
            handle,
            sftp: OnceCell::new(),
        }
    }

    /// Opened on first use, on the Session's own connection, so it never signs in a second time.
    async fn sftp(&self) -> Result<&SftpSession, FileError> {
        self.sftp
            .get_or_try_init(|| async {
                let no_sftp = |e: String| FileError::NoSftp(e);
                let channel = self
                    .handle
                    .channel_open_session()
                    .await
                    .map_err(|e| no_sftp(e.to_string()))?;
                channel
                    .request_subsystem(true, "sftp")
                    .await
                    .map_err(|e| no_sftp(e.to_string()))?;
                let sftp = SftpSession::new(channel.into_stream())
                    .await
                    .map_err(|e| no_sftp(e.to_string()))?;
                sftp.set_timeout(REQUEST_TIMEOUT_SECS);
                Ok(sftp)
            })
            .await
    }
}

fn file_error(path: &str, error: SftpError) -> FileError {
    match &error {
        SftpError::Status(status) if status.status_code == StatusCode::NoSuchFile => {
            FileError::NotFound(path.to_owned())
        }
        SftpError::Status(status) if status.status_code == StatusCode::PermissionDenied => {
            FileError::Denied(path.to_owned())
        }
        _ => FileError::Other(format!("{path}: {error}")),
    }
}

fn local_error(path: &Path, error: std::io::Error) -> FileError {
    match error.kind() {
        std::io::ErrorKind::NotFound => FileError::NotFound(path.display().to_string()),
        std::io::ErrorKind::PermissionDenied => FileError::Denied(path.display().to_string()),
        _ => FileError::Other(format!("{}: {error}", path.display())),
    }
}

type Progress<'a> = &'a (dyn Fn(u64, u64) + Send + Sync);

#[derive(Clone, Copy)]
struct Job<'a> {
    cancel: &'a CancellationToken,
    progress: Progress<'a>,
}

impl<'a> From<&'a Transfer> for Job<'a> {
    fn from(transfer: &'a Transfer) -> Self {
        Self {
            cancel: &transfer.cancel,
            progress: &*transfer.progress,
        }
    }
}

async fn copy(
    mut from: impl AsyncRead + Unpin,
    mut to: impl AsyncWrite + Unpin,
    total: u64,
    job: Job<'_>,
) -> Result<(), FileError> {
    let other = |e: std::io::Error| FileError::Other(e.to_string());
    let mut buffer = vec![0; CHUNK];
    let mut done = 0;
    let mut reported = Instant::now();
    (job.progress)(0, total);
    loop {
        let read = tokio::select! {
            read = from.read(&mut buffer) => read.map_err(other)?,
            _ = job.cancel.cancelled() => return Err(FileError::Cancelled),
        };
        if read == 0 {
            break;
        }
        tokio::select! {
            written = to.write_all(&buffer[..read]) => written.map_err(other)?,
            _ = job.cancel.cancelled() => return Err(FileError::Cancelled),
        }
        done += read as u64;
        if reported.elapsed() >= PROGRESS_EVERY {
            (job.progress)(done, total);
            reported = Instant::now();
        }
    }
    to.shutdown().await.map_err(other)?;
    (job.progress)(done, total);
    Ok(())
}

#[async_trait]
impl RemoteFiles for SshFiles {
    async fn list(&self, path: Option<String>) -> Result<RemoteListing, FileError> {
        let sftp = self.sftp().await?;
        let asked = path.unwrap_or_else(|| ".".to_owned());
        let path = sftp
            .canonicalize(asked.clone())
            .await
            .map_err(|e| file_error(&asked, e))?;
        let read = sftp
            .read_dir(path.clone())
            .await
            .map_err(|e| file_error(&path, e))?;

        let mut entries = Vec::new();
        for entry in read {
            let name = entry.file_name();
            let full = remote_join(&path, &name);
            let own = entry.metadata();
            let link = entry.file_type().is_symlink();
            // A link that leads nowhere still gets a row, as something that can't be opened.
            let target = if link {
                sftp.metadata(full.clone()).await.ok()
            } else {
                Some(own.clone())
            };
            let kind = match &target {
                Some(meta) if meta.file_type().is_dir() => RemoteKind::Folder,
                Some(meta) if meta.file_type().is_file() => RemoteKind::File,
                _ => RemoteKind::Other,
            };
            let shown = target.as_ref().unwrap_or(&own);
            entries.push(RemoteEntry {
                name,
                path: full,
                kind,
                link,
                size: shown.size.unwrap_or(0),
                modified: shown.mtime,
            });
        }
        sort_entries(&mut entries);
        Ok(RemoteListing {
            parent: remote_parent(&path),
            path,
            entries,
        })
    }

    async fn exists(&self, path: &str) -> Result<bool, FileError> {
        let sftp = self.sftp().await?;
        sftp.try_exists(path.to_owned())
            .await
            .map_err(|e| file_error(path, e))
    }

    async fn download(
        &self,
        remote: &str,
        local: &Path,
        transfer: &Transfer,
    ) -> Result<(), FileError> {
        download_file(self.sftp().await?, remote, local, transfer.into()).await
    }

    async fn upload(
        &self,
        local: &Path,
        remote: &str,
        transfer: &Transfer,
    ) -> Result<(), FileError> {
        upload_file(self.sftp().await?, local, remote, transfer.into()).await
    }

    async fn download_folder(
        &self,
        remote: &str,
        local: &Path,
        transfer: &Transfer,
    ) -> Result<FolderOutcome, FileError> {
        let sftp = self.sftp().await?;
        let meta = sftp
            .metadata(remote.to_owned())
            .await
            .map_err(|e| file_error(remote, e))?;
        let walk = walk_remote(sftp, remote, &transfer.cancel).await?;
        let mut outcome = FolderOutcome {
            left_out: walk.others.len() as u32,
            failed: walk.unreadable,
            ..Default::default()
        };

        make_local_folder(local, meta.permissions).await?;
        for folder in walk.folders.iter().skip(1) {
            let made = match local_relative(&folder.relative) {
                Some(relative) => make_local_folder(&local.join(relative), folder.mode).await,
                None => Err(unsafe_name(&folder.path)),
            };
            if let Err(e) = made {
                outcome.failed.push(e.to_string());
            }
        }

        let total: u64 = walk.files.iter().map(|file| file.size).sum();
        let mut base = 0_u64;
        for file in &walk.files {
            let progress = |done: u64, _: u64| (transfer.progress)(base + done, total);
            let job = Job {
                cancel: &transfer.cancel,
                progress: &progress,
            };
            let copied = match local_relative(&file.relative) {
                Some(relative) => download_file(sftp, &file.path, &local.join(relative), job).await,
                None => Err(unsafe_name(&file.path)),
            };
            tally(&mut outcome, copied)?;
            base += file.size;
        }
        (transfer.progress)(total, total);
        Ok(outcome)
    }

    async fn upload_folder(
        &self,
        local: &Path,
        remote: &str,
        transfer: &Transfer,
    ) -> Result<FolderOutcome, FileError> {
        let sftp = self.sftp().await?;
        let walk = walk_local(local, &transfer.cancel).await?;
        let mut outcome = FolderOutcome {
            left_out: walk.others,
            failed: walk.unreadable,
            ..Default::default()
        };

        make_remote_folder(sftp, remote, walk.mode).await?;
        for (relative, mode) in &walk.folders {
            let path = remote_join(remote, relative);
            if let Err(e) = make_remote_folder(sftp, &path, *mode).await {
                outcome.failed.push(e.to_string());
            }
        }

        let total: u64 = walk.files.iter().map(|file| file.size).sum();
        let mut base = 0_u64;
        for file in &walk.files {
            let progress = |done: u64, _: u64| (transfer.progress)(base + done, total);
            let job = Job {
                cancel: &transfer.cancel,
                progress: &progress,
            };
            let path = remote_join(remote, &file.relative);
            tally(
                &mut outcome,
                upload_file(sftp, &file.path, &path, job).await,
            )?;
            base += file.size;
        }
        (transfer.progress)(total, total);
        Ok(outcome)
    }

    async fn rename(&self, path: &str, name: &str) -> Result<(), FileError> {
        check_remote_name(name)?;
        let parent = remote_parent(path).ok_or_else(|| FileError::Denied(path.to_owned()))?;
        let target = remote_join(&parent, name);
        if target == path {
            return Ok(());
        }
        let sftp = self.sftp().await?;
        refuse_taken(sftp, &target).await?;
        sftp.rename(path.to_owned(), target)
            .await
            .map_err(|e| file_error(path, e))
    }

    async fn make_folder(&self, directory: &str, name: &str) -> Result<(), FileError> {
        check_remote_name(name)?;
        let sftp = self.sftp().await?;
        let target = remote_join(directory, name);
        refuse_taken(sftp, &target).await?;
        sftp.create_dir(target.clone())
            .await
            .map_err(|e| file_error(&target, e))
    }

    async fn delete(&self, path: &str) -> Result<(), FileError> {
        if remote_parent(path).is_none() {
            return Err(FileError::Denied(path.to_owned()));
        }
        let sftp = self.sftp().await?;
        let own = sftp
            .symlink_metadata(path.to_owned())
            .await
            .map_err(|e| file_error(path, e))?;
        if !own.file_type().is_dir() {
            return sftp
                .remove_file(path.to_owned())
                .await
                .map_err(|e| file_error(path, e));
        }

        // Read the whole folder first, so a part that can't be read stops it before anything goes.
        let walk = walk_remote(sftp, path, &CancellationToken::new()).await?;
        if let Some(unreadable) = walk.unreadable.into_iter().next() {
            return Err(FileError::Other(unreadable));
        }
        let files = walk.files.iter().map(|file| &file.path);
        for each in files.chain(&walk.others) {
            sftp.remove_file(each.clone())
                .await
                .map_err(|e| file_error(each, e))?;
        }
        for folder in walk.folders.iter().rev() {
            sftp.remove_dir(folder.path.clone())
                .await
                .map_err(|e| file_error(&folder.path, e))?;
        }
        Ok(())
    }
}

fn tally(outcome: &mut FolderOutcome, copied: Result<(), FileError>) -> Result<(), FileError> {
    match copied {
        Ok(()) => outcome.files += 1,
        Err(FileError::Cancelled) => return Err(FileError::Cancelled),
        Err(e) => outcome.failed.push(e.to_string()),
    }
    Ok(())
}

fn unsafe_name(path: &str) -> FileError {
    FileError::Other(format!("{path}: this name isn't safe to save here."))
}

/// A server chooses these names, so each part must stay inside the local folder as it is.
fn local_relative(relative: &str) -> Option<PathBuf> {
    relative
        .split('/')
        .all(|part| local_file_name(part) == part)
        .then(|| relative.split('/').collect())
}

async fn refuse_taken(sftp: &SftpSession, target: &str) -> Result<(), FileError> {
    // Not following links: a link that leads nowhere still holds its name.
    match sftp.symlink_metadata(target.to_owned()).await {
        Ok(_) => Err(FileError::Exists(target.to_owned())),
        Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => Ok(()),
        Err(e) => Err(file_error(target, e)),
    }
}

struct RemoteFolder {
    path: String,
    relative: String,
    mode: Option<u32>,
}

struct RemoteFile {
    path: String,
    relative: String,
    size: u64,
}

struct RemoteWalk {
    /// Parents before their contents, starting with the root itself.
    folders: Vec<RemoteFolder>,
    files: Vec<RemoteFile>,
    /// Links are never followed: one could loop, or lead a transfer out of the folder.
    others: Vec<String>,
    unreadable: Vec<String>,
}

async fn walk_remote(
    sftp: &SftpSession,
    root: &str,
    cancel: &CancellationToken,
) -> Result<RemoteWalk, FileError> {
    let mut walk = RemoteWalk {
        folders: vec![RemoteFolder {
            path: root.to_owned(),
            relative: String::new(),
            mode: None,
        }],
        files: Vec::new(),
        others: Vec::new(),
        unreadable: Vec::new(),
    };
    let mut next = 0;
    while next < walk.folders.len() {
        if cancel.is_cancelled() {
            return Err(FileError::Cancelled);
        }
        let (path, relative) = {
            let folder = &walk.folders[next];
            (folder.path.clone(), folder.relative.clone())
        };
        next += 1;
        let read = match sftp.read_dir(path.clone()).await {
            Ok(read) => read,
            Err(e) if path == root => return Err(file_error(root, e)),
            Err(e) => {
                walk.unreadable.push(file_error(&path, e).to_string());
                continue;
            }
        };
        for entry in read {
            let name = entry.file_name();
            if check_remote_name(&name).is_err() {
                walk.unreadable.push(unsafe_name(&name).to_string());
                continue;
            }
            let full = remote_join(&path, &name);
            let inner = if relative.is_empty() {
                name
            } else {
                format!("{relative}/{name}")
            };
            let meta = entry.metadata();
            let kind = entry.file_type();
            if kind.is_dir() {
                walk.folders.push(RemoteFolder {
                    path: full,
                    relative: inner,
                    mode: meta.permissions,
                });
            } else if kind.is_file() {
                walk.files.push(RemoteFile {
                    path: full,
                    relative: inner,
                    size: meta.size.unwrap_or(0),
                });
            } else {
                walk.others.push(full);
            }
        }
    }
    Ok(walk)
}

struct LocalFile {
    path: PathBuf,
    relative: String,
    size: u64,
}

struct LocalWalk {
    mode: Option<u32>,
    /// Parents before their contents, without the root.
    folders: Vec<(String, Option<u32>)>,
    files: Vec<LocalFile>,
    others: u32,
    unreadable: Vec<String>,
}

#[cfg(unix)]
fn local_mode(meta: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(meta.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn local_mode(_: &std::fs::Metadata) -> Option<u32> {
    None
}

async fn walk_local(root: &Path, cancel: &CancellationToken) -> Result<LocalWalk, FileError> {
    let top = tokio::fs::metadata(root)
        .await
        .map_err(|e| local_error(root, e))?;
    let mut walk = LocalWalk {
        mode: local_mode(&top),
        folders: Vec::new(),
        files: Vec::new(),
        others: 0,
        unreadable: Vec::new(),
    };
    let mut waiting = vec![(root.to_owned(), String::new())];
    while let Some((path, relative)) = waiting.pop() {
        if cancel.is_cancelled() {
            return Err(FileError::Cancelled);
        }
        let mut read = match tokio::fs::read_dir(&path).await {
            Ok(read) => read,
            Err(e) if relative.is_empty() => return Err(local_error(root, e)),
            Err(e) => {
                walk.unreadable.push(local_error(&path, e).to_string());
                continue;
            }
        };
        loop {
            let entry = match read.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => {
                    walk.unreadable.push(local_error(&path, e).to_string());
                    break;
                }
            };
            let full = entry.path();
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                let problem = format!("{}: this name can't be sent as text.", full.display());
                walk.unreadable.push(problem);
                continue;
            };
            let inner = if relative.is_empty() {
                name
            } else {
                format!("{relative}/{name}")
            };
            // Not following links, for the reason in `RemoteWalk`.
            let meta = match tokio::fs::symlink_metadata(&full).await {
                Ok(meta) => meta,
                Err(e) => {
                    walk.unreadable.push(local_error(&full, e).to_string());
                    continue;
                }
            };
            if meta.is_dir() {
                walk.folders.push((inner.clone(), local_mode(&meta)));
                waiting.push((full, inner));
            } else if meta.is_file() {
                walk.files.push(LocalFile {
                    path: full,
                    relative: inner,
                    size: meta.len(),
                });
            } else {
                walk.others += 1;
            }
        }
    }
    Ok(walk)
}

/// The owner keeps full access, or a read-only folder couldn't be filled.
const OWNER_ALL: u32 = 0o700;

async fn make_local_folder(path: &Path, mode: Option<u32>) -> Result<(), FileError> {
    let mut builder = tokio::fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode((mode.unwrap_or(0o755) & 0o777) | OWNER_ALL);
    #[cfg(not(unix))]
    let _ = mode;
    match builder.create(path).await {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => Err(local_error(path, e)),
        _ if !path.is_dir() => Err(FileError::Exists(path.display().to_string())),
        _ => Ok(()),
    }
}

async fn make_remote_folder(
    sftp: &SftpSession,
    path: &str,
    mode: Option<u32>,
) -> Result<(), FileError> {
    match sftp.metadata(path.to_owned()).await {
        Ok(meta) if meta.file_type().is_dir() => return Ok(()),
        Ok(_) => return Err(FileError::Exists(path.to_owned())),
        Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => {}
        Err(e) => return Err(file_error(path, e)),
    }
    sftp.create_dir(path.to_owned())
        .await
        .map_err(|e| file_error(path, e))?;
    if let Some(mode) = mode {
        let mut attributes = FileAttributes::empty();
        attributes.permissions = Some(mode | OWNER_ALL);
        sftp.set_metadata(path.to_owned(), attributes)
            .await
            .map_err(|e| file_error(path, e))?;
    }
    Ok(())
}

async fn download_file(
    sftp: &SftpSession,
    remote: &str,
    local: &Path,
    job: Job<'_>,
) -> Result<(), FileError> {
    let meta = sftp
        .metadata(remote.to_owned())
        .await
        .map_err(|e| file_error(remote, e))?;
    let source = sftp
        .open(remote.to_owned())
        .await
        .map_err(|e| file_error(remote, e))?;

    let directory = local.parent().unwrap_or(Path::new("."));
    let part = tempfile::NamedTempFile::new_in(directory).map_err(|e| local_error(directory, e))?;
    let target = tokio::fs::File::from_std(part.reopen().map_err(|e| local_error(local, e))?);
    copy(source, target, meta.size.unwrap_or(0), job).await?;

    // Private stays private, and nothing a server sent becomes executable by downloading it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions.unwrap_or(0o600) & 0o644;
        std::fs::set_permissions(part.path(), std::fs::Permissions::from_mode(mode | 0o600))
            .map_err(|e| local_error(local, e))?;
    }
    part.persist(local)
        .map_err(|e| local_error(local, e.error))?;
    Ok(())
}

async fn upload_file(
    sftp: &SftpSession,
    local: &Path,
    remote: &str,
    job: Job<'_>,
) -> Result<(), FileError> {
    let source = tokio::fs::File::open(local)
        .await
        .map_err(|e| local_error(local, e))?;
    let meta = source.metadata().await.map_err(|e| local_error(local, e))?;

    // Set when the part file is made, so a private file is never readable on the server.
    let mut attributes = FileAttributes::empty();
    attributes.permissions = local_mode(&meta);
    let part = format!("{remote}.ela-part");
    let target = sftp
        .open_with_flags_and_attributes(
            part.clone(),
            OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            attributes,
        )
        .await
        .map_err(|e| file_error(remote, e))?;
    let copied = copy(source, target, meta.len(), job).await;
    let moved = match copied {
        Ok(()) => replace_with_part(sftp, &part, remote).await,
        Err(e) => Err(e),
    };
    if moved.is_err() {
        let _ = sftp.remove_file(part).await;
    }
    moved
}

/// Plain SFTP rename refuses an existing target, so the old file has to go first. The part file
/// takes on its permissions, or a replaced script would stop being executable.
async fn replace_with_part(sftp: &SftpSession, part: &str, remote: &str) -> Result<(), FileError> {
    if let Ok(old) = sftp.metadata(remote.to_owned()).await {
        if let Some(permissions) = old.permissions {
            let mut mode = FileAttributes::empty();
            mode.permissions = Some(permissions);
            sftp.set_metadata(part.to_owned(), mode)
                .await
                .map_err(|e| file_error(remote, e))?;
        }
        sftp.remove_file(remote.to_owned())
            .await
            .map_err(|e| file_error(remote, e))?;
    }
    sftp.rename(part.to_owned(), remote.to_owned())
        .await
        .map_err(|e| file_error(remote, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_servers_folder_names_stay_inside_the_local_folder() {
        assert_eq!(local_relative("a/b c"), Some(PathBuf::from("a/b c")));
        for bad in ["a/../b", "..", "a\\b", "a/line\nbreak", "a//b"] {
            assert_eq!(local_relative(bad), None, "{bad}");
        }
    }
}
