mod algorithms;
mod files;
mod identity;
mod keys;
pub mod known_hosts;

use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use ela_session::{
    Capabilities, ExposeSecret, ForwardFailure, JumpHost, PasswordAnswer, PasswordNote, Prompt,
    Protocol, Question, RemoteFiles, RendererKind, ResolvedConfig, SecretString, Session,
    SessionCommand, SessionError, SessionIo, SessionSink, SessionStatus, Tried,
};
use ela_transport::{
    CONNECT_TIMEOUT, Direct, Opened, ROUTE_OPTION, Route, RouteStream, Transport,
    refuse_unsupported_route, route_field,
};
use russh::client::{AuthResult, KeyboardInteractiveAuthResponse};
use russh::keys::{
    Certificate, HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate,
};
use russh::{ChannelMsg, ChannelOpenFailure, Disconnect, MethodKind, Pty, client};
use serde_json::json;
use uuid::Uuid;

use identity::{IdentityFile, LockedSigner, SignError, Unlocker};
pub use identity::{Passphrases, SavedKey};
use keys::Agent;
pub use keys::KeySources;
use known_hosts::{HostFiles, Verdict};

pub const PROTOCOL_ID: &str = "ssh";

// As many as OpenSSH's NumberOfPasswordPrompts allows.
const PASSWORD_TRIES: usize = 3;

#[derive(Default, Clone)]
struct Keeping {
    passphrases: Option<Arc<dyn Passphrases>>,
    sources: Option<KeySources>,
}

impl Keeping {
    fn sources(&self) -> KeySources {
        self.sources.clone().unwrap_or_else(KeySources::from_env)
    }
}

#[derive(Default)]
pub struct SshProtocol {
    known_hosts: Option<PathBuf>,
    keeping: Keeping,
}

impl SshProtocol {
    pub fn with_known_hosts(path: PathBuf) -> Self {
        Self {
            known_hosts: Some(path),
            ..Self::default()
        }
    }

    pub fn remembering_passphrases(mut self, passphrases: Arc<dyn Passphrases>) -> Self {
        self.keeping.passphrases = Some(passphrases);
        self
    }

    pub fn finding_keys(mut self, sources: KeySources) -> Self {
        self.keeping.sources = Some(sources);
        self
    }

    pub fn transport(&self) -> Arc<dyn Transport> {
        Arc::new(SshTransport {
            known_hosts: self.known_hosts.clone(),
            keeping: self.keeping.clone(),
        })
    }
}

struct SshTransport {
    known_hosts: Option<PathBuf>,
    keeping: Keeping,
}

#[async_trait]
impl Transport for SshTransport {
    async fn connect(
        &self,
        jump_hosts: &[JumpHost],
        sink: &SessionSink,
    ) -> Result<Arc<dyn Route>, SessionError> {
        let known_hosts = host_files(self.known_hosts.as_ref())?;
        let chain = JumpChain::sign_in(jump_hosts, sink, &known_hosts, &self.keeping).await?;
        Ok(Arc::new(chain))
    }
}

/// A path given for tests is the only file read, so the machine's own Known Hosts stay out.
fn host_files(configured: Option<&PathBuf>) -> Result<HostFiles, SessionError> {
    if let Some(path) = configured {
        return Ok(HostFiles::only(path.clone()));
    }
    known_hosts::default_path()
        .map(HostFiles::openssh)
        .ok_or_else(|| {
            SessionError::protocol("Can't find your home folder, which holds ~/.ssh/known_hosts.")
        })
}

struct JumpChain {
    hops: Vec<(String, client::Handle<Handler>)>,
}

impl JumpChain {
    async fn sign_in(
        jump_hosts: &[JumpHost],
        sink: &SessionSink,
        known_hosts: &HostFiles,
        keeping: &Keeping,
    ) -> Result<Self, SessionError> {
        let mut hops: Vec<(String, client::Handle<Handler>)> = Vec::new();
        for jump in jump_hosts {
            let leg = Leg {
                config: &jump.config,
                jump_host: Some(&jump.name),
                via: hops.last(),
            };
            let handle = sign_in(leg, sink, known_hosts, keeping)
                .await
                .map_err(|error| on_jump_host(jump, error))?;
            hops.push((jump.name.clone(), handle));
        }
        Ok(Self { hops })
    }

    async fn disconnect(&self) {
        for (_, hop) in self.hops.iter().rev() {
            let _ = hop.disconnect(Disconnect::ByApplication, "", "en").await;
        }
    }
}

#[async_trait]
impl Route for JumpChain {
    async fn open(&self, host: &str, port: u16) -> Result<Opened, SessionError> {
        let last = self
            .hops
            .last()
            .ok_or_else(|| SessionError::protocol("The route has no Jump Host."))?;
        let channel = open_through(last, host, port).await?;
        Ok(Opened {
            stream: RouteStream::new(channel.into_stream()),
            local_addr: None,
        })
    }

    async fn close(&self) {
        self.disconnect().await;
    }
}

#[async_trait]
impl Protocol for SshProtocol {
    fn id(&self) -> &'static str {
        PROTOCOL_ID
    }
    fn display_name(&self) -> &'static str {
        "SSH"
    }
    fn default_port(&self) -> Option<u16> {
        Some(22)
    }
    fn renderer(&self) -> RendererKind {
        RendererKind::Terminal
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            resize: true,
            clipboard: false,
            file_transfer: true,
        }
    }
    fn settings_schema(&self) -> serde_json::Value {
        json!({
            "term": { "type": "string", "default": "xterm-256color", "label": "Terminal type" },
            "identity_file": {
                "type": "list",
                "label": "Identity files",
                "hint": "One path per line, tried before ssh-agent.",
                "group": "sign_in",
                "mono": true,
            },
            "certificate_file": {
                "type": "list",
                "label": "Certificates",
                "hint": "One path per line. A key's own -cert.pub is found without being listed.",
                "group": "sign_in",
                "mono": true,
            },
            "identities_only": {
                "type": "enum",
                "values": ["no", "yes"],
                "labels": { "no": "No", "yes": "Yes" },
                "default": "no",
                "label": "Only the identity files",
                "hint": "Leaves ssh-agent's other keys out, so a server that counts tries isn't used up.",
                "group": "sign_in",
            },
            "identity_agent": {
                "type": "string",
                "label": "ssh-agent socket",
                "hint": "A path, none, or SSH_AUTH_SOCK.",
                "group": "sign_in",
                "mono": true,
            },
            ROUTE_OPTION: route_field(),
            "server_alive_interval": {
                "type": "integer",
                "label": "Keepalive seconds",
                "hint": "How often to check the server still answers. 0 turns it off.",
                "default": 30,
            },
            "host_key_alias": {
                "type": "string",
                "label": "Host key alias",
                "hint": "The name to check the host key under, instead of the address.",
                "group": "advanced",
                "mono": true,
            },
            "user_known_hosts_file": {
                "type": "string",
                "label": "Known hosts file",
                "default": "~/.ssh/known_hosts",
                "group": "advanced",
                "mono": true,
            },
            "host_key_algorithms": algorithm_field("Host key algorithms"),
            "kex_algorithms": algorithm_field("Key exchange algorithms"),
            "pubkey_accepted_algorithms": algorithm_field("Key signature algorithms"),
            "ciphers": algorithm_field("Ciphers"),
            "macs": algorithm_field("MACs"),
        })
    }

    async fn connect(
        &self,
        config: ResolvedConfig,
        sink: &SessionSink,
    ) -> Result<Box<dyn Session>, SessionError> {
        refuse_unsupported_route(&config)?;
        let known_hosts = host_files(self.known_hosts.as_ref())?;
        let jump_hosts =
            JumpChain::sign_in(&config.jump_hosts, sink, &known_hosts, &self.keeping).await?;
        let leg = Leg {
            config: &config,
            jump_host: None,
            via: jump_hosts.hops.last(),
        };
        let handle = sign_in(leg, sink, &known_hosts, &self.keeping).await?;

        let channel = handle
            .channel_open_session()
            .await
            .map_err(SessionError::protocol)?;
        let mut channel = channel;
        let term = config.option_str("term").unwrap_or("xterm-256color");
        channel
            .request_pty(
                true,
                term,
                config.initial_size.width,
                config.initial_size.height,
                0,
                0,
                TERMINAL_MODES,
            )
            .await
            .map_err(SessionError::protocol)?;
        let mut early = granted(&mut channel, "a terminal").await?;
        channel
            .request_shell(true)
            .await
            .map_err(SessionError::protocol)?;
        early.extend(granted(&mut channel, "a shell").await?);

        Ok(Box::new(SshSession {
            handle: Arc::new(handle),
            channel,
            jump_hosts,
            early,
        }))
    }
}

fn algorithm_field(label: &str) -> serde_json::Value {
    json!({
        "type": "string",
        "label": label,
        "hint": "As in ~/.ssh/config: +name adds to the defaults, -name removes, ^name puts first.",
        "group": "advanced",
        "mono": true,
    })
}

/// What OpenSSH sends from a UTF-8 terminal. Without `IUTF8` the server's line editing counts
/// bytes, so Backspace at a cooked-mode prompt leaves half a character behind.
const TERMINAL_MODES: &[(Pty, u32)] = &[
    (Pty::VINTR, 3),
    (Pty::VQUIT, 28),
    (Pty::VERASE, 127),
    (Pty::VKILL, 21),
    (Pty::VEOF, 4),
    (Pty::VSTART, 17),
    (Pty::VSTOP, 19),
    (Pty::VSUSP, 26),
    (Pty::VREPRINT, 18),
    (Pty::VWERASE, 23),
    (Pty::VLNEXT, 22),
    (Pty::VDISCARD, 15),
    (Pty::ICRNL, 1),
    (Pty::IXON, 1),
    (Pty::IMAXBEL, 1),
    (Pty::IUTF8, 1),
    (Pty::ISIG, 1),
    (Pty::ICANON, 1),
    (Pty::IEXTEN, 1),
    (Pty::ECHO, 1),
    (Pty::ECHOE, 1),
    (Pty::ECHOK, 1),
    (Pty::ECHOCTL, 1),
    (Pty::ECHOKE, 1),
    (Pty::OPOST, 1),
    (Pty::ONLCR, 1),
    (Pty::CS8, 1),
    (Pty::TTY_OP_ISPEED, 38400),
    (Pty::TTY_OP_OSPEED, 38400),
];

/// Waits for the server to grant a request, keeping any output that comes first so none is lost.
async fn granted(
    channel: &mut russh::Channel<client::Msg>,
    what: &str,
) -> Result<Vec<Bytes>, SessionError> {
    let mut early = Vec::new();
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => return Ok(early),
            Some(ChannelMsg::Failure) => {
                return Err(SessionError::protocol(format!(
                    "The server refused {what}."
                )));
            }
            Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => {
                early.push(Bytes::copy_from_slice(&data));
            }
            Some(ChannelMsg::Eof | ChannelMsg::Close) | None => {
                return Err(SessionError::protocol(format!(
                    "The server closed the session instead of giving {what}."
                )));
            }
            Some(_) => {}
        }
    }
}

/// A banner is untrusted text shown in the terminal, so it may not carry escape sequences, as
/// OpenSSH filters it with `strnvis`.
fn banner_text(banner: &str) -> String {
    let mut shown = String::with_capacity(banner.len() + 16);
    for c in banner.chars() {
        match c {
            '\n' => shown.push_str("\r\n"),
            '\t' => shown.push('\t'),
            c if c.is_control() => {}
            c => shown.push(c),
        }
    }
    if !shown.is_empty() && !shown.ends_with('\n') {
        shown.push_str("\r\n");
    }
    shown
}

struct Leg<'a> {
    config: &'a ResolvedConfig,
    jump_host: Option<&'a str>,
    via: Option<&'a (String, client::Handle<Handler>)>,
}

async fn sign_in(
    leg: Leg<'_>,
    sink: &SessionSink,
    known_hosts: &HostFiles,
    keeping: &Keeping,
) -> Result<client::Handle<Handler>, SessionError> {
    let Leg {
        config,
        jump_host,
        via,
    } = leg;
    let jump_host = jump_host.map(str::to_owned);
    sink.status(SessionStatus::Connecting {
        jump_host: jump_host.clone(),
    });
    let sources = keeping.sources();
    // What the user said to check this host under, and where, as OpenSSH's HostKeyAlias and
    // UserKnownHostsFile do.
    let known_as = config.option_str("host_key_alias").unwrap_or(&config.host);
    let known_hosts = match config.option_str("user_known_hosts_file") {
        Some(path) => HostFiles {
            user: sources.expand(path),
            read_only: known_hosts.read_only.clone(),
        },
        None => known_hosts.clone(),
    };
    let keepalive = match config.option_u64("server_alive_interval") {
        Some(0) => None,
        Some(seconds) => Some(Duration::from_secs(seconds)),
        None => Some(Duration::from_secs(30)),
    };
    let ssh_config = Arc::new(client::Config {
        preferred: algorithms::preferred(
            known_hosts::preferred(&known_hosts, known_as, config.port),
            config,
        ),
        keepalive_interval: keepalive,
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    });
    let awaiting_user = Arc::new(AtomicBool::new(false));
    let handler = Handler {
        host: known_as.to_owned(),
        port: config.port,
        jump_host: jump_host.clone(),
        sink: sink.clone(),
        known_hosts,
        awaiting_user: awaiting_user.clone(),
    };
    let (host, port) = (config.host.as_str(), config.port);

    // The clock stops while the user decides whether to trust the host.
    let mut connecting = pin!(async {
        match via {
            None => {
                let opened = Direct
                    .open(host, port)
                    .await
                    .map_err(HandlerError::Rejected)?;
                client::connect_stream(ssh_config, opened.stream, handler).await
            }
            Some(previous) => {
                let channel = open_through(previous, host, port)
                    .await
                    .map_err(HandlerError::Rejected)?;
                client::connect_stream(ssh_config, channel.into_stream(), handler).await
            }
        }
    });
    let mut deadline = Instant::now() + CONNECT_TIMEOUT;
    let connected = loop {
        match tokio::time::timeout(Duration::from_millis(250), &mut connecting).await {
            Ok(result) => break result,
            Err(_) if awaiting_user.load(Ordering::Relaxed) => {
                deadline = Instant::now() + CONNECT_TIMEOUT
            }
            Err(_) if Instant::now() >= deadline => return Err(SessionError::Timeout),
            Err(_) => {}
        }
    };
    let mut handle = match connected {
        Err(HandlerError::Rejected(e)) => return Err(e),
        Err(HandlerError::Ssh(russh::Error::IO(e))) => return Err(SessionError::from_io(&e)),
        Err(HandlerError::Ssh(e)) => return Err(SessionError::protocol(e)),
        Ok(h) => h,
    };

    sink.status(SessionStatus::Authenticating {
        jump_host: jump_host.clone(),
    });
    let username = config.username.clone().unwrap_or_else(whoami);
    let unlocker = Unlocker {
        sink,
        jump_host: jump_host.as_deref(),
        passphrases: keeping.passphrases.as_ref(),
    };
    let sign_in = SignIn {
        username: &username,
        config,
        sources: &sources,
        unlocker: &unlocker,
        keep_password: config.connection.filter(|_| sink.can_keep_passwords()),
    };
    authenticate(&mut handle, sign_in).await?;
    Ok(handle)
}

fn on_jump_host(jump: &JumpHost, error: SessionError) -> SessionError {
    match error {
        SessionError::Cancelled => SessionError::Cancelled,
        error => SessionError::JumpHost {
            jump_host: jump.name.clone(),
            host: jump.config.host.clone(),
            port: jump.config.port,
            error: Box::new(error),
        },
    }
}

async fn open_through(
    (through, previous): &(String, client::Handle<Handler>),
    host: &str,
    port: u16,
) -> Result<russh::Channel<client::Msg>, SessionError> {
    previous
        .channel_open_direct_tcpip(host, port.into(), "127.0.0.1", 0)
        .await
        .map_err(|e| forward_failed(through, host, port, e))
}

fn forward_failed(through: &str, host: &str, port: u16, error: russh::Error) -> SessionError {
    let failure = match error {
        russh::Error::ChannelOpenFailure(ChannelOpenFailure::AdministrativelyProhibited) => {
            ForwardFailure::Prohibited
        }
        russh::Error::ChannelOpenFailure(ChannelOpenFailure::ConnectFailed) => {
            ForwardFailure::Unreachable
        }
        russh::Error::ChannelOpenFailure(_) => ForwardFailure::Other,
        error => return SessionError::protocol(error),
    };
    let address = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    SessionError::ForwardFailed {
        jump_host: through.to_owned(),
        address,
        failure,
    }
}

struct SignIn<'a> {
    username: &'a str,
    config: &'a ResolvedConfig,
    sources: &'a KeySources,
    unlocker: &'a Unlocker<'a>,
    keep_password: Option<Uuid>,
}

/// A named identity file goes first, then the saved password, then ssh-agent and the default key
/// files: an agent with many keys can use up the server's MaxAuthTries before a password that
/// works, and someone who named a key means it. Only then does the Tab ask.
async fn authenticate(
    handle: &mut client::Handle<Handler>,
    sign_in: SignIn<'_>,
) -> Result<(), SessionError> {
    let SignIn {
        username,
        config,
        sources,
        unlocker,
        keep_password,
    } = sign_in;
    let first = handle
        .authenticate_none(username)
        .await
        .map_err(SessionError::protocol)?;
    let Some(offered) = Offered::by(&first) else {
        return Ok(());
    };
    // `None` signs with SHA-1, which a server that names no hash gets only when the user allows
    // `ssh-rsa`, as OpenSSH's PubkeyAcceptedAlgorithms does.
    let rsa_hash = match handle.best_supported_rsa_hash().await.ok().flatten() {
        Some(Some(hash)) => Some(hash),
        _ if algorithms::allows_ssh_rsa(config) => None,
        _ => Some(HashAlg::Sha256),
    };
    let identities_only = config.option_str("identities_only") == Some("yes");
    let named = sources.named(config);
    let mut keys = Keys {
        username,
        rsa_hash,
        agent: sources.agent_for(config).await,
        certificates: sources.certificates(config),
        tried: Vec::new(),
        offered,
        tried_files: false,
        tried_agent: false,
    };
    let asker = Asker {
        sink: unlocker.sink,
        jump_host: unlocker.jump_host,
        username,
        config,
    };

    for path in &named {
        if keys
            .identity_file(handle, path, sources, unlocker, true)
            .await
        {
            return Ok(());
        }
    }

    if let Some(password) = &config.password {
        if keys.offered.password {
            let res = handle
                .authenticate_password(username, password.expose_secret())
                .await
                .map_err(SessionError::protocol)?;
            if res.success() {
                return Ok(());
            }
            keys.note_refusal(&res);
        } else if keys.offered.keyboard && keyboard_rounds(handle, &asker, Some(password)).await? {
            return Ok(());
        }
    }

    // IdentitiesOnly keeps to the named keys, so an agent full of keys can't use up MaxAuthTries;
    // a named key the agent holds still signs through it.
    if !identities_only && keys.agent_keys(handle).await {
        return Ok(());
    }
    let defaults = if identities_only && !named.is_empty() {
        Vec::new()
    } else {
        sources.defaults()
    };
    for path in defaults {
        if keys
            .identity_file(handle, &path, sources, unlocker, false)
            .await
        {
            return Ok(());
        }
    }

    let offered = keys.offered;
    // With keyboard-interactive alone the target shows the server's own questions, so a server
    // that wants only a code isn't sent a password it never asked for.
    let ask_password = offered.password || (offered.keyboard && unlocker.jump_host.is_some());
    if ask_password {
        let asking = PasswordAsk {
            asker: &asker,
            keep: keep_password,
            offered,
            tried: keys.tried_ways(),
        };
        if ask_for_password(handle, asking).await? {
            return Ok(());
        }
    } else if offered.keyboard
        && config.password.is_none()
        && keyboard_rounds(handle, &asker, None).await?
    {
        return Ok(());
    }

    Err(SessionError::AuthFailed(format!(
        "The server refused {username} with every way Ela tried: password, identity files and ssh-agent."
    )))
}

struct Keys<'a> {
    username: &'a str,
    rsa_hash: Option<HashAlg>,
    agent: Option<Agent>,
    tried: Vec<(PublicKey, bool)>,
    certificates: Vec<Certificate>,
    offered: Offered,
    tried_files: bool,
    tried_agent: bool,
}

impl Keys<'_> {
    fn hash_for(&self, key: &PublicKey) -> Option<HashAlg> {
        self.rsa_hash.filter(|_| key.algorithm().is_rsa())
    }

    fn tried_ways(&self) -> Vec<Tried> {
        let mut ways = Vec::new();
        if self.tried_files {
            ways.push(Tried::Keys);
        }
        if self.tried_agent {
            ways.push(Tried::Agent);
        }
        ways
    }

    /// A certificate and its plain key are separate tries, as OpenSSH offers both.
    fn first_try(&mut self, key: &PublicKey, certified: bool) -> bool {
        if self
            .tried
            .iter()
            .any(|(t, cert)| *cert == certified && t.key_data() == key.key_data())
        {
            return false;
        }
        self.tried.push((key.clone(), certified));
        true
    }

    fn note_refusal(&mut self, result: &AuthResult) {
        self.offered = Offered::by(result).unwrap_or(self.offered);
    }

    fn signed_in(&mut self, result: Result<AuthResult, String>) -> bool {
        match result {
            Ok(res) if res.success() => true,
            Ok(res) => {
                self.note_refusal(&res);
                false
            }
            Err(e) => {
                tracing::debug!("signing failed: {e}");
                false
            }
        }
    }

    async fn identity_file(
        &mut self,
        handle: &mut client::Handle<Handler>,
        path: &Path,
        sources: &KeySources,
        unlocker: &Unlocker<'_>,
        named: bool,
    ) -> bool {
        let file = match identity::read(path, sources.home()) {
            Ok(file) => file,
            Err(e) => {
                tracing::debug!("skipping identity file {}: {e}", path.display());
                return false;
            }
        };
        self.tried_files = true;
        let certificate = identity::certificate_beside(path);
        match file {
            IdentityFile::Open(key) => {
                let public = key.public_key().clone();
                self.sign_in_as(handle, &public, certificate, Signing::Key(Arc::new(key)))
                    .await
            }
            IdentityFile::Locked(locked) => match locked.public.clone() {
                // The agent already holds it unlocked, so there is nothing to ask.
                Some(public) if self.agent.as_ref().is_some_and(|a| a.holds(&public)) => {
                    self.sign_in_as(handle, &public, certificate, Signing::Agent)
                        .await
                }
                Some(public) => {
                    let signer = LockedSigner::new(&locked, unlocker);
                    self.sign_in_as(handle, &public, certificate, Signing::Locked(signer))
                        .await
                }
                // Without its public part a key's passphrase is needed before the server can say
                // whether it wants the key, which is worth asking only for a key someone named.
                None if !named => false,
                None => {
                    let Some(key) = unlocker.unlock(&locked).await else {
                        return false;
                    };
                    let public = key.public_key().clone();
                    self.sign_in_as(handle, &public, certificate, Signing::Key(Arc::new(key)))
                        .await
                }
            },
        }
    }

    async fn sign_in_as(
        &mut self,
        handle: &mut client::Handle<Handler>,
        public: &PublicKey,
        certificate: Option<Certificate>,
        mut signing: Signing<'_>,
    ) -> bool {
        let certificate = certificate
            .filter(|cert| cert.public_key() == public.key_data())
            .or_else(|| {
                self.certificates
                    .iter()
                    .find(|cert| cert.public_key() == public.key_data())
                    .cloned()
            });
        if let Some(cert) = certificate
            && self.first_try(public, true)
        {
            match self.attempt(handle, public, Some(cert), &mut signing).await {
                Attempt::SignedIn => return true,
                Attempt::Skipped => return false,
                Attempt::Refused => {}
            }
        }
        if !self.first_try(public, false) {
            return false;
        }
        self.attempt(handle, public, None, &mut signing).await == Attempt::SignedIn
    }

    async fn attempt(
        &mut self,
        handle: &mut client::Handle<Handler>,
        public: &PublicKey,
        certificate: Option<Certificate>,
        signing: &mut Signing<'_>,
    ) -> Attempt {
        let (username, hash) = (self.username, self.hash_for(public));
        let result = match (signing, certificate) {
            (Signing::Key(key), Some(cert)) => handle
                .authenticate_openssh_cert(username, key.clone(), cert)
                .await
                .map_err(|e| format!("{e:?}")),
            (Signing::Key(key), None) => {
                let key = PrivateKeyWithHashAlg::new(key.clone(), self.rsa_hash);
                handle
                    .authenticate_publickey(username, key)
                    .await
                    .map_err(|e| format!("{e:?}"))
            }
            (Signing::Agent, certificate) => {
                let Some(agent) = &mut self.agent else {
                    return Attempt::Refused;
                };
                match certificate {
                    Some(cert) => handle
                        .authenticate_certificate_with(username, cert, hash, &mut agent.client)
                        .await
                        .map_err(|e| format!("{e:?}")),
                    None => handle
                        .authenticate_publickey_with(
                            username,
                            public.clone(),
                            hash,
                            &mut agent.client,
                        )
                        .await
                        .map_err(|e| format!("{e:?}")),
                }
            }
            (Signing::Locked(signer), certificate) => {
                let result = match certificate {
                    Some(cert) => {
                        handle
                            .authenticate_certificate_with(username, cert, hash, signer)
                            .await
                    }
                    None => {
                        handle
                            .authenticate_publickey_with(username, public.clone(), hash, signer)
                            .await
                    }
                };
                match result {
                    Err(SignError::Skipped) => return Attempt::Skipped,
                    result => result.map_err(|e| e.to_string()),
                }
            }
        };
        if self.signed_in(result) {
            Attempt::SignedIn
        } else {
            Attempt::Refused
        }
    }

    async fn agent_keys(&mut self, handle: &mut client::Handle<Handler>) -> bool {
        let Some(agent) = &self.agent else {
            return false;
        };
        self.tried_agent = true;
        for held in agent.identities.clone() {
            let signed_in = match held.certificate {
                Some(cert) => {
                    self.first_try(&held.key, true)
                        && self
                            .attempt(handle, &held.key, Some(cert), &mut Signing::Agent)
                            .await
                            == Attempt::SignedIn
                }
                None => {
                    self.first_try(&held.key, false)
                        && self
                            .attempt(handle, &held.key, None, &mut Signing::Agent)
                            .await
                            == Attempt::SignedIn
                }
            };
            if signed_in {
                return true;
            }
        }
        false
    }
}

enum Signing<'a> {
    Key(Arc<PrivateKey>),
    Agent,
    Locked(LockedSigner<'a>),
}

#[derive(Debug, PartialEq, Eq)]
enum Attempt {
    SignedIn,
    Refused,
    // The user skipped the key's passphrase, so its plain key isn't worth asking about again.
    Skipped,
}

#[derive(Debug, Clone, Copy, Default)]
struct Offered {
    password: bool,
    keyboard: bool,
}

impl Offered {
    fn by(result: &AuthResult) -> Option<Self> {
        match result {
            AuthResult::Success => None,
            AuthResult::Failure {
                remaining_methods, ..
            } => Some(Self {
                password: remaining_methods.contains(&MethodKind::Password),
                keyboard: remaining_methods.contains(&MethodKind::KeyboardInteractive),
            }),
        }
    }
}

struct Asker<'a> {
    sink: &'a SessionSink,
    jump_host: Option<&'a str>,
    username: &'a str,
    config: &'a ResolvedConfig,
}

impl Asker<'_> {
    fn authenticating(&self) {
        self.sink.status(SessionStatus::Authenticating {
            jump_host: self.jump_host.map(str::to_owned),
        });
    }
}

/// A refused round starts over, as OpenSSH does, so a mistyped one-time code isn't the end. A round
/// that asked the user nothing was refused for the password alone, which won't do better twice.
async fn keyboard_rounds(
    handle: &mut client::Handle<Handler>,
    asker: &Asker<'_>,
    password: Option<&SecretString>,
) -> Result<bool, SessionError> {
    let mut refused = false;
    for _ in 0..PASSWORD_TRIES {
        match keyboard_interactive(handle, asker, password, refused).await? {
            Round::SignedIn => return Ok(true),
            Round::Refused { asked_user: false } => return Ok(false),
            Round::Refused { asked_user: true } => refused = true,
        }
    }
    Ok(false)
}

enum Round {
    SignedIn,
    Refused { asked_user: bool },
}

/// Answers the questions that ask for a password with `password`, once, since being asked again
/// means it was wrong, and asks the user on the Tab for every other one.
async fn keyboard_interactive(
    handle: &mut client::Handle<Handler>,
    asker: &Asker<'_>,
    password: Option<&SecretString>,
    refused: bool,
) -> Result<Round, SessionError> {
    let mut password = password;
    let mut asked_user = false;
    let mut reply = handle
        .authenticate_keyboard_interactive_start(asker.username, None)
        .await
        .map_err(SessionError::protocol)?;
    loop {
        let (name, instructions, prompts) = match reply {
            KeyboardInteractiveAuthResponse::Success => return Ok(Round::SignedIn),
            KeyboardInteractiveAuthResponse::Failure { .. } => {
                return Ok(Round::Refused { asked_user });
            }
            KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => (name, instructions, prompts),
        };
        let mut answers: Vec<Option<String>> = prompts
            .iter()
            .map(|prompt| {
                password
                    .filter(|_| asks_for_password(prompt))
                    .map(|password| password.expose_secret().to_owned())
            })
            .collect();
        if answers.iter().any(Option::is_some) {
            password = None;
        }
        let questions: Vec<Question> = prompts
            .iter()
            .zip(&answers)
            .filter(|(_, answer)| answer.is_none())
            .map(|(prompt, _)| Question {
                text: prompt.prompt.clone(),
                echo: prompt.echo,
            })
            .collect();
        if !questions.is_empty() {
            let asked = asker.sink.ask_answers(
                asker.jump_host.map(str::to_owned),
                Prompt::Questions {
                    username: asker.username.to_owned(),
                    host: asker.config.host.clone(),
                    port: asker.config.port,
                    name,
                    instructions,
                    questions,
                    refused: refused && !asked_user,
                },
            );
            asked_user = true;
            let typed = asked.await.ok_or(SessionError::Cancelled)?;
            asker.authenticating();
            let mut typed = typed.into_iter();
            for answer in answers.iter_mut().filter(|answer| answer.is_none()) {
                *answer = typed.next().map(|typed| typed.expose_secret().to_owned());
            }
        }
        let answers: Option<Vec<String>> = answers.into_iter().collect();
        let answers = answers.ok_or_else(|| {
            SessionError::protocol("Ela didn't get an answer for every question the server asked.")
        })?;
        reply = handle
            .authenticate_keyboard_interactive_respond(answers)
            .await
            .map_err(SessionError::protocol)?;
    }
}

// PAM's own prompt is "Password: ", but a translated one won't match and is asked on the Tab.
fn asks_for_password(prompt: &client::Prompt) -> bool {
    !prompt.echo && prompt.prompt.to_lowercase().contains("password")
}

struct PasswordAsk<'a> {
    asker: &'a Asker<'a>,
    keep: Option<Uuid>,
    offered: Offered,
    tried: Vec<Tried>,
}

async fn ask_for_password(
    handle: &mut client::Handle<Handler>,
    asking: PasswordAsk<'_>,
) -> Result<bool, SessionError> {
    let PasswordAsk {
        asker,
        keep,
        offered,
        tried,
    } = asking;
    let (sink, username, config) = (asker.sink, asker.username, asker.config);
    // A password that arrives with the configuration always comes from the Vault.
    let mut note = if config.password.is_some() {
        Some(PasswordNote::SavedWrong)
    } else if config.saved_password_lost {
        Some(PasswordNote::Missing)
    } else {
        None
    };
    for _ in 0..PASSWORD_TRIES {
        let asked = sink.ask_password(
            asker.jump_host.map(str::to_owned),
            Prompt::Password {
                username: username.to_owned(),
                host: config.host.clone(),
                port: config.port,
                note,
                can_remember: keep.is_some(),
                tried: tried.clone(),
            },
        );
        let Some(PasswordAnswer { password, remember }) = asked.await else {
            return Ok(false);
        };
        asker.authenticating();
        let signed_in = if offered.password {
            let res = handle
                .authenticate_password(username, password.expose_secret())
                .await
                .map_err(SessionError::protocol)?;
            if Offered::by(&res).is_some_and(|offered| !offered.password) {
                return Ok(false);
            }
            res.success()
        } else {
            keyboard_rounds(handle, asker, Some(&password)).await?
        };
        if signed_in {
            if let Some(connection) = keep.filter(|_| remember) {
                sink.keep_password(connection, password).await;
            }
            return Ok(true);
        }
        note = Some(PasswordNote::Wrong);
    }
    Ok(false)
}

/// The account database, as OpenSSH asks it, rather than `$USER`, which anything can set.
fn whoami() -> String {
    whoami::username()
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "root".into())
}

#[derive(Debug)]
enum HandlerError {
    Ssh(russh::Error),
    Rejected(SessionError),
}

impl From<russh::Error> for HandlerError {
    fn from(e: russh::Error) -> Self {
        Self::Ssh(e)
    }
}

struct Handler {
    host: String,
    port: u16,
    jump_host: Option<String>,
    sink: SessionSink,
    known_hosts: HostFiles,
    awaiting_user: Arc<AtomicBool>,
}

impl Handler {
    async fn ask(
        &self,
        key: &russh::keys::PublicKey,
        fingerprint: &str,
        known: Option<String>,
        known_types: Vec<String>,
    ) -> bool {
        self.awaiting_user.store(true, Ordering::Relaxed);
        let trust = self
            .sink
            .ask_trust(
                self.jump_host.clone(),
                Prompt::Trust {
                    algorithm: key.algorithm().as_str().to_owned(),
                    fingerprint: fingerprint.to_owned(),
                    known,
                    known_types,
                },
            )
            .await;
        self.awaiting_user.store(false, Ordering::Relaxed);
        if trust {
            self.sink.status(SessionStatus::Verifying {
                fingerprint: fingerprint.to_owned(),
                jump_host: self.jump_host.clone(),
            });
        }
        trust
    }
}

impl client::Handler for Handler {
    type Error = HandlerError;

    async fn auth_banner(
        &mut self,
        banner: &str,
        _: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // A framebuffer Session has no terminal, and its Jump Hosts' banners go nowhere.
        let _ = self.sink.terminal(banner_text(banner)).await;
        Ok(())
    }

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let presented = match server_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(cert) => cert.public_key().clone().into(),
        };
        let fingerprint = presented.fingerprint(HashAlg::Sha256).to_string();
        self.sink.status(SessionStatus::Verifying {
            fingerprint: fingerprint.clone(),
            jump_host: self.jump_host.clone(),
        });

        let (host, port, files) = (self.host.as_str(), self.port, &self.known_hosts);
        let verdict = known_hosts::verdict(files, host, port, server_key).map_err(|e| {
            // Never connect to a host that couldn't be checked.
            HandlerError::Rejected(SessionError::protocol(format!(
                "Can't check this host: {e}"
            )))
        })?;
        match verdict {
            Verdict::Known => Ok(true),
            Verdict::Revoked => Err(HandlerError::Rejected(SessionError::HostKeyRevoked(
                fingerprint,
            ))),
            Verdict::New { key, known_types } => {
                if !self.ask(&key, &fingerprint, None, known_types).await {
                    return Err(HandlerError::Rejected(SessionError::Cancelled));
                }
                known_hosts::learn(files, host, port, &key).map_err(HandlerError::Rejected)?;
                Ok(true)
            }
            Verdict::Changed { key, saved } => {
                let known = saved
                    .first()
                    .map(|saved| saved.fingerprint(HashAlg::Sha256).to_string());
                if !self
                    .ask(&key, &fingerprint, known.clone(), Vec::new())
                    .await
                {
                    let saved = known.unwrap_or_else(|| "the saved key".to_owned());
                    let detail = format!("{fingerprint} does not match {saved}");
                    return Err(HandlerError::Rejected(SessionError::HostKeyChanged(detail)));
                }
                known_hosts::replace(files, host, port, &key).map_err(HandlerError::Rejected)?;
                Ok(true)
            }
        }
    }
}

struct SshSession {
    handle: Arc<client::Handle<Handler>>,
    channel: russh::Channel<client::Msg>,
    early: Vec<Bytes>,
    // The target's connection runs inside these, so they live as long as the Session.
    jump_hosts: JumpChain,
}

#[async_trait]
impl Session for SshSession {
    fn files(&self) -> Option<Arc<dyn RemoteFiles>> {
        Some(Arc::new(files::SshFiles::new(self.handle.clone())))
    }

    async fn run(self: Box<Self>, mut io: SessionIo) -> Result<(), SessionError> {
        let SshSession {
            handle,
            channel,
            jump_hosts,
            early,
        } = *self;
        for data in early {
            io.sink.terminal(data).await?;
        }
        let (mut reader, writer) = channel.split();

        // Output runs in its own task so a slow UI (backpressure) never blocks input:
        // Ctrl+C must get through while a flood is still being drawn.
        let sink = io.sink.clone();
        let mut output = tokio::spawn(async move {
            while let Some(msg) = reader.wait().await {
                match msg {
                    ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                        sink.terminal(Bytes::copy_from_slice(&data)).await?;
                    }
                    ChannelMsg::ExitStatus { .. } | ChannelMsg::Eof | ChannelMsg::Close => break,
                    _ => {}
                }
            }
            Ok::<(), SessionError>(())
        });

        let result = loop {
            tokio::select! {
                res = &mut output => break res.unwrap_or(Ok(())),
                cmd = io.commands.recv() => match cmd {
                    Some(SessionCommand::Text { data }) => {
                        if let Err(e) = writer.data(data.as_bytes()).await {
                            break Err(SessionError::protocol(e));
                        }
                    }
                    Some(SessionCommand::Resize { width, height }) => {
                        let _ = writer.window_change(width, height, 0, 0).await;
                    }
                    Some(_) => {}
                    None => break Ok(()),
                },
                _ = io.cancel.cancelled() => break Err(SessionError::Cancelled),
            }
        };

        output.abort();
        let _ = handle.disconnect(Disconnect::ByApplication, "", "en").await;
        jump_hosts.disconnect().await;
        result
    }
}

#[cfg(test)]
mod tests {
    use ela_session::Size;

    use super::*;

    fn config(options: serde_json::Value) -> ResolvedConfig {
        ResolvedConfig {
            connection: None,
            host: "h".into(),
            port: 22,
            username: None,
            password: None,
            options: options.as_object().unwrap().clone(),
            initial_size: Size {
                width: 80,
                height: 24,
            },
            jump_hosts: Vec::new(),
            saved_password_lost: false,
        }
    }

    #[test]
    fn a_banner_keeps_its_text_and_loses_escape_sequences() {
        assert_eq!(
            banner_text("Authorized use only.\n\x1b]0;owned\x07Bye\x1b[2J\u{9b}31m"),
            "Authorized use only.\r\n]0;ownedBye[2J31m\r\n"
        );
        assert_eq!(banner_text("tab\there\r\n"), "tab\there\r\n");
        assert_eq!(banner_text(""), "");
    }

    #[test]
    fn the_terminal_is_utf_8_with_delete_for_backspace() {
        assert!(TERMINAL_MODES.contains(&(Pty::IUTF8, 1)));
        assert!(TERMINAL_MODES.contains(&(Pty::VERASE, 127)));
    }

    #[test]
    fn only_a_hidden_question_about_a_password_gets_the_password() {
        let prompt = |text: &str, echo| client::Prompt {
            prompt: text.into(),
            echo,
        };
        assert!(asks_for_password(&prompt("Password: ", false)));
        assert!(asks_for_password(&prompt("ops@gate's password: ", false)));
        assert!(!asks_for_password(&prompt("Verification code: ", false)));
        assert!(!asks_for_password(&prompt("Password: ", true)));
    }

    #[test]
    fn a_refused_forward_names_the_jump_host_and_the_address() {
        let failed = |host, failure| {
            forward_failed("gate", host, 22, russh::Error::ChannelOpenFailure(failure))
        };
        assert!(matches!(
            failed("db", ChannelOpenFailure::AdministrativelyProhibited),
            SessionError::ForwardFailed { jump_host, address, failure: ForwardFailure::Prohibited }
                if jump_host == "gate" && address == "db:22"
        ));
        assert!(matches!(
            failed("fe80::1", ChannelOpenFailure::ConnectFailed),
            SessionError::ForwardFailed { address, failure: ForwardFailure::Unreachable, .. }
                if address == "[fe80::1]:22"
        ));
    }

    #[test]
    fn a_failure_on_a_jump_host_says_which_but_a_cancel_stays_a_cancel() {
        let jump = JumpHost {
            name: "gate".into(),
            config: config(json!({})),
        };
        assert!(matches!(
            on_jump_host(&jump, SessionError::Timeout),
            SessionError::JumpHost { jump_host, host, port: 22, error }
                if jump_host == "gate" && host == "h" && matches!(*error, SessionError::Timeout)
        ));
        assert!(matches!(
            on_jump_host(&jump, SessionError::Cancelled),
            SessionError::Cancelled
        ));
    }
}
