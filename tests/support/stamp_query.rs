//! Deterministic fixtures shared by the unit oracle tests and standalone SQL diagnostic.

use libsql::{Connection, Value, params};

// Frozen pre-optimization SQL, deliberately independent of the production query/fallback.
pub(super) const ORIGINAL: &str = include_str!("../fixtures/stamp-record-original.sql");

pub(super) async fn values(conn: &Connection, sql: &str, user: i64) -> Vec<Vec<Value>> {
    let mut rows = conn.query(sql, params![user]).await.unwrap();
    let mut result = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        result.push((0..13).map(|i| row.get_value(i).unwrap()).collect());
    }
    result
}

pub(super) async fn seed(conn: &Connection, players: i64, days: i64) {
    conn.execute(
        "WITH RECURSIVE users(id) AS (SELECT 1 UNION ALL SELECT id + 1 FROM users WHERE id < ?1)
         INSERT INTO users (id, name, created_at) SELECT id, 'player ' || id, 0 FROM users",
        params![players],
    )
    .await
    .unwrap();
    conn.execute(
        "WITH RECURSIVE days(day) AS (SELECT 0 UNION ALL SELECT day + 1 FROM days WHERE day + 1 < ?1)
         INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms)
         SELECT u.id, d.day, d.day * 86400000,
            d.day * 86400000 + ((u.id * 173 + d.day * 97) % 1000000) + 60000,
            ((u.id * 173 + d.day * 97) % 1000000) + 60000
         FROM days d CROSS JOIN users u",
        params![days],
    )
    .await
    .unwrap();
}
