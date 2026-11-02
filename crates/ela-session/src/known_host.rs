use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct KnownHost {
    pub name: String,
    pub hashed: bool,
    pub known_as: Option<String>,
    pub algorithm: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownHostList {
    pub entries: Vec<KnownHost>,
    pub other_lines: u32,
}

/// Leaves the previous file next to it as `<name>.old`, like `ssh-keygen -R`.
pub fn rewrite_keeping_backup(path: &Path, text: &str) -> io::Result<()> {
    // A dotfile manager may keep the file behind a symlink; the rename below must not
    // replace the link itself.
    let path = fs::canonicalize(path)?;
    fs::copy(&path, path.with_extension("old"))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    temp.write_all(text.as_bytes())?;
    fs::set_permissions(temp.path(), fs::metadata(&path)?.permissions())?;
    temp.persist(&path).map_err(|e| e.error)?;
    Ok(())
}
