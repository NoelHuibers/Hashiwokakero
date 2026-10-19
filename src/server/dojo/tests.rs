//! Tests of the SAT Dojo against a fresh database file.

use dojo::{Answer, DEFAULT_BUDGET, Family, Outcome, Rejection, Report, RunResult, Score};
use libsql::{Connection, params};

use super::*;
use crate::model::DojoState;
use crate::server::auth::User;
use crate::server::{db, now_ms};

async fn test_db(name: &str) -> Connection {
    let path = std::env::temp_dir().join(format!("hashi-dojo-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let db = db::open_local(path.to_str().unwrap()).await.unwrap();
    let conn = db.connect().unwrap();
    db::migrate(&conn).await.unwrap();
    conn
}

async fn user(conn: &Connection, name: &str, registered: bool) -> User {
    conn.execute(
        "INSERT INTO users (username, name, created_at) VALUES (?1, ?2, 0)",
        params![registered.then(|| name.to_string()), name.to_string()],
    )
    .await
    .unwrap();
    User {
        id: conn.last_insert_rowid(),
        username: registered.then(|| name.into()),
        name: name.into(),
        created_at: 0,
        renamed_at: None,
    }
}

/// A tiny valid module; `extra` (below 128) sets its memory size, so the code differs.
fn module(extra: u8) -> Vec<u8> {
    vec![0, b'a', b's', b'm', 1, 0, 0, 0, 5, 3, 1, 0, extra]
}

fn form(name: &str, extra: u8) -> Form {
    Form {
        name: name.into(),
        wasm: module(extra),
        ..Default::default()
    }
}

/// Fuel per formula: `base` billions, a little more for each formula.
fn fingerprint(base: f64) -> Vec<u64> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "test values are small and positive"
    )]
    (0..20)
        .map(|i| ((base + f64::from(i) * 0.1) * 1e9) as u64)
        .collect()
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test PAR-2 values are small and positive"
)]
fn score(par2: f64, wrong: u32) -> Score {
    let results = [RunResult {
        name: "x".into(),
        family: Family::Hashi,
        expected: Answer::Sat,
        outcome: if wrong > 0 {
            Outcome::Wrong
        } else {
            Outcome::Solved(Answer::Sat)
        },
        fuel: (par2 * 1e9) as u64,
        wall_ms: 1,
    }];
    Score::from_results(&results, DEFAULT_BUDGET)
}

#[tokio::test]
async fn submissions_and_leaderboard() {
    let conn = test_db("board").await;
    let alice = user(&conn, "alice", true).await;
    let guest = user(&conn, "Guest 1", false).await;
    let season = season();

    assert_eq!(
        submit(&conn, Some(&guest), form("x", 0)).await,
        Err(SubmitError::SignIn)
    );
    assert_eq!(
        submit(&conn, Some(&alice), form("", 0)).await,
        Err(SubmitError::Name)
    );
    let bad_url = Form {
        source_url: "http://x".into(),
        ..form("x", 0)
    };
    assert_eq!(
        submit(&conn, Some(&alice), bad_url).await,
        Err(SubmitError::Url)
    );
    let not_wasm = Form {
        wasm: b"hello".to_vec(),
        ..form("x", 0)
    };
    assert_eq!(
        submit(&conn, Some(&alice), not_wasm).await,
        Err(SubmitError::NotWasm)
    );

    let a1 = submit(&conn, Some(&alice), form("first", 1)).await.unwrap();
    assert_eq!(
        submit(&conn, Some(&alice), form("again", 1)).await,
        Err(SubmitError::Duplicate)
    );
    let a2 = submit(&conn, Some(&alice), form("second", 2))
        .await
        .unwrap();
    let a3 = submit(&conn, Some(&alice), form("third", 3)).await.unwrap();
    assert_eq!(
        submit(&conn, Some(&alice), form("fourth", 4)).await,
        Err(SubmitError::Limit)
    );
    let house = submit(&conn, None, form("Hashi CDCL", 5)).await.unwrap();

    apply(
        &conn,
        &Report::Progress {
            id: a1,
            season,
            done: 3,
            total: 34,
        },
    )
    .await
    .unwrap();
    let s = submission(&conn, a1, Some(alice.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (s.state, s.progress, s.total, s.mine),
        (DojoState::Running, 3, 34, true)
    );

    apply(
        &conn,
        &Report::Finished {
            id: a1,
            season,
            score: score(2.0, 0),
            fingerprint: vec![],
        },
    )
    .await
    .unwrap();
    apply(
        &conn,
        &Report::Finished {
            id: a2,
            season,
            score: score(1.0, 0),
            fingerprint: vec![],
        },
    )
    .await
    .unwrap();
    apply(
        &conn,
        &Report::Finished {
            id: a3,
            season,
            score: score(0.5, 1),
            fingerprint: vec![],
        },
    )
    .await
    .unwrap();
    apply(
        &conn,
        &Report::Finished {
            id: house,
            season,
            score: score(1.5, 0),
            fingerprint: vec![],
        },
    )
    .await
    .unwrap();
    // Stale seasons are ignored
    apply(
        &conn,
        &Report::Finished {
            id: a1,
            season: season + 1,
            score: score(0.1, 0),
            fingerprint: vec![],
        },
    )
    .await
    .unwrap();

    let board = board(&conn).await.unwrap();
    let names: Vec<_> = board
        .entries
        .iter()
        .map(|e| (e.rank, e.name.as_str()))
        .collect();
    // Alice's best (the wrong one is disqualified), then the house entry
    assert_eq!(names, [(1, "second"), (2, "Hashi CDCL")]);
    assert_eq!(
        submission(&conn, a2, None).await.unwrap().unwrap().rank,
        Some(1)
    );
    assert_eq!(
        submission(&conn, a1, None).await.unwrap().unwrap().rank,
        None
    );

    let me = me(&conn, Some(&alice)).await.unwrap();
    assert_eq!(
        (me.registered, me.left_today, me.submissions.len()),
        (true, 0, 3)
    );

    apply(
        &conn,
        &Report::Rejected {
            id: house,
            season,
            reason: Rejection::NoStart,
        },
    )
    .await
    .unwrap();
    let s = submission(&conn, house, None).await.unwrap().unwrap();
    assert_eq!(
        (s.state, s.rejection),
        (DojoState::Rejected, Some(Rejection::NoStart))
    );
}

#[tokio::test]
async fn copies_and_throwaway_accounts() {
    let conn = test_db("copies").await;
    let alice = user(&conn, "alice2", true).await;
    let bob = user(&conn, "bob2", true).await;
    let season = season();

    // The same code with a custom section added (a renamed copy) is the same code
    let original = submit(&conn, Some(&alice), form("original", 40))
        .await
        .unwrap();
    let mut padded = module(40);
    padded.extend_from_slice(&[0, 4, 3, b'p', b'a', b'd']);
    let copy = Form {
        wasm: padded,
        ..form("mine now", 40)
    };
    assert_eq!(
        submit(&conn, Some(&bob), copy).await,
        Err(SubmitError::Duplicate)
    );

    // A changed copy behaves the same: only the earlier one is ranked
    let near = submit(&conn, Some(&bob), form("tweaked", 41))
        .await
        .unwrap();
    let other = submit(&conn, Some(&bob), form("really different", 42))
        .await
        .unwrap();
    for (id, fp) in [
        (near, fingerprint(1.005)),
        (original, fingerprint(1.0)),
        (other, fingerprint(3.0)),
    ] {
        apply(
            &conn,
            &Report::Finished {
                id,
                season,
                score: score(1.0, 0),
                fingerprint: fp,
            },
        )
        .await
        .unwrap();
    }
    let s = submission(&conn, near, None).await.unwrap().unwrap();
    assert_eq!((s.similar_to, s.rank), (Some(original), None));
    assert_eq!(
        submission(&conn, original, None)
            .await
            .unwrap()
            .unwrap()
            .similar_to,
        None
    );
    assert_eq!(
        submission(&conn, other, None)
            .await
            .unwrap()
            .unwrap()
            .similar_to,
        None
    );
    assert!(dojo::similar(&fingerprint(1.0), &fingerprint(1.01)));
    assert!(!dojo::similar(&fingerprint(1.0), &fingerprint(1.2)));

    // A new account waits a day
    let fresh = User {
        created_at: now_ms(),
        ..user(&conn, "fresh", true).await
    };
    assert_eq!(
        submit(&conn, Some(&fresh), form("x", 50)).await,
        Err(SubmitError::AccountAge)
    );

    // Three a day per network, whatever the account
    let carol = user(&conn, "carol", true).await;
    let dave = user(&conn, "dave", true).await;
    let net = |name: &str, extra| Form {
        network: Some("network-1".into()),
        ..form(name, extra)
    };
    submit(&conn, Some(&carol), net("c1", 60)).await.unwrap();
    submit(&conn, Some(&carol), net("c2", 61)).await.unwrap();
    submit(&conn, Some(&dave), net("d1", 62)).await.unwrap();
    assert_eq!(
        submit(&conn, Some(&dave), net("d2", 63)).await,
        Err(SubmitError::NetworkLimit)
    );
    // ...but other networks are fine
    let elsewhere = Form {
        network: Some("network-2".into()),
        ..form("d3", 64)
    };
    submit(&conn, Some(&dave), elsewhere).await.unwrap();
}

#[test]
fn code_hash_ignores_custom_sections() {
    let mut named = module(7);
    named.extend_from_slice(&[0, 5, 4, b'n', b'a', b'm', b'e']);
    assert_eq!(code_hash(&module(7)), code_hash(&named));
    assert_ne!(code_hash(&module(7)), code_hash(&module(8)));
    assert_eq!(code_hash(b"not wasm"), None);
}

/// A small but complete submission (WASI import, `_start`, memory, code and data) with two
/// custom sections, `producers` at the start and `name` at the end, like compilers write them.
/// Made by hand, its bytes are in `wasm_sections` below.
const FIXTURE: &[u8] = include_bytes!("../../golden/files/dojo.wasm");

/// The sections of a module: (id, the whole section). Written by hand, so the test doesn't
/// depend on wasmparser.
fn wasm_sections(wasm: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut sections = Vec::new();
    let mut at = 8;
    while at < wasm.len() {
        let start = at;
        let id = wasm[at];
        at += 1;
        let (mut size, mut shift) = (0, 0);
        loop {
            let byte = wasm[at];
            at += 1;
            size |= usize::from(byte & 0x7f) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                break;
            }
        }
        at += size;
        sections.push((id, wasm[start..at].to_vec()));
    }
    sections
}

fn wasm_module(sections: &[Vec<u8>]) -> Vec<u8> {
    let mut wasm = FIXTURE[..8].to_vec();
    for section in sections {
        wasm.extend_from_slice(section);
    }
    wasm
}

/// A custom section with a name and content (both shorter than 128 bytes).
fn custom_section(name: &str, content: &[u8]) -> Vec<u8> {
    let size = u8::try_from(1 + name.len() + content.len()).unwrap();
    let mut section = vec![0, size, u8::try_from(name.len()).unwrap()];
    section.extend_from_slice(name.as_bytes());
    section.extend_from_slice(content);
    section
}

#[test]
fn code_hashes_are_unchanged() {
    const COPIES: &str = "the hash the Dojo uses to spot resubmitted copies changed: every \
                          submission already in the database would look new, a copy of one \
                          of them would be accepted again";
    let expected = "5c165686b4b60e88bfacf31dffd5fad80c9a2f0292df6e1411cc149d0e6bd824";
    assert_eq!(code_hash(FIXTURE).as_deref(), Some(expected), "{COPIES}");
    // The sha256 column, of the whole file
    assert_eq!(
        hex(&Sha256::digest(FIXTURE)),
        "d851c9e958c6ec1e81c184c88271583feab0b4fda483215f4251e6e8b23dc893",
        "the SHA-256 of a submission changed (sha2): the sha256 column, which has to be \
         unique, wouldn't match the stored ones"
    );

    let sections = wasm_sections(FIXTURE);
    assert_eq!(
        sections.iter().map(|s| s.0).collect::<Vec<_>>(),
        [0, 1, 2, 3, 5, 7, 10, 11, 0],
        "the fixture's sections"
    );
    let code: Vec<Vec<u8>> = sections
        .iter()
        .filter(|s| s.0 != 0)
        .map(|s| s.1.clone())
        .collect();
    let mut renamed = sections.iter().map(|s| s.1.clone()).collect::<Vec<_>>();
    *renamed.last_mut().unwrap() = custom_section("name", b"\0\x05\x04mine");
    let mut padded = code.clone();
    padded.insert(3, custom_section("padding", &[0; 40]));
    padded.push(custom_section(".debug_info", b"\x01\x02\x03"));
    // Without custom sections, renamed, padded: the same code
    for (label, wasm) in [
        ("stripped", wasm_module(&code)),
        ("renamed", wasm_module(&renamed)),
        ("padded", wasm_module(&padded)),
    ] {
        assert_eq!(
            code_hash(&wasm).as_deref(),
            Some(expected),
            "{label}: custom sections must not count. {COPIES}"
        );
    }
    // Other code is another submission: `i32.const 42` becomes `i32.const 43`
    let mut other = FIXTURE.to_vec();
    let at = other.windows(2).position(|w| w == [0x41, 0x2a]).unwrap();
    other[at + 1] = 0x2b;
    assert_ne!(code_hash(&other).as_deref(), Some(expected), "{COPIES}");
}

#[test]
fn hidden_set_seeds_are_unchanged() {
    // The worker generates a season's hidden set from this seed (and caches it by a hash of
    // it): another seed is another set, mid-season the scores of earlier submissions wouldn't
    // be comparable with new ones
    assert_eq!(
        [
            test_seed_with("hashi-dojo-local", 1),
            test_seed_with("another secret", 2)
        ],
        [
            "bd369032b86123430d3ab641d2c7b087cfd96c2f53ec6afd392286ce6bc9afa2",
            "ec9c9e6fabc42cb17da57c3d5c0fb4b6394c54b5bbb54be90be2ed15c1d2790d"
        ],
        "the seed of a season's hidden set changed"
    );
}

#[tokio::test]
async fn stale_runs_are_retried() {
    let conn = test_db("stale").await;
    let alice = user(&conn, "bob", true).await;
    let id = submit(&conn, Some(&alice), form("x", 9)).await.unwrap();
    conn.execute(
        "UPDATE dojo_submissions SET state = 'running', attempts = 1, updated_at = 0 WHERE id = ?1",
        params![id],
    )
    .await
    .unwrap();
    housekeeping(&conn).await.unwrap();
    assert_eq!(
        submission(&conn, id, None).await.unwrap().unwrap().state,
        DojoState::Queued
    );
    conn.execute(
        "UPDATE dojo_submissions SET state = 'running', attempts = 3, updated_at = 0 WHERE id = ?1",
        params![id],
    )
    .await
    .unwrap();
    housekeeping(&conn).await.unwrap();
    assert_eq!(
        submission(&conn, id, None).await.unwrap().unwrap().state,
        DojoState::Failed
    );
}

/// What the stub worker of [`stub_worker`] was sent.
#[derive(Debug)]
struct SentJob {
    query: std::collections::HashMap<String, String>,
    auth: Option<String>,
    wasm: Vec<u8>,
}

type SentJobs = std::sync::Arc<std::sync::Mutex<Vec<SentJob>>>;

/// A worker on a free port that answers every job with `status` (a rejection with 422) and
/// keeps what it was sent. Returns its URL.
async fn stub_worker(status: axum::http::StatusCode) -> (String, SentJobs) {
    use axum::extract::{Query, State};
    use axum::http::HeaderMap;
    use axum::response::IntoResponse;

    async fn job(
        State((status, jobs)): State<(axum::http::StatusCode, SentJobs)>,
        Query(query): Query<std::collections::HashMap<String, String>>,
        headers: HeaderMap,
        wasm: axum::body::Bytes,
    ) -> axum::response::Response {
        let auth = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        jobs.lock().unwrap().push(SentJob {
            query,
            auth,
            wasm: wasm.to_vec(),
        });
        if status == axum::http::StatusCode::UNPROCESSABLE_ENTITY {
            (status, axum::Json(Rejection::Import("env.f".into()))).into_response()
        } else {
            status.into_response()
        }
    }

    let jobs = SentJobs::default();
    let app = axum::Router::new()
        .route("/jobs", axum::routing::post(job))
        .with_state((status, jobs.clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, jobs)
}

async fn state_and_token(conn: &Connection, id: i64) -> (String, Option<String>, i64) {
    let mut rows = conn
        .query(
            "SELECT state, job_token, attempts FROM dojo_submissions WHERE id = ?1",
            params![id],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    (
        row.get(0).unwrap(),
        row.get(1).unwrap(),
        row.get(2).unwrap(),
    )
}

/// The dispatcher against stub workers: what it sends, how many at a time, and what becomes of
/// a submission when the worker takes it, rejects it, fails or can't be reached.
#[tokio::test]
async fn queued_submissions_go_to_the_worker() {
    use axum::http::StatusCode;

    let conn = test_db("dispatch").await;
    let alice = user(&conn, "alice", true).await;
    let a1 = submit(&conn, Some(&alice), form("first", 1)).await.unwrap();
    let a2 = submit(&conn, Some(&alice), form("second", 2))
        .await
        .unwrap();
    let house = submit(&conn, None, form("Hashi CDCL", 3)).await.unwrap();

    // House entries first, then by age, at most MAX_RUNNING at a time
    let (url, jobs) = stub_worker(StatusCode::ACCEPTED).await;
    worker::dispatch_to(&conn, &url, "secret").await.unwrap();
    let sent = std::mem::take(&mut *jobs.lock().unwrap());
    let ids: Vec<i64> = sent
        .iter()
        .map(|j| j.query["id"].parse().unwrap())
        .collect();
    assert_eq!(ids, [house, a1]);
    for job in &sent {
        let id: i64 = job.query["id"].parse().unwrap();
        assert_eq!(job.auth.as_deref(), Some("Bearer secret"));
        assert_eq!(job.query["season"], season().to_string());
        assert_eq!(job.query["seed"], test_seed(season()));
        assert_eq!(job.wasm, module(if id == house { 3 } else { 1 }));
        // Reports about the job need the token it was sent with
        let (state, token, attempts) = state_and_token(&conn, id).await;
        assert_eq!(
            (state.as_str(), token.as_ref(), attempts),
            ("running", Some(&job.query["report_token"]), 1)
        );
    }
    assert_eq!(state_and_token(&conn, a2).await.0, "queued");
    // Both slots taken: nothing more goes out
    worker::dispatch_to(&conn, &url, "secret").await.unwrap();
    assert!(jobs.lock().unwrap().is_empty());

    // A module the worker can't run is rejected with its reason
    conn.execute(
        "UPDATE dojo_submissions SET state = 'done' WHERE id IN (?1, ?2)",
        params![a1, house],
    )
    .await
    .unwrap();
    let (url, jobs) = stub_worker(StatusCode::UNPROCESSABLE_ENTITY).await;
    worker::dispatch_to(&conn, &url, "secret").await.unwrap();
    assert_eq!(jobs.lock().unwrap().len(), 1);
    let s = submission(&conn, a2, None).await.unwrap().unwrap();
    assert_eq!(
        (s.state, s.rejection),
        (DojoState::Rejected, Some(Rejection::Import("env.f".into())))
    );

    // A failing or missing worker leaves the submission queued for the next round
    let a3 = submit(&conn, Some(&alice), form("third", 4)).await.unwrap();
    let (url, _) = stub_worker(StatusCode::INTERNAL_SERVER_ERROR).await;
    assert_eq!(
        worker::dispatch_to(&conn, &url, "secret").await,
        Err("worker answered 500 Internal Server Error".into())
    );
    assert_eq!(state_and_token(&conn, a3).await.0, "queued");
    let closed = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let err = worker::dispatch_to(&conn, &closed, "secret")
        .await
        .unwrap_err();
    assert!(err.starts_with("worker unreachable"), "{err}");
    assert_eq!(state_and_token(&conn, a3).await.0, "queued");
}
