//! Tests of community benchmarks against a fresh database file and stub workers.

use axum::http::StatusCode;
use dojo::Answer;
use dojo::community::{BenchmarkReport, PoolInstance, Refusal, Verdict};
use libsql::{Connection, params};

use super::benchmarks::{self, BenchmarkError, BenchmarkForm};
use super::*;
use crate::model::{BenchmarkAction, BenchmarkState};
use crate::server::auth::User;
use crate::server::{db, now_ms};

async fn test_db(name: &str) -> Connection {
    let path = std::env::temp_dir().join(format!("hashi-bench-{name}-{}.db", std::process::id()));
    db::remove_local(&path);
    let db = db::open_local(path.to_str().unwrap()).await.unwrap();
    let conn = db.connect().unwrap();
    db::migrate(&conn).await.unwrap();
    conn
}

async fn user(conn: &Connection, name: &str, registered: bool, created_at: i64) -> User {
    conn.execute(
        "INSERT INTO users (username, name, created_at) VALUES (?1, ?2, ?3)",
        params![
            registered.then(|| name.to_string()),
            name.to_string(),
            created_at
        ],
    )
    .await
    .unwrap();
    User {
        id: conn.last_insert_rowid(),
        username: registered.then(|| name.into()),
        name: name.into(),
        created_at,
        renamed_at: None,
    }
}

/// A formula of `n` unit clauses, a different one for every `n`.
fn formula(n: i32) -> String {
    use std::fmt::Write as _;
    let mut text = format!("c test\np cnf {n} {n}\n");
    for v in 1..=n {
        let _ = writeln!(text, "{v} 0");
    }
    text
}

fn form(family: &str, text: &str) -> BenchmarkForm {
    BenchmarkForm {
        family: family.into(),
        description: "a test\nwith two lines".into(),
        claimed: None,
        license: true,
        formula: text.as_bytes().to_vec(),
        from_gbd: false,
        network: None,
    }
}

async fn state(conn: &Connection, id: i64) -> (String, Option<String>) {
    let mut rows = conn
        .query(
            "SELECT state, reason FROM dojo_benchmarks WHERE id = ?1",
            params![id],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    (row.get(0).unwrap(), row.get(1).unwrap())
}

async fn count(conn: &Connection, sql: &str) -> i64 {
    let mut rows = conn.query(sql, ()).await.unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

/// Marks a benchmark as at the worker and applies its verdict.
async fn verdict(conn: &Connection, id: i64, verdict: Verdict) {
    conn.execute(
        "UPDATE dojo_benchmarks SET state = 'checking', attempts = attempts + 1 WHERE id = ?1",
        params![id],
    )
    .await
    .unwrap();
    benchmarks::apply(conn, &BenchmarkReport { id, verdict })
        .await
        .unwrap();
}

fn verified(answer: Answer) -> Verdict {
    Verdict::Verified {
        answer,
        model: (answer == Answer::Sat).then(|| vec![1]),
        solve_ms: 1200,
    }
}

/// What the form takes and what it refuses, like solver submissions: accounts with a username
/// and a day of age, three a day, no duplicates (also scrambled ones), a licence.
#[tokio::test]
async fn submissions_are_checked() {
    let conn = test_db("submit").await;
    let old = now_ms() - 2 * MS_PER_DAY;
    let ada = user(&conn, "ada", true, old).await;
    let guest = user(&conn, "Brave Heron", false, old).await;
    let fresh = user(&conn, "fresh", true, now_ms()).await;
    let submit = |u: &User, f: BenchmarkForm| {
        let (conn, u) = (conn.clone(), u.clone());
        async move { benchmarks::submit(&conn, &u, f, None).await }
    };
    assert_eq!(
        submit(&guest, form("x", &formula(1))).await,
        Err(BenchmarkError::SignIn)
    );
    assert_eq!(
        submit(&fresh, form("x", &formula(1))).await,
        Err(BenchmarkError::AccountAge)
    );
    for (f, error) in [
        (form("Bad Family", &formula(1)), BenchmarkError::Family),
        (form("-x", &formula(1)), BenchmarkError::Family),
        (form(&"x".repeat(41), &formula(1)), BenchmarkError::Family),
        (
            BenchmarkForm {
                description: "x".repeat(601),
                ..form("x", &formula(1))
            },
            BenchmarkError::Description,
        ),
        (
            BenchmarkForm {
                description: "bell\u{7}".into(),
                ..form("x", &formula(1))
            },
            BenchmarkError::Description,
        ),
        (
            BenchmarkForm {
                license: false,
                ..form("x", &formula(1))
            },
            BenchmarkError::License,
        ),
        (form("x", "not dimacs"), BenchmarkError::Invalid),
        (form("x", "p cnf 1 0\n"), BenchmarkError::Invalid),
        (
            form("x", &"c ".repeat(dojo::community::MAX_BYTES)),
            BenchmarkError::TooLarge,
        ),
    ] {
        assert_eq!(submit(&ada, f).await, Err(error));
    }
    // Uploaded plain, stored normalized under its GBD hash
    let id = submit(&ada, form("Scheduling", &formula(2))).await.unwrap();
    assert_eq!(state(&conn, id).await, ("queued".into(), None));
    let mut rows = conn
        .query(
            "SELECT family, cnf, hash, vars, clauses, license, description FROM dojo_benchmarks WHERE id = ?1",
            params![id],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    assert_eq!(row.get::<String>(0).unwrap(), "scheduling");
    assert_eq!(row.get::<Vec<u8>>(1).unwrap(), b"p cnf 2 2\n1 0\n2 0\n");
    // md5 of "1 0 2 0"
    assert_eq!(
        row.get::<String>(2).unwrap(),
        "4f658fd92709587c56c89300560e76e3"
    );
    assert_eq!(
        (row.get::<i64>(3).unwrap(), row.get::<i64>(4).unwrap()),
        (2, 2)
    );
    assert_eq!(row.get::<String>(5).unwrap(), "CC-BY-4.0");
    assert_eq!(row.get::<String>(6).unwrap(), "a test\nwith two lines");
    drop(rows);
    // The same formula again, or renamed and reordered: a duplicate
    assert_eq!(
        submit(&ada, form("x", &formula(2))).await,
        Err(BenchmarkError::Duplicate)
    );
    assert_eq!(
        submit(&ada, form("x", "p cnf 2 2\n-2 0\n-1 0\n")).await,
        Err(BenchmarkError::Duplicate)
    );
    // xz-compressed, like GBD's files
    let mut xz = Vec::new();
    lzma_rs::xz_compress(&mut formula(3).as_bytes(), &mut xz).unwrap();
    let packed = BenchmarkForm {
        formula: xz,
        ..form("x", "")
    };
    let hash = dojo::community::gbd_hash(&dojo::community::Formula::parse(&formula(3)).unwrap());
    // Fetched under another hash than it has: refused
    assert_eq!(
        benchmarks::submit(
            &conn,
            &ada,
            BenchmarkForm {
                formula: packed.formula.clone(),
                ..form("x", "")
            },
            Some(&"0".repeat(32))
        )
        .await,
        Err(BenchmarkError::Gbd)
    );
    let id3 = benchmarks::submit(&conn, &ada, packed, Some(&hash.to_uppercase()))
        .await
        .unwrap();
    assert!(id3 > id);
    // Three a day
    assert!(submit(&ada, form("x", &formula(4))).await.unwrap() > id3);
    assert_eq!(
        submit(&ada, form("x", &formula(5))).await,
        Err(BenchmarkError::Limit)
    );
    // And per network, whoever it is
    let networked = |n| BenchmarkForm {
        network: Some("net".into()),
        ..form("x", &formula(n))
    };
    let bob = user(&conn, "bob", true, old).await;
    for n in 6..9 {
        assert!(submit(&bob, networked(n)).await.is_ok(), "{n}");
    }
    let carol = user(&conn, "carol", true, old).await;
    assert_eq!(
        submit(&carol, networked(10)).await,
        Err(BenchmarkError::NetworkLimit)
    );

    // The page: the player's own and their count left
    let page = benchmarks_page(&conn, Some(&ada)).await.unwrap();
    assert!(page.registered);
    assert_eq!((page.left_today, page.mine.len(), page.per_set), (0, 3, 6));
    assert_eq!(page.mine.last().unwrap().state, BenchmarkState::Queued);
    let guest_page = benchmarks_page(&conn, None).await.unwrap();
    assert!(!guest_page.registered && guest_page.mine.is_empty());
    for code in [
        BenchmarkError::SignIn,
        BenchmarkError::Busy,
        BenchmarkError::Source,
        BenchmarkError::Server,
    ] {
        assert_ne!(code.code(), "");
    }
}

/// A GBD hash is 32 hex digits, and the file comes from the database's `/file/<hash>`.
#[tokio::test]
async fn files_come_from_gbd() {
    assert!(benchmarks::gbd_hash_ok(&"aB".repeat(16)));
    assert!(!benchmarks::gbd_hash_ok("../etc/passwd"));
    let app = axum::Router::new().route(
        "/file/{hash}",
        axum::routing::get(
            |axum::extract::Path(hash): axum::extract::Path<String>| async move {
                if hash == "a".repeat(32) {
                    (StatusCode::OK, b"p cnf 1 1\n1 0\n".to_vec())
                } else {
                    (StatusCode::NOT_FOUND, Vec::new())
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    assert_eq!(
        benchmarks::fetch_gbd(&url, &"a".repeat(32)).await.unwrap(),
        b"p cnf 1 1\n1 0\n"
    );
    assert_eq!(
        benchmarks::fetch_gbd(&url, &"b".repeat(32)).await,
        Err(BenchmarkError::Gbd)
    );
    assert_eq!(
        benchmarks::fetch_gbd(&url, "short").await,
        Err(BenchmarkError::Gbd)
    );
    assert_eq!(
        benchmarks::fetch_gbd("http://127.0.0.1:1", &"a".repeat(32)).await,
        Err(BenchmarkError::Gbd)
    );
}

/// The worker's verdicts, the moderator's decisions and what an author sees.
#[tokio::test]
async fn verdicts_and_moderation() {
    let conn = test_db("verdicts").await;
    // A season that starts before there is a pool has no draw
    worker::housekeeping_at(&conn, season()).await.unwrap();
    assert_eq!(benchmarks::digest(&conn, season()).await.unwrap(), "");
    let ada = user(&conn, "ada", true, 0).await;
    let mut ids = Vec::new();
    for (n, claimed) in [(1, None), (2, Some(Answer::Sat)), (3, None), (4, None)] {
        let f = BenchmarkForm {
            claimed,
            ..form("x", &formula(n))
        };
        ids.push(benchmarks::submit(&conn, &ada, f, None).await.unwrap());
        conn.execute("UPDATE dojo_benchmarks SET created_at = 0", ())
            .await
            .unwrap();
    }
    verdict(&conn, ids[0], verified(Answer::Unsat)).await;
    assert_eq!(state(&conn, ids[0]).await, ("verified".into(), None));
    // Expected SAT, it is UNSAT: told so, not in the pool
    verdict(&conn, ids[1], verified(Answer::Unsat)).await;
    assert_eq!(
        state(&conn, ids[1]).await,
        ("refused".into(), Some("wrong_claim".into()))
    );
    verdict(
        &conn,
        ids[2],
        Verdict::Refused {
            reason: Refusal::TooEasy,
        },
    )
    .await;
    assert_eq!(
        state(&conn, ids[2]).await,
        ("refused".into(), Some("too_easy".into()))
    );
    // Our failure: queued again, given up after a few attempts
    verdict(
        &conn,
        ids[3],
        Verdict::Failed {
            message: "x".into(),
        },
    )
    .await;
    assert_eq!(state(&conn, ids[3]).await.0, "queued");
    for _ in 0..3 {
        verdict(
            &conn,
            ids[3],
            Verdict::Failed {
                message: "x".into(),
            },
        )
        .await;
    }
    assert_eq!(
        state(&conn, ids[3]).await,
        ("refused".into(), Some("failed".into()))
    );
    // A verdict for something not at the worker changes nothing
    benchmarks::apply(
        &conn,
        &BenchmarkReport {
            id: ids[0],
            verdict: verified(Answer::Sat),
        },
    )
    .await
    .unwrap();
    assert_eq!(state(&conn, ids[0]).await, ("verified".into(), None));

    // Only verified ones can be accepted
    benchmark_action(&conn, ids[2], BenchmarkAction::Accept)
        .await
        .unwrap();
    assert_eq!(state(&conn, ids[2]).await.0, "refused");
    benchmark_action(&conn, ids[0], BenchmarkAction::Accept)
        .await
        .unwrap();
    assert_eq!(state(&conn, ids[0]).await, ("accepted".into(), None));
    let page = benchmarks_page(&conn, Some(&ada)).await.unwrap();
    assert_eq!(page.pool.len(), 1);
    assert_eq!(
        (page.pool[0].author.as_str(), page.pool[0].answer),
        ("ada", Answer::Unsat)
    );
    let list = admin_benchmarks(&conn).await.unwrap();
    assert_eq!(list.len(), 4);
    assert_eq!(
        list.iter().find(|b| b.id == ids[0]).unwrap().solve_ms,
        Some(1200)
    );
    benchmark_action(&conn, ids[0], BenchmarkAction::Reject)
        .await
        .unwrap();
    assert_eq!(
        state(&conn, ids[0]).await,
        ("rejected".into(), Some("moderation".into()))
    );
    benchmark_action(&conn, ids[0], BenchmarkAction::Accept)
        .await
        .unwrap();
    assert_eq!(state(&conn, ids[0]).await.0, "accepted");
    benchmark_action(&conn, ids[2], BenchmarkAction::Requeue)
        .await
        .unwrap();
    assert_eq!(state(&conn, ids[2]).await, ("queued".into(), None));
    benchmark_action(&conn, ids[1], BenchmarkAction::Delete)
        .await
        .unwrap();
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM dojo_benchmarks").await,
        3
    );
    let summary = benchmarks::summary(&conn).await.unwrap();
    assert_eq!(
        (summary.benchmarks, summary.authors),
        (1, vec!["ada".to_string()])
    );
}

/// Checks go to the worker one at a time with a token of their own; verdicts need that token;
/// checks that go quiet are tried again.
#[tokio::test]
async fn checks_go_to_the_worker() {
    let conn = test_db("checks").await;
    let ada = user(&conn, "ada", true, 0).await;
    let a = benchmarks::submit(&conn, &ada, form("x", &formula(1)), None)
        .await
        .unwrap();
    let b = benchmarks::submit(&conn, &ada, form("x", &formula(2)), None)
        .await
        .unwrap();
    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(
        String,
        Option<String>,
        Vec<u8>,
    )>::new()));
    let app =
        axum::Router::new().route(
            "/benchmarks",
            axum::routing::post({
                let sent = sent.clone();
                move |uri: axum::http::Uri,
                      headers: axum::http::HeaderMap,
                      body: axum::body::Bytes| async move {
                    let auth = headers
                        .get(axum::http::header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    sent.lock().unwrap().push((
                        uri.query().unwrap_or_default().to_string(),
                        auth,
                        body.to_vec(),
                    ));
                    StatusCode::ACCEPTED
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    benchmarks::dispatch_to(&conn, &client, &url, "secret")
        .await
        .unwrap();
    // One at a time
    benchmarks::dispatch_to(&conn, &client, &url, "secret")
        .await
        .unwrap();
    {
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert!(
            sent[0].0.starts_with(&format!("id={a}&report_token=")),
            "{}",
            sent[0].0
        );
        assert!(
            sent[0]
                .0
                .ends_with(&format!("&rules={}", dojo::RULES_VERSION))
        );
        assert_eq!(sent[0].1.as_deref(), Some("Bearer secret"));
        assert_eq!(sent[0].2, b"p cnf 1 1\n1 0\n");
    }
    assert_eq!(state(&conn, a).await.0, "checking");
    assert_eq!(state(&conn, b).await.0, "queued");
    // Gone quiet: queued again
    conn.execute(
        "UPDATE dojo_benchmarks SET updated_at = 0 WHERE id = ?1",
        params![a],
    )
    .await
    .unwrap();
    benchmarks::housekeeping(&conn, now_ms()).await.unwrap();
    assert_eq!(state(&conn, a).await.0, "queued");
    // A worker that refuses: an error, nothing changes
    let refusing = axum::Router::new().route(
        "/benchmarks",
        axum::routing::post(|| async { StatusCode::CONFLICT }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bad = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, refusing).await.unwrap() });
    assert!(
        benchmarks::dispatch_to(&conn, &client, &bad, "secret")
            .await
            .is_err()
    );
    assert!(
        benchmarks::dispatch_to(&conn, &client, "http://127.0.0.1:1", "secret")
            .await
            .is_err()
    );
    assert_eq!(state(&conn, a).await.0, "queued");
}

/// A season draws from the pool when it starts, and the draw stays: later acceptances wait for
/// the next season. The worker gets the draw before the season's first job, the archive credits
/// the authors, and deleting an account keeps drawn benchmarks without a name.
#[tokio::test]
async fn seasons_draw_from_the_pool() {
    let conn = test_db("draw").await;
    let (first, second) = (season(), season() + 1);
    let mut authors = Vec::new();
    for name in ["ada", "bob", "cy", "dee"] {
        authors.push(user(&conn, name, true, 0).await);
    }
    let accept = async |author: &User, family: &str, n: i32, answer: Answer| {
        let id = benchmarks::submit(&conn, author, form(family, &formula(n)), None)
            .await
            .unwrap();
        conn.execute("UPDATE dojo_benchmarks SET created_at = 0", ())
            .await
            .unwrap();
        verdict(&conn, id, verified(answer)).await;
        benchmark_action(&conn, id, BenchmarkAction::Accept)
            .await
            .unwrap();
        id
    };
    for (i, author) in authors.iter().enumerate() {
        for k in 0..3 {
            let n = i32::try_from(i * 3 + k + 1).unwrap();
            let answer = if k % 2 == 0 {
                Answer::Sat
            } else {
                Answer::Unsat
            };
            accept(author, &format!("fam{}", (i + k) % 4), n, answer).await;
        }
    }
    // The season starts: it draws, as many SAT as UNSAT
    worker::housekeeping_at(&conn, first).await.unwrap();
    let drawn = benchmarks::drawn(&conn, first).await.unwrap();
    assert_eq!(drawn.len(), dojo::community::PER_SET);
    let sat = drawn.iter().filter(|c| c.answer == Answer::Sat).count();
    assert_eq!(sat, dojo::community::PER_SET / 2);
    let digest = benchmarks::digest(&conn, first).await.unwrap();
    assert_eq!(digest.len(), 64);
    // The instances the worker gets are that draw
    let pool = benchmarks::instances(&conn, first).await.unwrap();
    assert_eq!(dojo::community::pool_digest(&pool), digest);
    assert_eq!(
        pool.iter().map(|p| p.origin.benchmark).collect::<Vec<_>>(),
        drawn.iter().map(|c| c.id).collect::<Vec<_>>()
    );
    assert!(
        pool.iter()
            .all(|p| (p.answer == Answer::Sat) == p.model.is_some())
    );
    // Accepted later: waits for the next season
    let late = user(&conn, "late", true, 0).await;
    accept(&late, "new", 40, Answer::Sat).await;
    worker::housekeeping_at(&conn, first).await.unwrap();
    assert_eq!(benchmarks::digest(&conn, first).await.unwrap(), digest);
    // The board counts the drawn formulas and credits the pool
    let b = board(&conn).await.unwrap();
    assert_eq!(b.formulas, dojo::SET_SIZE + 6);
    assert_eq!(b.community.benchmarks, 13);
    assert_eq!(b.community.authors.len(), 5);

    // The worker gets the draw once, then the jobs name it
    let puts = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Vec<PoolInstance>>::new()));
    let jobs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let has = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let app = axum::Router::new()
        .route(
            concat!("/pools/", "{digest}"),
            axum::routing::get({
                let has = has.clone();
                move || async move {
                    if has.load(std::sync::atomic::Ordering::Relaxed) {
                        StatusCode::OK
                    } else {
                        StatusCode::NOT_FOUND
                    }
                }
            })
            .put({
                let (puts, has) = (puts.clone(), has.clone());
                move |axum::Json(pool): axum::Json<Vec<PoolInstance>>| async move {
                    puts.lock().unwrap().push(pool);
                    has.store(true, std::sync::atomic::Ordering::Relaxed);
                    StatusCode::NO_CONTENT
                }
            }),
        )
        .route(
            "/jobs",
            axum::routing::post({
                let jobs = jobs.clone();
                move |uri: axum::http::Uri| async move {
                    jobs.lock()
                        .unwrap()
                        .push(uri.query().unwrap_or_default().to_string());
                    StatusCode::ACCEPTED
                }
            }),
        )
        .layer(axum::extract::DefaultBodyLimit::max(64 << 20));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let ada = authors[0].clone();
    for extra in [1, 2] {
        let solver = Form {
            name: format!("s{extra}"),
            wasm: vec![0, b'a', b's', b'm', 1, 0, 0, 0, 5, 3, 1, 0, extra],
            ..Default::default()
        };
        submit(&conn, Some(&ada), solver).await.unwrap();
    }
    worker::dispatch_to(&conn, &url, "secret").await.unwrap();
    assert_eq!(puts.lock().unwrap().len(), 1, "sent once");
    assert_eq!(puts.lock().unwrap()[0], pool);
    let jobs = jobs.lock().unwrap().clone();
    assert_eq!(jobs.len(), 2);
    assert!(
        jobs.iter().all(|q| q.ends_with(&format!("&pool={digest}"))),
        "{jobs:?}"
    );

    // The next season: the archived one credits its authors, the new one draws again
    worker::housekeeping_at(&conn, second).await.unwrap();
    let all = seasons(&conn, second).await.unwrap();
    let past = all.past.iter().find(|s| s.season == first).unwrap();
    assert_eq!(past.community, drawn);
    assert!(
        all.current.community.is_empty(),
        "never the running season's draw"
    );
    let archived = archived(&conn, first, second).await.unwrap().unwrap();
    assert_eq!(archived.community, drawn);
    assert_eq!(
        benchmarks::drawn(&conn, second).await.unwrap().len(),
        dojo::community::PER_SET
    );

    // Deleting an account: drawn benchmarks stay without a name, the others go
    let drawn_of_ada = drawn.iter().filter(|c| c.author == "ada").count()
        + benchmarks::drawn(&conn, second)
            .await
            .unwrap()
            .iter()
            .filter(|c| c.author == "ada" && !drawn.iter().any(|d| d.id == c.id))
            .count();
    crate::server::auth::erase(&conn, ada.id).await.unwrap();
    let left = count(
        &conn,
        "SELECT COUNT(*) FROM dojo_benchmarks WHERE user_id IS NULL",
    )
    .await;
    assert_eq!(usize::try_from(left).unwrap(), drawn_of_ada);
    let again = benchmarks::drawn(&conn, first).await.unwrap();
    assert_eq!(
        again.iter().filter(|c| c.author.is_empty()).count(),
        drawn.iter().filter(|c| c.author == "ada").count()
    );
    // A drawn benchmark is never deleted, only out of the pool
    let kept = drawn[0].id;
    benchmark_action(&conn, kept, BenchmarkAction::Delete)
        .await
        .unwrap();
    assert_eq!(state(&conn, kept).await.0, "rejected");
}
