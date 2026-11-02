use std::borrow::Cow;
use std::str::FromStr;

use ela_session::ResolvedConfig;
use russh::keys::Algorithm;
use russh::{Preferred, cipher, kex, mac};

use crate::known_hosts::wildcard;

const KEY_NAMES: [&str; 7] = [
    "ssh-ed25519",
    "ecdsa-sha2-nistp256",
    "ecdsa-sha2-nistp384",
    "ecdsa-sha2-nistp521",
    "rsa-sha2-512",
    "rsa-sha2-256",
    "ssh-rsa",
];

/// Old network gear may need `ssh-rsa` or SHA-1 key exchange, which only a list the user wrote
/// turns back on. A list that leaves nothing Ela speaks is ignored, since it couldn't connect.
pub(crate) fn preferred(mut preferred: Preferred, config: &ResolvedConfig) -> Preferred {
    if let Some(spec) = config.option_str("host_key_algorithms") {
        let all: Vec<Algorithm> = KEY_NAMES
            .iter()
            .filter_map(|name| Algorithm::from_str(name).ok())
            .collect();
        let key = modified(&preferred.key, &all, spec, |a| a.as_str().to_owned());
        if !preferred.host_key_certificates.is_empty() {
            preferred.host_key_certificates = Cow::Owned(key.clone());
        }
        preferred.key = Cow::Owned(key);
    }
    if let Some(spec) = config.option_str("kex_algorithms") {
        let all: Vec<kex::Name> = kex::ALL_KEX_ALGORITHMS.iter().map(|n| **n).collect();
        let mut chosen = modified(&preferred.kex, &all, spec, |n| n.as_ref().to_owned());
        // The extension markers ride along in the same list, and OpenSSH adds them itself.
        for marker in preferred.kex.iter() {
            let name = marker.as_ref();
            if (name.starts_with("ext-info-") || name.starts_with("kex-strict-"))
                && !chosen.contains(marker)
            {
                chosen.push(*marker);
            }
        }
        preferred.kex = Cow::Owned(chosen);
    }
    if let Some(spec) = config.option_str("ciphers") {
        let all: Vec<cipher::Name> = cipher::ALL_CIPHERS.iter().map(|n| **n).collect();
        let chosen = modified(&preferred.cipher, &all, spec, |n| n.as_ref().to_owned());
        preferred.cipher = Cow::Owned(chosen);
    }
    if let Some(spec) = config.option_str("macs") {
        let all: Vec<mac::Name> = mac::ALL_MAC_ALGORITHMS.iter().map(|n| **n).collect();
        let chosen = modified(&preferred.mac, &all, spec, |n| n.as_ref().to_owned());
        preferred.mac = Cow::Owned(chosen);
    }
    preferred
}

pub(crate) fn allows_ssh_rsa(config: &ResolvedConfig) -> bool {
    config
        .option_str("pubkey_accepted_algorithms")
        .is_some_and(|spec| {
            let all = ["ssh-rsa".to_owned()];
            modified(&[], &all, spec, Clone::clone).contains(&"ssh-rsa".to_owned())
        })
}

fn modified<T: Clone + PartialEq>(
    default: &[T],
    all: &[T],
    spec: &str,
    name: impl Fn(&T) -> String,
) -> Vec<T> {
    let spec = spec.trim();
    let (mode, list) = match spec.chars().next() {
        Some(mode @ ('+' | '-' | '^')) => (mode, &spec[1..]),
        _ => ('=', spec),
    };
    let patterns: Vec<&str> = list
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let matches = |item: &T| patterns.iter().any(|p| wildcard(p, &name(item)));
    let mut picked: Vec<T> = Vec::new();
    for pattern in &patterns {
        for item in all {
            if wildcard(pattern, &name(item)) && !picked.contains(item) {
                picked.push(item.clone());
            }
        }
    }
    let chosen: Vec<T> = match mode {
        '+' => default
            .iter()
            .cloned()
            .chain(picked.into_iter().filter(|item| !default.contains(item)))
            .collect(),
        '^' => {
            let rest: Vec<T> = default
                .iter()
                .filter(|item| !picked.contains(item))
                .cloned()
                .collect();
            picked.into_iter().chain(rest).collect()
        }
        '-' => default
            .iter()
            .filter(|item| !matches(item))
            .cloned()
            .collect(),
        _ => picked,
    };
    if chosen.is_empty() {
        tracing::warn!(
            spec,
            "no algorithm Ela knows is left, so the default list stays"
        );
        return default.to_vec();
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[String]) -> Vec<&str> {
        list.iter().map(String::as_str).collect()
    }

    fn apply(default: &[&str], spec: &str) -> Vec<String> {
        let default: Vec<String> = default.iter().map(|s| s.to_string()).collect();
        let all: Vec<String> = ["a", "b", "c", "d-sha1", "e-sha1"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        modified(&default, &all, spec, Clone::clone)
    }

    #[test]
    fn the_four_forms_of_a_list_work_as_in_openssh() {
        assert_eq!(names(&apply(&["a", "b"], "c,a")), ["c", "a"]);
        assert_eq!(
            names(&apply(&["a", "b"], "+d-sha1,a")),
            ["a", "b", "d-sha1"]
        );
        assert_eq!(names(&apply(&["a", "b"], "^c")), ["c", "a", "b"]);
        assert_eq!(names(&apply(&["a", "d-sha1", "e-sha1"], "-*-sha1")), ["a"]);
        assert_eq!(names(&apply(&["a"], "+*-sha1")), ["a", "d-sha1", "e-sha1"]);
    }

    #[test]
    fn a_list_that_leaves_nothing_keeps_the_default() {
        assert_eq!(names(&apply(&["a"], "unknown")), ["a"]);
        assert_eq!(names(&apply(&["a"], "-a")), ["a"]);
    }

    fn config(options: serde_json::Value) -> ResolvedConfig {
        ResolvedConfig {
            connection: None,
            host: "h".into(),
            port: 22,
            username: None,
            password: None,
            options: options.as_object().unwrap().clone(),
            initial_size: ela_session::Size {
                width: 80,
                height: 24,
            },
            jump_hosts: Vec::new(),
            saved_password_lost: false,
        }
    }

    #[test]
    fn old_gear_gets_ssh_rsa_and_sha1_key_exchange_when_asked_for() {
        let preferred = preferred(
            Preferred::DEFAULT,
            &config(serde_json::json!({
                "host_key_algorithms": "+ssh-rsa",
                "kex_algorithms": "+diffie-hellman-group14-sha1,diffie-hellman-group1-sha1",
                "ciphers": "^aes128-cbc",
            })),
        );
        assert_eq!(preferred.key.last(), Some(&Algorithm::Rsa { hash: None }));
        let kex: Vec<&str> = preferred.kex.iter().map(|n| n.as_ref()).collect();
        assert!(kex.contains(&"diffie-hellman-group14-sha1"));
        assert!(kex.contains(&"diffie-hellman-group1-sha1"));
        assert!(kex.contains(&"ext-info-c"));
        assert_eq!(preferred.cipher[0].as_ref(), "aes128-cbc");
    }

    #[test]
    fn ssh_rsa_signatures_need_to_be_allowed() {
        assert!(!allows_ssh_rsa(&config(serde_json::json!({}))));
        assert!(allows_ssh_rsa(&config(
            serde_json::json!({ "pubkey_accepted_algorithms": "+ssh-rsa" })
        )));
        assert!(!allows_ssh_rsa(&config(
            serde_json::json!({ "pubkey_accepted_algorithms": "ssh-ed25519" })
        )));
    }
}
