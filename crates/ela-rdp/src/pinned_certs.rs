use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ela_session::{KnownHost, KnownHostList, SessionError, rewrite_keeping_backup};
use sha2::{Digest, Sha256};

pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("ela").join("known_hosts"))
}

/// The form `openssl x509 -fingerprint -sha256` prints, so it can be compared by eye.
pub(crate) fn fingerprint(der: &[u8]) -> String {
    let hex: Vec<String> = Sha256::digest(der)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect();
    format!("SHA256:{}", hex.join(":"))
}

fn entry_name(host: &str, port: u16) -> String {
    format!("[{host}]:{port}")
}

/// The fingerprint saved for this server. A file that can't be read is an error, not an
/// empty list, so a broken file can't turn a changed certificate into a first connection.
pub(crate) fn saved(path: &Path, host: &str, port: u16) -> Result<Option<String>, SessionError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(SessionError::protocol(format!(
                "Can't read {}: {e}",
                path.display()
            )));
        }
    };
    let name = entry_name(host, port);
    Ok(text.lines().find_map(|line| match line.rsplit_once(' ') {
        Some((entry, fingerprint)) if entry == name => Some(fingerprint.to_owned()),
        _ => None,
    }))
}

pub(crate) fn save(
    path: &Path,
    host: &str,
    port: u16,
    fingerprint: &str,
) -> Result<(), SessionError> {
    write_entry(path, host, port, fingerprint)
        .map_err(|e| SessionError::protocol(format!("Can't update {}: {e}", path.display())))
}

fn write_entry(path: &Path, host: &str, port: u16, fingerprint: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let name = entry_name(host, port);
    let mut kept = String::with_capacity(text.len() + name.len() + fingerprint.len() + 2);
    for line in text.lines() {
        if line.rsplit_once(' ').is_none_or(|(entry, _)| entry != name) {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    kept.push_str(&format!("{name} {fingerprint}\n"));

    // A temp file starts private to the user, and the rename can't leave half a file.
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    temp.write_all(kept.as_bytes())?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub fn list(path: &Path) -> Result<KnownHostList, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(KnownHostList::default()),
        Err(e) => return Err(format!("Can't read {}: {e}", path.display())),
    };
    let mut found = KnownHostList::default();
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, fingerprint)) = line.rsplit_once(' ') else {
            found.other_lines += 1;
            continue;
        };
        let entry = KnownHost {
            name: name.to_owned(),
            hashed: false,
            known_as: None,
            algorithm: "certificate".to_owned(),
            fingerprint: fingerprint.to_owned(),
        };
        if !found.entries.contains(&entry) {
            found.entries.push(entry);
        }
    }
    Ok(found)
}

pub fn remove(path: &Path, name: &str, fingerprint: &str) -> Result<(), String> {
    let gone = || format!("{name} is no longer saved in {}.", path.display());
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(gone()),
        Err(e) => return Err(format!("Can't read {}: {e}", path.display())),
    };
    let entry = format!("{name} {fingerprint}");
    let kept: String = text
        .split_inclusive('\n')
        .filter(|line| line.trim_end() != entry)
        .collect();
    if kept.len() == text.len() {
        return Err(gone());
    }
    rewrite_keeping_backup(path, &kept).map_err(|e| format!("Can't update {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join("known_hosts");
        (dir, path)
    }

    #[test]
    fn fingerprints_look_like_openssl_prints_them() {
        let printed = fingerprint(b"abc");
        assert!(printed.starts_with("SHA256:BA:78:16:BF:"), "{printed}");
        assert!(printed.ends_with(":F2:00:15:AD"), "{printed}");
    }

    #[test]
    fn a_missing_file_means_nothing_is_saved() {
        let (_dir, path) = store();
        assert_eq!(saved(&path, "db", 3389).unwrap(), None);
    }

    #[test]
    fn an_unreadable_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(saved(dir.path(), "db", 3389).is_err());
    }

    #[test]
    fn entries_are_kept_per_host_and_port() {
        let (_dir, path) = store();
        save(&path, "db", 3389, "SHA256:AA").unwrap();
        save(&path, "db", 3390, "SHA256:BB").unwrap();
        save(&path, "other", 3389, "SHA256:CC").unwrap();
        assert_eq!(
            saved(&path, "db", 3389).unwrap().as_deref(),
            Some("SHA256:AA")
        );
        assert_eq!(
            saved(&path, "db", 3390).unwrap().as_deref(),
            Some("SHA256:BB")
        );
        assert_eq!(saved(&path, "db", 338).unwrap(), None);
    }

    #[test]
    fn saving_again_replaces_only_that_entry() {
        let (_dir, path) = store();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "# a note\n[db]:3389 SHA256:OLD\n[other]:3389 SHA256:CC\n",
        )
        .unwrap();
        save(&path, "db", 3389, "SHA256:NEW").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# a note\n[other]:3389 SHA256:CC\n[db]:3389 SHA256:NEW\n"
        );
    }

    #[test]
    fn lists_entries_once_and_counts_lines_it_cant_read() {
        let (_dir, path) = store();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "# a note\n\n[db]:3389 SHA256:AA\n[db]:3389 SHA256:AA\nbroken\n",
        )
        .unwrap();
        let found = list(&path).unwrap();
        assert_eq!(found.other_lines, 1);
        assert_eq!(
            found.entries,
            [KnownHost {
                name: "[db]:3389".to_owned(),
                hashed: false,
                known_as: None,
                algorithm: "certificate".to_owned(),
                fingerprint: "SHA256:AA".to_owned(),
            }]
        );
    }

    #[test]
    fn a_missing_file_lists_nothing_and_an_unreadable_one_is_an_error() {
        let (_dir, path) = store();
        assert_eq!(list(&path).unwrap(), KnownHostList::default());
        assert!(list(path.parent().unwrap().parent().unwrap()).is_err());
    }

    #[test]
    fn removing_touches_only_that_entry_and_keeps_a_backup() {
        let (_dir, path) = store();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = "# a note\n[db]:3389 SHA256:AA\n[db]:3390 SHA256:BB\n";
        fs::write(&path, original).unwrap();

        remove(&path, "[db]:3389", "SHA256:AA").unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# a note\n[db]:3390 SHA256:BB\n"
        );
        assert_eq!(saved(&path, "db", 3389).unwrap(), None);
        assert_eq!(
            fs::read_to_string(path.with_extension("old")).unwrap(),
            original
        );
    }

    #[test]
    fn an_entry_already_gone_or_replaced_leaves_the_file_alone() {
        let (_dir, path) = store();
        assert!(remove(&path, "[db]:3389", "SHA256:AA").is_err());

        save(&path, "db", 3389, "SHA256:NEW").unwrap();
        assert!(remove(&path, "[db]:3389", "SHA256:OLD").is_err());
        assert_eq!(
            saved(&path, "db", 3389).unwrap().as_deref(),
            Some("SHA256:NEW")
        );
        assert!(!path.with_extension("old").exists());
    }

    #[cfg(unix)]
    #[test]
    fn removing_keeps_a_symlink_and_the_file_mode() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("real");
        fs::write(&target, "[db]:3389 SHA256:AA\n[b]:3389 SHA256:BB\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.path().join("known_hosts");
        symlink(&target, &link).unwrap();

        remove(&link, "[db]:3389", "SHA256:AA").unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "[b]:3389 SHA256:BB\n");
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_new_file_is_private() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, path) = store();
        save(&path, "db", 3389, "SHA256:AA").unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
