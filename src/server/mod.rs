//! Server only code: database, users, the daily puzzle and the SAT Dojo.

pub mod admin;
pub mod auth;
pub mod daily;
pub mod db;
pub mod dojo;
mod router;

pub use router::router;

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

/// The client's address, as seen by the fly.io proxy (`None` locally).
pub fn client_ip(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("fly-client-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// An address made unrecognisable: hashed with a secret and the day, so it only says "the same
/// network as another request today" and can't be traced back or followed across days.
pub fn network_hash(ip: &str, day: i64) -> String {
    use sha2::{Digest, Sha256};
    let secret = std::env::var("HASHI_DAILY_SECRET").unwrap_or_else(|_| "hashi-daily".into());
    // IPv6 clients get a whole /64, count that as one network
    let network = if ip.contains(':') {
        ip.split(':').take(4).collect::<Vec<_>>().join(":")
    } else {
        ip.to_string()
    };
    hex(&Sha256::digest(
        format!("{secret}:network:{day}:{network}").as_bytes(),
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
    use tokio::sync::Semaphore;
    static PERMITS: Semaphore = Semaphore::const_new(2);

    let _permit = PERMITS.acquire().await.map_err(ServerFnError::new)?;
    tokio::task::spawn_blocking(job)
        .await
        .map_err(ServerFnError::new)?
        .map_err(ServerFnError::new)
}
