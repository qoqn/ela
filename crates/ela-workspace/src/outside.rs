use std::path::PathBuf;

use ela_core::{Outside, ResolvedConnection, Store};
use ela_import::{Context, SSH_PROTOCOL, SshConfig};
use ela_session::ProtocolRegistry;
use ela_transport::{Directory, JUMP_HOST_PROTOCOL};

use crate::{Address, resolve};

const SSH_CONFIG_SOURCE: &str = "~/.ssh/config";

pub struct SshConfigFile {
    pub path: PathBuf,
    pub context: Context,
}

impl SshConfigFile {
    pub fn from_home() -> Option<Self> {
        let home = std::env::home_dir()?;
        Some(Self {
            path: home.join(".ssh").join("config"),
            context: Context {
                local_user: whoami(),
                home,
            },
        })
    }
}

fn whoami() -> String {
    std::env::var("USER").unwrap_or_default()
}

/// Read once for one connect.
pub struct Outsides {
    ssh: Option<(SshConfig, Context)>,
}

impl Outsides {
    pub fn none() -> Self {
        Self { ssh: None }
    }

    /// A file that can't be read is left out with a warning, as if it were empty.
    pub(crate) fn read(file: Option<&SshConfigFile>) -> Self {
        let Some(file) = file else {
            return Self::none();
        };
        match SshConfig::read(&file.path, &file.context) {
            Ok(config) => Self {
                ssh: Some((
                    config,
                    Context {
                        home: file.context.home.clone(),
                        local_user: file.context.local_user.clone(),
                    },
                )),
            },
            Err(error) => {
                tracing::warn!(%error, "leaving out ~/.ssh/config");
                Self::none()
            }
        }
    }

    pub fn outside(&self, protocol: &str, host: &str) -> Option<Outside> {
        let (config, context) = self.ssh.as_ref().filter(|_| protocol == SSH_PROTOCOL)?;
        let found = config.host(host, context);
        let settings = found.settings();
        let hostname = found.hostname.clone();
        if hostname.is_none() && settings == ela_core::Settings::default() {
            return None;
        }
        Some(Outside {
            source: SSH_CONFIG_SOURCE.into(),
            hostname,
            settings,
        })
    }

    /// A typed address goes where `ssh <address>` would: what the user typed wins.
    pub(crate) fn apply_to(&self, mut address: Address) -> Address {
        let Some(outside) = self.outside(&address.protocol, &address.host) else {
            return address;
        };
        if let Some(hostname) = outside.hostname {
            address.host = hostname;
        }
        address.port = address.port.or(outside.settings.port);
        address.username = address
            .username
            .filter(|user| !user.is_empty())
            .or(outside.settings.username);
        for (key, value) in outside.settings.options {
            address.options.entry(key).or_insert(value);
        }
        address
    }
}

pub(crate) struct Saved<'a> {
    pub store: &'a Store,
    pub registry: &'a ProtocolRegistry,
    pub outsides: &'a Outsides,
}

impl Directory for Saved<'_> {
    fn saved(&self, name: &str) -> Vec<ResolvedConnection> {
        self.store
            .connections()
            .values()
            .filter(|c| c.name == name && c.protocol == JUMP_HOST_PROTOCOL)
            .filter_map(|c| resolve(self.store, self.registry, self.outsides, c.id).ok())
            .collect()
    }

    fn outside(&self, host: &str) -> Option<Outside> {
        self.outsides.outside(JUMP_HOST_PROTOCOL, host)
    }
}

pub(crate) struct Unsaved<'a>(pub &'a Outsides);

impl Directory for Unsaved<'_> {
    fn saved(&self, _: &str) -> Vec<ResolvedConnection> {
        Vec::new()
    }

    fn outside(&self, host: &str) -> Option<Outside> {
        self.0.outside(JUMP_HOST_PROTOCOL, host)
    }
}
