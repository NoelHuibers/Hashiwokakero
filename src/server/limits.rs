//! Admission to expensive work. Permits live as long as the actual blocking computation,
//! including after the HTTP request is cancelled. Password work has its own pool.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use leptos::prelude::ServerFnError;
use tokio::sync::Semaphore;

use crate::model::ApiError;

pub(super) static PUZZLES: CpuPool = CpuPool::new(2, 16);
pub(super) static IMPORTS: CpuPool = CpuPool::new(2, 16);
pub(super) static AUTH: CpuPool = CpuPool::new(2, 8);

pub(super) struct CpuPool {
    running: Semaphore,
    admitted: Semaphore,
}

impl CpuPool {
    const fn new(running: usize, waiting: usize) -> Self {
        Self {
            running: Semaphore::const_new(running),
            admitted: Semaphore::const_new(running + waiting),
        }
    }

    pub(super) async fn run<T: Send + 'static>(
        &'static self,
        job: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, ServerFnError> {
        let admitted = self
            .admitted
            .try_acquire()
            .map_err(|_| ApiError::TooManyAttempts.fail())?;
        let permit = tokio::time::timeout(Duration::from_secs(5), self.running.acquire())
            .await
            .map_err(|_| ApiError::TooManyAttempts.fail())?
            .map_err(ServerFnError::new)?;
        tokio::task::spawn_blocking(move || {
            let (_permit, _admitted) = (permit, admitted);
            job()
        })
        .await
        .map_err(ServerFnError::new)?
        .map_err(ServerFnError::new)
    }
}

#[derive(Default)]
struct Rates(HashMap<(String, bool), (u32, Instant)>);

impl Rates {
    fn admit(&mut self, network: String, auth: bool, now: Instant) -> bool {
        const WINDOW: Duration = Duration::from_secs(60);
        self.0
            .retain(|_, (_, since)| now.duration_since(*since) < WINDOW);
        let key = (network, auth);
        // Refuse new keys at capacity instead of letting arbitrary addresses grow memory.
        if self.0.len() >= 4096 && !self.0.contains_key(&key) {
            return false;
        }
        let entry = self.0.entry(key).or_insert((0, now));
        let limit = if auth { 20 } else { 32 };
        if entry.0 >= limit {
            return false;
        }
        entry.0 += 1;
        true
    }
}

/// Per-network budgets precede parsing/password hashing/solver work. In direct deployments the
/// socket peer supplies the address; forwarding headers only come from the configured Fly proxy.
pub async fn admission(req: Request, next: Next) -> Response {
    static RATES: Mutex<Option<Rates>> = Mutex::new(None);
    // Browser journeys share one loopback address. An explicit development-only switch keeps
    // their parallel requests independent; production ignores it even if accidentally set.
    if super::env_var("LEPTOS_ENV").is_some_and(|v| v.eq_ignore_ascii_case("dev"))
        && super::env_var("FLY_APP_NAME").is_none()
        && super::env_var("HASHI_RATE_LIMITS").as_deref() == Some("off")
    {
        return next.run(req).await;
    }
    let endpoint = req.uri().path().strip_prefix("/api/");
    let auth = matches!(endpoint, Some("register" | "login"));
    let expensive = auth
        || matches!(
            endpoint,
            Some(
                "generate_puzzle"
                    | "solve_puzzle"
                    | "puzzle_cnf"
                    | "start_daily"
                    | "daily_today"
                    | "daily_attempt_info"
                    | "submit_daily_attempt"
                    | "load_player"
                    | "load_hall_of_fame"
                    | "month_leaderboard"
                    | "daily_leaderboard"
                    | "load_dojo_board"
                    | "load_dojo_seasons"
                    | "load_dojo_benchmarks"
                    // Proves the puzzle unique with the SAT solver
                    | "post_group_puzzle"
            )
        )
        // Community benchmark uploads: a download from the GBD, decompression and parsing
        // before the per-account quotas can be checked
        || req.uri().path() == "/dojo/benchmarks/submit";
    if expensive {
        let key = req
            .extensions()
            .get::<super::ClientIdentity>()
            .ok_or("Client identity middleware missing")
            .and_then(|client| {
                client.network_hash(super::now_ms().div_euclid(crate::model::MS_PER_DAY))
            });
        match key {
            Ok(key) => {
                let allowed = RATES
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get_or_insert_with(Rates::default)
                    .admit(key, auth, Instant::now());
                if !allowed {
                    return (
                        StatusCode::TOO_MANY_REQUESTS,
                        [("retry-after", "60")],
                        "E:too_many_attempts",
                    )
                        .into_response();
                }
            }
            Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };

    #[tokio::test]
    async fn cancellation_keeps_the_computation_permit() {
        static POOL: CpuPool = CpuPool::new(1, 0);
        let (release, wait) = mpsc::channel::<()>();
        let (started, ready) = tokio::sync::oneshot::channel();
        let active = Arc::new(AtomicUsize::new(0));
        let count = active.clone();
        let caller = tokio::spawn(async move {
            POOL.run(move || {
                count.fetch_add(1, Ordering::SeqCst);
                let _ = started.send(());
                wait.recv().unwrap();
                count.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            })
            .await
        });
        ready.await.unwrap();
        caller.abort();
        let _ = caller.await;
        assert!(POOL.run(|| Ok(())).await.is_err());
        assert_eq!(active.load(Ordering::SeqCst), 1);
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while POOL.admitted.available_permits() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(POOL.run(|| Ok(())).await.is_ok());
    }

    #[test]
    fn network_limits_expire_and_auth_has_its_own_budget() {
        let now = Instant::now();
        let mut rates = Rates::default();
        for _ in 0..32 {
            assert!(rates.admit("a".into(), false, now));
        }
        assert!(!rates.admit("a".into(), false, now));
        assert!(rates.admit("a".into(), true, now));
        assert!(rates.admit("b".into(), false, now));
        assert!(rates.admit("a".into(), false, now + Duration::from_secs(60)));
    }
}
