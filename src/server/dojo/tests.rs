//! Tests of the SAT Dojo against a fresh database file.

use dojo::{
    Answer, DEFAULT_BUDGET, Family, InstanceResult, Outcome, RULES_VERSION, Rejection, Report,
    RunResult, Score,
};
use libsql::{Connection, params};

use super::*;
use crate::model::{DojoBoard, DojoState};
use crate::server::auth::User;
use crate::server::{db, now_ms};
use worker::housekeeping_at;

/// A fresh database, removed when the test ends (also when it fails).
async fn test_db(name: &str) -> db::TestConn {
    db::TestConn::new(&format!("dojo-{name}")).await
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
    // Solver names are on the board too
    assert_eq!(
        submit(&conn, Some(&alice), form("Fuck_SAT", 0)).await,
        Err(SubmitError::NameNotAllowed)
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
            results: Vec::new(),
            rules_version: RULES_VERSION,
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
            results: Vec::new(),
            rules_version: RULES_VERSION,
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
            results: Vec::new(),
            rules_version: RULES_VERSION,
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
            results: Vec::new(),
            rules_version: RULES_VERSION,
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
            results: Vec::new(),
            rules_version: RULES_VERSION,
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

    // A banned author's solvers leave the board until the ban is lifted
    let names = |b: DojoBoard| -> Vec<(u32, String)> {
        b.entries.into_iter().map(|e| (e.rank, e.name)).collect()
    };
    crate::server::admin::ban(&conn, alice.id, "")
        .await
        .unwrap();
    assert_eq!(
        names(super::board(&conn).await.unwrap()),
        [(1, "Hashi CDCL".to_string())]
    );
    crate::server::admin::unban(&conn, alice.id).await.unwrap();
    assert_eq!(
        names(super::board(&conn).await.unwrap()),
        [(1, "second".to_string()), (2, "Hashi CDCL".to_string())]
    );

    let rejected = submit(&conn, None, form("rejected", 8)).await.unwrap();
    apply(
        &conn,
        &Report::Rejected {
            id: rejected,
            season,
            reason: Rejection::NoStart,
        },
    )
    .await
    .unwrap();
    let s = submission(&conn, rejected, None).await.unwrap().unwrap();
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
                results: Vec::new(),
                rules_version: RULES_VERSION,
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
        // The worker has to evaluate with the site's rules, and send the set along
        assert_eq!(job.query["rules"], RULES_VERSION.to_string());
        assert_eq!(job.query.get("export").map(String::as_str), Some("1"));
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
        "UPDATE dojo_submissions SET state = 'done', rules_version = ?3 WHERE id IN (?1, ?2)",
        params![a1, house, RULES_VERSION],
    )
    .await
    .unwrap();
    // Once the site has the season's set, jobs come without asking for it
    conn.execute(
        "INSERT INTO dojo_season_sets (season, generator_version, archive, sha256, created_at) VALUES (?1, ?2, X'1f8b', 'h', 0)",
        params![season(), dojo::GENERATOR_VERSION],
    )
    .await
    .unwrap();
    let (url, jobs) = stub_worker(StatusCode::UNPROCESSABLE_ENTITY).await;
    worker::dispatch_to(&conn, &url, "secret").await.unwrap();
    assert_eq!(jobs.lock().unwrap().len(), 1);
    assert!(!jobs.lock().unwrap()[0].query.contains_key("export"));
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
    // Without a used up attempt or a token a late report could still use
    assert_eq!(state_and_token(&conn, a3).await, ("queued".into(), None, 0));
}

/// A finished evaluation with a result per formula: `solved` of `total` formulas solved with
/// `fuel` each, the others timed out.
fn finished(id: i64, season: u32, solved: usize, total: usize, rules_version: u32) -> Report {
    let results: Vec<InstanceResult> = (0..total)
        .map(|i| InstanceResult {
            name: format!("queens-{:02}", i + 1),
            family: Family::Queens,
            expected: Answer::Sat,
            outcome: if i < solved {
                Outcome::Solved(Answer::Sat)
            } else {
                Outcome::Timeout
            },
            fuel: if i < solved {
                1_000_000 * (id.unsigned_abs() + 1)
            } else {
                DEFAULT_BUDGET
            },
        })
        .collect();
    let runs: Vec<RunResult> = results
        .iter()
        .map(|r| RunResult {
            name: r.name.clone(),
            family: r.family,
            expected: r.expected,
            outcome: r.outcome,
            fuel: r.fuel,
            wall_ms: 1,
        })
        .collect();
    Report::Finished {
        id,
        season,
        score: Score::from_results(&runs, DEFAULT_BUDGET),
        fingerprint: dojo::fingerprint(&runs),
        results,
        rules_version,
    }
}

/// The number a query with one parameter gives.
async fn one(conn: &Connection, sql: &str, param: impl Into<libsql::Value>) -> i64 {
    let mut rows = conn.query(sql, vec![param.into()]).await.unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

async fn state_and_season(conn: &Connection, id: i64) -> (String, u32) {
    let mut rows = conn
        .query(
            "SELECT state, season FROM dojo_submissions WHERE id = ?1",
            params![id],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    (row.get(0).unwrap(), row.get(1).unwrap())
}

/// A season ends: its results are frozen with their ranks, its seed is revealed, everything is
/// evaluated again in the new one. Running it again changes nothing.
#[tokio::test]
async fn seasons_are_archived_when_the_next_one_starts() {
    let conn = test_db("seasons").await;
    let alice = user(&conn, "alice", true).await;
    let bob = user(&conn, "bob", true).await;
    let (first, second) = (season(), season() + 1);

    // The season starts with its commitment, the seed stays secret
    housekeeping_at(&conn, first).await.unwrap();
    let s = seasons(&conn, first).await.unwrap();
    assert_eq!(s.current.season, first);
    assert_eq!(s.current.commitment, commitment(&test_seed(first)));
    assert_eq!((s.current.seed, s.current.ended_at), (None, None));
    assert!(s.current.started_at.is_some());
    assert_eq!(s.past, Vec::new());
    assert_eq!(board(&conn).await.unwrap().commitment, s.current.commitment);

    let a = submit(&conn, Some(&alice), form("alice's", 1))
        .await
        .unwrap();
    let b = submit(&conn, Some(&bob), form("bob's", 2)).await.unwrap();
    let house = submit(&conn, None, form("Hashi CDCL", 3)).await.unwrap();
    let waiting = submit(&conn, Some(&bob), form("bob's next", 4))
        .await
        .unwrap();
    apply(&conn, &finished(a, first, 3, 4, RULES_VERSION))
        .await
        .unwrap();
    apply(&conn, &finished(b, first, 1, 4, RULES_VERSION))
        .await
        .unwrap();
    apply(&conn, &finished(house, first, 2, 4, RULES_VERSION))
        .await
        .unwrap();
    // Nothing changes while the season lasts
    housekeeping_at(&conn, first).await.unwrap();
    assert_eq!(
        one(&conn, "SELECT COUNT(*) FROM dojo_archive WHERE ?1 = ?1", 0).await,
        0
    );
    assert_eq!(state_and_season(&conn, a).await, ("done".into(), first));

    housekeeping_at(&conn, second).await.unwrap();
    let archived_season = archived(&conn, first, second).await.unwrap().unwrap();
    assert_eq!(archived_season.seed.as_deref(), Some(&*test_seed(first)));
    assert_eq!(
        archived_season.commitment,
        commitment(&test_seed(first)),
        "the commitment from the start"
    );
    let entries: Vec<(&str, Option<u32>, usize)> = archived_season
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.rank, e.results.len()))
        .collect();
    // Ranked by PAR-2: alice solved most; bob is ranked too, the queued one isn't archived
    assert_eq!(
        entries,
        [
            ("alice's", Some(1), 4),
            ("Hashi CDCL", Some(2), 4),
            ("bob's", Some(3), 4)
        ]
    );
    assert_eq!(archived_season.entries[0].author, "alice");
    assert_eq!(archived_season.entries[1].author, "Hashi");
    assert_eq!(
        archived_season.entries[0].results[3].outcome,
        Outcome::Timeout
    );
    // Everything is evaluated again in the new season, without the old per-formula results
    for id in [a, b, house, waiting] {
        assert_eq!(state_and_season(&conn, id).await, ("queued".into(), second));
    }
    assert_eq!(
        one(
            &conn,
            "SELECT COUNT(*) FROM dojo_submissions WHERE results IS NOT NULL OR fingerprint IS NOT NULL AND ?1 = ?1",
            0
        )
        .await,
        0
    );

    let s = seasons(&conn, second).await.unwrap();
    assert_eq!(s.current.season, second);
    assert_eq!(s.current.seed, None, "never the current season's seed");
    assert_eq!(s.past.len(), 1);
    let past = &s.past[0];
    assert_eq!(past.seed.as_deref(), Some(&*test_seed(first)));
    assert!(past.ended_at.is_some());
    assert_eq!((past.entries, past.board.len()), (3, 3));
    assert_eq!(past.board[0].name, "alice's");
    assert_eq!(past.set_size, None);
    // The revealed seed reveals the held-out family too, never the running season's
    let mystery = dojo::mystery_family(&test_seed(first));
    assert!(mystery.reserve());
    assert_eq!(past.mystery, Some(mystery));
    assert_eq!(archived_season.mystery, Some(mystery));
    assert_eq!(s.current.mystery, None);

    // Sets from before held-out families had none
    conn.execute(
        "UPDATE dojo_seasons SET generator_version = 1 WHERE season = ?1",
        params![first],
    )
    .await
    .unwrap();
    assert_eq!(seasons(&conn, second).await.unwrap().past[0].mystery, None);

    // Again, also with a result for the old season coming in late: nothing changes
    let ended = past.ended_at;
    apply(&conn, &finished(waiting, first, 4, 4, RULES_VERSION))
        .await
        .unwrap();
    housekeeping_at(&conn, second).await.unwrap();
    let again = seasons(&conn, second).await.unwrap();
    assert_eq!(again.past[0].ended_at, ended);
    assert_eq!(again.past[0].entries, 3);
    assert_eq!(
        one(&conn, "SELECT COUNT(*) FROM dojo_seasons WHERE ?1 = ?1", 0).await,
        2
    );

    // A seed in the current season's row is never shown
    conn.execute(
        "UPDATE dojo_seasons SET seed = 'leaked', ended_at = 1 WHERE season = ?1",
        params![second],
    )
    .await
    .unwrap();
    let s = seasons(&conn, second).await.unwrap();
    assert_eq!((s.current.seed, s.current.ended_at), (None, None));
    assert_eq!(s.past.len(), 1);
    assert!(archived(&conn, second, second).await.unwrap().is_none());
    assert!(archived(&conn, second + 1, second).await.unwrap().is_none());

    // Deleting an account keeps the numbers, not who or what
    crate::server::auth::erase(&conn, alice.id).await.unwrap();
    let archived_season = archived(&conn, first, second).await.unwrap().unwrap();
    let gone = &archived_season.entries[0];
    assert_eq!(
        (
            gone.name.as_str(),
            gone.author.as_str(),
            gone.rank,
            &gone.source_url
        ),
        ("", "", Some(1), &None)
    );
    assert_eq!(gone.results.len(), 4);
    let board = &seasons(&conn, second).await.unwrap().past[0].board;
    assert_eq!((board[0].name.as_str(), board[0].author.as_str()), ("", ""));
    assert_eq!(
        one(
            &conn,
            "SELECT COUNT(*) FROM dojo_archive WHERE user_id = ?1",
            alice.id
        )
        .await,
        0
    );
}

/// Two servers freezing the same season at the same moment: one waits for the other, and every
/// result is archived exactly once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn freezing_twice_at_once_is_safe() {
    let file = db::TempDb::new("dojo-race");
    let database = db::open_local(file.as_str()).await.unwrap();
    let conn = db::connect(&database).unwrap();
    db::migrate(&conn).await.unwrap();
    let (first, second) = (season(), season() + 1);
    let mut ids = Vec::new();
    for i in 0..6 {
        let author = user(&conn, &format!("racer{i}"), true).await;
        let id = submit(&conn, Some(&author), form("racer", 10 + i))
            .await
            .unwrap();
        apply(&conn, &finished(id, first, 2, 3, RULES_VERSION))
            .await
            .unwrap();
        ids.push(id);
    }
    let runs: Vec<_> = (0..2)
        .map(|_| {
            let conn = db::connect(&database).unwrap();
            tokio::spawn(async move { housekeeping_at(&conn, second).await })
        })
        .collect();
    for run in runs {
        run.await.unwrap().unwrap();
    }
    assert_eq!(
        one(
            &conn,
            "SELECT COUNT(*) FROM dojo_archive WHERE season = ?1",
            first
        )
        .await,
        6
    );
    assert_eq!(
        one(
            &conn,
            "SELECT COUNT(*) FROM dojo_archive WHERE season = ?1 AND rank IS NOT NULL",
            first
        )
        .await,
        6
    );
    for id in ids {
        assert_eq!(state_and_season(&conn, id).await, ("queued".into(), second));
    }
}

/// Results from older rules are evaluated again in the same season, nothing is archived.
#[tokio::test]
async fn newer_rules_evaluate_again() {
    let conn = test_db("rules").await;
    let alice = user(&conn, "alice", true).await;
    let current = season();
    let old = submit(&conn, Some(&alice), form("old rules", 1))
        .await
        .unwrap();
    let new = submit(&conn, Some(&alice), form("new rules", 2))
        .await
        .unwrap();
    let newer = submit(&conn, Some(&alice), form("newer rules", 3))
        .await
        .unwrap();
    apply(&conn, &finished(old, current, 2, 2, RULES_VERSION - 1))
        .await
        .unwrap();
    apply(&conn, &finished(new, current, 2, 2, RULES_VERSION))
        .await
        .unwrap();
    // From a worker deployed before the site
    apply(&conn, &finished(newer, current, 2, 2, RULES_VERSION + 1))
        .await
        .unwrap();
    // Rows from before versions existed count as version 0
    let legacy = submit(&conn, None, form("legacy", 4)).await.unwrap();
    conn.execute(
        "UPDATE dojo_submissions SET state = 'done', rules_version = NULL WHERE id = ?1",
        params![legacy],
    )
    .await
    .unwrap();

    housekeeping_at(&conn, current).await.unwrap();
    assert_eq!(
        state_and_season(&conn, old).await,
        ("queued".into(), current)
    );
    assert_eq!(
        state_and_season(&conn, legacy).await,
        ("queued".into(), current)
    );
    assert_eq!(state_and_season(&conn, new).await, ("done".into(), current));
    assert_eq!(
        state_and_season(&conn, newer).await,
        ("done".into(), current)
    );
    assert_eq!(
        one(&conn, "SELECT COUNT(*) FROM dojo_archive WHERE ?1 = ?1", 0).await,
        0
    );
    let s = seasons(&conn, current).await.unwrap();
    assert_eq!(s.past, Vec::new());
    assert_eq!(
        (s.current.season, s.current.rules_version),
        (current, RULES_VERSION)
    );
    // The stored rules version of the result
    assert_eq!(
        one(
            &conn,
            "SELECT rules_version FROM dojo_submissions WHERE id = ?1",
            new
        )
        .await,
        i64::from(RULES_VERSION)
    );
}

#[test]
fn results_as_csv() {
    let entry = |id, name: &str, author: &str, rank, results: Vec<InstanceResult>| ArchivedEntry {
        id,
        name: name.into(),
        author: author.into(),
        house: id == 2,
        rank,
        source_url: None,
        created_at: 0,
        rules_version: Some(1),
        score: None,
        breakdown: None,
        contribution: None,
        results,
    };
    let run = |name: &str, family, expected, outcome, fuel| InstanceResult {
        name: name.into(),
        family,
        expected,
        outcome,
        fuel,
    };
    let season = ArchivedSeason {
        season: 3,
        started_at: Some(1),
        ended_at: Some(2),
        commitment: "c".into(),
        seed: Some("s".into()),
        generator_version: 1,
        rules_version: 1,
        budget: dojo::budget_of(1),
        set_sha256: None,
        mystery: None,
        community: Vec::new(),
        vbs: None,
        entries: vec![
            entry(
                1,
                "Fast, \"really\"",
                "=cmd()",
                Some(1),
                vec![
                    run(
                        "hashi-01",
                        Family::Hashi,
                        Answer::Sat,
                        Outcome::Solved(Answer::Sat),
                        123,
                    ),
                    run(
                        "pigeonhole-01",
                        Family::Pigeonhole,
                        Answer::Unsat,
                        Outcome::Timeout,
                        dojo::budget_of(1),
                    ),
                ],
            ),
            entry(
                2,
                "Hashi CDCL",
                "Hashi",
                None,
                vec![run(
                    "hashi-01",
                    Family::Hashi,
                    Answer::Sat,
                    Outcome::Wrong,
                    7,
                )],
            ),
            entry(3, "old", "x", None, Vec::new()),
        ],
    };
    assert_eq!(
        csv(&season),
        "season,entry,solver,author,house,rank,rules_version,instance,family,expected,outcome,fuel\n\
         3,1,\"Fast, \"\"really\"\"\",'=cmd(),0,1,1,hashi-01,hashi,SAT,solved,123\n\
         3,1,\"Fast, \"\"really\"\"\",'=cmd(),0,1,1,pigeonhole-01,pigeonhole,UNSAT,timeout,10000000000\n\
         3,2,Hashi CDCL,Hashi,1,,1,hashi-01,hashi,SAT,wrong,7\n"
    );
    let json = serde_json::to_value(&season).unwrap();
    assert_eq!(json["season"], 3);
    assert_eq!(json["seed"], "s");
    assert_eq!(json["budget"], 10_000_000_000_u64);
    assert_eq!(dojo::budget_of(RULES_VERSION), DEFAULT_BUDGET);
    assert_eq!(json["entries"][0]["rank"], 1);
    assert_eq!(json["entries"][1]["rank"], serde_json::Value::Null);
    assert_eq!(
        json["entries"][0]["results"][0],
        serde_json::json!({"name": "hashi-01", "family": "Hashi", "expected": "Sat", "outcome": {"Solved": "Sat"}, "fuel": 123})
    );
}

/// The worker posts the set of a job with the job's token; only the current season's is kept.
#[tokio::test]
async fn sets_come_with_the_jobs_token() {
    use axum::http::{HeaderMap, StatusCode, header};

    let conn = test_db("sets").await;
    let alice = user(&conn, "alice", true).await;
    let current = season();
    let id = submit(&conn, Some(&alice), form("x", 1)).await.unwrap();
    conn.execute(
        "UPDATE dojo_submissions SET job_token = 'job' WHERE id = ?1",
        params![id],
    )
    .await
    .unwrap();
    let bearer = |token: &str| {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        headers
    };
    let gz: &[u8] = &[0x1f, 0x8b, 8, 0];
    assert!(seasons::wants_set(&conn, current).await.unwrap());
    let store = async |headers: HeaderMap, season, body: &[u8], now| {
        let query = SetQuery {
            id,
            season,
            generator: dojo::GENERATOR_VERSION,
        };
        seasons::store_set(&conn, &headers, &query, body, now).await
    };
    assert_eq!(
        store(bearer("guess"), current, gz, current).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        store(HeaderMap::new(), current, gz, current).await,
        StatusCode::UNAUTHORIZED
    );
    // A job of another season, or a season that is over by now
    assert_eq!(
        store(bearer("job"), current + 1, gz, current).await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        store(bearer("job"), current, gz, current + 1).await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        store(bearer("job"), current, b"not gzip", current).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(seasons::wants_set(&conn, current).await.unwrap());
    assert_eq!(
        store(bearer("job"), current, gz, current).await,
        StatusCode::NO_CONTENT
    );
    assert!(!seasons::wants_set(&conn, current).await.unwrap());
    // Stored, private until the season is over
    let s = seasons(&conn, current).await.unwrap();
    assert_eq!((s.current.set_size, s.current.set_sha256), (None, None));
    housekeeping_at(&conn, current + 1).await.unwrap();
    let s = seasons(&conn, current + 1).await.unwrap();
    assert_eq!(s.past[0].set_size, Some(4));
    assert_eq!(
        s.past[0].set_sha256.as_deref(),
        Some(&*hex(&Sha256::digest(gz)))
    );
    // The new season wants its own
    assert!(seasons::wants_set(&conn, current + 1).await.unwrap());
}

/// Reports of an evaluation that is over (finished, requeued, or replaced by another job) change
/// nothing: the token is checked in the same statement that writes.
#[tokio::test]
async fn callbacks_cannot_change_a_completed_or_replaced_evaluation() {
    let conn = test_db("callback-lifecycle").await;
    let id = submit(&conn, None, form("callback", 61)).await.unwrap();
    conn.execute(
        "UPDATE dojo_submissions SET state = 'running', job_token = 'first' WHERE id = ?1",
        params![id],
    )
    .await
    .unwrap();
    let done = finished(id, season(), 1, 1, RULES_VERSION);
    assert!(
        worker::apply_for_job(&conn, &done, Some("first"))
            .await
            .unwrap()
    );
    let failed = Report::Failed {
        id,
        season: season(),
        message: "late failure".into(),
    };
    assert!(
        !worker::apply_for_job(&conn, &failed, Some("first"))
            .await
            .unwrap()
    );
    assert_eq!(
        submission(&conn, id, None).await.unwrap().unwrap().state,
        DojoState::Done
    );
    crate::server::admin::submission_action(&conn, id, crate::model::SubmissionAction::Requeue)
        .await
        .unwrap();
    assert!(
        !worker::apply_for_job(&conn, &done, Some("first"))
            .await
            .unwrap()
    );
    conn.execute(
        "UPDATE dojo_submissions SET state = 'running', job_token = 'replacement' WHERE id = ?1",
        params![id],
    )
    .await
    .unwrap();
    assert!(
        !worker::apply_for_job(&conn, &done, Some("first"))
            .await
            .unwrap()
    );
    assert!(
        worker::apply_for_job(&conn, &done, Some("replacement"))
            .await
            .unwrap()
    );
    assert_eq!(state_and_token(&conn, id).await.1, None);
}

/// A worker that reports back before it even answered the job: the dispatcher claimed the
/// submission before sending it, so taking the job doesn't overwrite the result.
#[tokio::test]
async fn a_callback_before_acceptance_is_not_overwritten_by_dispatch() {
    use axum::{Router, extract::Query, routing::post};
    use std::collections::HashMap;
    let conn = test_db("fast-callback").await;
    let id = submit(&conn, None, form("fast", 62)).await.unwrap();
    let callback_conn = conn.clone();
    let worker = Router::new().route(
        "/jobs",
        post(move |Query(q): Query<HashMap<String, String>>| {
            let conn = callback_conn.clone();
            async move {
                assert_eq!(q["rules"], RULES_VERSION.to_string());
                let report = finished(id, season(), 1, 1, RULES_VERSION);
                assert!(
                    worker::apply_for_job(&conn, &report, Some(&q["report_token"]))
                        .await
                        .unwrap()
                );
                axum::http::StatusCode::ACCEPTED
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, worker).await.unwrap();
    });
    worker::dispatch_to(&conn, &url, "test-token")
        .await
        .unwrap();
    assert_eq!(
        submission(&conn, id, None).await.unwrap().unwrap().state,
        DojoState::Done
    );
    server.abort();
}

/// A finished evaluation with these results: per formula its family, answer and the fuel if
/// solved.
fn finished_with(id: i64, season: u32, results: &[(Family, Answer, Option<u64>)]) -> Report {
    let results: Vec<InstanceResult> = results
        .iter()
        .enumerate()
        .map(|(i, &(family, expected, fuel))| InstanceResult {
            name: format!("f-{i:02}"),
            family,
            expected,
            outcome: fuel.map_or(Outcome::Timeout, |_| Outcome::Solved(expected)),
            fuel: fuel.unwrap_or(DEFAULT_BUDGET),
        })
        .collect();
    let runs: Vec<RunResult> = results
        .iter()
        .map(|r| RunResult {
            name: r.name.clone(),
            family: r.family,
            expected: r.expected,
            outcome: r.outcome,
            fuel: r.fuel,
            wall_ms: 1,
        })
        .collect();
    Report::Finished {
        id,
        season,
        score: Score::from_results(&runs, DEFAULT_BUDGET),
        fingerprint: dojo::fingerprint(&runs),
        results,
        rules_version: RULES_VERSION,
    }
}

/// SAT/UNSAT/ALL, the VBS, contributions and the cactus plot: on the board, on a submission's
/// page and in the archive, with the held-out family kept secret until then.
#[tokio::test]
async fn metrics_beyond_par2() {
    use Answer::{Sat, Unsat};
    use Family::{Hashi, HashiParity, Tseitin};
    const G: u64 = 1_000_000_000;
    let conn = test_db("metrics").await;
    let alice = user(&conn, "alice", true).await;
    let (first, second) = (season(), season() + 1);
    housekeeping_at(&conn, first).await.unwrap();
    let a = submit(&conn, Some(&alice), form("alice's", 1))
        .await
        .unwrap();
    let worse = submit(&conn, Some(&alice), form("alice's worse", 2))
        .await
        .unwrap();
    let house = submit(&conn, None, form("Hashi CDCL", 3)).await.unwrap();
    // alice is fast on Hashi, the house entry alone proves the parity formulas
    let set = |hashi: [Option<u64>; 2], parity: [Option<u64>; 2], tseitin: Option<u64>| {
        [
            (Hashi, Sat, hashi[0]),
            (Hashi, Sat, hashi[1]),
            (HashiParity, Unsat, parity[0]),
            (HashiParity, Unsat, parity[1]),
            (Tseitin, Unsat, tseitin),
        ]
    };
    for (id, results) in [
        (a, set([Some(G), Some(2 * G)], [None, None], Some(G))),
        (worse, set([Some(3 * G), None], [None, None], None)),
        (house, set([Some(4 * G), None], [Some(G), Some(G)], Some(G))),
    ] {
        apply(&conn, &finished_with(id, first, &results))
            .await
            .unwrap();
    }

    let b = board(&conn).await.unwrap();
    let names: Vec<&str> = b.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Hashi CDCL", "alice's"]);
    let portfolio = b.portfolio.clone().unwrap();
    assert_eq!(portfolio.entries, 2);
    // VBS: 1, 2, 1, 1, 1 billion
    assert_eq!((portfolio.vbs.solved, portfolio.vbs.total), (5, 5));
    assert!((portfolio.vbs.par2 - 1.2).abs() < 1e-9);
    assert_eq!(portfolio.cactus.vbs, [G, G, G, G, 2 * G]);
    assert_eq!(portfolio.cactus.formulas, 5);
    assert_eq!(portfolio.cactus.curves[1].fuel, [G, G, 2 * G]);
    assert_eq!(portfolio.cactus.curves[0].id, house);
    // Without the house entry the parity formulas are lost: 2 × (14 − 1) billion over 5
    let house_entry = b.entries[0].contribution.unwrap();
    assert!((house_entry.marginal - 26.0 / 5.0).abs() < 1e-9);
    assert_eq!(house_entry.sota, 3.0);
    let alice_entry = b.entries[1].contribution.unwrap();
    assert_eq!(alice_entry.sota, 2.0);
    assert!((house_entry.relative + alice_entry.relative - 1.0).abs() < 1e-9);
    // While the season runs the Tseitin formula is the mystery's, outside SAT and UNSAT
    let split = b.entries[1].breakdown.clone().unwrap();
    assert_eq!(split.held_out, 1);
    let (sat, unsat) = (split.total.sat.unwrap(), split.total.unsat.unwrap());
    assert_eq!(
        (sat.solved, sat.total, unsat.solved, unsat.total),
        (2, 2, 0, 2)
    );
    assert_eq!(split.total.all.solved, 3);
    let mystery = split.families.last().unwrap();
    assert_eq!(mystery.family, Family::Mystery);
    assert_eq!(mystery.breakdown.sat, None);
    assert!(!serde_json::to_string(&b).unwrap().contains("Tseitin"));

    // Its page: the same numbers and the plot with its own curve
    let page = submission(&conn, a, Some(alice.id)).await.unwrap().unwrap();
    assert_eq!(page.contribution, b.entries[1].contribution);
    assert_eq!(page.breakdown, b.entries[1].breakdown);
    assert_eq!(page.portfolio, b.portfolio);
    // Not ranked: no contribution, but its curve in the plot
    let page = submission(&conn, worse, None).await.unwrap().unwrap();
    assert_eq!(page.contribution, None);
    let curves = page.portfolio.unwrap().cactus.curves;
    assert_eq!(curves.len(), 3);
    assert_eq!((curves[2].id, curves[2].fuel.clone()), (worse, vec![3 * G]));
    assert_eq!(page.breakdown.unwrap().total.all.solved, 1);
    // Nothing to plot before it is done
    let waiting = submit(&conn, None, form("Hashi DPLL", 4)).await.unwrap();
    let page = submission(&conn, waiting, None).await.unwrap().unwrap();
    assert_eq!((page.portfolio, page.breakdown), (None, None));

    // Archived: the same contributions, the family under its own name
    housekeeping_at(&conn, second).await.unwrap();
    let past = seasons(&conn, second).await.unwrap().past.remove(0);
    assert_eq!(past.portfolio.unwrap().vbs, portfolio.vbs);
    assert_eq!(past.board[0].contribution, b.entries[0].contribution);
    let revealed = past.board[1].breakdown.clone().unwrap();
    assert_eq!(revealed.held_out, 0);
    assert_eq!(revealed.families.last().unwrap().family, Tseitin);
    assert_eq!(revealed.total.unsat.unwrap().total, 3);
    let json = archived(&conn, first, second).await.unwrap().unwrap();
    assert_eq!(json.vbs, Some(portfolio.vbs));
    let entry = |id| json.entries.iter().find(|e| e.id == id).unwrap();
    assert_eq!(entry(house).contribution, b.entries[0].contribution);
    // Not ranked: its breakdown, no contribution
    assert_eq!(entry(worse).contribution, None);
    assert_eq!(entry(worse).breakdown.as_ref().unwrap().total.all.solved, 1);
    // Old scores in the board without the new fields still load
    let old = r#"{"rank":1,"id":1,"name":"x","author":"y","house":false,"created_at":0,
        "score":{"par2":1.0,"solved":1,"total":1,"wrong":0,"budget":10,"fuel":0.5,"families":[]}}"#;
    let entry: crate::model::DojoEntry = serde_json::from_str(old).unwrap();
    assert_eq!((entry.breakdown, entry.contribution), (None, None));
}
