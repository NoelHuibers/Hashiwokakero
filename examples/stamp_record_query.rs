//! Manual stamp-query diagnostic: pinned libSQL, real migrations, fresh temporary databases.
//! Run with `--baseline` for the nine round-one workloads, or `[players days samples]`
//! for a bounded workload in each mode. With no arguments, use `32 20 3`.
//! Append `--explain` to print both query plans for the first workload.

use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use hashi::server::db::{migrate, open_local};
use libsql::Connection;

#[path = "../tests/support/stamp_query.rs"]
mod fixture;
use fixture::{ORIGINAL, seed, values};

// The exact asset used by the server. No copied optimized SQL or benchmark-only public API.
const CANDIDATE: &str = include_str!("../src/server/daily/stamp_record.sql");
const MODES: [&str; 5] = [
    "distinct",
    "sparse",
    "equal times",
    "exact ties",
    "large logs",
];

#[derive(Debug)]
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "hashi-stamp-diagnostic-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // Never reuse an existing path; Drop only removes a directory this process created.
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

async fn customize(conn: &Connection, mode: &str) {
    let sql = match mode {
        "sparse" => "DELETE FROM attempts WHERE user_id = 1 AND day NOT IN (0, 180, 364)",
        "equal times" => "UPDATE attempts SET time_ms = 60000 + (user_id / 10) * 1000",
        "exact ties" => {
            "UPDATE attempts SET time_ms = 60000, finished_at = day * 86400000 + 60000 WHERE user_id IN (1, 2)"
        }
        "large logs" => {
            "UPDATE attempts SET log = zeroblob(50000), bridges = zeroblob(2000) WHERE user_id = 1"
        }
        "distinct" => return,
        _ => unreachable!("fixed workload names"),
    };
    conn.execute(sql, ()).await.unwrap();
}

fn workloads(args: &[String]) -> (Vec<(i64, i64, &'static str)>, usize) {
    if args == ["--baseline"] {
        return (
            vec![
                (1, 1, "distinct"),
                (10, 30, "distinct"),
                (100, 180, "distinct"),
                (1000, 180, "distinct"),
                (1000, 365, "distinct"),
                (1000, 365, "sparse"),
                (1000, 180, "equal times"),
                (1000, 180, "exact ties"),
                (1000, 180, "large logs"),
            ],
            5,
        );
    }
    let (players, days, samples) = match args {
        [] => (32, 20, 3),
        [players, days, samples] => (
            players.parse::<i64>().expect("players must be an integer"),
            days.parse::<i64>().expect("days must be an integer"),
            samples
                .parse::<usize>()
                .expect("samples must be an integer"),
        ),
        _ => panic!("usage: stamp_record_query [--baseline | players days samples] [--explain]"),
    };
    assert!(
        players > 0 && days > 0 && samples > 0,
        "arguments must be positive"
    );
    (MODES.map(|mode| (players, days, mode)).into(), samples)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let explain = args.last().is_some_and(|arg| arg == "--explain");
    if explain {
        args.pop();
    }
    let (workloads, samples) = workloads(&args);
    let mut first = true;
    for (players, days, mode) in workloads {
        // Reverse declaration order closes all rows/connections before removing the directory,
        // including WAL/SHM files. No application DB path or credentials are consulted.
        let dir = TempDir::new();
        let path = dir.0.join("attempts.db");
        let database = open_local(path.to_str().expect("UTF-8 temporary path"))
            .await
            .unwrap();
        let conn = database.connect().unwrap();
        migrate(&conn).await.unwrap();
        if first {
            let mut rows = conn.query("SELECT sqlite_version()", ()).await.unwrap();
            println!(
                "libSQL SQLite {}",
                rows.next()
                    .await
                    .unwrap()
                    .unwrap()
                    .get::<String>(0)
                    .unwrap()
            );
        }
        seed(&conn, players, days).await;
        customize(&conn, mode).await;
        if explain && first {
            for (name, sql) in [("original", ORIGINAL), ("optimized", CANDIDATE)] {
                println!("{name} query plan:");
                let mut plan = conn
                    .query(&format!("EXPLAIN QUERY PLAN {sql}"), [1])
                    .await
                    .unwrap();
                while let Some(row) = plan.next().await.unwrap() {
                    println!("{}", row.get::<String>(3).unwrap());
                }
            }
        }
        first = false;
        let expected = values(&conn, ORIGINAL, 1).await;
        assert_eq!(values(&conn, CANDIDATE, 1).await, expected);
        let mut old_ms = Vec::with_capacity(samples);
        let mut new_ms = Vec::with_capacity(samples);
        // Warm both with the equality check, then alternate. No timing thresholds.
        for _ in 0..samples {
            for (sql, timings) in [(ORIGINAL, &mut old_ms), (CANDIDATE, &mut new_ms)] {
                let start = Instant::now();
                let actual = values(&conn, sql, 1).await;
                timings.push(start.elapsed().as_secs_f64() * 1000.0);
                assert_eq!(actual, expected);
            }
        }
        old_ms.sort_by(f64::total_cmp);
        new_ms.sort_by(f64::total_cmp);
        println!(
            "players={players} days={days} mode={mode:?} rows={} old_ms={old_ms:.3?} new_ms={new_ms:.3?} speedup={:.2}x",
            expected.len(),
            old_ms[samples / 2] / new_ms[samples / 2]
        );
    }
}
