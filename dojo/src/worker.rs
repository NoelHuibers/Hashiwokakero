//! The worker that evaluates submissions for hashi.rs.
//!
//! It runs on its own machine that only starts when there is work: the website posts a job,
//! the worker answers right away, evaluates in the background and posts [`Report`]s back.
//! When nothing happened for a while it exits, which stops the machine.
//!
//! Settings (environment):
//! - `DOJO_TOKEN`: shared secret the website uses to send jobs. Reports back carry the job's
//!   own token instead, so the worker holds no key that could change other submissions
//! - `APP_URL`: where reports go, e.g. `https://hashi.rs` (posted to `/dojo/report`)
//! - `PORT` (8080), `DOJO_THREADS` (all cores), `DOJO_MEMORY_MB` (1024 per run)
//! - `DOJO_IDLE_EXIT_SECS`: exit after this long without work (unset: never)
//! - `DOJO_CACHE_DIR`: keeps generated sets here (a volume), generating one takes minutes
//!
//! `POST /jobs?id=&season=&seed=` with the WebAssembly module as body: 202 when queued, 422 with
//! a [`Rejection`] when the module can't be run.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
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
use crate::{EVALUATION_VERSION, MAX_WASM_BYTES, Report, Score, WorkerReport};

#[derive(Deserialize)]
struct JobQuery {
    id: i64,
    season: u32,
    seed: String,
    /// Reports about this job carry this token: valid for this one job only.
    report_token: String,
    #[serde(default)]
    evaluation_version: String,
}

struct Job {
    id: i64,
    season: u32,
    seed: String,
    report_token: Arc<str>,
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
    if q.evaluation_version != EVALUATION_VERSION {
        return (StatusCode::CONFLICT, "incompatible evaluator version").into_response();
    }
    shared.active.store(now_secs(), Ordering::Relaxed);
    if let Err(reason) = crate::validate::validate(&wasm) {
        return (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(reason)).into_response();
    }
    if shared
        .pending
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            (n < 2).then_some(n + 1)
        })
        .is_err()
    {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let job = Job {
        id: q.id,
        season: q.season,
        seed: q.seed,
        report_token: q.report_token.into(),
        wasm,
    };
    if shared.jobs.send(job).is_ok() {
        StatusCode::ACCEPTED.into_response()
    } else {
        shared.pending.fetch_sub(1, Ordering::Relaxed);
        StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}

/// Posts reports to the website, retrying a few times.
#[derive(Clone)]
struct Reporter {
    client: reqwest::Client,
    url: String,
}

impl Reporter {
    async fn send(&self, report: &Report, token: &str) {
        for attempt in 0..6u32 {
            let sent = self
                .client
                .post(&self.url)
                .bearer_auth(token)
                .json(&WorkerReport {
                    evaluation_version: EVALUATION_VERSION.into(),
                    report: report.clone(),
                })
                .timeout(Duration::from_secs(30))
                .send()
                .await;
            match sent {
                Ok(r) if r.status().is_success() => return,
                // A finished or requeued evaluation invalidates its token. Retrying a late
                // progress callback cannot make it current again.
                Ok(r)
                    if r.status() == StatusCode::UNAUTHORIZED
                        || r.status() == StatusCode::CONFLICT =>
                {
                    return;
                }
                Ok(r) => eprintln!("report for #{} refused: {}", report.id(), r.status()),
                Err(e) => eprintln!("report for #{} failed: {e}", report.id()),
            }
            tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
        }
    }
}

/// The evaluator's settings from the environment (see the top of this file).
#[derive(Debug, Default)]
struct Settings {
    threads: Option<usize>,
    memory_mb: Option<usize>,
    cache: Option<PathBuf>,
}

impl Settings {
    fn from_env() -> Self {
        let var = |name| std::env::var(name).ok();
        Self {
            threads: var("DOJO_THREADS").and_then(|t| t.parse().ok()),
            memory_mb: var("DOJO_MEMORY_MB").and_then(|t| t.parse().ok()),
            cache: var("DOJO_CACHE_DIR").map(PathBuf::from),
        }
    }
}

/// Evaluates jobs one after another on a blocking thread (each one uses every core). It holds
/// no sender of `jobs`, so it returns once the server that queues them is gone.
fn evaluator(
    mut jobs: mpsc::UnboundedReceiver<Job>,
    reporter: &Reporter,
    pending: &AtomicUsize,
    active: &AtomicU64,
    runtime: &tokio::runtime::Handle,
    settings: &Settings,
) {
    let sandbox = match Sandbox::new() {
        Ok(s) => s,
        Err(e) => panic!("no sandbox: {e}"),
    };
    let mut options = Options::default();
    if let Some(threads) = settings.threads {
        options.threads = threads;
    }
    if let Some(mb) = settings.memory_mb {
        options.limits.memory_bytes = mb << 20;
    }
    // Sets by seed: every submission of a season gets the same one
    let mut sets: HashMap<String, Arc<Vec<Instance>>> = HashMap::new();
    let cache = settings.cache.as_deref();
    while let Some(job) = jobs.blocking_recv() {
        let (id, season) = (job.id, job.season);
        let token = job.report_token.clone();
        let send = |report: Report| runtime.block_on(reporter.send(&report, &token));
        let started = Instant::now();
        let set = sets
            .entry(job.seed.clone())
            .or_insert_with(|| Arc::new(load_or_generate(cache, &job.seed)))
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
                        let token = token.clone();
                        runtime.spawn(async move { reporter.send(&report, &token).await });
                    }
                });
                if results.len() == set.len() {
                    let score = Score::from_results(&results, options.limits.fuel);
                    let fingerprint = crate::fingerprint(&results);
                    Report::Finished {
                        id,
                        season,
                        score,
                        fingerprint,
                    }
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
        pending.fetch_sub(1, Ordering::Relaxed);
        active.store(now_secs(), Ordering::Relaxed);
    }
}

/// The name of the cached set of `seed`. It must not reveal the seed.
fn cache_name(seed: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(format!("file:{EVALUATION_VERSION}:{seed}").as_bytes());
    let hex = digest.iter().take(12).fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    });
    format!("set-{hex}.json")
}

/// The set for `seed`, from the cache directory if it was generated before.
fn load_or_generate(cache: Option<&std::path::Path>, seed: &str) -> Vec<Instance> {
    let path = cache.map(|dir| dir.join(cache_name(seed)));
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
    if token.trim().len() < 32 {
        return Err("DOJO_TOKEN must contain at least 32 characters".into());
    }
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
        };
        let handle = tokio::runtime::Handle::current();
        let (pending, active) = (shared.pending.clone(), shared.active.clone());
        let settings = Settings::from_env();
        std::thread::spawn(move || {
            evaluator(rx, &reporter, &pending, &active, &handle, &settings);
        });

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

        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
            .await
            .map_err(|e| e.to_string())?;
        eprintln!("dojo worker on :{port}, reporting to {app_url}");
        axum::serve(listener, app(shared))
            .await
            .map_err(|e| e.to_string())
    })
}

/// The worker's HTTP routes.
fn app(shared: Shared) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/jobs", post(submit))
        .layer(DefaultBodyLimit::max(MAX_WASM_BYTES + 4096))
        .with_state(shared)
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

    /// Generating a hidden set takes minutes, workers keep them on a volume. A new name or
    /// format makes every worker generate its sets again.
    #[test]
    fn cached_sets_still_load() {
        const CACHE: &str = "the cached sets on the worker's volume wouldn't be found or \
                             wouldn't load any more: every set would be generated again";
        assert_eq!(
            cache_name("golden"),
            "set-7e38cd301c2e58c07d289f0c.json",
            "{CACHE}"
        );
        let cached = r#"[{"name":"pigeonhole-01","family":"Pigeonhole","expected":"Unsat","cnf":{"vars":2,"clauses":[[1],[2],[-1,-2]]},"witness":null},{"name":"queens-01","family":"Queens","expected":"Sat","cnf":{"vars":1,"clauses":[[1]]},"witness":[1]}]"#;
        let set: Vec<Instance> = serde_json::from_str(cached).expect(CACHE);
        assert_eq!(
            (set[0].family, set[0].expected, &set[0].cnf.clauses[2]),
            (
                crate::Family::Pigeonhole,
                crate::Answer::Unsat,
                &vec![-1, -2]
            ),
            "{CACHE}"
        );
        assert_eq!(set[1].witness, Some(vec![1]), "{CACHE}");
        assert_eq!(serde_json::to_string(&set).unwrap(), cached, "{CACHE}");
    }

    /// A set of tiny formulas for `seed` in a fresh cache directory: generating a real one
    /// takes minutes. Even ones are unsatisfiable.
    fn cached_set(seed: &str) -> PathBuf {
        use crate::cnf::Cnf;
        use crate::{Answer, Family};
        let dir = std::env::temp_dir().join(format!("dojo-worker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let set: Vec<Instance> = bench::SPEC
            .iter()
            .enumerate()
            .map(|(i, &(family, _))| {
                let unsat = i % 2 == 0;
                Instance {
                    name: format!("{}-{i:02}", family.id()),
                    family,
                    expected: if unsat { Answer::Unsat } else { Answer::Sat },
                    cnf: if unsat {
                        Cnf::new(1, vec![vec![1], vec![-1]])
                    } else {
                        Cnf::new(1, vec![vec![1]])
                    },
                    witness: (!unsat).then(|| vec![1]),
                }
            })
            .collect();
        assert!(set.iter().any(|i| i.family == Family::Queens));
        std::fs::write(
            dir.join(cache_name(seed)),
            serde_json::to_vec(&set).unwrap(),
        )
        .unwrap();
        dir
    }

    /// The whole worker against a stub website: it checks the token and the module, answers
    /// at once, evaluates in the background and posts its reports with the job's token.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn evaluates_jobs_and_reports_back() {
        // The website, keeping the reports it gets
        let (sent, mut reports) = mpsc::unbounded_channel::<(Option<String>, Report)>();
        let site = Router::new().route(
            "/dojo/report",
            post(
                move |headers: HeaderMap, axum::Json(envelope): axum::Json<WorkerReport>| {
                    assert_eq!(envelope.evaluation_version, EVALUATION_VERSION);
                    let report = envelope.report;
                    let auth = headers
                        .get(header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    sent.send((auth, report)).unwrap();
                    async { StatusCode::NO_CONTENT }
                },
            ),
        );
        let site_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let site_url = format!("http://{}/dojo/report", site_listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(site_listener, site).await.unwrap() });

        let cache = cached_set("worker-test");
        let (jobs, rx) = mpsc::unbounded_channel();
        let shared = Shared {
            token: "secret".into(),
            jobs,
            pending: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicU64::new(0)),
        };
        let reporter = Reporter {
            client: reqwest::Client::new(),
            url: site_url,
        };
        let settings = Settings {
            threads: Some(2),
            memory_mb: Some(64),
            cache: Some(cache.clone()),
        };
        let handle = tokio::runtime::Handle::current();
        let (pending, active) = (shared.pending.clone(), shared.active.clone());
        let evaluating = tokio::task::spawn_blocking({
            let pending = pending.clone();
            move || evaluator(rx, &reporter, &pending, &active, &handle, &settings)
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            axum::serve(listener, app(shared))
                .with_graceful_shutdown(async {
                    stopped.await.ok();
                })
                .await
                .unwrap();
        });

        let client = reqwest::Client::new();
        let health = client.get(format!("{url}/health")).send().await.unwrap();
        assert_eq!(health.text().await.unwrap(), "ok");
        let job = format!(
            "{url}/jobs?id=7&season=2&seed=worker-test&report_token=job-token&evaluation_version={EVALUATION_VERSION}"
        );
        let post =
            |token: &str, wasm: Vec<u8>| client.post(&job).bearer_auth(token).body(wasm).send();

        // Answers UNSAT to everything: right on the even formulas, wrong on the others
        let unsat = wat::parse_str(
            r#"(module
                (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 100) "s UNSATISFIABLE\n")
                (data (i32.const 0) "\64\00\00\00\10\00\00\00")
                (func (export "_start") (drop (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))))"#,
        )
        .unwrap();
        let wrong_token = post("guess", unsat.clone()).await.unwrap();
        assert_eq!(wrong_token.status(), StatusCode::UNAUTHORIZED);
        let not_wasm = post("secret", b"\0asm junk".to_vec()).await.unwrap();
        assert_eq!(not_wasm.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(matches!(
            not_wasm.json::<crate::Rejection>().await.unwrap(),
            crate::Rejection::Invalid(_)
        ));
        assert_eq!(
            post("secret", unsat).await.unwrap().status(),
            StatusCode::ACCEPTED
        );

        let mut next = async || {
            tokio::time::timeout(Duration::from_secs(120), reports.recv())
                .await
                .expect("a report in time")
                .expect("the website is up")
        };
        let total = crate::count(bench::SPEC.len());
        assert_eq!(
            next().await,
            (
                Some("Bearer job-token".into()),
                Report::Progress {
                    id: 7,
                    season: 2,
                    done: 0,
                    total
                }
            )
        );
        let (auth, report) = next().await;
        assert_eq!(auth.as_deref(), Some("Bearer job-token"));
        let Report::Finished {
            id: 7,
            season: 2,
            score,
            fingerprint,
        } = report
        else {
            panic!("{report:?}");
        };
        assert_eq!(
            (score.solved, score.wrong, score.total),
            (total / 2, total / 2, total)
        );
        assert_eq!(fingerprint.len(), bench::SPEC.len());

        // The website got the last report before the worker got its answer. Stopping the server
        // drops the last job sender: the evaluator gets that answer, finishes the job and returns.
        // Without waiting for it the test could end while the evaluator is still in its loop, and
        // the coverage of the loop, counted when it goes round, would come out as zero.
        drop(client);
        stop.send(()).unwrap();
        server.await.unwrap();
        tokio::time::timeout(Duration::from_secs(60), evaluating)
            .await
            .expect("the evaluator stops when the server does")
            .unwrap();
        assert_eq!(pending.load(Ordering::Relaxed), 0);
        std::fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn rejection_is_json() {
        let json = serde_json::to_string(&crate::Rejection::Import("env.f".into())).unwrap();
        assert_eq!(json, r#"{"Import":"env.f"}"#);
    }
}
