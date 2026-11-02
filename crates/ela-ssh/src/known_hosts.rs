//! Reads and changes the known_hosts files the way OpenSSH does, with one parser for checking a
//! server, learning and replacing its key, and the Known Hosts view. Replacing a saved key edits
//! a file other tools rely on, so it only touches entries for this host and keeps a backup.

use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use data_encoding::BASE64;
use ela_session::{KnownHost, KnownHostList, SessionError, rewrite_keeping_backup};
use hmac::{Hmac, KeyInit, Mac};
use russh::Preferred;
use russh::keys::ssh_key::certificate::CertType;
use russh::keys::{
    Algorithm, Certificate, HashAlg, PublicKey, PublicKeyOrCertificate, parse_public_key_base64,
};
use sha1::Sha1;

pub fn default_path() -> Option<PathBuf> {
    std::env::home_dir().map(|home| home.join(".ssh").join("known_hosts"))
}

#[derive(Debug, Clone)]
pub(crate) struct HostFiles {
    pub user: PathBuf,
    pub read_only: Vec<PathBuf>,
}

impl HostFiles {
    pub(crate) fn openssh(user: PathBuf) -> Self {
        let mut second = user.clone().into_os_string();
        second.push("2");
        Self {
            user,
            read_only: vec![
                PathBuf::from(second),
                PathBuf::from("/etc/ssh/ssh_known_hosts"),
                PathBuf::from("/etc/ssh/ssh_known_hosts2"),
            ],
        }
    }

    pub(crate) fn only(user: PathBuf) -> Self {
        Self {
            user,
            read_only: Vec::new(),
        }
    }

    fn all(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(self.user.as_path()).chain(self.read_only.iter().map(PathBuf::as_path))
    }

    fn entries_for(&self, host: &str, port: u16) -> Result<Vec<Owned>, SessionError> {
        let name = as_written(&host.to_lowercase(), port);
        let mut found = Vec::new();
        for path in self.all() {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(unreadable(path)(e)),
            };
            for line in text.lines() {
                if let Line::Entry(entry) = parse_line(line)
                    && names_match(entry.hosts, &name)
                {
                    found.push(Owned {
                        marker: entry.marker,
                        key: entry.key,
                    });
                }
            }
        }
        Ok(found)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    Known,
    Revoked,
    /// `known_types` lists the other key types saved for the host; empty means a first connection.
    New {
        key: PublicKey,
        known_types: Vec<String>,
    },
    Changed {
        key: PublicKey,
        saved: Vec<PublicKey>,
    },
}

struct Owned {
    marker: Marker,
    key: PublicKey,
}

pub(crate) fn verdict(
    files: &HostFiles,
    host: &str,
    port: u16,
    presented: &PublicKeyOrCertificate,
) -> Result<Verdict, SessionError> {
    let entries = files.entries_for(host, port)?;
    let revoked = |key: &PublicKey| {
        entries
            .iter()
            .any(|e| e.marker == Marker::Revoked && same_key(&e.key, key))
    };
    let key = match presented {
        PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
        PublicKeyOrCertificate::Certificate(cert) => {
            let key = PublicKey::from(cert.public_key().clone());
            let ca = PublicKey::from(cert.signature_key().clone());
            if revoked(&key) || revoked(&ca) {
                return Ok(Verdict::Revoked);
            }
            let trusted_ca = entries
                .iter()
                .any(|e| e.marker == Marker::CertAuthority && same_key(&e.key, &ca));
            if trusted_ca && certifies(cert, &ca, host, SystemTime::now()) {
                return Ok(Verdict::Known);
            }
            // As OpenSSH does when no authority vouches for it: judge the key inside.
            key
        }
    };
    if revoked(&key) {
        return Ok(Verdict::Revoked);
    }
    let plain: Vec<&PublicKey> = entries
        .iter()
        .filter(|e| e.marker == Marker::None)
        .map(|e| &e.key)
        .collect();
    // Any copy that matches wins, so a stale entry elsewhere (another file, a pattern line)
    // doesn't block a key the user accepted.
    if plain.iter().any(|saved| same_key(saved, &key)) {
        return Ok(Verdict::Known);
    }
    let saved: Vec<PublicKey> = plain
        .iter()
        .filter(|saved| saved.algorithm() == key.algorithm())
        .map(|saved| (*saved).clone())
        .collect();
    if !saved.is_empty() {
        return Ok(Verdict::Changed { key, saved });
    }
    let mut known_types: Vec<String> = Vec::new();
    for saved in plain {
        let name = saved.algorithm().as_str().to_owned();
        if !known_types.contains(&name) {
            known_types.push(name);
        }
    }
    Ok(Verdict::New { key, known_types })
}

fn same_key(a: &PublicKey, b: &PublicKey) -> bool {
    a.key_data() == b.key_data()
}

/// What OpenSSH checks of a host certificate besides the authority: its type, its validity,
/// that it names this host (or no host at all), and no critical options, since host
/// certificates define none.
fn certifies(cert: &Certificate, ca: &PublicKey, host: &str, now: SystemTime) -> bool {
    let now = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let fingerprint = ca.fingerprint(HashAlg::Sha256);
    let principals = cert.valid_principals();
    cert.cert_type() == CertType::Host
        && cert.validate_at(now, [&fingerprint]).is_ok()
        && cert.critical_options().is_empty()
        && (principals.is_empty() || principals.iter().any(|p| p.eq_ignore_ascii_case(host)))
}

/// Puts the key types already saved for the host first, as OpenSSH orders `HostKeyAlgorithms`,
/// so a host known by its ECDSA key isn't asked to prove itself with a new Ed25519 one. Host
/// certificates are asked for only where an authority could vouch for them.
pub(crate) fn preferred(files: &HostFiles, host: &str, port: u16) -> Preferred {
    let entries = files.entries_for(host, port).unwrap_or_default();
    let known: Vec<Algorithm> = entries
        .iter()
        .filter(|e| e.marker == Marker::None)
        .map(|e| e.key.algorithm())
        .collect();
    let order = order_keys(&Preferred::DEFAULT.key, &known);
    let covered = entries.iter().any(|e| e.marker == Marker::CertAuthority);
    Preferred {
        host_key_certificates: if covered {
            Cow::Owned(order.clone())
        } else {
            Cow::Borrowed(&[])
        },
        key: Cow::Owned(order),
        ..Preferred::DEFAULT
    }
}

fn order_keys(default: &[Algorithm], known: &[Algorithm]) -> Vec<Algorithm> {
    let is_known = |algorithm: &Algorithm| {
        known.iter().any(|k| match (k, algorithm) {
            // A saved `ssh-rsa` key signs with any of the RSA hashes.
            (Algorithm::Rsa { .. }, Algorithm::Rsa { .. }) => true,
            (k, a) => k == a,
        })
    };
    let (mut first, rest): (Vec<_>, Vec<_>) = default.iter().cloned().partition(is_known);
    first.extend(rest);
    first
}

/// Hashes the new name when every entry in the file is already hashed, as `HashKnownHosts yes`
/// (Debian's and Ubuntu's default) leaves it.
pub(crate) fn learn(
    files: &HostFiles,
    host: &str,
    port: u16,
    key: &PublicKey,
) -> Result<(), SessionError> {
    let path = files.user.as_path();
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(unreadable(path)(e)),
    };
    let name = as_written(&host.to_lowercase(), port);
    let name = if all_hashed(&text) {
        hash_name(&name)?
    } else {
        name
    };
    let encoded = key.to_openssh().map_err(unwritable(path))?;
    let mut line = String::new();
    if !text.is_empty() && !text.ends_with('\n') {
        line.push('\n');
    }
    line.push_str(&format!("{name} {encoded}\n"));
    append(path, &line).map_err(unwritable(path))
}

fn append(path: &Path, line: &str) -> io::Result<()> {
    if let Some(dir) = path.parent()
        && !dir.exists()
    {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(dir)?;
    }
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?;
    file.write_all(line.as_bytes())?;
    file.sync_all()
}

fn all_hashed(text: &str) -> bool {
    let mut any = false;
    for line in text.lines() {
        if let Line::Entry(Entry {
            marker: Marker::None,
            hosts,
            ..
        }) = parse_line(line)
        {
            if !hosts.split(',').all(|name| name.starts_with("|1|")) {
                return false;
            }
            any = true;
        }
    }
    any
}

fn hash_name(name: &str) -> Result<String, SessionError> {
    let mut salt = [0u8; 20];
    getrandom::fill(&mut salt).map_err(SessionError::protocol)?;
    let mac = Hmac::<Sha1>::new_from_slice(&salt)
        .map_err(SessionError::protocol)?
        .chain_update(name)
        .finalize()
        .into_bytes();
    Ok(format!(
        "|1|{}|{}",
        BASE64.encode(&salt),
        BASE64.encode(&mac)
    ))
}

/// Leaves a copy of the old file next to it as `known_hosts.old`, like `ssh-keygen -R`. Only names
/// written for exactly this host go: a pattern also covers other hosts, and the new entry wins over
/// it anyway.
pub(crate) fn replace(
    files: &HostFiles,
    host: &str,
    port: u16,
    key: &PublicKey,
) -> Result<(), SessionError> {
    let path = fs::canonicalize(&files.user).map_err(unreadable(&files.user))?;
    let text = fs::read_to_string(&path).map_err(unreadable(&path))?;
    let name = as_written(&host.to_lowercase(), port);
    let mut kept = String::with_capacity(text.len());
    let mut changed = false;
    for line in text.split_inclusive('\n') {
        let hosts = match parse_line(line) {
            Line::Entry(Entry {
                marker: Marker::None,
                hosts,
                key: saved,
            }) if saved.algorithm() == key.algorithm() && !same_key(&saved, key) => hosts,
            _ => {
                kept.push_str(line);
                continue;
            }
        };
        let names: Vec<&str> = hosts.split(',').collect();
        let left: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| !names_this_host(n, &name))
            .collect();
        if left.len() == names.len() {
            kept.push_str(line);
            continue;
        }
        changed = true;
        if left.iter().any(|n| !is_negated(n)) {
            let rest = &line.trim_start()[hosts.len()..];
            kept.push_str(&left.join(","));
            kept.push_str(rest);
        }
    }
    if changed {
        rewrite_keeping_backup(&path, &kept).map_err(unwritable(&path))?;
    }
    let files = HostFiles {
        user: path.clone(),
        read_only: files.read_only.clone(),
    };
    learn(&files, host, port, key)?;
    let presented = PublicKeyOrCertificate::PublicKey {
        key: key.clone(),
        hash_alg: None,
    };
    match verdict(&files, host, port, &presented)? {
        Verdict::Known => Ok(()),
        _ => Err(SessionError::protocol(format!(
            "{} still doesn't list the new key",
            path.display()
        ))),
    }
}

fn names_this_host(name: &str, written: &str) -> bool {
    if name.starts_with("|1|") {
        return hash_names(name, written);
    }
    name.eq_ignore_ascii_case(written)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marker {
    None,
    Revoked,
    CertAuthority,
}

struct Entry<'a> {
    marker: Marker,
    hosts: &'a str,
    key: PublicKey,
}

enum Line<'a> {
    NotAnEntry,
    Other,
    Entry(Entry<'a>),
}

fn parse_line(line: &str) -> Line<'_> {
    let line = line.trim_start();
    if line.is_empty() || line.starts_with('#') {
        return Line::NotAnEntry;
    }
    let mut fields = line.split_ascii_whitespace();
    let marker = match line.split_ascii_whitespace().next() {
        Some("@revoked") => Marker::Revoked,
        Some("@cert-authority") => Marker::CertAuthority,
        Some(other) if other.starts_with('@') => return Line::Other,
        _ => Marker::None,
    };
    if marker != Marker::None {
        fields.next();
    }
    let (Some(hosts), Some(_), Some(encoded)) = (fields.next(), fields.next(), fields.next())
    else {
        return Line::Other;
    };
    match parse_public_key_base64(encoded) {
        Ok(key) => Line::Entry(Entry { marker, hosts, key }),
        Err(_) => Line::Other,
    }
}

/// OpenSSH's `match_hostname`: any pattern may match, but a matching negation rules the line out.
fn names_match(hosts: &str, name: &str) -> bool {
    let mut matched = false;
    for pattern in hosts.split(',') {
        if pattern.starts_with("|1|") {
            matched |= hash_names(pattern, name);
        } else if let Some(negated) = pattern.strip_prefix('!') {
            if wildcard(&negated.to_lowercase(), name) {
                return false;
            }
        } else if wildcard(&pattern.to_lowercase(), name) {
            matched = true;
        }
    }
    matched
}

pub(crate) fn wildcard(pattern: &str, text: &str) -> bool {
    let (pattern, text) = (pattern.as_bytes(), text.as_bytes());
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some(b'*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(&c) if c == b'?' || c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((sp, st)) => {
                    p = sp + 1;
                    t = st + 1;
                    star = Some((sp, st + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

fn fingerprint_of(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

fn is_negated(name: &str) -> bool {
    name.starts_with('!')
}

fn as_written(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
    }
}

fn hash_names(hashed: &str, name: &str) -> bool {
    let Some((salt, hash)) = hashed
        .strip_prefix("|1|")
        .and_then(|rest| rest.split_once('|'))
    else {
        return false;
    };
    let (Ok(salt), Ok(hash)) = (
        BASE64.decode(salt.as_bytes()),
        BASE64.decode(hash.as_bytes()),
    ) else {
        return false;
    };
    Hmac::<Sha1>::new_from_slice(&salt)
        .is_ok_and(|mac| mac.chain_update(name).verify_slice(&hash).is_ok())
}

/// Lines with a marker are counted with the ones Ela can't read: the view manages keys, and
/// dropping an `@revoked` line would trust its key again.
pub fn list(path: &Path, saved_hosts: &[(String, u16)]) -> Result<KnownHostList, String> {
    let saved_names: Vec<String> = saved_hosts
        .iter()
        .map(|(host, port)| as_written(host, *port))
        .collect();
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(KnownHostList::default()),
        Err(e) => return Err(format!("Can't read {}: {e}", path.display())),
    };
    let mut found = KnownHostList::default();
    let mut seen = HashSet::new();
    for line in text.lines() {
        let (hosts, key) = match parse_line(line) {
            Line::NotAnEntry => continue,
            Line::Entry(Entry {
                marker: Marker::None,
                hosts,
                key,
            }) => (hosts, key),
            _ => {
                found.other_lines += 1;
                continue;
            }
        };
        let fingerprint = fingerprint_of(&key);
        for name in hosts.split(',').filter(|name| !is_negated(name)) {
            if seen.insert((name.to_owned(), fingerprint.clone())) {
                let hashed = name.starts_with('|');
                // A hash can only be checked against a name, never read back, so the names
                // to try come from saved Connections.
                let known_as = hashed
                    .then(|| saved_names.iter().find(|saved| hash_names(name, saved)))
                    .flatten()
                    .cloned();
                found.entries.push(KnownHost {
                    name: name.to_owned(),
                    hashed,
                    known_as,
                    algorithm: key.algorithm().as_str().to_owned(),
                    fingerprint: fingerprint.clone(),
                });
            }
        }
    }
    Ok(found)
}

/// Named by what the entry says rather than its line number, so a file another tool changed
/// after it was listed can't make this remove the wrong line.
pub fn remove(path: &Path, name: &str, fingerprint: &str) -> Result<(), String> {
    let gone = || format!("{name} is no longer saved in {}.", path.display());
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(gone()),
        Err(e) => return Err(format!("Can't read {}: {e}", path.display())),
    };
    let mut kept = String::with_capacity(text.len());
    let mut removed = false;
    for line in text.split_inclusive('\n') {
        let hosts = match parse_line(line) {
            Line::Entry(Entry {
                marker: Marker::None,
                hosts,
                key,
            }) if fingerprint_of(&key) == fingerprint => hosts,
            _ => {
                kept.push_str(line);
                continue;
            }
        };
        let names: Vec<&str> = hosts.split(',').collect();
        if is_negated(name) || !names.contains(&name) {
            kept.push_str(line);
            continue;
        }
        removed = true;
        let left: Vec<&str> = names.iter().copied().filter(|n| *n != name).collect();
        // Negations alone match nothing, so the line has no more use.
        if left.iter().any(|n| !is_negated(n)) {
            let rest = &line.trim_start()[hosts.len()..];
            kept.push_str(&left.join(","));
            kept.push_str(rest);
        }
    }
    if !removed {
        return Err(gone());
    }
    rewrite_keeping_backup(path, &kept).map_err(|e| format!("Can't update {}: {e}", path.display()))
}

fn unreadable<E: std::fmt::Display>(path: &Path) -> impl FnOnce(E) -> SessionError {
    move |e| SessionError::protocol(format!("Can't read {}: {e}", path.display()))
}

fn unwritable<E: std::fmt::Display>(path: &Path) -> impl FnOnce(E) -> SessionError {
    move |e| SessionError::protocol(format!("Can't update {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF";
    const NEW: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
    const OTHER: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X";
    const ECDSA: &str = "AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBJ7jFsxKweTYq+aVXounTmN4gVAifKt+r4/8xTekYIUa31KFzlQrV0HPkvlfiiC5s3LRpsXCECFjGBQ6zsG4tHw=";
    const HASHED: &str = "|1|O33ESRMWPVkMYIwJ1Uw+n877jTo=|nuuC5vEqXlEZ/8BXQR7m619W6Ak=";
    // `ssh-keygen -H` of `[db.example]:2222`.
    const HASHED_PORT: &str = "|1|u6OyS7DMCH4RG8pJnAo/yWdyWX8=|xHCmx+reUcLXjeZyUL10tiXE99M=";

    fn key(encoded: &str) -> PublicKey {
        parse_public_key_base64(encoded).unwrap()
    }

    fn only(path: &Path) -> HostFiles {
        HostFiles::only(path.to_owned())
    }

    fn judge(files: &HostFiles, host: &str, port: u16, encoded: &str) -> Verdict {
        let presented = PublicKeyOrCertificate::PublicKey {
            key: key(encoded),
            hash_alg: None,
        };
        verdict(files, host, port, &presented).unwrap()
    }

    fn known(path: &Path, host: &str, port: u16, encoded: &str) -> bool {
        judge(&only(path), host, port, encoded) == Verdict::Known
    }

    fn file_with(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        fs::write(&path, text).unwrap();
        (dir, path)
    }

    fn entries(path: &Path) -> Vec<String> {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| l.split(' ').take(3).collect::<Vec<_>>().join(" "))
            .collect()
    }

    #[test]
    fn replaces_only_this_hosts_old_key() {
        let original = format!(
            "# a comment before the entry\n\
             unrelated.example ssh-ed25519 {OTHER}\n\
             [db.example]:2222 ssh-ed25519 {OLD} a comment\n\
             # a comment after it\n\
             db.example ssh-ed25519 {OLD}\n\
             [db.example]:2222 ecdsa-sha2-nistp256 {ECDSA}\n\
             sibling.example ssh-ed25519 {OLD}\n"
        );
        let (_dir, path) = file_with(&original);

        replace(&only(&path), "db.example", 2222, &key(NEW)).unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!(
                "# a comment before the entry\n\
                 unrelated.example ssh-ed25519 {OTHER}\n\
                 # a comment after it\n\
                 db.example ssh-ed25519 {OLD}\n\
                 [db.example]:2222 ecdsa-sha2-nistp256 {ECDSA}\n\
                 sibling.example ssh-ed25519 {OLD}\n\
                 [db.example]:2222 {}\n",
                key(NEW).to_openssh().unwrap()
            )
        );
        assert!(known(&path, "db.example", 2222, NEW));
        assert_eq!(
            fs::read_to_string(path.with_extension("old")).unwrap(),
            original
        );
    }

    #[test]
    fn a_line_naming_several_hosts_keeps_the_others() {
        let (_dir, path) = file_with(&format!("db.example,10.0.0.5,db ssh-ed25519 {OLD} note\n"));
        replace(&only(&path), "10.0.0.5", 22, &key(NEW)).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with(&format!("db.example,db ssh-ed25519 {OLD} note\n")),
            "{text}"
        );
        assert!(known(&path, "db.example", 22, OLD));
    }

    #[test]
    fn hashed_entries_are_found() {
        let (_dir, path) = file_with(&format!(
            "{HASHED} ssh-ed25519 {OLD}\nother.example ssh-ed25519 {OLD}\n"
        ));
        replace(&only(&path), "example.com", 22, &key(NEW)).unwrap();
        let lines = entries(&path);
        assert_eq!(lines[0], format!("other.example ssh-ed25519 {OLD}"));
        assert_eq!(lines[1], format!("example.com ssh-ed25519 {NEW}"));
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn replacing_without_an_exact_entry_adds_one_and_leaves_patterns_alone() {
        let original = format!("*.example ssh-ed25519 {OLD}\n");
        let (_dir, path) = file_with(&original);
        replace(&only(&path), "db.example", 22, &key(NEW)).unwrap();
        assert_eq!(
            entries(&path),
            [
                format!("*.example ssh-ed25519 {OLD}"),
                format!("db.example ssh-ed25519 {NEW}")
            ]
        );
        assert!(known(&path, "db.example", 22, NEW));
        assert!(!path.with_extension("old").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_and_the_file_mode_survive() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let (dir, target) = file_with(&format!("db.example ssh-ed25519 {OLD}\n"));
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.path().join("link");
        symlink(&target, &link).unwrap();

        replace(&only(&link), "db.example", 22, &key(NEW)).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(known(&target, "db.example", 22, NEW));
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    fn fingerprint(encoded: &str) -> String {
        fingerprint_of(&key(encoded))
    }

    #[test]
    fn lists_one_row_per_name_and_key() {
        let (_dir, path) = file_with(&format!(
            "# a comment\n\n\
             db.example,10.0.0.5 ssh-ed25519 {OLD} note\n\
             db.example ssh-ed25519 {OLD}\n\
             [db.example]:2222\tecdsa-sha2-nistp256 {ECDSA}\n\
             {HASHED} ssh-ed25519 {OTHER}\n"
        ));
        let found = list(&path, &[]).unwrap();
        assert_eq!(found.other_lines, 0);
        let rows: Vec<(&str, bool, &str)> = found
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.hashed, e.algorithm.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("db.example", false, "ssh-ed25519"),
                ("10.0.0.5", false, "ssh-ed25519"),
                ("[db.example]:2222", false, "ecdsa-sha2-nistp256"),
                (HASHED, true, "ssh-ed25519"),
            ]
        );
        assert_eq!(found.entries[0].fingerprint, fingerprint(OLD));
    }

    fn saved(hosts: &[(&str, u16)]) -> Vec<(String, u16)> {
        hosts
            .iter()
            .map(|(host, port)| (host.to_string(), *port))
            .collect()
    }

    #[test]
    fn a_hashed_name_is_known_as_the_saved_host_it_hashes() {
        let (_dir, path) = file_with(&format!(
            "{HASHED} ssh-ed25519 {OLD}\n\
             {HASHED_PORT} ssh-ed25519 {NEW}\n\
             |1|AAAA|AAAA ssh-ed25519 {OTHER}\n\
             example.com ssh-ed25519 {OTHER}\n"
        ));
        let found = list(
            &path,
            &saved(&[
                ("db.example", 22),
                ("db.example", 2222),
                ("example.com", 22),
            ]),
        )
        .unwrap();
        let rows: Vec<(&str, Option<&str>)> = found
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.known_as.as_deref()))
            .collect();
        assert_eq!(
            rows,
            [
                (HASHED, Some("example.com")),
                (HASHED_PORT, Some("[db.example]:2222")),
                ("|1|AAAA|AAAA", None),
                ("example.com", None),
            ]
        );
    }

    #[test]
    fn a_hashed_name_needs_the_same_port_to_be_known() {
        let (_dir, path) = file_with(&format!("{HASHED_PORT} ssh-ed25519 {NEW}\n"));
        let found = list(&path, &saved(&[("db.example", 22), ("db.example", 22222)])).unwrap();
        assert_eq!(found.entries[0].known_as, None);
    }

    #[test]
    fn a_known_hashed_name_is_still_removed_by_its_hash() {
        let (_dir, path) = file_with(&format!("{HASHED_PORT} ssh-ed25519 {NEW}\n"));
        let entry = list(&path, &saved(&[("db.example", 2222)]))
            .unwrap()
            .entries[0]
            .clone();
        assert!(remove(&path, "[db.example]:2222", &entry.fingerprint).is_err());
        remove(&path, &entry.name, &entry.fingerprint).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn lines_it_cant_manage_are_counted_and_not_listed() {
        let (_dir, path) = file_with(&format!(
            "@revoked db.example ssh-ed25519 {OLD}\n\
             @cert-authority *.example ssh-ed25519 {OTHER}\n\
             broken line\n\
             *.example,!bad.example ssh-ed25519 {NEW}\n"
        ));
        let found = list(&path, &[]).unwrap();
        assert_eq!(found.other_lines, 3);
        let names: Vec<&str> = found.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["*.example"]);
    }

    #[test]
    fn a_missing_file_lists_nothing_and_an_unreadable_one_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            list(&dir.path().join("none"), &[]).unwrap(),
            KnownHostList::default()
        );
        assert!(list(dir.path(), &[]).is_err());
    }

    #[test]
    fn removing_touches_only_that_name_and_key_and_keeps_a_backup() {
        let original = format!(
            "# a comment\n\
             db.example,10.0.0.5 ssh-ed25519 {OLD} note\n\
             db.example ecdsa-sha2-nistp256 {ECDSA}\n\
             other.example ssh-ed25519 {OTHER}\n"
        );
        let (_dir, path) = file_with(&original);

        remove(&path, "db.example", &fingerprint(OLD)).unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!(
                "# a comment\n\
                 10.0.0.5 ssh-ed25519 {OLD} note\n\
                 db.example ecdsa-sha2-nistp256 {ECDSA}\n\
                 other.example ssh-ed25519 {OTHER}\n"
            )
        );
        assert_eq!(
            fs::read_to_string(path.with_extension("old")).unwrap(),
            original
        );
    }

    #[test]
    fn after_removing_the_server_counts_as_new() {
        let (_dir, path) = file_with(&format!("[db.example]:2222 ssh-ed25519 {OLD}\n"));
        remove(&path, "[db.example]:2222", &fingerprint(OLD)).unwrap();
        assert!(!known(&path, "db.example", 2222, OLD));
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn every_copy_of_an_entry_goes() {
        let (_dir, path) = file_with(&format!(
            "db.example ssh-ed25519 {OLD}\ndb.example ssh-ed25519 {OLD}\n"
        ));
        remove(&path, "db.example", &fingerprint(OLD)).unwrap();
        assert!(list(&path, &[]).unwrap().entries.is_empty());
    }

    #[test]
    fn a_hashed_name_can_be_removed() {
        let (_dir, path) = file_with(&format!(
            "{HASHED} ssh-ed25519 {OLD}\nother.example ssh-ed25519 {OTHER}\n"
        ));
        remove(&path, HASHED, &fingerprint(OLD)).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("other.example ssh-ed25519 {OTHER}\n")
        );
    }

    #[test]
    fn a_revoked_line_is_never_removed() {
        let original = format!("@revoked db.example ssh-ed25519 {OLD}\n");
        let (_dir, path) = file_with(&original);
        assert!(remove(&path, "db.example", &fingerprint(OLD)).is_err());
        assert!(remove(&path, "@revoked", &fingerprint(OLD)).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn a_negation_cant_be_removed_and_doesnt_outlive_its_line() {
        let original = format!("*.example,!bad.example ssh-ed25519 {OLD}\n");
        let (_dir, path) = file_with(&original);
        assert!(remove(&path, "!bad.example", &fingerprint(OLD)).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);

        remove(&path, "*.example", &fingerprint(OLD)).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn an_entry_already_gone_leaves_the_file_alone() {
        let original = format!("db.example ssh-ed25519 {OLD}\n");
        let (dir, path) = file_with(&original);
        assert!(remove(&path, "db.example", &fingerprint(NEW)).is_err());
        assert!(remove(&path, "other.example", &fingerprint(OLD)).is_err());
        assert!(remove(&dir.path().join("none"), "db.example", &fingerprint(OLD)).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert!(!path.with_extension("old").exists());
    }

    #[cfg(unix)]
    #[test]
    fn removing_keeps_a_symlink_and_the_file_mode() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let (dir, target) = file_with(&format!(
            "db.example ssh-ed25519 {OLD}\nother.example ssh-ed25519 {OTHER}\n"
        ));
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.path().join("link");
        symlink(&target, &link).unwrap();

        remove(&link, "db.example", &fingerprint(OLD)).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(list(&target, &[]).unwrap().entries.len(), 1);
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn a_changed_key_names_the_saved_key_of_its_type_only() {
        let (_dir, path) = file_with(&format!(
            "db.example ssh-ed25519 {OLD}\ndb.example ecdsa-sha2-nistp256 {ECDSA}\nother.example ssh-ed25519 {OTHER}\n"
        ));
        assert_eq!(
            judge(&only(&path), "db.example", 22, NEW),
            Verdict::Changed {
                key: key(NEW),
                saved: vec![key(OLD)]
            }
        );
    }

    #[test]
    fn a_new_type_for_a_known_host_is_not_a_first_connection() {
        let (_dir, path) = file_with(&format!("db.example ecdsa-sha2-nistp256 {ECDSA}\n"));
        assert_eq!(
            judge(&only(&path), "db.example", 22, NEW),
            Verdict::New {
                key: key(NEW),
                known_types: vec!["ecdsa-sha2-nistp256".into()]
            }
        );
        assert_eq!(
            judge(&only(&path), "fresh.example", 22, NEW),
            Verdict::New {
                key: key(NEW),
                known_types: Vec::new()
            }
        );
    }

    #[test]
    fn a_revoked_key_is_refused_even_where_it_is_also_saved() {
        let (_dir, path) = file_with(&format!(
            "db.example ssh-ed25519 {OLD}\n@revoked * ssh-ed25519 {OLD}\n"
        ));
        assert_eq!(judge(&only(&path), "db.example", 22, OLD), Verdict::Revoked);
        assert_eq!(
            judge(&only(&path), "fresh.example", 22, OLD),
            Verdict::Revoked
        );
        assert!(matches!(
            judge(&only(&path), "db.example", 22, NEW),
            Verdict::Changed { .. }
        ));
    }

    #[test]
    fn patterns_negations_case_and_whitespace_match_as_in_openssh() {
        let (_dir, path) = file_with(&format!(
            "  *.EXAMPLE,!bad.example\tssh-ed25519   {OLD}\n\
             [db?.lan]:2222 ssh-ed25519 {NEW}\n"
        ));
        assert!(known(&path, "web.example", 22, OLD));
        assert!(known(&path, "Web.Example", 22, OLD));
        assert!(!known(&path, "bad.example", 22, OLD));
        assert!(!known(&path, "web.example", 2200, OLD));
        assert!(known(&path, "db1.lan", 2222, NEW));
        assert!(!known(&path, "db10.lan", 2222, NEW));
    }

    #[test]
    fn wildcards_match_like_openssh_globs() {
        assert!(wildcard("*", "anything"));
        assert!(wildcard("a*b*c", "aXXbYYc"));
        assert!(wildcard("a?c", "abc"));
        assert!(!wildcard("a?c", "ac"));
        assert!(!wildcard("*.example", "example"));
        assert!(wildcard("**", ""));
    }

    #[test]
    fn read_only_files_count_and_are_never_written() {
        let (_dir, user) = file_with("");
        let (_global_dir, global) = file_with(&format!("db.example ssh-ed25519 {OLD}\n"));
        let files = HostFiles {
            user: user.clone(),
            read_only: vec![global.clone(), PathBuf::from("/nonexistent/known_hosts")],
        };
        assert_eq!(judge(&files, "db.example", 22, OLD), Verdict::Known);
        assert!(matches!(
            judge(&files, "db.example", 22, NEW),
            Verdict::Changed { .. }
        ));
        replace(&files, "db.example", 22, &key(NEW)).unwrap();
        assert_eq!(judge(&files, "db.example", 22, NEW), Verdict::Known);
        assert_eq!(
            fs::read_to_string(&global).unwrap(),
            format!("db.example ssh-ed25519 {OLD}\n")
        );
    }

    #[test]
    fn known_key_types_are_asked_for_first() {
        let (_dir, path) = file_with(&format!(
            "db.example ecdsa-sha2-nistp256 {ECDSA}\nother.example ssh-ed25519 {OTHER}\n"
        ));
        let order = preferred(&only(&path), "db.example", 22).key;
        assert_eq!(
            order[0],
            Algorithm::Ecdsa {
                curve: russh::keys::EcdsaCurve::NistP256
            }
        );
        assert_eq!(order.len(), Preferred::DEFAULT.key.len());
        assert_eq!(
            preferred(&only(&path), "fresh.example", 22).key,
            Preferred::DEFAULT.key
        );
        assert!(
            preferred(&only(&path), "db.example", 22)
                .host_key_certificates
                .is_empty()
        );
    }

    #[test]
    fn a_saved_rsa_key_puts_every_rsa_hash_first() {
        let order = order_keys(&Preferred::DEFAULT.key, &[Algorithm::Rsa { hash: None }]);
        assert!(
            order[..3]
                .iter()
                .all(|a| matches!(a, Algorithm::Rsa { .. }))
        );
    }

    #[test]
    fn certificates_are_asked_for_only_where_an_authority_covers_the_host() {
        let (_dir, path) = file_with(&format!("@cert-authority *.corp ssh-ed25519 {OTHER}\n"));
        assert!(
            !preferred(&only(&path), "db.corp", 22)
                .host_key_certificates
                .is_empty()
        );
        assert!(
            preferred(&only(&path), "db.example", 22)
                .host_key_certificates
                .is_empty()
        );
    }

    #[test]
    fn a_hashed_file_gets_hashed_entries_and_a_plain_one_plain() {
        let (_dir, path) = file_with(&format!("{HASHED} ssh-ed25519 {OLD}\n"));
        learn(&only(&path), "db.example", 2222, &key(NEW)).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let added = text.lines().nth(1).unwrap();
        assert!(added.starts_with("|1|"), "{added}");
        assert!(!added.contains("db.example"));
        assert!(known(&path, "db.example", 2222, NEW));

        let (_dir, plain) = file_with(&format!("other.example ssh-ed25519 {OLD}"));
        learn(&only(&plain), "DB.example", 22, &key(NEW)).unwrap();
        assert_eq!(entries(&plain)[1], format!("db.example ssh-ed25519 {NEW}"));
    }

    #[test]
    fn learning_creates_the_folder_and_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".ssh").join("known_hosts");
        learn(&only(&path), "db.example", 22, &key(NEW)).unwrap();
        assert!(known(&path, "db.example", 22, NEW));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }
}

#[cfg(test)]
mod with_ssh_keygen {
    use std::process::Command;

    use super::*;

    const KEY: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";

    fn keygen(args: &[&str]) -> Option<std::process::Output> {
        Command::new("ssh-keygen").args(args).output().ok()
    }

    #[test]
    fn ssh_keygen_finds_and_removes_what_ela_hashed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let other = "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X";
        fs::write(&path, format!("old.example ssh-ed25519 {other}\n")).unwrap();
        let Some(_) = keygen(&["-H", "-f", path.to_str().unwrap()]) else {
            return;
        };
        let key = parse_public_key_base64(KEY).unwrap();
        learn(&HostFiles::only(path.clone()), "db.example", 2222, &key).unwrap();

        let found = keygen(&["-F", "[db.example]:2222", "-f", path.to_str().unwrap()]).unwrap();
        assert!(found.status.success(), "{found:?}");
        assert!(String::from_utf8_lossy(&found.stdout).contains(KEY));

        let removed = keygen(&["-R", "[db.example]:2222", "-f", path.to_str().unwrap()]).unwrap();
        assert!(removed.status.success(), "{removed:?}");
        let files = HostFiles::only(path.clone());
        let presented = PublicKeyOrCertificate::PublicKey {
            key,
            hash_alg: None,
        };
        assert!(matches!(
            verdict(&files, "db.example", 2222, &presented).unwrap(),
            Verdict::New { .. }
        ));
    }
}
