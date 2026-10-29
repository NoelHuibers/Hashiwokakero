//! Migrating databases of older versions: for every schema version, a database with rows written
//! at that version is migrated to the current one, and the rows have to load as before (with the
//! defaults of the columns that came later).

use dojo::{Family, FamilyScore, Score};
use libsql::{Connection, params};

use super::{
    MIGRATIONS, TempDb, TestConn, connect, migrate, migrate_to, open_local, sweep_test_files,
    test_temp_path,
};
use crate::golden::puzzle;
use crate::model::{DojoState, FeedbackKind, FeedbackStatus, Flag, Stamp};
use crate::server::auth::user_by_id;
use crate::server::daily::{attempt, get_or_create, leaderboard};
use crate::server::dojo::submission;

/// A fresh database file, removed again when dropped.
struct TestDb {
    conn: Connection,
    database: libsql::Database,
    /// Removed after the connections are closed: fields are dropped in order.
    _file: TempDb,
}

async fn test_db(version: usize) -> TestDb {
    let file = TempDb::new(&format!("migrate-{version}"));
    let database = open_local(file.as_str()).await.unwrap();
    let conn = database.connect().unwrap();
    TestDb {
        conn,
        database,
        _file: file,
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
            unverified: 0,
        }],
        unverified: 0,
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
    if version >= 8 {
        run("UPDATE users SET renamed_at = 500 WHERE id = 1").await;
    }
    if version >= 9 {
        // A season archived with the submission in it
        run(r"
            UPDATE dojo_submissions SET rules_version = 1, results = '[]';
            INSERT INTO dojo_seasons (season, started_at, ended_at, commitment, seed, generator_version, rules_version)
                VALUES (1, 1000, 3000, 'c', 's', 1, 1);
            INSERT INTO dojo_season_sets (season, generator_version, archive, sha256, created_at)
                VALUES (1, 1, X'1f8b', 'h', 3000);
            INSERT INTO dojo_archive (season, submission_id, user_id, house, name, created_at, wrong, rank)
                VALUES (1, 1, 1, 0, 'mine', 1000, 0, 1);
        ")
        .await;
    }
    if version >= 10 {
        // A community benchmark in the pool, drawn for season 1
        run(r"
            INSERT INTO dojo_benchmarks (id, user_id, family, description, cnf, hash, shape, vars, clauses, state,
                answer, license, created_at, updated_at, accepted_at)
                VALUES (1, 1, 'tiny', 'two clauses', X'70', 'h', 's', 1, 2, 'accepted', 'UNSAT', 'CC-BY-4.0', 1000, 2000, 2000);
            INSERT INTO dojo_season_pool (season, position, benchmark_id) VALUES (1, 0, 1);
            UPDATE dojo_seasons SET pool_digest = 'd';
        ")
        .await;
    }
    if version >= 11 {
        // A banned player
        run(r"
            INSERT INTO users (id, username, name, password_hash, created_at, banned_at, ban_reason)
                VALUES (3, 'cheater', 'cheater', NULL, 3000, 4000, 'a bot');
        ")
        .await;
    }
    if version >= 15 {
        // A group of the two players
        run(r"
            INSERT INTO groups (id, name, invite, owner_id, created_at) VALUES (1, 'Team', 'abcdefgh23', 1, 5000);
            INSERT INTO group_members (group_id, user_id, joined_at, colour) VALUES (1, 1, 5000, 0);
            INSERT INTO group_members (group_id, user_id, joined_at, colour) VALUES (1, 2, 6000, 1);
        ")
        .await;
    }
    // 12 only added indexes
    if version >= 14 {
        run("UPDATE attempts SET attempt_id = 'existing-attempt-identity' WHERE user_id = 2").await;
    }
    if version >= 13 {
        run(r#"UPDATE attempts SET paid_actions = '[{"id":"saved-intent","action":"Check","bridges":[0,0],"outcome":{"Check":[]}}]' WHERE user_id = 2"#).await;
    }
    if version >= 16 {
        // noel's rival, the guest
        run("INSERT INTO rivals (user_id, rival_id, created_at) VALUES (1, 2, 5000)").await;
    }
    if version >= 17 {
        // noel pressed 凄 on the guest's result in their group
        run("INSERT INTO group_hanko (group_id, day, target_id, giver_id, kind, created_at) VALUES (1, 20745, 2, 1, 'wow', 7000)")
            .await;
    }
    if version >= 18 {
        // The group's river crossed its stream, noel laid planks of it
        run(r"
            UPDATE groups SET river_banked = 30;
            UPDATE group_members SET river_seen = 400, river_levels_seen = 1 WHERE user_id = 1;
            INSERT INTO group_river (group_id, level, target, completed_at) VALUES (1, 0, 400, 7000);
            INSERT INTO group_river_crew (group_id, level, user_id) VALUES (1, 0, 1);
        ")
        .await;
    }
    if version >= 20 {
        // noel's puzzle for the group, the guest solved it
        run(r#"
            INSERT INTO group_puzzles (id, group_id, author_id, title, puzzle, solution, created_at, closes_at)
                VALUES (1, 1, 1, 'Pair', '{"width":3,"height":1,"islands":[{"pos":{"x":0,"y":0},"required":1},{"pos":{"x":2,"y":0},"required":1}]}', '[1]', 5000, 604805000);
            INSERT INTO group_puzzle_attempts (puzzle_id, user_id, started_at, finished_at, time_ms, bridges)
                VALUES (1, 2, 6000, 9000, 3000, '[1]');
        "#)
        .await;
    }
    if version >= 22 {
        // Virtual keys preserve the original server time and legacy tie-breaker.
        let mut rows = conn
            .query(
                "SELECT time_ms, rank_ms, rank_finish FROM attempts WHERE user_id = 1",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        assert_eq!(row.get::<i64>(0).unwrap(), row.get::<i64>(1).unwrap());
        assert_eq!(row.get::<i64>(2).unwrap(), 96000);
    }
    if version >= 23 {
        // The guest reported noel's name, and blocked him
        run(r"
            INSERT INTO reports (reporter_id, player_id, reason, note, name, created_at, updated_at)
                VALUES (2, 1, 'name', 'rude', 'noel', 8000, 8000);
            INSERT INTO blocks (user_id, blocked_id, created_at) VALUES (2, 1, 8000);
        ")
        .await;
    }
    if version >= 24 {
        // A bug report from the app, answered
        run(r#"
            INSERT INTO contact_messages (name, email, message, created_at, category, context, locale, status, handled_at)
                VALUES ('Kim', '', 'The board froze', 9000, 'bug', '{"app":"1.1.0","platform":"android","player":2}', 'de', 'answered', 9500);
        "#)
        .await;
    }
    if version >= 25 {
        // An issue was made of the bug, it was fixed and Kim was told
        run("UPDATE contact_messages SET status = 'issue', github_issue = 190, issue_checked_at = 9600,
                 issue_closed_at = 9600, fixed_mailed_at = 9600 WHERE message = 'The board froze'")
        .await;
    }
    if version >= 21 {
        // The standings of the month of the daily, stored once it was over: noel won it
        run(r"
            INSERT INTO closed_months (month, first_day, last_day) VALUES (24321, 20727, 20757);
            INSERT INTO month_standings (month, place, user_id, points, solved, first_finish)
                VALUES (24321, 1, 1, 20, 1, 96000);
        ")
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
        25,
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

        let identity = attempt(conn, 2, DAY).await.unwrap().unwrap().attempt_id;
        if version >= 14 {
            assert_eq!(identity, "existing-attempt-identity");
        } else {
            assert_eq!(identity.len(), 32);
        }
        let other_identity = attempt(conn, 1, DAY).await.unwrap().unwrap().attempt_id;
        assert_ne!(identity, other_identity);

        let noel = user_by_id(conn, 1).await.unwrap().unwrap();
        assert_eq!(
            (
                noel.username.as_deref(),
                noel.name.as_str(),
                noel.created_at,
                noel.renamed_at
            ),
            (Some("noel"), "noel", 1000, (version >= 8).then_some(500)),
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

        // Rivals came with version 16
        let rivals = crate::server::daily::rivals(conn, 1).await.unwrap();
        assert_eq!(
            rivals,
            if version >= 16 { vec![2] } else { vec![] },
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
        let receipts: String = conn
            .query("SELECT paid_actions FROM attempts WHERE user_id = 2", ())
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap();
        assert_eq!(receipts.contains("saved-intent"), version >= 13, "{label}");
        let defaults = [
            (
                "SELECT COUNT(*) FROM attempts WHERE paid_actions = '[]'",
                if version >= 13 { 1 } else { 2 },
            ),
            (
                "SELECT COUNT(*) FROM users WHERE deleted_at IS NULL",
                2 + i64::from(version >= 11),
            ),
            // Nobody was banned before bans
            ("SELECT COUNT(*) FROM users WHERE banned_at IS NULL", 2),
            (
                "SELECT COUNT(*) FROM users WHERE banned_at = 4000 AND ban_reason = 'a bot'",
                i64::from(version >= 11),
            ),
            ("SELECT COUNT(*) FROM attempts WHERE run IS NULL", 2),
            // The seal pressed, not seen yet
            (
                "SELECT COUNT(*) FROM group_hanko WHERE seen = 0",
                i64::from(version >= 17),
            ),
            (
                "SELECT COUNT(*) FROM attempts WHERE daypart IS NULL",
                if version >= 5 { 1 } else { 2 },
            ),
            (
                "SELECT COUNT(*) FROM stamps WHERE announced = 0",
                i64::from(version >= 4),
            ),
            // Results from before rules versions are evaluated again
            (
                "SELECT COUNT(*) FROM dojo_submissions WHERE rules_version IS NULL",
                i64::from((2..9).contains(&version)),
            ),
            (
                "SELECT COUNT(*) FROM dojo_archive WHERE rank = 1",
                i64::from(version >= 9),
            ),
            // Seasons from before the pool have none
            (
                "SELECT COUNT(*) FROM dojo_seasons WHERE pool_digest IS NULL",
                i64::from(version == 9),
            ),
            (
                "SELECT COUNT(*) FROM dojo_benchmarks b JOIN dojo_season_pool p ON p.benchmark_id = b.id WHERE b.attempts = 0",
                i64::from(version >= 10),
            ),
            (
                "SELECT COUNT(*) FROM group_members m JOIN groups g ON g.id = m.group_id WHERE g.owner_id = 1",
                if version >= 15 { 2 } else { 0 },
            ),
            // Rivers came with version 18: the group's from before has nothing in it yet
            (
                "SELECT COALESCE((SELECT river_banked FROM groups WHERE id = 1), 0)",
                if version >= 18 { 30 } else { 0 },
            ),
            (
                "SELECT COUNT(*) FROM group_members WHERE river_seen = 0 AND river_levels_seen = 0",
                match version {
                    ..15 => 0,
                    15..=17 => 2,
                    _ => 1,
                },
            ),
            (
                "SELECT COUNT(*) FROM group_river JOIN group_river_crew USING (group_id, level)",
                i64::from(version >= 18),
            ),
            // 19 only added the trigger that takes a group's memberships along
            (
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = 'group_members_gone'",
                1,
            ),
            (
                "SELECT COUNT(*) FROM group_puzzle_attempts a JOIN group_puzzles p ON p.id = a.puzzle_id
                 WHERE p.author_id = 1 AND a.helps = '[]' AND p.kudos_seen = 0",
                i64::from(version >= 20),
            ),
            (
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = 'group_puzzles_gone'",
                1,
            ),
            // 12 only added the indexes of ranked times
            (
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index'
                    AND name IN ('attempts_user_ranked', 'attempts_day_ranked')",
                2,
            ),
            (
                "SELECT COUNT(*) FROM closed_months JOIN month_standings USING (month)
                 WHERE user_id = 1 AND place = 1",
                i64::from(version >= 21),
            ),
            (
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'closed_month_%'",
                4,
            ),
            // Messages from before issues were looked at: not yet
            (
                "SELECT COUNT(*) FROM contact_messages
                 WHERE issue_checked_at IS NULL AND issue_closed_at IS NULL AND fixed_mailed_at IS NULL",
                if version >= 25 { 1 } else { i64::from(version >= 7) * (1 + i64::from(version >= 24)) },
            ),
            // Open, not resolved
            (
                "SELECT COUNT(*) FROM reports r JOIN blocks b ON b.user_id = r.reporter_id
                 WHERE r.resolved_at IS NULL AND r.resolution IS NULL",
                i64::from(version >= 23),
            ),
        ];
        for (sql, expected) in defaults {
            assert_eq!(count(conn, sql).await, expected, "{label}: {sql}");
        }
        // Messages from before the feedback form: something else, new, without a context
        if version >= 7 {
            let messages = crate::server::contact::list(conn).await.unwrap();
            let old = messages.iter().find(|m| m.message == "Hi").unwrap();
            assert_eq!(
                (
                    old.kind,
                    old.status,
                    &old.context,
                    &old.locale,
                    old.handled_at
                ),
                (FeedbackKind::Other, FeedbackStatus::New, &None, &None, None),
                "{label}"
            );
            let bug = messages.iter().find(|m| m.message == "The board froze");
            assert_eq!(bug.is_some(), version >= 24, "{label}");
            if let Some(bug) = bug {
                let (status, issue) = if version >= 25 {
                    (FeedbackStatus::Issue, Some(190))
                } else {
                    (FeedbackStatus::Answered, None)
                };
                assert_eq!(
                    (
                        bug.kind,
                        bug.status,
                        bug.locale.as_deref(),
                        bug.handled_at,
                        bug.github_issue
                    ),
                    (FeedbackKind::Bug, status, Some("de"), Some(9500), issue),
                    "{label}"
                );
                let context = bug.context.as_ref().unwrap();
                assert_eq!(
                    (context.app.as_deref(), context.player),
                    (Some("1.1.0"), Some(2))
                );
            }
        }
        // A review of an old result drops the standings of its month, and only of its month
        conn.execute_batch(
            "INSERT INTO closed_months (month, first_day, last_day) VALUES (24320, 20697, 20726);
             UPDATE attempts SET flag = 'review' WHERE user_id = 1;",
        )
        .await
        .unwrap();
        assert_eq!(
            count(conn, "SELECT COUNT(*) FROM closed_months").await,
            1,
            "{label}"
        );
        assert_eq!(
            count(conn, "SELECT COUNT(*) FROM month_standings").await,
            0,
            "{label}"
        );
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
    let file = TempDb::new("busy");
    let database = open_local(file.as_str()).await.unwrap();
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
}

/// A request still reading (its query's statement not run to the end) while another one writes.
/// In SQLite's default rollback journal mode the open read kept a lock on the file, the write
/// couldn't commit until the read was over and failed with "database is locked" after the busy
/// timeout (the browser tests' sign ins did that: they kept their query open while the password
/// hash was checked). Local files are in WAL mode, there a read never holds up a write.
#[tokio::test]
async fn a_read_in_progress_doesnt_hold_up_a_write() {
    let file = TempDb::new("wal");
    let database = open_local(file.as_str()).await.unwrap();
    let reader = connect(&database).unwrap();
    let writer = connect(&database).unwrap();
    migrate(&reader).await.unwrap();
    let mut rows = writer.query("PRAGMA journal_mode", ()).await.unwrap();
    let mode: String = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(mode, "wal");
    drop(rows);
    writer
        .execute_batch(
            "INSERT INTO users (username, name, created_at) VALUES ('first', 'first', 1);
             INSERT INTO users (username, name, created_at) VALUES ('second', 'second', 2);",
        )
        .await
        .unwrap();

    // One row read, the statement goes on
    let mut rows = reader
        .query("SELECT id FROM users ORDER BY id", ())
        .await
        .unwrap();
    assert!(rows.next().await.unwrap().is_some());
    let started = std::time::Instant::now();
    writer
        .execute(
            "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES ('ab', 1, 1, 2)",
            (),
        )
        .await
        .unwrap_or_else(|e| panic!("a write behind a read in progress: {e}"));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "the write waited for the read: {:?}",
        started.elapsed()
    );
    // The read goes on where it was, then sees the write
    assert!(rows.next().await.unwrap().is_some());
    drop(rows);
    assert_eq!(count(&reader, "SELECT COUNT(*) FROM sessions").await, 1);
}

/// The four files SQLite may make for `file`.
fn sqlite_files(file: &std::path::Path) -> Vec<std::path::PathBuf> {
    ["", "-wal", "-shm", "-journal"]
        .iter()
        .map(|suffix| {
            let mut path = file.as_os_str().to_owned();
            path.push(suffix);
            path.into()
        })
        .collect()
}

/// A test's database file goes away with its WAL and shared-memory files (they stayed in the
/// temp directory, a tmpfs, hundreds of them a day).
#[tokio::test]
async fn temp_db_files_are_removed_when_dropped() {
    let file = TempDb::new("guard");
    let files = sqlite_files(file.path());
    assert!(file.path().starts_with(std::env::temp_dir()));
    assert!(
        file.path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(&format!("hashi-test-{}-guard-", std::process::id()))
    );
    let database = open_local(file.as_str()).await.unwrap();
    let conn = connect(&database).unwrap();
    migrate(&conn).await.unwrap();
    // A journal of rollback mode too (auth's sign-in test runs in it)
    std::fs::write(&files[3], b"journal").unwrap();
    assert!(files.iter().all(|f| f.exists()), "{files:?}");
    drop((conn, database));
    drop(file);
    assert!(files.iter().all(|f| !f.exists()), "{files:?}");

    // Names are unique: tests run in parallel
    assert_ne!(TempDb::new("guard").path(), TempDb::new("guard").path());
}

/// Also when the test fails.
#[test]
fn temp_db_files_are_removed_on_a_panic() {
    let mut files = Vec::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let file = TempDb::new("panic");
        files = sqlite_files(file.path());
        for path in &files {
            std::fs::write(path, b"x").unwrap();
        }
        panic!("a failing test");
    }));
    assert!(result.is_err());
    assert_eq!(files.len(), 4);
    assert!(files.iter().all(|f| !f.exists()), "{files:?}");
}

/// The connection of the dojo tests takes its file with it.
#[tokio::test]
async fn test_conn_files_are_removed_when_dropped() {
    let conn = TestConn::new("conn").await;
    let files = sqlite_files(conn.file.path());
    assert!(files[0].exists());
    assert!(count(&conn, "SELECT COUNT(*) FROM schema_migrations").await > 0);
    drop(conn);
    assert!(files.iter().all(|f| !f.exists()), "{files:?}");
}

/// What test processes that no longer run left behind is removed, the files of running ones
/// and everything else stay.
#[test]
fn files_of_finished_test_processes_are_swept() {
    if !std::path::Path::new("/proc/self").exists() {
        return;
    }
    let dir = test_temp_path("sweep");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Above the highest process id Linux hands out (2^22)
    let gone = 4_000_000_000_u32;
    let own = std::process::id();
    let swept = [
        format!("hashi-test-{gone}-dojo-x-0.db"),
        format!("hashi-test-{gone}-dojo-x-0.db-wal"),
        format!("hashi-test-{gone}-http"),
    ];
    let kept = [
        format!("hashi-test-{own}-dojo-x-0.db"),
        format!("hashi-other-{gone}.db"),
        "hashi-test-x-1.db".to_string(),
    ];
    for name in swept.iter().chain(&kept) {
        std::fs::write(dir.join(name), b"x").unwrap();
    }
    std::fs::remove_file(dir.join(&swept[2])).unwrap();
    std::fs::create_dir_all(dir.join(&swept[2]).join("site/pkg")).unwrap();
    std::fs::write(dir.join(&swept[2]).join("hashi.db"), b"x").unwrap();

    sweep_test_files(&dir);
    for name in &swept {
        assert!(!dir.join(name).exists(), "{name}");
    }
    for name in &kept {
        assert!(dir.join(name).exists(), "{name}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn failed_migration_rolls_back_schema_and_version_and_can_retry() {
    let db = test_db(100).await;
    migrate_to(&db.conn, 2).await.unwrap();
    write_rows(&db.conn, 2).await;
    db.conn
        .execute_batch(
            "CREATE TRIGGER interrupt_migration BEFORE INSERT ON schema_migrations
        WHEN NEW.version = 3 BEGIN SELECT RAISE(ABORT, 'injected failure'); END;",
        )
        .await
        .unwrap();
    assert!(
        migrate(&db.conn)
            .await
            .unwrap_err()
            .contains("injected failure")
    );
    assert_eq!(
        count(&db.conn, "SELECT MAX(version) FROM schema_migrations").await,
        2
    );
    assert!(
        db.conn
            .query("SELECT flag FROM attempts", ())
            .await
            .is_err()
    );
    db.conn
        .execute("DROP TRIGGER interrupt_migration", ())
        .await
        .unwrap();
    migrate(&db.conn).await.unwrap();
    assert_eq!(count(&db.conn, "SELECT COUNT(*) FROM users").await, 2);
    assert_eq!(
        count(&db.conn, "SELECT COUNT(*) FROM schema_migrations").await,
        i64::try_from(MIGRATIONS.len()).unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn starting_replicas_serialize_migrations() {
    let db = test_db(101).await;
    let first = connect(&db.database).unwrap();
    let second = connect(&db.database).unwrap();
    let (a, b) = tokio::join!(migrate(&first), migrate(&second));
    a.unwrap();
    b.unwrap();
    assert_eq!(
        count(&first, "SELECT COUNT(*) FROM schema_migrations").await,
        i64::try_from(MIGRATIONS.len()).unwrap()
    );
}
