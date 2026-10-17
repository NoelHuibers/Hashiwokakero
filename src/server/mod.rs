//! Server only code: database, users, the daily puzzle and the SAT Dojo.

pub mod auth;
pub mod daily;
pub mod db;
pub mod dojo;

use leptos::prelude::ServerFnError;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
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
