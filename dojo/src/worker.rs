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
//! - Proofs (see [`crate::proof`]): `DOJO_DRAT_TRIM` and `DOJO_CAKE_LPR` (else found on the
//!   `PATH`; the worker refuses to start without them unless `DOJO_CHECKER=builtin`),
//!   `DOJO_CHECK_SECS` (300) and `DOJO_CHECK_MEMORY_MB` (1024) per proof, `DOJO_CHECKS` (1)
//!   checks at a time, `DOJO_PROOF_DIR` (the temp directory) for the proof files of the runs,
//!   `DOJO_PROOF_MAX_MB` (256) per proof
//!
//! `POST /jobs?id=&season=&seed=&report_token=[&rules=][&export=1]` with the WebAssembly module
//! as body: 202 when queued, 422 with a [`Rejection`] when the module can't be run, 409 when the
//! website expects other rules (`rules`, its [`RULES_VERSION`]) than this worker's: one of the two
//! wasn't deployed yet. With `export=1` the worker also posts the set it evaluated on as a tarball
//! to `/dojo/set` (see [`crate::archive`]), which the website publishes once the season is over.
//! With `pool=<digest>` the season's hidden set has community benchmarks: the worker must have
//! that draw (`GET /pools/<digest>` says whether it does, `PUT /pools/<digest>` sends it as JSON,
//! see [`crate::community`]), otherwise it answers 428.
//!
//! `POST /benchmarks?id=&report_token=[&rules=]` with a community benchmark (DIMACS) as body:
//! queued for verification (202), the verdict goes to `/dojo/benchmark-report`.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::bench::{self, Instance};
use crate::community::{BenchmarkReport, PoolInstance, Verdict};
use crate::eval::{Options, evaluate};
use crate::proof::{CheckLimits, Checker, Tools};
use crate::sandbox::Sandbox;
use crate::{GENERATOR_VERSION, MAX_WASM_BYTES, RULES_VERSION, Report, Score};

#[derive(Deserialize)]
struct JobQuery {
    id: i64,
    season: u32,
    seed: String,
    /// Reports about this job carry this token: valid for this one job only.
    report_token: String,
    /// The rules the website expects the job to be evaluated with.
    rules: Option<u32>,
    /// Whether the website wants the set too (`1`).
    #[serde(default)]
    export: u8,
    /// The [`crate::community::pool_digest`] of the community benchmarks the season's hidden
    /// set has, empty for none. The worker must have them (`PUT /pools/<digest>`).
    #[serde(default)]
    pool: String,
}

/// A season's draw from the community pool.
#[derive(Clone, Debug)]
struct Pool {
    digest: String,
    instances: Arc<Vec<PoolInstance>>,
}

struct Job {
    id: i64,
    season: u32,
    seed: String,
    report_token: Arc<str>,
    export: bool,
    wasm: Bytes,
    pool: Option<Pool>,
}

/// A community benchmark to verify.
struct Check {
    id: i64,
    report_token: Arc<str>,
    dimacs: Bytes,
}

enum Work {
    Evaluate(Job),
    Verify(Check),
}

#[derive(Clone)]
struct Shared {
    token: Arc<str>,
    jobs: mpsc::UnboundedSender<Work>,
    /// Jobs waiting or running.
    pending: Arc<AtomicUsize>,
    /// Seconds since the epoch of the last request or finished job.
    active: Arc<AtomicU64>,
    /// Draws from the community pool by digest, kept in `cache` too.
    pools: Arc<Mutex<HashMap<String, Arc<Vec<PoolInstance>>>>>,
    cache: Option<PathBuf>,
}

impl Shared {
    /// The pool with this digest, from memory or the cache directory.
    fn pool(&self, digest: &str) -> Option<Arc<Vec<PoolInstance>>> {
        let known = self
            .pools
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(digest)
            .cloned();
        if known.is_some() {
            return known;
        }
        let path = self.cache.as_ref()?.join(pool_file(digest)?);
        let pool: Vec<PoolInstance> = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        if crate::community::pool_digest(&pool) != digest {
            return None;
        }
        let pool = Arc::new(pool);
        self.pools
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(digest.to_string(), pool.clone());
        Some(pool)
    }
}

/// The file of a cached pool; `None` for a digest that isn't hex (no paths from requests).
fn pool_file(digest: &str) -> Option<String> {
    (digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| format!("pool-{digest}.json"))
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
    if q.rules.is_some_and(|rules| rules != RULES_VERSION) {
        return (
            StatusCode::CONFLICT,
            format!("this worker evaluates with rules version {RULES_VERSION}"),
        )
            .into_response();
    }
    let pool = if q.pool.is_empty() {
        None
    } else {
        match shared.pool(&q.pool) {
            Some(instances) => Some(Pool {
                digest: q.pool,
                instances,
            }),
            // The website sends the pool first, then the job again
            None => {
                return (StatusCode::PRECONDITION_REQUIRED, "send the pool first").into_response();
            }
        }
    };
    if let Err(reason) = crate::validate::validate(&wasm) {
        return (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(reason)).into_response();
    }
    queue(
        &shared,
        Work::Evaluate(Job {
            id: q.id,
            season: q.season,
            seed: q.seed,
            report_token: q.report_token.into(),
            export: q.export == 1,
            wasm,
            pool,
        }),
    )
}

fn queue(shared: &Shared, work: Work) -> Response {
    shared.pending.fetch_add(1, Ordering::Relaxed);
    if shared.jobs.send(work).is_ok() {
        StatusCode::ACCEPTED.into_response()
    } else {
        shared.pending.fetch_sub(1, Ordering::Relaxed);
        StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}

/// `GET /pools/<digest>`: 200 if the worker has that draw from the community pool, 404 if not.
async fn has_pool(
    State(shared): State<Shared>,
    Path(digest): Path<String>,
    headers: HeaderMap,
) -> StatusCode {
    if !authorized(&headers, &shared.token) {
        return StatusCode::UNAUTHORIZED;
    }
    if shared.pool(&digest).is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

/// `PUT /pools/<digest>`: a season's draw from the community pool, as JSON. Refused unless it
/// has that digest and every formula the hash it claims.
async fn put_pool(
    State(shared): State<Shared>,
    Path(digest): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    if !authorized(&headers, &shared.token) {
        return StatusCode::UNAUTHORIZED;
    }
    shared.active.store(now_secs(), Ordering::Relaxed);
    let Some(file) = pool_file(&digest) else {
        return StatusCode::UNPROCESSABLE_ENTITY;
    };
    let Ok(pool) = serde_json::from_slice::<Vec<PoolInstance>>(&body) else {
        return StatusCode::UNPROCESSABLE_ENTITY;
    };
    let genuine = crate::community::pool_digest(&pool) == digest
        && pool.iter().all(|p| {
            crate::community::Formula::parse(&p.dimacs)
                .is_ok_and(|f| crate::community::gbd_hash(&f) == p.origin.hash)
        });
    if !genuine {
        return StatusCode::UNPROCESSABLE_ENTITY;
    }
    if let Some(dir) = &shared.cache {
        let path = dir.join(&file);
        let tmp = path.with_extension("tmp");
        if let Err(e) = std::fs::write(&tmp, &body).and_then(|()| std::fs::rename(&tmp, &path)) {
            eprintln!("could not cache pool {digest}: {e}");
        }
    }
    shared
        .pools
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(digest, Arc::new(pool));
    StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
struct CheckQuery {
    id: i64,
    report_token: String,
    rules: Option<u32>,
}

/// `POST /benchmarks?id=&report_token=[&rules=]` with a community benchmark (DIMACS) as body:
/// queued for verification (202), the verdict goes to `/dojo/benchmark-report`.
async fn check_benchmark(
    State(shared): State<Shared>,
    Query(q): Query<CheckQuery>,
    headers: HeaderMap,
    dimacs: Bytes,
) -> Response {
    if !authorized(&headers, &shared.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    shared.active.store(now_secs(), Ordering::Relaxed);
    if q.rules.is_some_and(|rules| rules != RULES_VERSION) {
        return StatusCode::CONFLICT.into_response();
    }
    queue(
        &shared,
        Work::Verify(Check {
            id: q.id,
            report_token: q.report_token.into(),
            dimacs,
        }),
    )
}

/// The verdict on a community benchmark (see [`bench::verify_benchmark`]).
fn verify(dimacs: &[u8]) -> Verdict {
    use crate::community::{Formula, FormulaError, Refusal};
    let text = String::from_utf8_lossy(dimacs);
    let formula = match Formula::parse(&text) {
        Ok(f) => f,
        Err(FormulaError::TooLarge) => {
            return Verdict::Refused {
                reason: Refusal::TooLarge,
            };
        }
        Err(FormulaError::Invalid(message)) => {
            return Verdict::Refused {
                reason: Refusal::Invalid(message),
            };
        }
        Err(FormulaError::Empty) => {
            return Verdict::Refused {
                reason: Refusal::Invalid("no clauses".into()),
            };
        }
    };
    let cnf = crate::cnf::Cnf::new(formula.vars, formula.clauses);
    std::panic::catch_unwind(|| bench::verify_benchmark(&cnf)).unwrap_or_else(|_| Verdict::Failed {
        message: "the verification panicked".into(),
    })
}

/// Posts reports to the website, retrying a few times.
#[derive(Clone)]
struct Reporter {
    client: reqwest::Client,
    /// The website, e.g. `https://hashi.rs`.
    app_url: String,
}

impl Reporter {
    async fn send(&self, report: &Report, token: &str) {
        let url = format!("{}/dojo/report", self.app_url);
        let body = serde_json::to_vec(report).unwrap_or_default();
        self.post(
            &url,
            body,
            "application/json",
            token,
            &format!("report for #{}", report.id()),
        )
        .await;
    }

    /// The verdict on a community benchmark.
    async fn send_benchmark(&self, report: &BenchmarkReport, token: &str) {
        let url = format!("{}/dojo/benchmark-report", self.app_url);
        let body = serde_json::to_vec(report).unwrap_or_default();
        self.post(
            &url,
            body,
            "application/json",
            token,
            &format!("verdict on benchmark #{}", report.id),
        )
        .await;
    }

    /// The set of a job as a tarball, for the website's season archive.
    async fn send_set(&self, id: i64, season: u32, archive: Vec<u8>, token: &str) {
        let url = format!(
            "{}/dojo/set?id={id}&season={season}&generator={GENERATOR_VERSION}",
            self.app_url
        );
        self.post(
            &url,
            archive,
            "application/gzip",
            token,
            &format!("set for #{id}"),
        )
        .await;
    }

    async fn post(&self, url: &str, body: Vec<u8>, kind: &str, token: &str, what: &str) {
        for attempt in 0..6u32 {
            let sent = self
                .client
                .post(url)
                .bearer_auth(token)
                .header(header::CONTENT_TYPE, kind)
                .body(body.clone())
                .timeout(Duration::from_secs(60))
                .send()
                .await;
            match sent {
                Ok(r) if r.status().is_success() => return,
                // Refused for good (another season by now, a stale token): no use trying again
                Ok(r) if r.status().is_client_error() => {
                    eprintln!("{what} refused: {}", r.status());
                    return;
                }
                Ok(r) => eprintln!("{what} refused: {}", r.status()),
                Err(e) => eprintln!("{what} failed: {e}"),
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
    /// The built-in checker if `None` (tests only, the worker needs the verified one).
    checker: Option<Checker>,
    proof_dir: Option<PathBuf>,
    proof_max_mb: Option<u64>,
}

impl Settings {
    fn from_env() -> Result<Self, String> {
        let var = |name| std::env::var(name).ok().filter(|v: &String| !v.is_empty());
        let tools = if var("DOJO_CHECKER").as_deref() == Some("builtin") {
            Tools::Builtin
        } else {
            Tools::find().ok_or(
                "drat-trim and cake_lpr weren't found (DOJO_DRAT_TRIM, DOJO_CAKE_LPR or the \
                 PATH); DOJO_CHECKER=builtin checks proofs with the built-in checker instead",
            )?
        };
        let defaults = CheckLimits::default();
        let limits = CheckLimits {
            time: var("DOJO_CHECK_SECS")
                .and_then(|s| s.parse().ok())
                .map_or(defaults.time, Duration::from_secs),
            memory_mb: var("DOJO_CHECK_MEMORY_MB")
                .and_then(|s| s.parse().ok())
                .unwrap_or(defaults.memory_mb),
        };
        let slots = var("DOJO_CHECKS").and_then(|s| s.parse().ok()).unwrap_or(1);
        let mut checker = Checker::new(tools, limits, slots);
        let proof_dir = var("DOJO_PROOF_DIR").map(PathBuf::from);
        if let Some(dir) = &proof_dir {
            checker.scratch.clone_from(dir);
        }
        Ok(Self {
            threads: var("DOJO_THREADS").and_then(|t| t.parse().ok()),
            memory_mb: var("DOJO_MEMORY_MB").and_then(|t| t.parse().ok()),
            cache: var("DOJO_CACHE_DIR").map(PathBuf::from),
            checker: Some(checker),
            proof_dir,
            proof_max_mb: var("DOJO_PROOF_MAX_MB").and_then(|t| t.parse().ok()),
        })
    }
}

/// Evaluates jobs one after another on a blocking thread (each one uses every core). It holds
/// no sender of `jobs`, so it returns once the server that queues them is gone.
fn evaluator(
    mut jobs: mpsc::UnboundedReceiver<Work>,
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
    if let Some(checker) = &settings.checker {
        options.checker = checker.clone();
    }
    if let Some(dir) = &settings.proof_dir {
        let _ = std::fs::create_dir_all(dir);
        options.proof_root.clone_from(dir);
    }
    if let Some(mb) = settings.proof_max_mb {
        options.limits.proof_bytes = mb << 20;
    }
    eprintln!("checking proofs with {}", options.checker.tools.describe());
    // Sets by seed and pool: every submission of a season gets the same one
    let mut sets: HashMap<(String, String), Arc<Vec<Instance>>> = HashMap::new();
    let cache = settings.cache.as_deref();
    while let Some(work) = jobs.blocking_recv() {
        let job = match work {
            Work::Evaluate(job) => job,
            Work::Verify(check) => {
                let started = Instant::now();
                let report = BenchmarkReport {
                    id: check.id,
                    verdict: verify(&check.dimacs),
                };
                eprintln!(
                    "benchmark #{}: {:?} in {:.1?}",
                    check.id,
                    match &report.verdict {
                        Verdict::Verified { answer, .. } => format!("verified {}", answer.id()),
                        other => format!("{other:?}"),
                    },
                    started.elapsed()
                );
                runtime.block_on(reporter.send_benchmark(&report, &check.report_token));
                pending.fetch_sub(1, Ordering::Relaxed);
                active.store(now_secs(), Ordering::Relaxed);
                continue;
            }
        };
        let (id, season) = (job.id, job.season);
        let token = job.report_token.clone();
        let send = |report: Report| runtime.block_on(reporter.send(&report, &token));
        let started = Instant::now();
        options.set_seed.clone_from(&job.seed);
        let key = (
            job.seed.clone(),
            job.pool
                .as_ref()
                .map(|p| p.digest.clone())
                .unwrap_or_default(),
        );
        let set = sets
            .entry(key.clone())
            .or_insert_with(|| Arc::new(load_or_generate(cache, &job.seed, job.pool.as_ref())))
            .clone();
        if sets.len() > 3 {
            sets.retain(|k, _| *k == key);
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
                    if job.export {
                        let info = crate::archive::SetInfo::new(&job.seed, Some(season));
                        let archive = crate::archive::write_tar_gz(&set, &info);
                        runtime.block_on(reporter.send_set(id, season, archive, &token));
                    }
                    let score = Score::from_results(&results, options.limits.fuel);
                    let fingerprint = crate::fingerprint(&results);
                    Report::Finished {
                        id,
                        season,
                        score,
                        fingerprint,
                        results: results.iter().map(crate::RunResult::published).collect(),
                        rules_version: RULES_VERSION,
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

/// The name of the cached set of `seed` (with the draw `pool` from the community pool, empty for
/// none). It must not reveal the seed. Sets of another generator or another draw are other
/// files, a new one never loads a set an older one cached.
fn cache_name(seed: &str, pool: &str) -> String {
    cache_name_for(seed, pool, GENERATOR_VERSION)
}

fn cache_name_for(seed: &str, pool: &str, generator: u32) -> String {
    use sha2::Digest;
    let key = if pool.is_empty() {
        format!("file:g{generator}:{seed}")
    } else {
        format!("file:g{generator}:{seed}:{pool}")
    };
    let digest = sha2::Sha256::digest(key.as_bytes());
    let hex = digest.iter().take(12).fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    });
    format!("set-{hex}.json")
}

/// The hidden set for `seed` with the community benchmarks of `pool`, from the cache directory
/// if it was generated before.
fn load_or_generate(
    cache: Option<&std::path::Path>,
    seed: &str,
    pool: Option<&Pool>,
) -> Vec<Instance> {
    let digest = pool.map_or("", |p| p.digest.as_str());
    let instances: &[PoolInstance] = pool.map_or(&[], |p| p.instances.as_slice());
    let path = cache.map(|dir| dir.join(cache_name(seed, digest)));
    if let Some(set) = path
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice::<Vec<Instance>>(&bytes).ok())
        .filter(|set| set.len() == bench::hidden_spec(seed).len() + instances.len())
    {
        return set;
    }
    let started = Instant::now();
    let set = bench::generate_hidden_with(seed, instances);
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
        let settings = Settings::from_env()?;
        let shared = Shared {
            token: token.clone(),
            jobs: tx,
            pending: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicU64::new(now_secs())),
            pools: Arc::default(),
            cache: settings.cache.clone(),
        };
        let reporter = Reporter {
            client: reqwest::Client::new(),
            app_url: app_url.trim_end_matches('/').to_string(),
        };
        let handle = tokio::runtime::Handle::current();
        let (pending, active) = (shared.pending.clone(), shared.active.clone());
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
        .route(
            "/jobs",
            post(submit).layer(DefaultBodyLimit::max(MAX_WASM_BYTES + 4096)),
        )
        .route(
            "/benchmarks",
            post(check_benchmark).layer(DefaultBodyLimit::max(crate::community::MAX_BYTES + 4096)),
        )
        .route(
            "/pools/{digest}",
            get(has_pool).merge(put(put_pool).layer(DefaultBodyLimit::max(
                // JSON of a draw: the formulas, their models, a little more
                3 * crate::community::PER_SET * crate::community::MAX_BYTES,
            ))),
        )
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
            cache_name_for("golden", "", 1),
            "set-4098d811dbc4a13744ae1527.json",
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
        // Community benchmarks keep where they come from
        let community = r#"[{"name":"community-01","family":"Community","expected":"Sat","cnf":{"vars":1,"clauses":[[1]]},"witness":[1],"origin":{"benchmark":3,"hash":"h","family":"f","author":"a"}}]"#;
        let set: Vec<Instance> = serde_json::from_str(community).expect(CACHE);
        assert_eq!(set[0].origin.as_ref().map(|o| o.benchmark), Some(3));
        assert_eq!(serde_json::to_string(&set).unwrap(), community);
    }

    /// A new generator makes other sets from the same seed: they must not be taken from the
    /// cache of an older one.
    #[test]
    fn cached_sets_depend_on_the_generator() {
        assert_eq!(
            cache_name("golden", ""),
            cache_name_for("golden", "", GENERATOR_VERSION)
        );
        assert_ne!(
            cache_name_for("golden", "", 1),
            cache_name_for("golden", "", 2)
        );
        assert_ne!(
            cache_name_for("golden", "", 1),
            cache_name_for("golden2", "", 1)
        );
        // Another draw from the community pool, another set
        assert_ne!(cache_name("golden", ""), cache_name("golden", "ab"));
        // The name doesn't give the seed away
        assert!(!cache_name("golden", "").contains("golden"));
        assert_eq!(pool_file("../x"), None);
        assert_eq!(
            pool_file(&"a".repeat(64)),
            Some(format!("pool-{}.json", "a".repeat(64)))
        );
    }

    /// A draw of two tiny community benchmarks: unsatisfiable, then satisfiable.
    fn tiny_pool() -> Vec<PoolInstance> {
        use crate::community::{Formula, Origin, gbd_hash};
        ["p cnf 1 2\n1 0\n-1 0\n", "p cnf 1 1\n1 0\n"]
            .iter()
            .enumerate()
            .map(|(i, dimacs)| PoolInstance {
                origin: Origin {
                    benchmark: i64::try_from(i).unwrap() + 1,
                    hash: gbd_hash(&Formula::parse(dimacs).unwrap()),
                    family: "tiny".into(),
                    author: "ada".into(),
                },
                answer: if i == 0 {
                    crate::Answer::Unsat
                } else {
                    crate::Answer::Sat
                },
                dimacs: (*dimacs).to_string(),
                model: (i == 1).then(|| vec![1]),
            })
            .collect()
    }

    /// A set of tiny formulas for `seed` and [`tiny_pool`] in a fresh cache directory:
    /// generating a real one takes minutes. Even ones are unsatisfiable.
    fn cached_set(seed: &str) -> PathBuf {
        use crate::cnf::Cnf;
        use crate::{Answer, Family};
        let dir = std::env::temp_dir().join(format!("dojo-worker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let set: Vec<Instance> = bench::hidden_spec(seed)
            .iter()
            .enumerate()
            .map(|(i, spec)| {
                let family = spec.family;
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
                    origin: None,
                }
            })
            .collect();
        assert!(set.iter().any(|i| i.family == Family::Queens));
        let pool = tiny_pool();
        let mut set = set;
        set.extend(bench::community(seed, &pool));
        std::fs::write(
            dir.join(cache_name(seed, &crate::community::pool_digest(&pool))),
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
        let (sets_sent, mut sets) = mpsc::unbounded_channel::<(Option<String>, String, Bytes)>();
        let (checked_sent, mut checked) =
            mpsc::unbounded_channel::<(Option<String>, BenchmarkReport)>();
        let auth = |headers: &HeaderMap| {
            headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let site = Router::new()
            .route(
                "/dojo/report",
                post(
                    move |headers: HeaderMap, axum::Json(report): axum::Json<Report>| {
                        sent.send((auth(&headers), report)).unwrap();
                        async { StatusCode::NO_CONTENT }
                    },
                ),
            )
            .route(
                "/dojo/set",
                post(
                    move |headers: HeaderMap, uri: axum::http::Uri, body: Bytes| {
                        let query = uri.query().unwrap_or_default().to_string();
                        sets_sent.send((auth(&headers), query, body)).unwrap();
                        async { StatusCode::NO_CONTENT }
                    },
                ),
            )
            .route(
                "/dojo/benchmark-report",
                post(
                    move |headers: HeaderMap, axum::Json(report): axum::Json<BenchmarkReport>| {
                        checked_sent.send((auth(&headers), report)).unwrap();
                        async { StatusCode::NO_CONTENT }
                    },
                ),
            )
            .layer(DefaultBodyLimit::max(64 << 20));
        let site_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let site_url = format!("http://{}", site_listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(site_listener, site).await.unwrap() });

        let cache = cached_set("worker-test");
        let (jobs, rx) = mpsc::unbounded_channel();
        let shared = Shared {
            token: "secret".into(),
            jobs,
            pending: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicU64::new(0)),
            pools: Arc::default(),
            cache: Some(cache.clone()),
        };
        let reporter = Reporter {
            client: reqwest::Client::new(),
            app_url: site_url,
        };
        let settings = Settings {
            threads: Some(2),
            memory_mb: Some(64),
            cache: Some(cache.clone()),
            proof_dir: Some(cache.join("proofs")),
            proof_max_mb: Some(1),
            ..Settings::default()
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
        let pool = tiny_pool();
        let digest = crate::community::pool_digest(&pool);
        let job = format!(
            "{url}/jobs?id=7&season=2&seed=worker-test&report_token=job-token&rules={RULES_VERSION}&export=1&pool={digest}"
        );
        // The pool of the season first: the worker asks for it, checks it and keeps it
        let pool_url = format!("{url}/pools/{digest}");
        let status = |r: reqwest::Result<reqwest::Response>| r.unwrap().status();
        let has = || client.get(&pool_url).bearer_auth("secret").send();
        assert_eq!(status(has().await), StatusCode::NOT_FOUND);
        assert_eq!(
            status(
                client
                    .post(&job)
                    .bearer_auth("secret")
                    .body(b"\0asm".to_vec())
                    .send()
                    .await
            ),
            StatusCode::PRECONDITION_REQUIRED
        );
        let put =
            |url: String, body: Vec<u8>| client.put(url).bearer_auth("secret").body(body).send();
        let json = serde_json::to_vec(&pool).unwrap();
        assert_eq!(
            status(put(pool_url.replace("/pools/", "/pools/0"), json.clone()).await),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(put(format!("{url}/pools/{}", "0".repeat(64)), json.clone()).await),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let mut forged = pool.clone();
        forged[0].dimacs = "p cnf 1 1\n-1 0\n".into();
        assert_eq!(
            status(put(pool_url.clone(), serde_json::to_vec(&forged).unwrap()).await),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(put(pool_url.clone(), b"[".to_vec()).await),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(client.put(&pool_url).body(json.clone()).send().await),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status(put(pool_url.clone(), json).await),
            StatusCode::NO_CONTENT
        );
        assert_eq!(status(has().await), StatusCode::OK);
        assert!(cache.join(pool_file(&digest).unwrap()).exists());
        assert_eq!(
            status(client.get(&pool_url).send().await),
            StatusCode::UNAUTHORIZED
        );
        let post =
            |token: &str, wasm: Vec<u8>| client.post(&job).bearer_auth(token).body(wasm).send();
        // A website that expects other rules gets nothing evaluated
        let other_rules = client
            .post(job.replace(
                &format!("rules={RULES_VERSION}"),
                &format!("rules={}", RULES_VERSION + 1),
            ))
            .bearer_auth("secret")
            .body(b"\0asm junk".to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(other_rules.status(), StatusCode::CONFLICT);

        // Answers UNSAT to everything: right on the even formulas, wrong on the others ...
        // ... with a proof (any proof: unit propagation refutes these formulas)
        let unsat = crate::sandbox::testing::unsat_solver(Some(b"0\n"), "");
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
        let total = crate::SET_SIZE + 2;
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
        // The set it evaluated on comes first, as asked for, with the job's token
        let (auth, query, archive) = tokio::time::timeout(Duration::from_secs(120), sets.recv())
            .await
            .expect("the set in time")
            .expect("the website is up");
        assert_eq!(auth.as_deref(), Some("Bearer job-token"));
        assert_eq!(
            query,
            format!("id=7&season=2&generator={GENERATOR_VERSION}")
        );
        let (info, archived) = crate::archive::read_tar_gz(&archive).unwrap();
        assert_eq!(info, crate::archive::SetInfo::new("worker-test", Some(2)));
        assert_eq!(archived.len(), total as usize);
        // With the community benchmarks at the end, credited
        let last = &archived[archived.len() - 1];
        assert_eq!(
            (last.name.as_str(), last.family),
            ("community-02", crate::Family::Community)
        );
        assert_eq!(last.origin.as_ref().map(|o| o.author.as_str()), Some("ada"));
        let (auth, report) = next().await;
        assert_eq!(auth.as_deref(), Some("Bearer job-token"));
        let Report::Finished {
            id: 7,
            season: 2,
            score,
            fingerprint,
            results,
            rules_version,
        } = report
        else {
            panic!("{report:?}");
        };
        assert_eq!(
            (score.solved, score.wrong, score.unverified, score.total),
            (total / 2, total / 2, 0, total)
        );
        assert_eq!(fingerprint.len(), total as usize);
        assert_eq!(rules_version, RULES_VERSION);
        // Every formula in set order: right on the even ones, wrong on the others
        assert_eq!(
            results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            archived.iter().map(|i| i.name.as_str()).collect::<Vec<_>>()
        );
        for (i, r) in results.iter().enumerate() {
            let expected = if i % 2 == 0 {
                crate::Outcome::Solved(crate::Answer::Unsat)
            } else {
                crate::Outcome::Wrong
            };
            assert_eq!(r.outcome, expected, "{}", r.name);
        }

        // Community benchmarks: verified one after another, the verdict with the check's token
        let bench_url = |id: i64| {
            format!("{url}/benchmarks?id={id}&report_token=check-{id}&rules={RULES_VERSION}")
        };
        assert_eq!(
            client
                .post(bench_url(1))
                .bearer_auth("guess")
                .body("p cnf 1 1\n1 0\n")
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            client
                .post(format!(
                    "{url}/benchmarks?id=1&report_token=t&rules={}",
                    RULES_VERSION + 1
                ))
                .bearer_auth("secret")
                .body("p cnf 1 1\n1 0\n")
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
        for (id, dimacs) in [(1, "p cnf 1 1\n1 0\n"), (2, "not a formula")] {
            let sent = client
                .post(bench_url(id))
                .bearer_auth("secret")
                .body(dimacs)
                .send();
            assert_eq!(sent.await.unwrap().status(), StatusCode::ACCEPTED);
        }
        let mut verdicts = Vec::new();
        for _ in 0..2 {
            verdicts.push(
                tokio::time::timeout(Duration::from_secs(120), checked.recv())
                    .await
                    .expect("a verdict in time")
                    .expect("the website is up"),
            );
        }
        assert_eq!(
            verdicts,
            [
                (
                    Some("Bearer check-1".into()),
                    BenchmarkReport {
                        id: 1,
                        verdict: Verdict::Refused {
                            reason: crate::community::Refusal::TooEasy
                        }
                    }
                ),
                (
                    Some("Bearer check-2".into()),
                    BenchmarkReport {
                        id: 2,
                        verdict: Verdict::Refused {
                            reason: crate::community::Refusal::Invalid(
                                "clauses before the p cnf line".into()
                            )
                        }
                    }
                ),
            ]
        );

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
        // The proof directories of the runs are gone
        assert_eq!(std::fs::read_dir(cache.join("proofs")).unwrap().count(), 0);
        std::fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn rejection_is_json() {
        let json = serde_json::to_string(&crate::Rejection::Import("env.f".into())).unwrap();
        assert_eq!(json, r#"{"Import":"env.f"}"#);
    }
}
