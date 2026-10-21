//! Server only code: database, users, the daily puzzle, groups, replays and the SAT Dojo.

pub mod admin;
pub mod auth;
pub mod contact;
pub mod daily;
pub mod db;
pub mod dojo;
pub mod groups;
pub mod limits;
pub mod names;
pub mod og;
pub mod replay;
mod router;

pub use router::{build_id, router};

use std::fmt::Write as _;

use leptos::prelude::ServerFnError;

/// An environment variable. The HTTP tests can't set real ones: `set_var` is `unsafe` (other
/// tests run in the same process at the same time), so they bring their own settings.
pub fn env_var(name: &str) -> Option<String> {
    #[cfg(test)]
    if let Some(value) = router::tests::env_var(name) {
        return Some(value.to_string());
    }
    std::env::var(name).ok()
}

/// Lowercase hex digits of some bytes.
pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(2 * bytes.len()), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// Whether the request carries `Authorization: Bearer <value of the env variable>`, compared in
/// constant time. Always false when the variable isn't set.
pub fn bearer_ok(headers: &axum::http::HeaderMap, env: &str) -> bool {
    let Some(token) = env_var(env).filter(|t| !t.is_empty()) else {
        return false;
    };
    let expected = format!("Bearer {token}");
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.len() == expected.len()
                && v.bytes()
                    .zip(expected.bytes())
                    .fold(0, |acc, (a, b)| acc | (a ^ b))
                    == 0
        })
}

/// Trusted transport identity resolved once, before admission and handler extraction.
///
/// A missing identity is explicit: it must never become a shared fictional network or a
/// nullable network hash on an HTTP write protected by a network quota.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientIdentity(Option<std::net::IpAddr>);

impl ClientIdentity {
    fn resolve(
        headers: &axum::http::HeaderMap,
        peer: Option<std::net::SocketAddr>,
        fly: bool,
    ) -> Self {
        // Preserve the deployment's Fly trust contract: only Fly supplies fly-client-ip.
        // Direct deployments ignore every forwarding header, including fly-client-ip.
        let forwarded = fly
            .then(|| headers.get("fly-client-ip"))
            .flatten()
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        Self(forwarded.or_else(|| peer.map(|peer| peer.ip())))
    }

    pub(super) fn network_hash(self, day: i64) -> Result<String, &'static str> {
        self.0
            .map(|ip| network_hash(&ip.to_string(), day))
            .ok_or("Client address unavailable")
    }
}

/// Runs outside admission, so Axum handlers and Leptos' request Parts share this extension.
async fn identify_client(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
    fly: bool,
) -> axum::response::Response {
    let peer = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|peer| peer.0);
    let client = ClientIdentity::resolve(req.headers(), peer, fly);
    req.extensions_mut().insert(client);
    next.run(req).await
}

/// An address made unrecognisable: hashed with a secret and the day, so it only says "the same
/// network as another request today" and can't be traced back or followed across days.
pub fn network_hash(ip: &str, day: i64) -> String {
    // IPv6 clients get a whole /64, count that as one network
    let network = match ip.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V6(ip)) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                v4.to_string()
            } else {
                std::net::Ipv6Addr::from(u128::from(ip) & (u128::MAX << 64)).to_string()
            }
        }
        Ok(std::net::IpAddr::V4(ip)) => ip.to_string(),
        Err(_) => "unknown".into(),
    };
    keyed_hash("network", day, &network)
}

/// SHA-256 of a value with the server's secret and the day: equal values match on the same day,
/// but without the secret nobody can guess the value or link two days.
pub fn keyed_hash(kind: &str, day: i64, value: &str) -> String {
    use sha2::{Digest, Sha256};
    let secret = std::env::var("HASHI_DAILY_SECRET").unwrap_or_else(|_| "hashi-daily".into());
    hex(&Sha256::digest(
        format!("{secret}:{kind}:{day}:{value}").as_bytes(),
    ))
}

/// A count or length as stored in the database.
pub fn count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Runs CPU bound work off the async runtime. The fly.io VM has a single shared CPU, so only a
/// couple of jobs run at the same time and the rest queue up.
pub async fn run_blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, ServerFnError> {
    limits::PUZZLES.run(job).await
}

/// Password hashing has reserved capacity, independent of public puzzle computations.
pub async fn run_auth_blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, ServerFnError> {
    limits::AUTH.run(job).await
}

/// Production must not silently use the published development seeds or an empty secret.
pub fn validate_configuration() -> Result<(), String> {
    validate_secrets(env_var)
}

fn validate_secrets(var: impl Fn(&str) -> Option<String>) -> Result<(), String> {
    let production = var("LEPTOS_ENV").is_some_and(|v| v.eq_ignore_ascii_case("prod"))
        || var("FLY_APP_NAME").is_some();
    if !production {
        return Ok(());
    }
    for name in ["HASHI_DAILY_SECRET", "DOJO_SECRET"] {
        if var(name).is_none_or(|v| v.trim().len() < 32) {
            return Err(format!(
                "{name} must contain at least 32 characters in production"
            ));
        }
    }
    if var("DOJO_URL").is_some_and(|v| !v.is_empty())
        && var("DOJO_TOKEN").is_none_or(|v| v.trim().len() < 32)
    {
        return Err(
            "DOJO_TOKEN must contain at least 32 characters when the worker is enabled".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod configuration_tests {
    use super::*;
    #[test]
    fn ipv6_networks_use_parsed_prefixes() {
        assert_eq!(
            network_hash("2001:db8::1", 1),
            network_hash("2001:0db8:0:0::2", 1)
        );
        assert_ne!(
            network_hash("2001:db8::1", 1),
            network_hash("2001:db8:0:1::1", 1)
        );
        assert_eq!(
            network_hash("::ffff:192.0.2.1", 1),
            network_hash("192.0.2.1", 1)
        );
    }
    #[test]
    fn production_requires_secrets_and_development_does_not() {
        assert!(validate_secrets(|_| None).is_ok());
        assert!(validate_secrets(|k| (k == "LEPTOS_ENV").then(|| "PROD".into())).is_err());
        let settings = |k: &str| match k {
            "LEPTOS_ENV" => Some("PROD".into()),
            "HASHI_DAILY_SECRET" | "DOJO_SECRET" => Some("s".repeat(32)),
            _ => None,
        };
        assert!(validate_secrets(settings).is_ok());
        assert!(
            validate_secrets(|k| if k == "DOJO_URL" {
                Some("http://worker".into())
            } else {
                settings(k)
            })
            .is_err()
        );
    }
    #[test]
    fn transport_identity_respects_the_deployment_trust_boundary() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("forwarded", "for=192.0.2.1".parse().unwrap());
        headers.insert("x-forwarded-for", "192.0.2.2".parse().unwrap());
        headers.insert("fly-client-ip", "192.0.2.3".parse().unwrap());
        let peer = "198.51.100.1:1234".parse().unwrap();
        assert_eq!(
            ClientIdentity::resolve(&headers, Some(peer), false).0,
            Some(peer.ip())
        );
        assert_eq!(ClientIdentity::resolve(&headers, None, false).0, None);
        assert_eq!(
            ClientIdentity::resolve(&headers, Some(peer), true).0,
            Some("192.0.2.3".parse().unwrap())
        );
        for value in [None, Some("invalid"), Some("192.0.2.3, 192.0.2.4")] {
            headers.remove("fly-client-ip");
            if let Some(value) = value {
                headers.insert("fly-client-ip", value.parse().unwrap());
            }
            assert_eq!(
                ClientIdentity::resolve(&headers, Some(peer), true).0,
                Some(peer.ip())
            );
            assert!(
                ClientIdentity::resolve(&headers, None, true)
                    .network_hash(1)
                    .is_err()
            );
        }
    }
}
