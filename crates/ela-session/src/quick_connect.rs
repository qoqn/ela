use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct QuickTarget {
    pub protocol: String,
    pub username: Option<String>,
    pub host: String,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QuickConnectError {
    #[error("Type an address to connect to.")]
    Empty,
    #[error("“{0}” isn't a port. A port is a number from 1 to 65535.")]
    InvalidPort(String),
    #[error("The IPv6 address is missing its closing “]”.")]
    UnterminatedIpv6,
}

impl QuickTarget {
    pub fn parse(input: &str, default_protocol: &str) -> Result<Self, QuickConnectError> {
        let input = input.trim();
        let (protocol, rest) = match input.split_once("://") {
            Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
            None => (default_protocol.to_owned(), input),
        };
        let rest = rest.trim_end_matches('/');
        let (username, hostport) = match rest.rsplit_once('@') {
            Some((user, hp)) if !user.is_empty() => (Some(user.to_owned()), hp),
            Some((_, hp)) => (None, hp),
            None => (None, rest),
        };
        let (host, port) = if let Some(after) = hostport.strip_prefix('[') {
            let (host, tail) = after
                .split_once(']')
                .ok_or(QuickConnectError::UnterminatedIpv6)?;
            (host, tail.strip_prefix(':'))
        } else if hostport.matches(':').count() == 1 {
            let (h, p) = hostport.split_once(':').unwrap();
            (h, Some(p))
        } else {
            (hostport, None) // bare host or unbracketed IPv6
        };
        if host.is_empty() {
            return Err(QuickConnectError::Empty);
        }
        let port = port
            .map(|p| {
                p.parse::<u16>()
                    .map_err(|_| QuickConnectError::InvalidPort(p.to_owned()))
            })
            .transpose()?;
        Ok(Self {
            protocol,
            username,
            host: host.to_owned(),
            port,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> QuickTarget {
        QuickTarget::parse(s, "ssh").unwrap()
    }

    #[test]
    fn bare_host_defaults_to_ssh() {
        assert_eq!(
            p("example.com"),
            QuickTarget {
                protocol: "ssh".into(),
                username: None,
                host: "example.com".into(),
                port: None
            }
        );
    }

    #[test]
    fn user_host_port() {
        let t = p("root@10.0.0.1:2222");
        assert_eq!(
            (t.username.as_deref(), t.host.as_str(), t.port),
            (Some("root"), "10.0.0.1", Some(2222))
        );
    }

    #[test]
    fn scheme_selects_protocol() {
        let t = p("RDP://admin@win-ad");
        assert_eq!(
            (t.protocol.as_str(), t.username.as_deref(), t.host.as_str()),
            ("rdp", Some("admin"), "win-ad")
        );
    }

    #[test]
    fn ipv6() {
        assert_eq!(p("[::1]:22").host, "::1");
        assert_eq!(p("[::1]:22").port, Some(22));
        assert_eq!(p("fe80::1").host, "fe80::1");
    }

    #[test]
    fn errors() {
        assert_eq!(
            QuickTarget::parse("  ", "ssh"),
            Err(QuickConnectError::Empty)
        );
        assert_eq!(
            QuickTarget::parse("h:99999", "ssh"),
            Err(QuickConnectError::InvalidPort("99999".into()))
        );
        assert_eq!(
            QuickTarget::parse("[::1", "ssh"),
            Err(QuickConnectError::UnterminatedIpv6)
        );
    }
}
