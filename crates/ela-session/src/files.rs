use std::path::Path;

use async_trait::async_trait;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RemoteKind {
    Folder,
    File,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    /// What a link points to, so a link to a folder opens like a folder.
    pub kind: RemoteKind,
    pub link: bool,
    pub size: u64,
    pub modified: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct RemoteListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<RemoteEntry>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FileError {
    #[error("Cancelled.")]
    Cancelled,
    #[error("{0} doesn't exist.")]
    NotFound(String),
    #[error("{0} already exists.")]
    Exists(String),
    #[error("“{0}” can't be used as a name.")]
    BadName(String),
    #[error("No permission to use {0}.")]
    Denied(String),
    #[error("This server doesn't offer SFTP: {0}")]
    NoSftp(String),
    #[error("{0}")]
    Other(String),
}

pub struct Transfer {
    pub cancel: CancellationToken,
    pub progress: Box<dyn Fn(u64, u64) + Send + Sync>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderOutcome {
    pub files: u32,
    pub left_out: u32,
    pub failed: Vec<String>,
}

impl FolderOutcome {
    pub fn left_out_text(&self) -> Option<String> {
        match self.left_out {
            0 => None,
            1 => Some("1 link or special file was left out.".to_owned()),
            many => Some(format!("{many} links or special files were left out.")),
        }
    }

    pub fn failure_text(&self) -> Option<String> {
        let first = self.failed.first()?;
        let attempted = self.files as usize + self.failed.len();
        let mut text = format!(
            "{} of {attempted} items failed. First: {first}",
            self.failed.len()
        );
        if let Some(left_out) = self.left_out_text() {
            text.push(' ');
            text.push_str(&left_out);
        }
        Some(text)
    }
}

/// A failed or cancelled transfer leaves whatever was at the destination as it was. A folder is
/// merged into one of the same name, one whole file at a time, so stopping keeps finished files.
#[async_trait]
pub trait RemoteFiles: Send + Sync {
    /// `None` is where the server starts a login, usually the home folder.
    async fn list(&self, path: Option<String>) -> Result<RemoteListing, FileError>;
    async fn exists(&self, path: &str) -> Result<bool, FileError>;
    async fn download(
        &self,
        remote: &str,
        local: &Path,
        transfer: &Transfer,
    ) -> Result<(), FileError>;
    async fn upload(
        &self,
        local: &Path,
        remote: &str,
        transfer: &Transfer,
    ) -> Result<(), FileError>;
    async fn download_folder(
        &self,
        remote: &str,
        local: &Path,
        transfer: &Transfer,
    ) -> Result<FolderOutcome, FileError>;
    async fn upload_folder(
        &self,
        local: &Path,
        remote: &str,
        transfer: &Transfer,
    ) -> Result<FolderOutcome, FileError>;
    /// Never overwrites: a name already taken is `FileError::Exists`.
    async fn rename(&self, path: &str, name: &str) -> Result<(), FileError>;
    async fn make_folder(&self, directory: &str, name: &str) -> Result<(), FileError>;
    /// A link goes on its own. What it points to stays, inside a folder as well.
    async fn delete(&self, path: &str) -> Result<(), FileError>;
}

pub fn check_remote_name(name: &str) -> Result<(), FileError> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
        return Err(FileError::BadName(name.to_owned()));
    }
    Ok(())
}

pub fn remote_join(directory: &str, name: &str) -> String {
    if directory.ends_with('/') {
        format!("{directory}{name}")
    } else {
        format!("{directory}/{name}")
    }
}

pub fn remote_parent(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        return None;
    }
    match path.rfind('/') {
        Some(0) => Some("/".to_owned()),
        Some(at) => Some(path[..at].to_owned()),
        None => None,
    }
}

/// A server chooses these names, so one can't be allowed to point outside the local folder.
pub fn local_file_name(remote: &str) -> String {
    let name = remote
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    let name: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '\\')
        .collect();
    if name.is_empty() || name == "." || name == ".." {
        "download".to_owned()
    } else {
        name
    }
}

pub fn sort_entries(entries: &mut [RemoteEntry]) {
    entries.sort_by_cached_key(|entry| {
        (
            entry.kind != RemoteKind::Folder,
            entry.name.to_lowercase(),
            entry.name.clone(),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joining_and_parents_handle_the_root() {
        assert_eq!(remote_join("/", "etc"), "/etc");
        assert_eq!(remote_join("/home/me", "a b"), "/home/me/a b");
        assert_eq!(remote_parent("/home/me"), Some("/home".into()));
        assert_eq!(remote_parent("/home"), Some("/".into()));
        assert_eq!(remote_parent("/home/me/"), Some("/home".into()));
        assert_eq!(remote_parent("/"), None);
    }

    #[test]
    fn a_servers_name_stays_inside_the_local_folder() {
        assert_eq!(local_file_name("/srv/dump.sql"), "dump.sql");
        assert_eq!(local_file_name("../../.bashrc"), ".bashrc");
        assert_eq!(local_file_name("/srv/.."), "download");
        assert_eq!(local_file_name("/"), "download");
        assert_eq!(local_file_name("a\\..\\b"), "a..b");
        assert_eq!(local_file_name("line\nbreak"), "linebreak");
    }

    #[test]
    fn a_new_name_stays_in_its_folder() {
        for bad in ["", ".", "..", "a/b", "/", "../x", "nul\0"] {
            assert_eq!(
                check_remote_name(bad),
                Err(FileError::BadName(bad.to_owned()))
            );
        }
        for good in ["a", ".hidden", "..a", "with space", "a\\b"] {
            assert_eq!(check_remote_name(good), Ok(()));
        }
    }

    #[test]
    fn a_folder_outcome_says_what_went_wrong() {
        let clean = FolderOutcome {
            files: 3,
            ..Default::default()
        };
        assert_eq!(clean.left_out_text(), None);
        assert_eq!(clean.failure_text(), None);

        let links = FolderOutcome {
            files: 3,
            left_out: 2,
            failed: vec![],
        };
        assert_eq!(
            links.left_out_text().unwrap(),
            "2 links or special files were left out."
        );

        let failed = FolderOutcome {
            files: 3,
            left_out: 1,
            failed: vec!["No permission to use /a.".into(), "other".into()],
        };
        assert_eq!(
            failed.failure_text().unwrap(),
            "2 of 5 items failed. First: No permission to use /a. \
             1 link or special file was left out."
        );
    }

    #[test]
    fn folders_come_first_then_names_ignoring_case() {
        let entry = |name: &str, kind| RemoteEntry {
            name: name.into(),
            path: name.into(),
            kind,
            link: false,
            size: 0,
            modified: None,
        };
        let mut entries = [
            entry("b.txt", RemoteKind::File),
            entry("Z", RemoteKind::Folder),
            entry("A.txt", RemoteKind::File),
            entry("a", RemoteKind::Folder),
        ];
        sort_entries(&mut entries);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a", "Z", "A.txt", "b.txt"]);
    }
}
