//! The database: Turso (libSQL) in production, a local SQLite file in development and tests.
//!
//! - `TURSO_DATABASE_URL` + `TURSO_AUTH_TOKEN`: remote Turso database
//! - otherwise `HASHI_DB_PATH` (default `hashi.db`): local file

use std::sync::OnceLock;

use leptos::prelude::ServerFnError;
use libsql::{Builder, Connection, Database, params};

use super::{count, now_ms};

#[cfg(test)]
mod tests;

static DB: OnceLock<Database> = OnceLock::new();

/// Schema versions, applied in order and recorded in `schema_migrations`. A new one also needs
/// rows for its tables or columns in the migration test (`db/tests.rs`).
const MIGRATIONS: &[&str] = &[
    r"
CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY,
    -- NULL for guests
    username      TEXT UNIQUE COLLATE NOCASE,
    name          TEXT NOT NULL,
    password_hash TEXT,
    created_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    -- SHA-256 of the cookie value, the token itself is never stored
    token_hash TEXT PRIMARY KEY,
    user_id    INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS sessions_user ON sessions (user_id);

-- One puzzle per UTC day, stored so it never changes once played
CREATE TABLE IF NOT EXISTS dailies (
    day        INTEGER PRIMARY KEY,
    difficulty TEXT NOT NULL,
    puzzle     TEXT NOT NULL,
    solution   TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS attempts (
    user_id     INTEGER NOT NULL,
    day         INTEGER NOT NULL,
    started_at  INTEGER NOT NULL,
    finished_at INTEGER,
    hints       INTEGER NOT NULL DEFAULT 0,
    checks      INTEGER NOT NULL DEFAULT 0,
    revealed    INTEGER NOT NULL DEFAULT 0,
    -- finished_at - started_at + penalties
    time_ms     INTEGER,
    moves       INTEGER,
    bridges     TEXT,
    PRIMARY KEY (user_id, day)
);
CREATE INDEX IF NOT EXISTS attempts_ranking ON attempts (day, time_ms);
",
    r"
-- SAT Dojo submissions, see server/dojo.rs
CREATE TABLE IF NOT EXISTS dojo_submissions (
    id          INTEGER PRIMARY KEY,
    -- NULL for house entries (our own solvers)
    user_id     INTEGER,
    house       INTEGER NOT NULL DEFAULT 0,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    source_url  TEXT,
    wasm        BLOB NOT NULL,
    sha256      TEXT NOT NULL UNIQUE,
    size        INTEGER NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    -- queued | running | done | rejected | failed
    state       TEXT NOT NULL,
    -- the season of the hidden set the result belongs to
    season      INTEGER NOT NULL,
    attempts    INTEGER NOT NULL DEFAULT 0,
    progress    INTEGER NOT NULL DEFAULT 0,
    total       INTEGER NOT NULL DEFAULT 0,
    -- dojo::Score as JSON, and its numbers for sorting
    score       TEXT,
    par2        REAL,
    solved      INTEGER,
    wrong       INTEGER NOT NULL DEFAULT 0,
    -- dojo::Rejection as JSON
    error       TEXT
);
CREATE INDEX IF NOT EXISTS dojo_board ON dojo_submissions (season, state, par2);
CREATE INDEX IF NOT EXISTS dojo_user ON dojo_submissions (user_id, created_at);
CREATE INDEX IF NOT EXISTS dojo_queue ON dojo_submissions (state, created_at);
",
    "
-- Fair play: why a daily result isn't ranked, its move log, the (hashed) device that started it
ALTER TABLE attempts ADD COLUMN flag TEXT;
ALTER TABLE attempts ADD COLUMN log TEXT;
ALTER TABLE attempts ADD COLUMN device TEXT;
CREATE INDEX IF NOT EXISTS attempts_device ON attempts (day, device);
-- Dojo: the code without custom sections, how it did per formula, the earlier submission it
-- copies, and the (hashed, per day) network it came from
ALTER TABLE dojo_submissions ADD COLUMN code_hash TEXT;
ALTER TABLE dojo_submissions ADD COLUMN fingerprint TEXT;
ALTER TABLE dojo_submissions ADD COLUMN similar_to INTEGER;
ALTER TABLE dojo_submissions ADD COLUMN ip_hash TEXT;
-- the token reports about the running evaluation have to carry
ALTER TABLE dojo_submissions ADD COLUMN job_token TEXT;
CREATE INDEX IF NOT EXISTS dojo_code ON dojo_submissions (code_hash);
CREATE INDEX IF NOT EXISTS dojo_network ON dojo_submissions (ip_hash, created_at);
",
    "
-- The stamp book: when a player earned each stamp, see server/daily/stamps.rs
CREATE TABLE IF NOT EXISTS stamps (
    user_id   INTEGER NOT NULL,
    stamp     TEXT NOT NULL,
    earned_at INTEGER NOT NULL,
    -- the player was told (a toast on the daily page)
    announced INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, stamp)
);
",
    "
-- More stamps: what the move log of a finished daily says (model::Run as JSON, worked out once),
-- and whether it was finished at night or over lunch in the player's own time (the time zone
-- itself isn't kept)
ALTER TABLE attempts ADD COLUMN run TEXT;
ALTER TABLE attempts ADD COLUMN daypart TEXT;
",
    "
-- The stamp a player chose to show next to their name on the leaderboards (NULL: none)
ALTER TABLE users ADD COLUMN stamp TEXT;
",
    "
-- Deleted accounts (by the player, or guests nobody can sign in to any more): everything
-- personal goes, the ranked times stay without a name, so old rankings and everyone else's medals
-- don't change. See server/auth.rs
ALTER TABLE users ADD COLUMN deleted_at INTEGER;

-- Messages from the contact form, also mailed to us, see server/contact.rs
CREATE TABLE IF NOT EXISTS contact_messages (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    email      TEXT NOT NULL,
    message    TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    -- the (hashed, per day) network it came from, to limit messages per network
    ip_hash    TEXT,
    -- when the mail went out, NULL: not sent (no SMTP configured, or it failed)
    mailed_at  INTEGER
);
CREATE INDEX IF NOT EXISTS contact_network ON contact_messages (ip_hash, created_at);
",
];

pub async fn init() -> Result<(), String> {
    let url = std::env::var("TURSO_DATABASE_URL").unwrap_or_default();
    let db = if url.is_empty() {
        let path = std::env::var("HASHI_DB_PATH").unwrap_or_else(|_| "hashi.db".into());
        println!("database: local file {path} (set TURSO_DATABASE_URL to use Turso)");
        open_local(&path).await?
    } else {
        let token = std::env::var("TURSO_AUTH_TOKEN").unwrap_or_default();
        println!("database: Turso {url}");
        Builder::new_remote(url, token)
            .build()
            .await
            .map_err(|e| format!("could not connect to Turso: {e}"))?
    };
    install(db).await
}

/// Migrates `db` and makes it the database of every request. The HTTP tests bring their own.
pub async fn install(db: Database) -> Result<(), String> {
    migrate(&db.connect().map_err(|e| e.to_string())?).await?;
    DB.set(db)
        .map_err(|_| "database initialized twice".to_string())
}

pub async fn open_local(path: &str) -> Result<Database, String> {
    Builder::new_local(path)
        .build()
        .await
        .map_err(|e| format!("could not open {path}: {e}"))
}

pub async fn migrate(conn: &Connection) -> Result<(), String> {
    migrate_to(conn, MIGRATIONS.len()).await
}

/// Applies the first `version` migrations (those not applied yet), like an older server did.
async fn migrate_to(conn: &Connection, version: usize) -> Result<(), String> {
    let err = |e: libsql::Error| format!("migration failed: {e}");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL)",
        (),
    )
    .await
    .map_err(err)?;
    let mut rows = conn
        .query(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            (),
        )
        .await
        .map_err(err)?;
    let current: i64 = match rows.next().await.map_err(err)? {
        Some(row) => row.get(0).map_err(err)?,
        None => 0,
    };
    // A version that doesn't fit (never written by this code) skips everything, like before
    let applied = usize::try_from(current).unwrap_or(usize::MAX);
    for (i, sql) in MIGRATIONS.iter().enumerate().take(version).skip(applied) {
        conn.execute_batch(sql).await.map_err(err)?;
        conn.execute(
            "INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![count(i + 1), now_ms()],
        )
        .await
        .map_err(err)?;
    }
    Ok(())
}

/// A connection for the current request.
pub fn conn() -> Result<Connection, ServerFnError> {
    DB.get()
        .ok_or_else(|| crate::model::ApiError::Internal.fail())?
        .connect()
        .map_err(db_err)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "made for `.map_err(db_err)`, which hands over the error by value"
)]
pub fn db_err(e: libsql::Error) -> ServerFnError {
    // Details go to the log, not to the player
    eprintln!("database error: {e}");
    crate::model::ApiError::Database.fail()
}
