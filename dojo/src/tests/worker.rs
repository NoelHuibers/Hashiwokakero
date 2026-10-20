//! Tests of the worker's parts that the end-to-end test in `worker.rs` doesn't reach: pools from
//! the cache directory, a worker whose evaluator is gone, benchmarks that aren't formulas, and
//! how reports are retried.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use super::{CheckQuery, Reporter, Shared, Work, check_benchmark, pool_file, put_pool, verify};
use crate::community::{
    Formula, FormulaError, MAX_BYTES, MAX_CLAUSES, Origin, PoolInstance, Refusal, Verdict,
    gbd_hash, pool_digest,
};
use crate::proof::TempDir;
use crate::{Answer, Report};

/// A draw of one satisfiable community benchmark.
fn pool() -> Vec<PoolInstance> {
    let dimacs = "p cnf 2 2\n1 2 0\n-1 0\n";
    vec![PoolInstance {
        origin: Origin {
            benchmark: 9,
            hash: gbd_hash(&Formula::parse(dimacs).unwrap()),
            family: "tiny".into(),
            author: "grace".into(),
        },
        answer: Answer::Sat,
        dimacs: dimacs.into(),
        model: Some(vec![-1, 2]),
    }]
}

fn shared(cache: Option<std::path::PathBuf>) -> (Shared, mpsc::UnboundedReceiver<Work>) {
    let (jobs, rx) = mpsc::unbounded_channel();
    let shared = Shared {
        token: "secret".into(),
        jobs,
        pending: Arc::new(AtomicUsize::new(0)),
        active: Arc::new(AtomicU64::new(0)),
        pools: Arc::default(),
        cache,
    };
    (shared, rx)
}

fn bearer(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    headers
}

/// A restarted worker finds the draws it was sent before in its cache directory, but only
/// genuine ones: a file whose formulas don't match its digest is ignored.
#[test]
fn pools_load_from_the_cache_directory() {
    let dir = TempDir::new(&std::env::temp_dir(), "dojo-worker-pools").unwrap();
    let draw = pool();
    let digest = pool_digest(&draw);
    let file = dir.path().join(pool_file(&digest).unwrap());
    std::fs::write(&file, serde_json::to_vec(&draw).unwrap()).unwrap();

    let (worker, _jobs) = shared(Some(dir.path().to_path_buf()));
    assert_eq!(worker.pool(&digest).as_deref(), Some(&draw));
    // Kept in memory from then on
    std::fs::remove_file(&file).unwrap();
    assert_eq!(worker.pool(&digest).as_deref(), Some(&draw));

    // Another worker, whose cache has a tampered or broken file under that name
    let (worker, _jobs) = shared(Some(dir.path().to_path_buf()));
    let mut tampered = draw;
    tampered[0].answer = Answer::Unsat;
    std::fs::write(&file, serde_json::to_vec(&tampered).unwrap()).unwrap();
    assert_eq!(worker.pool(&digest), None);
    std::fs::write(&file, b"[{").unwrap();
    assert_eq!(worker.pool(&digest), None);
    // No paths from requests, no cache at all
    assert_eq!(worker.pool("../../etc/passwd"), None);
    let (worker, _jobs) = shared(None);
    assert_eq!(worker.pool(&digest), None);
}

/// A genuine draw is taken even when the cache directory can't be written: the worker keeps it
/// in memory.
#[tokio::test]
async fn pools_are_kept_when_the_cache_cannot_be_written() {
    let dir = TempDir::new(&std::env::temp_dir(), "dojo-worker-readonly").unwrap();
    // A file where the cache directory should be
    let cache = dir.path().join("not-a-directory");
    std::fs::write(&cache, b"").unwrap();
    let (worker, _jobs) = shared(Some(cache));
    let draw = pool();
    let digest = pool_digest(&draw);
    let status = put_pool(
        State(worker.clone()),
        Path(digest.clone()),
        bearer("secret"),
        Bytes::from(serde_json::to_vec(&draw).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(worker.pool(&digest).as_deref(), Some(&draw));
}

/// Without its evaluator (it panicked) the worker can't take work: 503, and nothing counts as
/// pending, so it can still exit when idle.
#[tokio::test]
async fn work_is_refused_without_an_evaluator() {
    let (worker, jobs) = shared(None);
    drop(jobs);
    let response = check_benchmark(
        State(worker.clone()),
        Query(CheckQuery {
            id: 1,
            report_token: "t".into(),
            rules: None,
        }),
        bearer("secret"),
        Bytes::from_static(b"p cnf 1 1\n1 0\n"),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(worker.pending.load(Ordering::Relaxed), 0);
    assert!(worker.active.load(Ordering::Relaxed) > 0);
}

/// Files that aren't a formula the Dojo takes are refused before anything runs, with the
/// reason the website shows.
#[test]
fn benchmarks_that_are_no_formulas_are_refused() {
    let sandbox = crate::sandbox::Sandbox::new().unwrap();
    let verdict = |text: &[u8]| verify(&sandbox, 64 << 20, text);
    let refused = |reason| Verdict::Refused { reason };
    assert_eq!(
        verdict(" ".repeat(MAX_BYTES + 1).as_bytes()),
        refused(Refusal::TooLarge)
    );
    // Within the size, but too many clauses (empty ones are two bytes each)
    let many = format!("p cnf 1 1\n{}", "0\n".repeat(MAX_CLAUSES + 1));
    assert!(many.len() <= MAX_BYTES);
    assert_eq!(Formula::parse(&many), Err(FormulaError::TooLarge));
    assert_eq!(verdict(many.as_bytes()), refused(Refusal::TooLarge));
    assert_eq!(
        verdict(b"c nothing but a header\np cnf 3 0\n"),
        refused(Refusal::Invalid("no clauses".into()))
    );
    assert_eq!(
        verdict(b"p cnf 1 1\n2 0\n"),
        refused(Refusal::Invalid("literal 2 beyond 1 variables".into()))
    );
    // Not UTF-8: read leniently, the stray byte isn't a literal
    assert_eq!(
        verdict(b"p cnf 1 1\n\xff 0\n"),
        refused(Refusal::Invalid("not a literal: \u{fffd}".into()))
    );
}

/// How a scripted website answers one connection.
#[derive(Clone, Copy, Debug)]
enum Answering {
    /// Closes the connection without an answer.
    Hangs,
    Status(u16),
}

/// A website on a free port that answers the connections in turn as `script` says (the last
/// one for every connection after), and records when each request came and its authorization.
async fn scripted_site(script: Vec<Answering>) -> (String, Arc<Mutex<Vec<(Instant, String)>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        for n in 0.. {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let answer = script[n.min(script.len() - 1)];
            // The request's head (the bodies here are small enough to come with it)
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buffer).await {
                    Ok(0) | Err(_) => break,
                    Ok(read) => request.extend_from_slice(&buffer[..read]),
                }
            }
            let head = String::from_utf8_lossy(&request).to_string();
            let auth = head
                .lines()
                .find_map(|l| {
                    let (name, value) = l.split_once(':')?;
                    name.eq_ignore_ascii_case("authorization")
                        .then(|| value.trim().to_string())
                })
                .unwrap_or_default();
            log.lock().unwrap().push((Instant::now(), auth));
            match answer {
                Answering::Hangs => drop(stream),
                Answering::Status(code) => {
                    let response = format!(
                        "HTTP/1.1 {code} X\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            }
        }
    });
    (url, seen)
}

fn finished() -> Report {
    Report::Failed {
        id: 3,
        season: 1,
        message: "test".into(),
    }
}

/// A report the website couldn't take (no answer, a server error) is sent again after a
/// growing pause, until it is taken; always with the job's token.
#[tokio::test]
async fn reports_are_retried_until_taken() {
    let (url, seen) = scripted_site(vec![
        Answering::Hangs,
        Answering::Status(503),
        Answering::Status(204),
    ])
    .await;
    let reporter = Reporter {
        client: reqwest::Client::new(),
        app_url: url,
    };
    let started = Instant::now();
    reporter.send(&finished(), "job-token").await;
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3, "{seen:?}");
    assert!(
        seen.iter().all(|(_, auth)| auth == "Bearer job-token"),
        "{seen:?}"
    );
    // One second after the first attempt, two after the second
    let pauses: Vec<Duration> = seen.windows(2).map(|w| w[1].0 - w[0].0).collect();
    assert!(pauses[0] >= Duration::from_secs(1), "{pauses:?}");
    assert!(pauses[1] >= Duration::from_secs(2), "{pauses:?}");
    assert!(started.elapsed() < Duration::from_secs(30));
}

/// A report the website refuses (another season by now, a stale token) isn't sent again.
#[tokio::test]
async fn refused_reports_are_not_retried() {
    let (url, seen) = scripted_site(vec![Answering::Status(409), Answering::Status(204)]).await;
    let reporter = Reporter {
        client: reqwest::Client::new(),
        app_url: url,
    };
    let started = Instant::now();
    reporter.send(&finished(), "job-token").await;
    // Without the pause before a second attempt
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(seen.lock().unwrap().len(), 1);
}
