mod route;

use std::collections::BTreeSet;

use ela_core::{CredentialRef, Outside, ResolvedConnection};
use serde_json::{Map, Value};
use uuid::Uuid;

pub use route::{
    CONNECT_TIMEOUT, Direct, Opened, Route, RouteStream, Stream, Transport,
    refuse_unsupported_route, route,
};

const SSH_PORT: u16 = 22;

// A Jump Host is an SSH endpoint by definition, so only SSH Connections can be one.
pub const JUMP_HOST_PROTOCOL: &str = "ssh";
pub const ROUTE_OPTION: &str = "proxy_jump";

pub fn route_field() -> Value {
    serde_json::json!({
        "type": "list",
        "label": "Jump Host",
        "hint": "A saved connection's name or user@host:port, one per line, in the order they are passed.",
        "group": "route",
        "mono": true,
    })
}

pub fn parse_route(value: Option<&Value>) -> Result<Vec<JumpSpec>, RouteError> {
    // Text is read strictly, as OpenSSH reads `ProxyJump`, so "gate," isn't quietly one hop.
    let hops: Vec<String> = match value {
        Some(Value::String(text)) if !text.trim().is_empty() => {
            text.split(',').map(|hop| hop.trim().to_owned()).collect()
        }
        other => ela_core::option_list(other),
    };
    if hops.len() == 1 && hops[0].eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    hops.iter().map(|hop| parse_jump(hop)).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpSpec {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hop {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub credential: Option<CredentialRef>,
    pub connection: Option<Uuid>,
    pub options: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    #[error("“{0}” isn't a Jump Host Ela can read. Write it as user@host:port.")]
    Unreadable(String),
    #[error(
        "Several SSH connections are named “{0}”, so Ela can't tell which one is the Jump Host."
    )]
    Ambiguous(String),
    #[error("The Jump Hosts lead back to “{0}”, so the route never ends.")]
    Loop(String),
    #[error(
        "The Jump Host “{0}” goes through a ProxyCommand, which Ela can't do yet. It did not connect directly instead."
    )]
    ProxyCommand(String),
}

pub fn parse_proxy_jump(text: &str) -> Result<Vec<JumpSpec>, RouteError> {
    parse_route(Some(&Value::String(text.to_owned())))
}

fn parse_jump(text: &str) -> Result<JumpSpec, RouteError> {
    let unreadable = || RouteError::Unreadable(text.to_owned());
    let rest = match text.strip_prefix("ssh://") {
        Some(uri) => uri.strip_suffix('/').unwrap_or(uri),
        None => text,
    };
    let (user, address) = match rest.rsplit_once('@') {
        Some(("", _)) => return Err(unreadable()),
        Some((user, address)) => (Some(user.to_owned()), address),
        None => (None, rest),
    };
    let (host, port) = match address.strip_prefix('[') {
        Some(bracketed) => {
            let (host, after) = bracketed.split_once(']').ok_or_else(unreadable)?;
            match after {
                "" => (host, None),
                _ => (host, Some(after.strip_prefix(':').ok_or_else(unreadable)?)),
            }
        }
        None => match address.split_once(':') {
            // More than one colon is an IPv6 address without a port.
            Some((host, port)) if !port.contains(':') => (host, Some(port)),
            _ => (address, None),
        },
    };
    let port = port
        .map(|port| port.parse().ok().filter(|port| *port != 0))
        .map(|port| port.ok_or_else(unreadable))
        .transpose()?;
    if host.is_empty() || host.contains(|c: char| c.is_whitespace() || c == '/') {
        return Err(unreadable());
    }
    Ok(JumpSpec {
        user,
        host: host.to_owned(),
        port,
    })
}

/// Where a route's hop names are looked up: saved SSH Connections, then what a source outside
/// Ela, such as `~/.ssh/config`, says about a hop that isn't one.
pub trait Directory {
    fn saved(&self, name: &str) -> Vec<ResolvedConnection>;

    fn outside(&self, _host: &str) -> Option<Outside> {
        None
    }
}

impl<F: Fn(&str) -> Vec<ResolvedConnection>> Directory for F {
    fn saved(&self, name: &str) -> Vec<ResolvedConnection> {
        self(name)
    }
}

/// Follows OpenSSH: a hop named like a saved SSH Connection takes its address and sign-in from
/// it, and only the first hop's own Jump Hosts are followed, since later hops are reached through
/// the earlier ones.
/// `sign_in` names the options that say how to sign in to a Jump Host, from the Jump Host
/// Protocol's schema, so this crate needn't know them.
pub fn plan(
    target: &Map<String, Value>,
    directory: impl Directory,
    sign_in: &[String],
) -> Result<Vec<Hop>, RouteError> {
    let mut hops = Vec::new();
    let planning = Planning { directory, sign_in };
    planning.expand(target, &mut BTreeSet::new(), &mut hops)?;
    Ok(hops)
}

struct Planning<'a, D> {
    directory: D,
    sign_in: &'a [String],
}

enum Found {
    Saved(ResolvedConnection),
    Written(Option<Outside>),
}

impl<D: Directory> Planning<'_, D> {
    fn expand(
        &self,
        from: &Map<String, Value>,
        seen: &mut BTreeSet<String>,
        hops: &mut Vec<Hop>,
    ) -> Result<(), RouteError> {
        for (index, spec) in parse_route(from.get(ROUTE_OPTION))?.into_iter().enumerate() {
            let mut saved = self.directory.saved(&spec.host);
            if saved.len() > 1 {
                return Err(RouteError::Ambiguous(spec.host));
            }
            let found = match saved.pop() {
                Some(connection) => Found::Saved(connection),
                None => Found::Written(self.directory.outside(&spec.host)),
            };
            let own_options = match &found {
                Found::Saved(connection) => Some(&connection.options),
                Found::Written(outside) => outside.as_ref().map(|o| &o.settings.options),
            };
            if let (0, Some(options)) = (index, own_options) {
                if !seen.insert(spec.host.clone()) {
                    return Err(RouteError::Loop(spec.host));
                }
                if has_text(options, "proxy_command") {
                    return Err(RouteError::ProxyCommand(spec.host));
                }
                self.expand(options, seen, hops)?;
            }
            hops.push(self.hop(spec, found, from));
        }
        Ok(())
    }

    fn hop(&self, spec: JumpSpec, found: Found, from: &Map<String, Value>) -> Hop {
        let saved = match found {
            Found::Saved(saved) => saved,
            Found::Written(outside) => {
                let below = outside.unwrap_or_default();
                // Not saved, so sign in as the outside source says, then the way the Connection
                // that names it does, minus its password.
                let mut options = self.sign_in_options(&below.settings.options);
                for (key, value) in self.sign_in_options(from) {
                    options.entry(key).or_insert(value);
                }
                return Hop {
                    name: spec.host.clone(),
                    host: below.hostname.unwrap_or(spec.host),
                    port: spec.port.or(below.settings.port).unwrap_or(SSH_PORT),
                    username: spec.user.or(below.settings.username),
                    credential: None,
                    connection: None,
                    options,
                };
            }
        };
        // Another account or port may well be another machine, which the password isn't meant for.
        let same_account = spec
            .user
            .as_ref()
            .is_none_or(|user| Some(user) == saved.username.as_ref())
            && spec.port.is_none_or(|port| port == saved.port);
        Hop {
            name: spec.host,
            host: saved.host,
            port: spec.port.unwrap_or(saved.port),
            username: spec.user.or(saved.username),
            credential: saved.credential.filter(|_| same_account),
            connection: same_account.then_some(saved.id),
            options: self.sign_in_options(&saved.options),
        }
    }

    fn sign_in_options(&self, options: &Map<String, Value>) -> Map<String, Value> {
        options
            .iter()
            .filter(|(key, _)| self.sign_in.contains(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
}

fn has_text(options: &Map<String, Value>, key: &str) -> bool {
    options
        .get(key)
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use ela_core::Environment;
    use serde_json::json;

    use super::*;

    fn spec(user: Option<&str>, host: &str, port: Option<u16>) -> JumpSpec {
        JumpSpec {
            user: user.map(Into::into),
            host: host.into(),
            port,
        }
    }

    fn options(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    fn saved(name: &str, host: &str, options_: Value) -> ResolvedConnection {
        ResolvedConnection {
            id: Uuid::new_v4(),
            name: name.into(),
            protocol: "ssh".into(),
            host: host.into(),
            port: 2200,
            username: Some("admin".into()),
            credential: Some(CredentialRef(Uuid::new_v4())),
            environment: Environment::default(),
            options: options(options_),
        }
    }

    fn lookup(all: &[ResolvedConnection]) -> impl Fn(&str) -> Vec<ResolvedConnection> {
        move |name| all.iter().filter(|c| c.name == name).cloned().collect()
    }

    fn plan(
        target: &Map<String, Value>,
        saved: impl Fn(&str) -> Vec<ResolvedConnection>,
    ) -> Result<Vec<Hop>, RouteError> {
        super::plan(target, saved, &["identity_file".to_owned()])
    }

    fn names(hops: &[Hop]) -> Vec<&str> {
        hops.iter().map(|hop| hop.name.as_str()).collect()
    }

    #[test]
    fn reads_the_forms_openssh_accepts() {
        let parsed = parse_proxy_jump(
            "gate, me@gate:2222,ssh://me@[fe80::1]:22/,[::1],fe80::2,ops@corp@bastion",
        )
        .unwrap();
        assert_eq!(
            parsed,
            [
                spec(None, "gate", None),
                spec(Some("me"), "gate", Some(2222)),
                spec(Some("me"), "fe80::1", Some(22)),
                spec(None, "::1", None),
                spec(None, "fe80::2", None),
                spec(Some("ops@corp"), "bastion", None),
            ]
        );
        assert_eq!(parse_proxy_jump(" none "), Ok(vec![]));
        assert_eq!(parse_proxy_jump(""), Ok(vec![]));
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        for bad in [
            "gate,",
            "@gate",
            "gate:0",
            "gate:x",
            "gate:70000",
            "[::1",
            "[::1]x",
            "a b",
            "ssh://gate/path",
        ] {
            assert_eq!(
                parse_proxy_jump(bad),
                Err(RouteError::Unreadable(
                    bad.rsplit(',').next().unwrap().trim().into()
                )),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_saved_jump_host_lends_its_address_and_sign_in() {
        let gate = saved(
            "gate",
            "gate.example",
            json!({ "identity_file": "~/.ssh/gate", "term": "x" }),
        );
        let hops = plan(
            &options(json!({ "proxy_jump": "gate" })),
            lookup(std::slice::from_ref(&gate)),
        )
        .unwrap();
        assert_eq!(
            hops,
            [Hop {
                name: "gate".into(),
                host: "gate.example".into(),
                port: 2200,
                username: Some("admin".into()),
                credential: gate.credential,
                connection: Some(gate.id),
                options: options(json!({ "identity_file": "~/.ssh/gate" })),
            }]
        );
    }

    #[test]
    fn another_account_or_port_on_a_saved_jump_host_gets_no_password_and_keeps_none() {
        let gate = saved("gate", "gate.example", json!({}));
        let all = [gate.clone()];
        let plan_ =
            |route: &str| plan(&options(json!({ "proxy_jump": route })), lookup(&all)).unwrap();
        let same = &plan_("admin@gate:2200")[0];
        assert_eq!(same.credential, gate.credential);
        assert_eq!(same.connection, Some(gate.id));
        let other_user = &plan_("root@gate")[0];
        assert_eq!(other_user.username.as_deref(), Some("root"));
        assert_eq!(other_user.credential, None);
        assert_eq!(other_user.connection, None);
        let other_port = &plan_("gate:22")[0];
        assert_eq!(other_port.port, 22);
        assert_eq!(other_port.credential, None);
        assert_eq!(other_port.connection, None);
    }

    #[test]
    fn an_unsaved_jump_host_uses_the_identity_files_but_no_password() {
        let target =
            options(json!({ "proxy_jump": "me@10.0.0.1", "identity_file": "~/k", "term": "x" }));
        let hops = plan(&target, |_| vec![]).unwrap();
        assert_eq!(
            hops,
            [Hop {
                name: "10.0.0.1".into(),
                host: "10.0.0.1".into(),
                port: 22,
                username: Some("me".into()),
                credential: None,
                connection: None,
                options: options(json!({ "identity_file": "~/k" })),
            }]
        );
    }

    #[test]
    fn the_first_jump_hosts_own_route_comes_first_and_later_ones_are_not_followed() {
        let all = [
            saved("gate", "g", json!({ "proxy_jump": "outer" })),
            saved("outer", "o", json!({ "proxy_jump": "edge" })),
            saved("edge", "e", json!({})),
            saved("inner", "i", json!({ "proxy_jump": "elsewhere" })),
        ];
        let hops = plan(
            &options(json!({ "proxy_jump": "gate,inner" })),
            lookup(&all),
        )
        .unwrap();
        assert_eq!(names(&hops), ["edge", "outer", "gate", "inner"]);
    }

    #[test]
    fn a_route_that_leads_back_on_itself_is_refused() {
        let all = [
            saved("a", "a", json!({ "proxy_jump": "b" })),
            saved("b", "b", json!({ "proxy_jump": "a" })),
        ];
        assert_eq!(
            plan(&options(json!({ "proxy_jump": "a" })), lookup(&all)),
            Err(RouteError::Loop("a".into()))
        );
        let selfish = [saved("s", "s", json!({ "proxy_jump": "s" }))];
        assert_eq!(
            plan(&options(json!({ "proxy_jump": "s" })), lookup(&selfish)),
            Err(RouteError::Loop("s".into()))
        );
    }

    #[test]
    fn a_name_two_connections_share_is_refused() {
        let all = [saved("gate", "a", json!({})), saved("gate", "b", json!({}))];
        assert_eq!(
            plan(&options(json!({ "proxy_jump": "gate" })), lookup(&all)),
            Err(RouteError::Ambiguous("gate".into()))
        );
    }

    #[test]
    fn a_first_jump_host_behind_a_proxy_command_is_refused() {
        let all = [saved("gate", "g", json!({ "proxy_command": "nc %h %p" }))];
        assert_eq!(
            plan(&options(json!({ "proxy_jump": "gate" })), lookup(&all)),
            Err(RouteError::ProxyCommand("gate".into()))
        );
        let later = [
            saved("gate", "g", json!({})),
            saved("inner", "i", json!({ "proxy_command": "x" })),
        ];
        assert!(
            plan(
                &options(json!({ "proxy_jump": "gate,inner" })),
                lookup(&later)
            )
            .is_ok()
        );
    }

    #[test]
    fn no_route_means_no_hops() {
        assert_eq!(plan(&options(json!({})), |_| unreachable!()), Ok(vec![]));
        assert_eq!(
            plan(
                &options(json!({ "proxy_jump": "none" })),
                |_| unreachable!()
            ),
            Ok(vec![])
        );
    }

    #[test]
    fn a_route_reads_as_a_list_too_with_commas_inside_none() {
        assert_eq!(
            parse_route(Some(&json!(["gate", " ops@10.0.0.9:2200 ", ""]))),
            Ok(vec![
                spec(None, "gate", None),
                spec(Some("ops"), "10.0.0.9", Some(2200))
            ])
        );
        assert_eq!(parse_route(Some(&json!(["none"]))), Ok(vec![]));
        assert_eq!(parse_route(None), Ok(vec![]));
    }

    #[test]
    fn only_the_sign_in_options_named_go_to_a_hop() {
        let target = options(json!({
            "proxy_jump": ["10.0.0.9"],
            "identity_file": ["~/.ssh/a"],
            "term": "xterm",
        }));
        let hops = super::plan(&target, |_: &str| vec![], &["identity_file".to_owned()]).unwrap();
        assert_eq!(
            hops[0].options,
            options(json!({ "identity_file": ["~/.ssh/a"] }))
        );
        let hops = super::plan(&target, |_: &str| vec![], &[]).unwrap();
        assert!(hops[0].options.is_empty());
    }

    struct WithFile(Vec<ResolvedConnection>);

    impl Directory for WithFile {
        fn saved(&self, name: &str) -> Vec<ResolvedConnection> {
            self.0.iter().filter(|c| c.name == name).cloned().collect()
        }

        fn outside(&self, host: &str) -> Option<Outside> {
            let options = match host {
                "gate" => json!({ "identity_file": ["~/.ssh/gate"], "proxy_jump": ["edge"] }),
                "edge" => json!({ "term": "not a sign-in option" }),
                _ => return None,
            };
            Some(Outside {
                source: "~/.ssh/config".into(),
                hostname: Some(format!("{host}.example.net")),
                settings: ela_core::Settings {
                    port: Some(2201),
                    username: Some("file-user".into()),
                    options: options.as_object().unwrap().clone(),
                    ..Default::default()
                },
            })
        }
    }

    #[test]
    fn a_hop_that_isnt_saved_takes_what_the_outside_source_says() {
        let target = options(json!({ "proxy_jump": ["gate"], "identity_file": ["~/.ssh/target"] }));
        let hops =
            super::plan(&target, WithFile(Vec::new()), &["identity_file".to_owned()]).unwrap();
        assert_eq!(
            names(&hops),
            ["edge", "gate"],
            "the first hop's own route comes first"
        );
        let gate = &hops[1];
        assert_eq!(gate.host, "gate.example.net");
        assert_eq!(gate.port, 2201);
        assert_eq!(gate.username.as_deref(), Some("file-user"));
        assert_eq!(gate.options["identity_file"], json!(["~/.ssh/gate"]));
        // edge is named by gate, so it signs in the way gate does.
        assert_eq!(hops[0].options["identity_file"], json!(["~/.ssh/gate"]));

        let written = options(json!({ "proxy_jump": ["ops@gate:22"] }));
        let hops = super::plan(&written, WithFile(Vec::new()), &[]).unwrap();
        assert_eq!(hops.last().unwrap().port, 22, "what the route says wins");
        assert_eq!(hops.last().unwrap().username.as_deref(), Some("ops"));
    }
}
