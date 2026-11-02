use async_trait::async_trait;
use ela_core::{CredentialRef, JumpHost, ResolvedConfig, ResolvedConnection, Size};
use ela_session::{
    Answer, Prompt, Resolve, SecretString, SessionError, SessionSink, SessionStatus,
};
use ela_transport::Hop;
use ela_vault::{Vault, VaultError};

use crate::{Address, Shared};

pub(crate) struct Pending {
    secrets: Secrets,
    target: Target,
    hops: Vec<Hop>,
    size: Size,
}

enum Target {
    Saved(ResolvedConnection),
    Address { address: Address, port: u16 },
}

impl Pending {
    pub(crate) fn saved(
        vault: Result<Shared<Vault>, String>,
        target: ResolvedConnection,
        hops: Vec<Hop>,
        size: Size,
    ) -> Self {
        Self {
            secrets: Secrets::new(vault),
            target: Target::Saved(target),
            hops,
            size,
        }
    }

    pub(crate) fn address(
        vault: Result<Shared<Vault>, String>,
        address: Address,
        port: u16,
        hops: Vec<Hop>,
        size: Size,
    ) -> Self {
        Self {
            secrets: Secrets::new(vault),
            target: Target::Address { address, port },
            hops,
            size,
        }
    }
}

#[async_trait]
impl Resolve for Pending {
    async fn resolve(self: Box<Self>, sink: &SessionSink) -> Result<ResolvedConfig, SessionError> {
        let Pending {
            mut secrets,
            target,
            hops,
            size,
        } = *self;
        let mut jump_hosts = Vec::with_capacity(hops.len());
        for hop in hops {
            let saved = secrets.read(hop.credential, sink, Some(&hop.name)).await?;
            jump_hosts.push(JumpHost {
                name: hop.name,
                config: ResolvedConfig {
                    connection: hop.connection,
                    host: hop.host,
                    port: hop.port,
                    username: hop.username,
                    password: saved.password,
                    options: hop.options,
                    initial_size: size,
                    jump_hosts: Vec::new(),
                    saved_password_lost: saved.lost,
                },
            });
        }
        let mut config = match target {
            Target::Saved(connection) => {
                let saved = secrets.read(connection.credential, sink, None).await?;
                let mut config = connection.into_config(saved.password, size);
                config.saved_password_lost = saved.lost;
                config
            }
            Target::Address { address, port } => ResolvedConfig {
                connection: None,
                host: address.host,
                port,
                username: address.username.filter(|u| !u.is_empty()),
                password: None,
                options: address.options,
                initial_size: size,
                jump_hosts: Vec::new(),
                saved_password_lost: false,
            },
        };
        config.jump_hosts = jump_hosts;
        Ok(config)
    }
}

#[derive(Default)]
struct Saved {
    password: Option<SecretString>,
    lost: bool,
}

struct Secrets {
    vault: Result<Shared<Vault>, String>,
    // Once the user goes on without the Vault, the rest of the route doesn't ask again.
    without_vault: bool,
}

impl Secrets {
    fn new(vault: Result<Shared<Vault>, String>) -> Self {
        Self {
            vault,
            without_vault: false,
        }
    }

    /// A locked Vault asks on the Tab. Going on without it leaves the Secret out, and the
    /// Protocol asks for the password the way it would for an unsaved one.
    async fn read(
        &mut self,
        reference: Option<CredentialRef>,
        sink: &SessionSink,
        jump_host: Option<&str>,
    ) -> Result<Saved, SessionError> {
        let Some(reference) = reference else {
            return Ok(Saved::default());
        };
        let vault = match &self.vault {
            Ok(vault) => vault.clone(),
            Err(why) => {
                tracing::warn!(%why, "no Vault to read a saved password from");
                return Ok(Saved {
                    password: None,
                    lost: true,
                });
            }
        };
        let mut wrong = false;
        loop {
            match with_vault(&vault, move |vault| vault.read(reference)).await? {
                Ok(password) => {
                    return Ok(Saved {
                        password: Some(password),
                        lost: false,
                    });
                }
                Err(VaultError::Locked) if !self.without_vault => {}
                Err(VaultError::Locked) => return Ok(Saved::default()),
                Err(VaultError::Missing | VaultError::Absent) => {
                    return Ok(Saved {
                        password: None,
                        lost: true,
                    });
                }
                Err(error) => return Err(SessionError::protocol(error)),
            }
            let prompt = Prompt::Vault {
                wrong,
                for_jump_host: jump_host.map(str::to_owned),
            };
            let master = match sink.ask(None, prompt).await {
                None => return Err(SessionError::Cancelled),
                Some(Answer::Vault(Some(master))) => master,
                Some(_) => {
                    self.without_vault = true;
                    return Ok(Saved::default());
                }
            };
            sink.status(SessionStatus::Connecting { jump_host: None });
            match with_vault(&vault, move |vault| vault.unlock(&master)).await? {
                Ok(()) => wrong = false,
                Err(VaultError::WrongPassword) => wrong = true,
                Err(error) => return Err(SessionError::protocol(error)),
            }
        }
    }
}

/// Argon2 takes a noticeable moment and holds the lock meanwhile, so it runs off the async
/// workers.
async fn with_vault<T: Send + 'static>(
    vault: &Shared<Vault>,
    use_vault: impl FnOnce(&mut Vault) -> T + Send + 'static,
) -> Result<T, SessionError> {
    let vault = vault.clone();
    tokio::task::spawn_blocking(move || {
        let mut vault = vault
            .lock()
            .map_err(|_| SessionError::protocol("The Vault failed in another part of Ela."))?;
        Ok(use_vault(&mut vault))
    })
    .await
    .map_err(SessionError::protocol)?
}
