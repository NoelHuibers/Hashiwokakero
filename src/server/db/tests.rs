//! Migrating databases of older versions: for every schema version, a database with rows written
//! at that version is migrated to the current one, and the rows have to load as before (with the
//! defaults of the columns that came later).

use dojo::{Family, FamilyScore, Score};
use libsql::{Connection, params};

use super::{MIGRATIONS, connect, migrate, migrate_to, open_local};
use crate::golden::puzzle;
use crate::model::{DojoState, Flag, Stamp};
use crate::server::auth::user_by_id;
use crate::server::daily::{attempt, get_or_create, leaderboard};
use crate::server::dojo::submission;

/// A fresh database file, removed again when dropped.
struct TestDb {
    path: std::path::PathBuf,
    conn: Connection,
    _database: libsql::Database,
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn test_db(version: usize) -> TestDb {
    let path =
        std::env::temp_dir().join(format!("hashi-migrate-{version}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let database = open_local(path.to_str().unwrap()).await.unwrap();
    let conn = database.connect().unwrap();
    TestDb {
        path,
        conn,
        _database: database,
    }
}

const DAY: i64 = 20_745;
const PUZZLE: &str = r#"{"width":3,"height":3,"islands":[{"pos":{"x":0,"y":0},"required":2},{"pos":{"x":2,"y":0},"required":3},{"pos":{"x":2,"y":2},"required":1}]}"#;

fn score() -> Score {
    Score {
        par2: 2.5,
        solved: 1,
        total: 1,
        wrong: 0,
        budget: 10_000_000_000,
        fuel: 1.25,
        families: vec![FamilyScore {
            family: Family::Hashi,
            solved: 1,
            total: 1,
            par2: 1.25,
        }],
    }
}

/// Rows like a server at schema `version` wrote them: only the tables and columns it had.
async fn write_rows(conn: &Connection, version: usize) {
    let run = |sql: &'static str| async move { conn.execute_batch(sql).await.unwrap() };
    // 1: accounts, sessions, a daily and two attempts, one finished
    run(r"
        INSERT INTO users (id, username, name, password_hash, created_at)
            VALUES (1, 'noel', 'noel', '$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0c2FsdA$aGFzaGhhc2hoYXNoaGFzaGhhc2hoYXNoaGFzaGhhc2g', 1000);
        INSERT INTO users (id, username, name, password_hash, created_at)
            VALUES (2, NULL, 'Brave Heron 42', NULL, 2000);
        INSERT INTO sessions (token_hash, user_id, created_at, expires_at)
            VALUES ('ab', 1, 1000, 9000000000000);
        INSERT INTO attempts (user_id, day, started_at, finished_at, hints, checks, revealed, time_ms, moves, bridges)
            VALUES (1, 20745, 1000, 96000, 1, 0, 0, 125000, 12, '[2,1]');
        INSERT INTO attempts (user_id, day, started_at) VALUES (2, 20745, 5000);
    ")
    .await;
    conn.execute(
        "INSERT INTO dailies (day, difficulty, puzzle, solution, created_at) VALUES (?1, 'easy', ?2, '[2,1]', 500)",
        params![DAY, PUZZLE],
    )
    .await
    .unwrap();
    if version >= 2 {
        // A Dojo submission, scored
        conn.execute(
            "INSERT INTO dojo_submissions (id, user_id, name, description, source_url, wasm, sha256, size, created_at, updated_at, state, season, attempts, progress, total, score, par2, solved)
             VALUES (1, 1, 'mine', 'CDCL', 'https://example.com', X'0061736d01000000', 'ab', 8, 1000, 2000, 'done', 1, 1, 1, 1, ?1, 2.5, 1)",
            params![serde_json::to_string(&score()).unwrap()],
        )
        .await
        .unwrap();
    }
    if version >= 3 {
        // Fair play and the Dojo's copy detection
        run(r"
            UPDATE attempts SET flag = 'pace', log = '[]', device = 'd' WHERE user_id = 2;
            UPDATE dojo_submissions SET code_hash = 'cd', fingerprint = '[1]', ip_hash = 'n', job_token = 't';
        ")
        .await;
    }
    if version >= 4 {
        run("INSERT INTO stamps (user_id, stamp, earned_at) VALUES (1, 'first', 96000)").await;
    }
    if version >= 5 {
        run("UPDATE attempts SET daypart = 'lunch' WHERE user_id = 1").await;
    }
    if version >= 6 {
        run("UPDATE users SET stamp = 'first' WHERE id = 1").await;
    }
    if version >= 7 {
        run("INSERT INTO contact_messages (name, email, message, created_at) VALUES ('', 'a@b.c', 'Hi', 1)")
            .await;
    }
}

async fn count(conn: &Connection, sql: &str) -> i64 {
    let mut rows = conn.query(sql, ()).await.unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

#[tokio::test]
async fn old_databases_migrate_with_their_data() {
    assert_eq!(
        MIGRATIONS.len(),
        7,
        "a new migration needs rows in `write_rows`"
    );
    for version in 1..=MIGRATIONS.len() {
        let db = test_db(version).await;
        let conn = &db.conn;
        migrate_to(conn, version).await.unwrap();
        write_rows(conn, version).await;
        migrate(conn)
            .await
            .unwrap_or_else(|e| panic!("from version {version}: {e}"));
        let label = format!("migrated from version {version}");

        let noel = user_by_id(conn, 1).await.unwrap().unwrap();
        assert_eq!(
            (
                noel.username.as_deref(),
                noel.name.as_str(),
                noel.created_at
            ),
            (Some("noel"), "noel", 1000),
            "{label}"
        );
        assert_eq!(user_by_id(conn, 2).await.unwrap().unwrap().username, None);
        assert_eq!(
            count(conn, "SELECT COUNT(*) FROM sessions").await,
            1,
            "{label}"
        );

        // The stored daily is loaded, not generated again
        let daily = get_or_create(conn, DAY).await.unwrap();
        assert_eq!(
            (daily.puzzle, daily.solution),
            (puzzle(), vec![2, 1]),
            "{label}"
        );

        let finished = attempt(conn, 1, DAY).await.unwrap().unwrap();
        assert_eq!(
            (
                finished.started_at,
                finished.finished_at,
                finished.hints,
                finished.time_ms,
                finished.bridges,
                finished.flag
            ),
            (1000, Some(96_000), 1, Some(125_000), Some(vec![2, 1]), None),
            "{label}"
        );
        // Unflagged until version 3 brought flags
        let running = attempt(conn, 2, DAY).await.unwrap().unwrap();
        let flag = (version >= 3).then_some(Flag::Pace);
        assert_eq!((running.finished_at, running.flag), (None, flag), "{label}");

        let board = leaderboard(conn, DAY, Some(1)).await.unwrap();
        assert_eq!(board.entries.len(), 1, "{label}");
        let shown = (version >= 6).then_some(Stamp::First);
        assert_eq!(
            (
                board.entries[0].user_id,
                board.entries[0].time_ms,
                board.entries[0].stamp
            ),
            (1, 125_000, shown),
            "{label}"
        );

        if version >= 2 {
            let s = submission(conn, 1, Some(1)).await.unwrap().unwrap();
            assert_eq!(
                (s.state, s.score, s.similar_to, s.mine),
                (DojoState::Done, Some(score()), None, true),
                "{label}"
            );
        }
        // Columns that came later are empty for old rows
        let defaults = [
            ("SELECT COUNT(*) FROM users WHERE deleted_at IS NULL", 2),
            ("SELECT COUNT(*) FROM attempts WHERE run IS NULL", 2),
            (
                "SELECT COUNT(*) FROM attempts WHERE daypart IS NULL",
                if version >= 5 { 1 } else { 2 },
            ),
            (
                "SELECT COUNT(*) FROM stamps WHERE announced = 0",
                i64::from(version >= 4),
            ),
        ];
        for (sql, expected) in defaults {
            assert_eq!(count(conn, sql).await, expected, "{label}: {sql}");
        }
        // Every version recorded, and migrating again changes nothing
        let versions = i64::try_from(MIGRATIONS.len()).unwrap();
        assert_eq!(
            count(conn, "SELECT COUNT(*) FROM schema_migrations").await,
            versions
        );
        migrate(conn).await.unwrap();
        assert_eq!(
            count(conn, "SELECT MAX(version) FROM schema_migrations").await,
            versions
        );
    }
}

/// Two requests writing at the same moment: the second waits for the first instead of failing with
/// "database is locked" (it did, the browser tests' parallel players got errors). Local queries
/// block their thread while they wait, the first one commits on another.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_waits_for_another_one() {
    let path = std::env::temp_dir().join(format!("hashi-busy-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let database = open_local(path.to_str().unwrap()).await.unwrap();
    let first = connect(&database).unwrap();
    let second = connect(&database).unwrap();
    migrate(&first).await.unwrap();
    first.execute("BEGIN IMMEDIATE", ()).await.unwrap();
    first
        .execute(
            "INSERT INTO users (username, name, created_at) VALUES ('first', 'first', 1)",
            (),
        )
        .await
        .unwrap();
    let commit = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        first.execute("COMMIT", ()).await.unwrap();
    });
    second
        .execute(
            "INSERT INTO users (username, name, created_at) VALUES ('second', 'second', 2)",
            (),
        )
        .await
        .unwrap();
    commit.await.unwrap();
    assert_eq!(count(&second, "SELECT COUNT(*) FROM users").await, 2);
    let _ = std::fs::remove_file(&path);
}
