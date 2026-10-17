//! Server only code: database, users, the daily puzzle and the SAT Dojo.

pub mod auth;
pub mod daily;
pub mod db;
pub mod dojo;

use std::fmt::Write as _;

use leptos::prelude::ServerFnError;

/// Lowercase hex digits of some bytes.
pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(2 * bytes.len()), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
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
