use std::path::{Path, PathBuf};

use ela_session::ResolvedConfig;
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::AgentClient;
use russh::keys::{Certificate, PublicKey};
use tokio::net::UnixStream;

// OpenSSH's own list without the security-key types, which need the hardware to sign.
const DEFAULT_NAMES: [&str; 3] = ["id_rsa", "id_ecdsa", "id_ed25519"];

// Where agents put their socket when nothing exported `SSH_AUTH_SOCK`, as a desktop launch may
// not: systemd's user ssh-agent (Arch, then Fedora), GNOME's gcr and older gnome-keyring, then
// gpg-agent.
const RUNTIME_SOCKETS: [&str; 5] = [
    "ssh-agent.socket",
    "openssh_agent",
    "gcr/ssh",
    "keyring/ssh",
    "gnupg/S.gpg-agent.ssh",
];

#[derive(Debug, Clone, Default)]
pub struct KeySources {
    home: Option<PathBuf>,
    agents: Vec<PathBuf>,
}

impl KeySources {
    pub fn new(home: Option<PathBuf>, agents: Vec<PathBuf>) -> Self {
        Self { home, agents }
    }

    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let agents = match std::env::var_os("SSH_AUTH_SOCK") {
            Some(socket) if !socket.is_empty() => vec![PathBuf::from(socket)],
            _ => std::env::var_os("XDG_RUNTIME_DIR")
                .map(|dir| runtime_sockets(Path::new(&dir)))
                .unwrap_or_default(),
        };
        Self { home, agents }
    }

    pub(crate) fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    pub(crate) fn expand(&self, path: &str) -> PathBuf {
        match (path.strip_prefix("~/"), self.home()) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(path),
        }
    }

    pub(crate) fn named(&self, config: &ResolvedConfig) -> Vec<PathBuf> {
        config
            .option_list("identity_file")
            .iter()
            .map(|path| self.expand(path))
            .collect()
    }

    pub(crate) fn certificates(&self, config: &ResolvedConfig) -> Vec<Certificate> {
        config
            .option_list("certificate_file")
            .iter()
            .filter_map(|path| {
                let path = self.expand(path);
                let text = std::fs::read_to_string(&path).ok()?;
                Certificate::from_openssh(text.trim())
                    .inspect_err(|e| tracing::debug!("skipping {}: {e}", path.display()))
                    .ok()
            })
            .collect()
    }

    /// OpenSSH's IdentityAgent: `none`, `SSH_AUTH_SOCK`, another variable, or a socket path.
    pub(crate) async fn agent_for(&self, config: &ResolvedConfig) -> Option<Agent> {
        let socket = match config.option_str("identity_agent") {
            None => return self.agent().await,
            Some(none) if none.eq_ignore_ascii_case("none") => return None,
            Some(path) => match path
                .strip_prefix('$')
                .or((path == "SSH_AUTH_SOCK").then_some(path))
            {
                Some(variable) => PathBuf::from(std::env::var_os(variable)?),
                None => self.expand(path),
            },
        };
        Self::agent_at(&[socket]).await
    }

    pub(crate) fn defaults(&self) -> Vec<PathBuf> {
        let Some(home) = self.home() else {
            return Vec::new();
        };
        DEFAULT_NAMES
            .iter()
            .map(|name| home.join(".ssh").join(name))
            .filter(|path| path.is_file())
            .collect()
    }

    /// An agent that holds no keys is passed over, because a socket of an agent nobody uses can
    /// sit in front of the one that has them.
    pub(crate) async fn agent(&self) -> Option<Agent> {
        Self::agent_at(&self.agents).await
    }

    async fn agent_at(sockets: &[PathBuf]) -> Option<Agent> {
        for socket in sockets {
            let Ok(mut client) = AgentClient::connect_uds(socket).await else {
                continue;
            };
            let identities: Vec<Held> = match client.request_identities().await {
                Ok(identities) => identities
                    .into_iter()
                    .map(|identity| match identity {
                        AgentIdentity::Certificate { certificate, .. } => Held {
                            key: certificate.public_key().clone().into(),
                            certificate: Some(certificate),
                        },
                        identity => Held {
                            key: identity.public_key().into_owned(),
                            certificate: None,
                        },
                    })
                    .collect(),
                Err(error) => {
                    tracing::debug!("agent at {} didn't answer: {error}", socket.display());
                    continue;
                }
            };
            if !identities.is_empty() {
                return Some(Agent { client, identities });
            }
        }
        None
    }
}

fn runtime_sockets(dir: &Path) -> Vec<PathBuf> {
    RUNTIME_SOCKETS
        .iter()
        .map(|name| dir.join(name))
        .filter(|path| path.exists())
        .collect()
}

pub(crate) struct Agent {
    pub client: AgentClient<UnixStream>,
    pub identities: Vec<Held>,
}

#[derive(Clone)]
pub(crate) struct Held {
    pub key: PublicKey,
    pub certificate: Option<Certificate>,
}

impl Agent {
    pub fn holds(&self, key: &PublicKey) -> bool {
        self.identities
            .iter()
            .any(|held| held.key.key_data() == key.key_data())
    }
}

#[cfg(test)]
mod tests {
    use ela_session::Size;
    use serde_json::json;

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
    fn named_files_split_and_expand_home() {
        let sources = KeySources::new(Some("/home/me".into()), Vec::new());
        let files = sources.named(&config(json!({ "identity_file": "~/.ssh/a, /abs/b ,," })));
        assert_eq!(
            files,
            [PathBuf::from("/home/me/.ssh/a"), PathBuf::from("/abs/b")]
        );
        assert!(KeySources::default().named(&config(json!({}))).is_empty());
    }

    #[test]
    fn default_files_are_the_ones_that_exist_in_openssh_order() {
        let home = tempfile::tempdir().unwrap();
        let ssh = home.path().join(".ssh");
        std::fs::create_dir(&ssh).unwrap();
        for name in ["id_ed25519", "id_rsa", "id_ed25519.pub", "id_dsa"] {
            std::fs::write(ssh.join(name), "").unwrap();
        }
        let sources = KeySources::new(Some(home.path().into()), Vec::new());
        assert_eq!(
            sources.defaults(),
            [ssh.join("id_rsa"), ssh.join("id_ed25519")]
        );
        assert!(KeySources::default().defaults().is_empty());
    }

    #[test]
    fn only_runtime_sockets_that_exist_are_candidates_in_order() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("gnupg")).unwrap();
        std::fs::create_dir_all(dir.path().join("gcr")).unwrap();
        for name in ["gnupg/S.gpg-agent.ssh", "gcr/ssh"] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        assert_eq!(
            runtime_sockets(dir.path()),
            [
                dir.path().join("gcr/ssh"),
                dir.path().join("gnupg/S.gpg-agent.ssh")
            ]
        );
    }
}
