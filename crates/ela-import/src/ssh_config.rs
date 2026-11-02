use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ela_core::{Connection, Settings};
use serde_json::{Map, Value};
use uuid::Uuid;

const MAX_INCLUDE_DEPTH: usize = 16;

pub struct Context {
    pub home: PathBuf,
    pub local_user: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("{0} doesn't exist.")]
    NotFound(PathBuf),
    #[error("Can't read {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
}

pub const SSH_PROTOCOL: &str = "ssh";

/// What `~/.ssh/config` says about one host, as OpenSSH would read it for `ssh <alias>`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SshHost {
    pub alias: String,
    pub hostname: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub identity_files: Vec<String>,
    pub certificate_files: Vec<String>,
    pub identities_only: Option<bool>,
    pub identity_agent: Option<String>,
    pub user_known_hosts_file: Option<String>,
    pub host_key_alias: Option<String>,
    pub server_alive_interval: Option<u32>,
    pub proxy_jump: Option<String>,
    pub proxy_command: Option<String>,
    pub algorithms: BTreeMap<&'static str, String>,
}

impl SshHost {
    /// The settings layer for Inheritance: whatever a Connection and its Folders don't set.
    pub fn settings(&self) -> Settings {
        let mut options = Map::new();
        for (key, value) in [
            ("identity_agent", &self.identity_agent),
            ("user_known_hosts_file", &self.user_known_hosts_file),
            ("host_key_alias", &self.host_key_alias),
            ("proxy_command", &self.proxy_command),
        ] {
            if let Some(value) = value {
                options.insert(key.into(), Value::from(value.clone()));
            }
        }
        for (key, values) in [
            ("identity_file", &self.identity_files),
            ("certificate_file", &self.certificate_files),
        ] {
            if !values.is_empty() {
                options.insert(key.into(), Value::from(values.clone()));
            }
        }
        if let Some(jump) = &self.proxy_jump {
            let hops: Vec<String> = jump.split(',').map(|hop| hop.trim().to_owned()).collect();
            options.insert("proxy_jump".into(), Value::from(hops));
        }
        if let Some(only) = self.identities_only {
            let only = if only { "yes" } else { "no" };
            options.insert("identities_only".into(), Value::from(only));
        }
        if let Some(seconds) = self.server_alive_interval {
            options.insert("server_alive_interval".into(), Value::from(seconds));
        }
        for (key, value) in &self.algorithms {
            options.insert((*key).into(), Value::from(value.clone()));
        }
        Settings {
            port: self.port,
            username: self.user.clone(),
            options,
            ..Settings::default()
        }
    }

    pub fn address(&self) -> &str {
        self.hostname.as_deref().unwrap_or(&self.alias)
    }

    /// Only the name: the rest is read from the file at every connect, so the file stays where
    /// the host is described and changes to it reach Ela.
    fn into_connection(self) -> Connection {
        let host = self.alias.clone();
        Connection::new(self.alias, SSH_PROTOCOL, host)
    }
}

/// Read once per connect and asked about each host on the route.
pub struct SshConfig {
    parsed: Parsed,
}

impl SshConfig {
    /// A missing file is an empty one, as it is to OpenSSH.
    pub fn read(path: &Path, context: &Context) -> Result<Self, ImportError> {
        match read_parsed(path, context) {
            Ok(parsed) => Ok(Self { parsed }),
            Err(ImportError::NotFound(_)) => Ok(Self {
                parsed: Parsed::default(),
            }),
            Err(error) => Err(error),
        }
    }

    pub fn host(&self, alias: &str, context: &Context) -> SshHost {
        evaluate(
            alias,
            &self.parsed.directives,
            context,
            &mut BTreeSet::new(),
        )
    }
}

#[derive(Debug, Default)]
pub struct Imported {
    pub hosts: Vec<SshHost>,
    pub notes: Vec<String>,
}

/// Importing again adds what is new: a host keeps its Connection, and whatever the user changed
/// on it, for as long as a Connection has its name.
pub fn new_connections(
    hosts: Vec<SshHost>,
    existing: &BTreeMap<Uuid, Connection>,
) -> (Vec<Connection>, usize) {
    let before = hosts.len();
    let connections: Vec<Connection> = hosts
        .into_iter()
        .filter(|host| {
            !existing
                .values()
                .any(|c| c.protocol == "ssh" && c.name == host.alias)
        })
        .map(SshHost::into_connection)
        .collect();
    let skipped = before - connections.len();
    (connections, skipped)
}

#[derive(Debug)]
enum Guard {
    Host(Vec<String>),
    Match {
        place: String,
        criteria: Vec<Criterion>,
    },
}

#[derive(Debug)]
struct Criterion {
    negated: bool,
    keyword: String,
    argument: String,
}

struct Directive {
    guards: Vec<Rc<Guard>>,
    key: String,
    values: Vec<String>,
}

#[derive(Default)]
struct Parsed {
    aliases: Vec<String>,
    directives: Vec<Directive>,
    notes: BTreeSet<String>,
}

fn read_parsed(path: &Path, context: &Context) -> Result<Parsed, ImportError> {
    let text = fs::read_to_string(path).map_err(|source| match source.kind() {
        io::ErrorKind::NotFound => ImportError::NotFound(path.to_owned()),
        _ => ImportError::Io {
            path: path.to_owned(),
            source,
        },
    })?;
    let mut parsed = Parsed::default();
    parse_file(path, &text, &[], 0, context, &mut parsed);
    Ok(parsed)
}

pub fn read_ssh_config(path: &Path, context: &Context) -> Result<Imported, ImportError> {
    let parsed = read_parsed(path, context)?;
    let mut notes = parsed.notes;
    let hosts = parsed
        .aliases
        .iter()
        .map(|alias| evaluate(alias, &parsed.directives, context, &mut notes))
        .collect();
    Ok(Imported {
        hosts,
        notes: notes.into_iter().collect(),
    })
}

fn parse_file(
    path: &Path,
    text: &str,
    outer: &[Rc<Guard>],
    depth: usize,
    context: &Context,
    parsed: &mut Parsed,
) {
    let mut guards = outer.to_vec();
    for (index, line) in text.lines().enumerate() {
        let Some((key, values)) = parse_line(line) else {
            continue;
        };
        let place = format!("{}:{}", path.display(), index + 1);
        match key.as_str() {
            "host" => {
                for pattern in &values {
                    let plain = !pattern.contains(['*', '?', '!']);
                    if plain && !parsed.aliases.contains(pattern) {
                        parsed.aliases.push(pattern.clone());
                    }
                }
                guards = outer.to_vec();
                guards.push(Rc::new(Guard::Host(values)));
            }
            "match" => {
                guards = outer.to_vec();
                let guard = match parse_criteria(&values) {
                    Some(criteria) => Guard::Match { place, criteria },
                    None => {
                        parsed.notes.insert(format!(
                            "{place}: this Match line can't be read, so its settings were left out."
                        ));
                        Guard::Match {
                            place,
                            criteria: vec![Criterion {
                                negated: false,
                                keyword: "unreadable".into(),
                                argument: String::new(),
                            }],
                        }
                    }
                };
                guards.push(Rc::new(guard));
            }
            "include" => {
                if depth + 1 >= MAX_INCLUDE_DEPTH {
                    parsed.notes.insert(format!(
                        "{place}: Include goes more than {MAX_INCLUDE_DEPTH} levels deep, so it was left out."
                    ));
                    continue;
                }
                for pattern in &values {
                    for included in include_paths(pattern, context) {
                        match fs::read_to_string(&included) {
                            Ok(text) => {
                                parse_file(&included, &text, &guards, depth + 1, context, parsed);
                            }
                            Err(error) => {
                                parsed.notes.insert(format!(
                                    "{place}: can't read {}: {error}",
                                    included.display()
                                ));
                            }
                        }
                    }
                }
            }
            _ => parsed.directives.push(Directive {
                guards: guards.clone(),
                key,
                values,
            }),
        }
    }
}

fn parse_line(line: &str) -> Option<(String, Vec<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let end = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let key = line[..end].to_ascii_lowercase();
    let rest = line[end..].trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest);
    let values = split_words(rest);
    if values.is_empty() {
        None
    } else {
        Some((key, values))
    }
}

fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&next) = chars.peek() {
        if next.is_whitespace() {
            chars.next();
            continue;
        }
        if next == '#' {
            break;
        }
        let mut word = String::new();
        if next == '"' {
            chars.next();
            word.extend(chars.by_ref().take_while(|&c| c != '"'));
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                word.push(c);
                chars.next();
            }
        }
        words.push(word);
    }
    words
}

fn parse_criteria(words: &[String]) -> Option<Vec<Criterion>> {
    let mut criteria = Vec::new();
    let mut words = words.iter();
    while let Some(word) = words.next() {
        let (negated, word) = match word.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, word.as_str()),
        };
        let (keyword, inline) = match word.split_once('=') {
            Some((keyword, argument)) => (keyword, Some(argument.to_owned())),
            None => (word, None),
        };
        let keyword = keyword.to_ascii_lowercase();
        let argument = match keyword.as_str() {
            "all" | "canonical" | "final" => String::new(),
            _ => inline.or_else(|| words.next().cloned())?,
        };
        criteria.push(Criterion {
            negated,
            keyword,
            argument,
        });
    }
    (!criteria.is_empty()).then_some(criteria)
}

fn include_paths(pattern: &str, context: &Context) -> Vec<PathBuf> {
    let expanded = match pattern.strip_prefix("~/") {
        Some(rest) => context.home.join(rest),
        None if Path::new(pattern).is_absolute() => PathBuf::from(pattern),
        None => context.home.join(".ssh").join(pattern),
    };
    let Ok(paths) = glob::glob(&expanded.to_string_lossy()) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = paths.flatten().filter(|path| path.is_file()).collect();
    paths.sort();
    paths
}

#[derive(Default)]
struct Found {
    hostname: Option<String>,
    port: Option<u16>,
    user: Option<String>,
    identity_files: Vec<String>,
    certificate_files: Vec<String>,
    identities_only: Option<bool>,
    identity_agent: Option<String>,
    user_known_hosts_file: Option<String>,
    host_key_alias: Option<String>,
    server_alive_interval: Option<u32>,
    route_decided: bool,
    proxy_jump: Option<String>,
    proxy_command: Option<String>,
    algorithms: BTreeMap<&'static str, String>,
}

/// Algorithm lists, under the option names Ela keeps them as. They stay in OpenSSH's own syntax,
/// `+`, `-` and `^` included, and `ela-ssh` reads them.
const ALGORITHMS: [(&str, &str); 6] = [
    ("hostkeyalgorithms", "host_key_algorithms"),
    ("kexalgorithms", "kex_algorithms"),
    ("pubkeyacceptedalgorithms", "pubkey_accepted_algorithms"),
    ("pubkeyacceptedkeytypes", "pubkey_accepted_algorithms"),
    ("ciphers", "ciphers"),
    ("macs", "macs"),
];

fn evaluate(
    alias: &str,
    directives: &[Directive],
    context: &Context,
    notes: &mut BTreeSet<String>,
) -> SshHost {
    let mut found = Found::default();
    for directive in directives {
        let applies = directive
            .guards
            .iter()
            .all(|guard| guard_applies(guard, alias, &found, context, notes));
        if applies {
            apply(directive, alias, &mut found, notes);
        }
    }
    let host = found.hostname.clone().unwrap_or_else(|| alias.to_owned());
    let remote_user = found.user.as_deref().unwrap_or(&context.local_user);
    let tokens = Tokens {
        alias,
        host: &host,
        user: remote_user,
        port: found.port.unwrap_or(22),
        context,
    };
    let expand = |files: &[String]| files.iter().map(|file| tokens.expand(file)).collect();
    let expand_one = |value: &Option<String>| value.as_deref().map(|v| tokens.expand(v));
    SshHost {
        alias: alias.to_owned(),
        identity_files: expand(&found.identity_files),
        certificate_files: expand(&found.certificate_files),
        identity_agent: expand_one(&found.identity_agent),
        user_known_hosts_file: expand_one(&found.user_known_hosts_file),
        identities_only: found.identities_only,
        host_key_alias: found.host_key_alias.clone(),
        server_alive_interval: found.server_alive_interval,
        port: found.port,
        user: found.user.clone(),
        proxy_jump: found.proxy_jump,
        proxy_command: found.proxy_command,
        algorithms: found.algorithms,
        hostname: found.hostname.is_some().then_some(host),
    }
}

/// OpenSSH keeps the first value it obtains for most settings, and collects every `IdentityFile`.
fn apply(directive: &Directive, alias: &str, found: &mut Found, notes: &mut BTreeSet<String>) {
    let value = &directive.values[0];
    match directive.key.as_str() {
        "hostname" if found.hostname.is_none() => {
            found.hostname = Some(value.replace("%h", alias).replace("%%", "%"));
        }
        "port" if found.port.is_none() => match value.parse() {
            Ok(port) => found.port = Some(port),
            Err(_) => {
                notes.insert(format!(
                    "{alias}: “{value}” isn't a port, so it was left out."
                ));
            }
        },
        "user" if found.user.is_none() => found.user = Some(value.clone()),
        "identityfile" if !value.eq_ignore_ascii_case("none") => {
            if !found.identity_files.contains(value) {
                found.identity_files.push(value.clone());
            }
        }
        "certificatefile" if !value.eq_ignore_ascii_case("none") => {
            if !found.certificate_files.contains(value) {
                found.certificate_files.push(value.clone());
            }
        }
        "identitiesonly" if found.identities_only.is_none() => {
            found.identities_only = Some(value.eq_ignore_ascii_case("yes"));
        }
        "identityagent" if found.identity_agent.is_none() => {
            found.identity_agent = Some(value.clone());
        }
        "userknownhostsfile" if found.user_known_hosts_file.is_none() => {
            found.user_known_hosts_file = Some(value.clone());
        }
        "hostkeyalias" if found.host_key_alias.is_none() => {
            found.host_key_alias = Some(value.clone());
        }
        "serveraliveinterval" if found.server_alive_interval.is_none() => match value.parse() {
            Ok(seconds) => found.server_alive_interval = Some(seconds),
            Err(_) => {
                notes.insert(format!(
                    "{alias}: “{value}” isn't a number of seconds, so ServerAliveInterval was left out."
                ));
            }
        },
        key if ALGORITHMS.iter().any(|(name, _)| *name == key) => {
            if let Some((_, option)) = ALGORITHMS.iter().find(|(name, _)| *name == key) {
                found
                    .algorithms
                    .entry(option)
                    .or_insert_with(|| value.clone());
            }
        }
        "proxyjump" if !found.route_decided => {
            found.route_decided = true;
            if !value.eq_ignore_ascii_case("none") {
                found.proxy_jump = Some(value.clone());
            }
        }
        "proxycommand" if !found.route_decided => {
            found.route_decided = true;
            if !value.eq_ignore_ascii_case("none") {
                found.proxy_command = Some(directive.values.join(" "));
            }
        }
        _ => {}
    }
}

fn guard_applies(
    guard: &Guard,
    alias: &str,
    found: &Found,
    context: &Context,
    notes: &mut BTreeSet<String>,
) -> bool {
    match guard {
        Guard::Host(patterns) => {
            let lower = alias.to_ascii_lowercase();
            let hits = |pattern: &str| wildcard(&pattern.to_ascii_lowercase(), &lower);
            let excluded = patterns
                .iter()
                .any(|p| p.strip_prefix('!').is_some_and(hits));
            !excluded && patterns.iter().any(|p| !p.starts_with('!') && hits(p))
        }
        Guard::Match { place, criteria } => criteria.iter().all(|criterion| {
            match criterion_holds(criterion, alias, found, context) {
                Some(holds) => holds != criterion.negated,
                None => {
                    if criterion.keyword != "unreadable" {
                        notes.insert(format!(
                            "{place}: Match {} isn't evaluated on import, and no command is run, so its settings were left out.",
                            criterion.keyword
                        ));
                    }
                    false
                }
            }
        }),
    }
}

/// `None` is a condition Ela can't or won't decide. `exec` in particular: importing a file must
/// never run the commands written in it.
fn criterion_holds(
    criterion: &Criterion,
    alias: &str,
    found: &Found,
    context: &Context,
) -> Option<bool> {
    let argument = criterion.argument.as_str();
    match criterion.keyword.as_str() {
        // `final` re-reads the file after everything else, which applying it in order approximates.
        "all" | "final" => Some(true),
        "canonical" => Some(false),
        "host" => {
            let host = found.hostname.as_deref().unwrap_or(alias);
            Some(pattern_list(
                &argument.to_ascii_lowercase(),
                &host.to_ascii_lowercase(),
            ))
        }
        "originalhost" => Some(pattern_list(
            &argument.to_ascii_lowercase(),
            &alias.to_ascii_lowercase(),
        )),
        "user" => Some(pattern_list(
            argument,
            found.user.as_deref().unwrap_or(&context.local_user),
        )),
        "localuser" => Some(pattern_list(argument, &context.local_user)),
        _ => None,
    }
}

fn pattern_list(list: &str, text: &str) -> bool {
    let mut hit = false;
    for pattern in list.split(',') {
        if let Some(pattern) = pattern.strip_prefix('!') {
            if wildcard(pattern, text) {
                return false;
            }
        } else if wildcard(pattern, text) {
            hit = true;
        }
    }
    hit
}

fn wildcard(pattern: &str, text: &str) -> bool {
    let (pattern, text): (Vec<char>, Vec<char>) =
        (pattern.chars().collect(), text.chars().collect());
    let (mut p, mut t) = (0, 0);
    let mut retry = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                retry = Some((p, t));
                p += 1;
            }
            Some(&c) if c == '?' || c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => {
                let Some((star, from)) = retry else {
                    return false;
                };
                retry = Some((star, from + 1));
                p = star + 1;
                t = from + 1;
            }
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

struct Tokens<'a> {
    alias: &'a str,
    host: &'a str,
    user: &'a str,
    port: u16,
    context: &'a Context,
}

impl Tokens<'_> {
    fn expand(&self, value: &str) -> String {
        let mut out = String::new();
        let mut chars = value.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => out.push('%'),
                Some('h') => out.push_str(self.host),
                Some('n') => out.push_str(self.alias),
                Some('r') => out.push_str(self.user),
                Some('u') => out.push_str(&self.context.local_user),
                Some('p') => out.push_str(&self.port.to_string()),
                Some('d') => out.push_str(&self.context.home.to_string_lossy()),
                Some(other) => {
                    out.push('%');
                    out.push(other);
                }
                None => out.push('%'),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    struct Fixture {
        dir: TempDir,
        context: Context,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            fs::create_dir_all(dir.path().join(".ssh/conf.d")).unwrap();
            let context = Context {
                home: dir.path().to_owned(),
                local_user: "me".into(),
            };
            Self { dir, context }
        }

        fn write(&self, name: &str, text: &str) -> PathBuf {
            let path = self.dir.path().join(".ssh").join(name);
            fs::write(&path, text).unwrap();
            path
        }

        fn read(&self, text: &str) -> Imported {
            read_ssh_config(&self.write("config", text), &self.context).unwrap()
        }

        fn host(&self, text: &str, alias: &str) -> SshHost {
            self.read(text)
                .hosts
                .into_iter()
                .find(|host| host.alias == alias)
                .unwrap()
        }
    }

    #[test]
    fn a_plain_host_keeps_its_fields() {
        let host = Fixture::new().host(
            "Host db\n  HostName db.internal\n  Port 2222\n  User postgres\n",
            "db",
        );
        assert_eq!(
            host,
            SshHost {
                alias: "db".into(),
                hostname: Some("db.internal".into()),
                port: Some(2222),
                user: Some("postgres".into()),
                ..Default::default()
            }
        );
    }

    #[test]
    fn without_a_hostname_the_alias_is_the_address() {
        let host = Fixture::new().host("Host web.example.com\n", "web.example.com");
        assert_eq!(host.address(), "web.example.com");
        assert_eq!((host.port, host.user), (None, None));
    }

    #[test]
    fn patterns_supply_settings_but_are_not_hosts() {
        let imported = Fixture::new().read(
            "Host db *.lan !skip\n  User shared\nHost *\n  Port 2200\nHost a.lan\n  Port 1\n",
        );
        let aliases: Vec<&str> = imported.hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(aliases, ["db", "a.lan"]);
        assert_eq!(imported.hosts[0].user.as_deref(), Some("shared"));
        assert_eq!(imported.hosts[0].port, Some(2200));
        assert_eq!(imported.hosts[1].user.as_deref(), Some("shared"));
    }

    #[test]
    fn the_first_value_wins_as_in_openssh() {
        let host = Fixture::new().host("Host *\n  User first\nHost db\n  User second\n", "db");
        assert_eq!(host.user.as_deref(), Some("first"));
    }

    #[test]
    fn a_negated_pattern_excludes_a_host() {
        let host = Fixture::new().host(
            "Host db\n  Port 1\nHost * !db\n  User others\nHost web\n",
            "db",
        );
        assert_eq!(host.user, None);
    }

    #[test]
    fn keys_ignore_case_and_accept_equals_and_quotes() {
        let host = Fixture::new().host(
            "HOST db\n  hostNAME=db.internal\n  Port = 2222\n  IdentityFile \"~/my keys/id\" # work\n",
            "db",
        );
        assert_eq!(host.address(), "db.internal");
        assert_eq!(host.port, Some(2222));
        assert_eq!(host.identity_files, ["~/my keys/id"]);
    }

    #[test]
    fn identity_files_collect_and_expand_tokens() {
        let fixture = Fixture::new();
        let host = fixture.host(
            "Host db\n  HostName db.internal\n  User pg\n  IdentityFile ~/.ssh/%n_%r_%h\n  IdentityFile %d/key_%u\nHost *\n  IdentityFile ~/.ssh/shared\n  IdentityFile none\n",
            "db",
        );
        let home = fixture.context.home.display();
        assert_eq!(
            host.identity_files,
            [
                "~/.ssh/db_pg_db.internal".to_owned(),
                format!("{home}/key_me"),
                "~/.ssh/shared".to_owned(),
            ]
        );
    }

    #[test]
    fn hostname_tokens_use_the_alias() {
        let host = Fixture::new().host("Host db\n  HostName %h.internal\n", "db");
        assert_eq!(host.address(), "db.internal");
    }

    #[test]
    fn include_is_read_in_place_with_globs_and_relative_paths() {
        let fixture = Fixture::new();
        fixture.write("conf.d/a.conf", "Host from-a\n  User a\n");
        fixture.write("conf.d/b.conf", "Host from-b\n  User b\n");
        fixture.write("absolute.conf", "Host from-absolute\n");
        let absolute = fixture.dir.path().join(".ssh/absolute.conf");
        let imported = fixture.read(&format!(
            "Include conf.d/*.conf\nInclude {}\nInclude ~/.ssh/missing\nHost last\n",
            absolute.display()
        ));
        let aliases: Vec<&str> = imported.hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(aliases, ["from-a", "from-b", "from-absolute", "last"]);
        assert!(imported.notes.is_empty(), "{:?}", imported.notes);
    }

    #[test]
    fn an_include_inside_a_block_only_applies_to_that_block() {
        let fixture = Fixture::new();
        fixture.write("extra", "User fromextra\nHost nested\n  Port 7\n");
        let imported = fixture.read("Host db\n  Include extra\nHost web\n  Port 80\n");
        let host = |alias: &str| imported.hosts.iter().find(|h| h.alias == alias).unwrap();
        assert_eq!(host("db").user.as_deref(), Some("fromextra"));
        assert_eq!(host("web").user, None);
        assert_eq!(host("web").port, Some(80));
    }

    #[test]
    fn a_block_in_an_included_file_ends_with_that_file() {
        let fixture = Fixture::new();
        fixture.write("extra", "Host other\n  User o\n");
        let imported = fixture.read("Include extra\nUser global\nHost db\n");
        let db = imported.hosts.iter().find(|h| h.alias == "db").unwrap();
        assert_eq!(db.user.as_deref(), Some("global"));
    }

    #[test]
    fn an_include_loop_ends_with_a_note() {
        let fixture = Fixture::new();
        let imported = fixture.read("Host db\nInclude config\n");
        assert_eq!(imported.hosts.len(), 1);
        assert!(imported.notes.iter().any(|n| n.contains("levels deep")));
    }

    #[test]
    fn match_host_sees_the_hostname_and_originalhost_the_alias() {
        let fixture = Fixture::new();
        let host = fixture.host(
            "Host db\n  HostName db.internal\nMatch host *.internal\n  User byhost\nMatch originalhost db\n  Port 2\n",
            "db",
        );
        assert_eq!(host.user.as_deref(), Some("byhost"));
        assert_eq!(host.port, Some(2));
    }

    #[test]
    fn match_user_all_negation_and_lists() {
        let fixture = Fixture::new();
        let text = "Host db\n  User pg\nMatch user pg host db,web\n  Port 1\nMatch !localuser me\n  IdentityFile ~/never\nMatch localuser=me\n  ProxyJump gate\nMatch all\n  IdentityFile ~/always\n";
        let host = fixture.host(text, "db");
        assert_eq!(host.port, Some(1));
        assert_eq!(host.identity_files, ["~/always"]);
        assert_eq!(host.proxy_jump.as_deref(), Some("gate"));
    }

    #[test]
    fn match_exec_is_never_run_and_is_reported() {
        let fixture = Fixture::new();
        let marker = fixture.dir.path().join("ran");
        let imported = fixture.read(&format!(
            "Host db\nMatch exec \"touch {}\"\n  User wrong\n",
            marker.display()
        ));
        assert!(!marker.exists());
        assert_eq!(imported.hosts[0].user, None);
        assert_eq!(imported.notes.len(), 1);
        assert!(
            imported.notes[0].contains("Match exec"),
            "{:?}",
            imported.notes
        );
        assert!(imported.notes[0].contains("no command is run"));
    }

    #[test]
    fn a_negated_exec_is_not_taken_as_true() {
        let imported = Fixture::new().read("Host db\nMatch !exec \"false\"\n  User wrong\n");
        assert_eq!(imported.hosts[0].user, None);
    }

    #[test]
    fn an_unreadable_match_is_reported_and_skipped() {
        let imported = Fixture::new().read("Host db\nMatch host\n  User wrong\n");
        assert_eq!(imported.hosts[0].user, None);
        assert!(imported.notes[0].contains("can't be read"));
    }

    #[test]
    fn proxy_jump_and_proxy_command_are_kept_and_the_first_decides() {
        let fixture = Fixture::new();
        let jump = fixture.host("Host a\n  ProxyJump gate,gate2\n  ProxyCommand nc x\n", "a");
        assert_eq!(jump.proxy_jump.as_deref(), Some("gate,gate2"));
        assert_eq!(jump.proxy_command, None);

        let command = fixture.host("Host a\n  ProxyCommand nc %h %p\n  ProxyJump gate\n", "a");
        assert_eq!(command.proxy_command.as_deref(), Some("nc %h %p"));
        assert_eq!(command.proxy_jump, None);

        let none = fixture.host("Host a\n  ProxyJump none\nHost *\n  ProxyJump gate\n", "a");
        assert_eq!(none.proxy_jump, None);
    }

    #[test]
    fn a_bad_port_is_reported_and_left_out() {
        let imported = Fixture::new().read("Host db\n  Port many\n");
        assert_eq!(imported.hosts[0].port, None);
        assert!(imported.notes[0].contains("isn't a port"));
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let imported = Fixture::new().read("# top\n\n   # indented\nHost db # trailing\n");
        assert_eq!(imported.hosts.len(), 1);
        assert_eq!(imported.hosts[0].alias, "db");
    }

    #[test]
    fn a_missing_file_is_named() {
        let fixture = Fixture::new();
        let missing = fixture.dir.path().join(".ssh/nope");
        assert!(matches!(
            read_ssh_config(&missing, &fixture.context),
            Err(ImportError::NotFound(_))
        ));
    }

    #[test]
    fn wildcards_behave_like_ssh_patterns() {
        assert!(wildcard("*.lan", "a.lan"));
        assert!(wildcard("a?c", "abc"));
        assert!(wildcard("*", ""));
        assert!(wildcard("a*b*c", "axxbyyc"));
        assert!(!wildcard("*.lan", "a.lan.evil"));
        assert!(!wildcard("a?c", "ac"));
        assert!(pattern_list("db,web", "web"));
        assert!(!pattern_list("*,!web", "web"));
        assert!(!pattern_list("!web", "db"));
    }

    #[test]
    fn an_imported_host_is_its_name_and_the_file_says_the_rest() {
        let host = SshHost {
            alias: "db".into(),
            hostname: Some("db.internal".into()),
            port: Some(2222),
            user: Some("pg".into()),
            identity_files: vec!["~/a".into()],
            ..Default::default()
        };
        let (connections, skipped) = new_connections(vec![host], &BTreeMap::new());
        assert_eq!(skipped, 0);
        let connection = &connections[0];
        assert_eq!(connection.protocol, "ssh");
        assert_eq!(connection.name, "db");
        assert_eq!(connection.host, "db");
        assert_eq!(connection.settings, Settings::default());
    }

    #[test]
    fn a_host_becomes_a_settings_layer_with_every_keyword() {
        let host = Fixture::new().host(
            "Host db\n  HostName db.internal\n  Port 2222\n  User pg\n  IdentityFile ~/.ssh/%n_key\n  CertificateFile ~/.ssh/%n-cert.pub\n  IdentitiesOnly yes\n  IdentityAgent ~/agent.sock\n  UserKnownHostsFile ~/.ssh/known_%n\n  HostKeyAlias db-alias\n  ServerAliveInterval 15\n  ProxyJump gate,ops@hop:2200\n  HostKeyAlgorithms +ssh-rsa\n  PubkeyAcceptedKeyTypes +ssh-rsa\n  KexAlgorithms +diffie-hellman-group14-sha1\n",
            "db",
        );
        assert_eq!(host.address(), "db.internal");
        let settings = host.settings();
        assert_eq!(settings.port, Some(2222));
        assert_eq!(settings.username.as_deref(), Some("pg"));
        assert_eq!(
            Value::Object(settings.options),
            serde_json::json!({
                "identity_file": ["~/.ssh/db_key"],
                "certificate_file": ["~/.ssh/db-cert.pub"],
                "identities_only": "yes",
                "identity_agent": "~/agent.sock",
                "user_known_hosts_file": "~/.ssh/known_db",
                "host_key_alias": "db-alias",
                "server_alive_interval": 15,
                "proxy_jump": ["gate", "ops@hop:2200"],
                "host_key_algorithms": "+ssh-rsa",
                "pubkey_accepted_algorithms": "+ssh-rsa",
                "kex_algorithms": "+diffie-hellman-group14-sha1",
            })
        );
    }

    #[test]
    fn a_missing_file_is_an_empty_one_when_connecting() {
        let fixture = Fixture::new();
        let config = SshConfig::read(&fixture.dir.path().join("none"), &fixture.context).unwrap();
        let host = config.host("db", &fixture.context);
        assert_eq!(host.address(), "db");
        assert_eq!(host.settings(), Settings::default());
    }

    #[test]
    fn a_wildcard_block_applies_when_connecting_though_import_skips_it() {
        let fixture = Fixture::new();
        let path = fixture.dir.path().join("config");
        fs::write(&path, "Host *.corp\n  User admin\n  HostName %h.internal\n").unwrap();
        let config = SshConfig::read(&path, &fixture.context).unwrap();
        let host = config.host("db.corp", &fixture.context);
        assert_eq!(host.address(), "db.corp.internal");
        assert_eq!(host.user.as_deref(), Some("admin"));
    }

    #[test]
    fn importing_again_adds_only_what_is_new() {
        let host = |alias: &str| SshHost {
            alias: alias.into(),
            ..Default::default()
        };
        let existing = Connection::new("db", "ssh", "changed.by.user");
        let other_protocol = Connection::new("web", "rdp", "web");
        let existing =
            BTreeMap::from([(existing.id, existing), (other_protocol.id, other_protocol)]);
        let (connections, skipped) =
            new_connections(vec![host("db"), host("web"), host("new")], &existing);
        assert_eq!(skipped, 1);
        let names: Vec<&str> = connections.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["web", "new"]);
    }
}
