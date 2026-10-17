//! The worker that evaluates submissions for hashi.rs.
//!
//! It runs on its own machine that only starts when there is work: the website posts a job,
//! the worker answers right away, evaluates in the background and posts [`Report`]s back.
//! When nothing happened for a while it exits, which stops the machine.
//!
//! Settings (environment):
//! - `DOJO_TOKEN`: shared secret, required on every request in both directions
//! - `APP_URL`: where reports go, e.g. `https://hashi.rs` (posted to `/dojo/report`)
//! - `PORT` (8080), `DOJO_THREADS` (all cores), `DOJO_MEMORY_MB` (1024 per run)
//! - `DOJO_IDLE_EXIT_SECS`: exit after this long without work (unset: never)
//! - `DOJO_CACHE_DIR`: keeps generated sets here (a volume), generating one takes minutes
//!
//! `POST /jobs?id=&season=&seed=` with the WebAssembly module as body: 202 when queued, 422 with
//! a [`Rejection`] when the module can't be run.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::bench::{self, Instance};
use crate::eval::{Options, evaluate};
use crate::sandbox::Sandbox;
use crate::{MAX_WASM_BYTES, Report, Score};

#[derive(Deserialize)]
struct JobQuery {
    id: i64,
    season: u32,
    seed: String,
}

struct Job {
    id: i64,
    season: u32,
    seed: String,
    wasm: Bytes,
}

#[derive(Clone)]
struct Shared {
    token: Arc<str>,
    jobs: mpsc::UnboundedSender<Job>,
    /// Jobs waiting or running.
    pending: Arc<AtomicUsize>,
    /// Seconds since the epoch of the last request or finished job.
    active: Arc<AtomicU64>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let expected = format!("Bearer {token}");
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.len() == expected.len()
                && v.bytes()
                    .zip(expected.bytes())
                    .fold(0, |acc, (a, b)| acc | (a ^ b))
                    == 0
        })
}

async fn submit(
    State(shared): State<Shared>,
    Query(q): Query<JobQuery>,
    headers: HeaderMap,
    wasm: Bytes,
) -> Response {
    if !authorized(&headers, &shared.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    shared.active.store(now_secs(), Ordering::Relaxed);
    if let Err(reason) = crate::validate::validate(&wasm) {
        return (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(reason)).into_response();
    }
    shared.pending.fetch_add(1, Ordering::Relaxed);
    let job = Job {
        id: q.id,
        season: q.season,
        seed: q.seed,
        wasm,
    };
    match shared.jobs.send(job) {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

/// Posts reports to the website, retrying a few times.
#[derive(Clone)]
struct Reporter {
    client: reqwest::Client,
    url: String,
    token: Arc<str>,
}

impl Reporter {
    async fn send(&self, report: &Report) {
        for attempt in 0..6u32 {
            let sent = self
                .client
                .post(&self.url)
                .bearer_auth(&*self.token)
                .json(report)
                .timeout(Duration::from_secs(30))
                .send()
                .await;
            match sent {
                Ok(r) if r.status().is_success() => return,
                Ok(r) => eprintln!("report for #{} refused: {}", report.id(), r.status()),
                Err(e) => eprintln!("report for #{} failed: {e}", report.id()),
            }
            tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
        }
    }
}

/// Evaluates jobs one after another on a blocking thread (each one uses every core).
fn evaluator(
    mut jobs: mpsc::UnboundedReceiver<Job>,
    reporter: &Reporter,
    shared: &Shared,
    runtime: &tokio::runtime::Handle,
) {
    let sandbox = match Sandbox::new() {
        Ok(s) => s,
        Err(e) => panic!("no sandbox: {e}"),
    };
    let mut options = Options::default();
    if let Some(threads) = std::env::var("DOJO_THREADS")
        .ok()
        .and_then(|t| t.parse().ok())
    {
        options.threads = threads;
    }
    if let Some(mb) = std::env::var("DOJO_MEMORY_MB")
        .ok()
        .and_then(|t| t.parse::<usize>().ok())
    {
        options.limits.memory_bytes = mb << 20;
    }
    // Sets by seed: every submission of a season gets the same one
    let mut sets: HashMap<String, Arc<Vec<Instance>>> = HashMap::new();
    let cache = std::env::var("DOJO_CACHE_DIR")
        .ok()
        .map(std::path::PathBuf::from);
    let send = |report: Report| runtime.block_on(reporter.send(&report));

    while let Some(job) = jobs.blocking_recv() {
        let (id, season) = (job.id, job.season);
        let started = Instant::now();
        let set = sets
            .entry(job.seed.clone())
            .or_insert_with(|| Arc::new(load_or_generate(cache.as_deref(), &job.seed)))
            .clone();
        if sets.len() > 3 {
            sets.retain(|seed, _| *seed == job.seed);
        }
        let total = crate::count(set.len());
        let report = match sandbox.load(&job.wasm) {
            Err(reason) => Report::Rejected { id, season, reason },
            Ok(solver) => {
                send(Report::Progress {
                    id,
                    season,
                    done: 0,
                    total,
                });
                let last = Mutex::new(Instant::now());
                let results = evaluate(&sandbox, &solver, &set, &options, |done, _| {
                    // At most every few seconds, the page polls anyway
                    let due = {
                        let mut last = last
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let due = last.elapsed() > Duration::from_secs(3) && done < set.len();
                        if due {
                            *last = Instant::now();
                        }
                        due
                    };
                    if due {
                        let report = Report::Progress {
                            id,
                            season,
                            done: crate::count(done),
                            total,
                        };
                        let reporter = reporter.clone();
                        runtime.spawn(async move { reporter.send(&report).await });
                    }
                });
                if results.len() == set.len() {
                    let score = Score::from_results(&results, options.limits.fuel);
                    Report::Finished { id, season, score }
                } else {
                    Report::Failed {
                        id,
                        season,
                        message: "some runs were lost".into(),
                    }
                }
            }
        };
        eprintln!(
            "#{id} season {season}: {:?} in {:.1?}",
            short(&report),
            started.elapsed()
        );
        send(report);
        shared.pending.fetch_sub(1, Ordering::Relaxed);
        shared.active.store(now_secs(), Ordering::Relaxed);
    }
}

/// The set for `seed`, from the cache directory if it was generated before.
fn load_or_generate(cache: Option<&std::path::Path>, seed: &str) -> Vec<Instance> {
    // The file name must not reveal the seed
    let name = {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(format!("file:{seed}").as_bytes());
        let hex = digest.iter().take(12).fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        });
        format!("set-{hex}.json")
    };
    let path = cache.map(|dir| dir.join(&name));
    if let Some(set) = path
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice::<Vec<Instance>>(&bytes).ok())
        .filter(|set| set.len() == bench::SPEC.len())
    {
        return set;
    }
    let started = Instant::now();
    let set = bench::generate(seed);
    eprintln!(
        "generated a set of {} in {:.1?}",
        set.len(),
        started.elapsed()
    );
    if let Some(path) = path {
        let tmp = path.with_extension("tmp");
        let written = serde_json::to_vec(&set)
            .map_err(|e| e.to_string())
            .and_then(|json| std::fs::write(&tmp, json).map_err(|e| e.to_string()))
            .and_then(|()| std::fs::rename(&tmp, &path).map_err(|e| e.to_string()));
        if let Err(e) = written {
            eprintln!("could not cache the set: {e}");
        }
    }
    set
}

fn short(report: &Report) -> String {
    match report {
        Report::Finished { score, .. } => format!(
            "solved {}/{} par2 {:.3}",
            score.solved, score.total, score.par2
        ),
        Report::Rejected { reason, .. } => format!("rejected {reason:?}"),
        other => format!("{other:?}"),
    }
}

pub fn serve() -> Result<(), String> {
    let token: Arc<str> = std::env::var("DOJO_TOKEN")
        .map_err(|_| "DOJO_TOKEN is not set")?
        .into();
    let app_url = std::env::var("APP_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let idle_exit = std::env::var("DOJO_IDLE_EXIT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());

    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let (tx, rx) = mpsc::unbounded_channel();
        let shared = Shared {
            token: token.clone(),
            jobs: tx,
            pending: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicU64::new(now_secs())),
        };
        let reporter = Reporter {
            client: reqwest::Client::new(),
            url: format!("{}/dojo/report", app_url.trim_end_matches('/')),
            token,
        };
        let handle = tokio::runtime::Handle::current();
        let evaluator_shared = shared.clone();
        std::thread::spawn(move || evaluator(rx, &reporter, &evaluator_shared, &handle));

        if let Some(idle) = idle_exit {
            let shared = shared.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    let quiet = now_secs().saturating_sub(shared.active.load(Ordering::Relaxed));
                    if shared.pending.load(Ordering::Relaxed) == 0 && quiet >= idle {
                        eprintln!("idle for {quiet} s, exiting");
                        std::process::exit(0);
                    }
                }
            });
        }

        let app = Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/jobs", post(submit))
            .layer(DefaultBodyLimit::max(MAX_WASM_BYTES + 4096))
            .with_state(shared);
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
            .await
            .map_err(|e| e.to_string())?;
        eprintln!("dojo worker on :{port}, reporting to {app_url}");
        axum::serve(listener, app).await.map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_the_token() {
        let mut headers = HeaderMap::new();
        assert!(!authorized(&headers, "secret"));
        headers.insert(header::AUTHORIZATION, "Bearer secret".parse().unwrap());
        assert!(authorized(&headers, "secret"));
        assert!(!authorized(&headers, "secreT"));
        assert!(!authorized(&headers, "secret2"));
    }

    #[test]
    fn rejection_is_json() {
        let json = serde_json::to_string(&crate::Rejection::Import("env.f".into())).unwrap();
        assert_eq!(json, r#"{"Import":"env.f"}"#);
    }
}
