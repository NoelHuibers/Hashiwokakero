//! The database: Turso (libSQL) in production, a local SQLite file in development and tests.
//!
//! - `TURSO_DATABASE_URL` + `TURSO_AUTH_TOKEN`: remote Turso database
//! - otherwise `HASHI_DB_PATH` (default `hashi.db`): local file

use std::sync::OnceLock;
use std::time::Duration;

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
-- SAT Dojo submissions, see server/dojo/
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
    "
-- When the player last changed their username, which they can do once a month, see
-- server/auth.rs
ALTER TABLE users ADD COLUMN renamed_at INTEGER;
",
    "
-- Dojo seasons, see server/dojo/seasons.rs. A result's rules version (dojo::RULES_VERSION, NULL:
-- from before versions) and its run of every formula (dojo::InstanceResult as JSON), private
-- until the season is archived
ALTER TABLE dojo_submissions ADD COLUMN rules_version INTEGER;
ALTER TABLE dojo_submissions ADD COLUMN results TEXT;

-- Every season: the commitment to its hidden set's seed from the start, the seed itself once
-- it is over
CREATE TABLE IF NOT EXISTS dojo_seasons (
    season            INTEGER PRIMARY KEY,
    started_at        INTEGER NOT NULL,
    -- NULL while it runs
    ended_at          INTEGER,
    -- SHA-256 (hex) of the seed
    commitment        TEXT NOT NULL,
    -- revealed when the season is over
    seed              TEXT,
    -- dojo::GENERATOR_VERSION and dojo::RULES_VERSION the last deploy of the season ran
    generator_version INTEGER NOT NULL,
    rules_version     INTEGER NOT NULL
);

-- The hidden set a season was evaluated on, as the worker sent it (a .tar.gz, see
-- dojo::archive). Only published once the season is over
CREATE TABLE IF NOT EXISTS dojo_season_sets (
    season            INTEGER PRIMARY KEY,
    generator_version INTEGER NOT NULL,
    archive           BLOB NOT NULL,
    sha256            TEXT NOT NULL,
    created_at        INTEGER NOT NULL
);

-- The final results of a season, frozen when the next one starts. user_id, name and source_url
-- go when the author deletes their account, the numbers stay
CREATE TABLE IF NOT EXISTS dojo_archive (
    season        INTEGER NOT NULL,
    submission_id INTEGER NOT NULL,
    -- NULL for house entries and deleted accounts
    user_id       INTEGER,
    house         INTEGER NOT NULL,
    name          TEXT NOT NULL,
    source_url    TEXT,
    created_at    INTEGER NOT NULL,
    par2          REAL,
    solved        INTEGER,
    wrong         INTEGER NOT NULL DEFAULT 0,
    score         TEXT,
    fingerprint   TEXT,
    results       TEXT,
    rules_version INTEGER,
    -- the place on the final leaderboard, NULL: not ranked
    rank          INTEGER,
    PRIMARY KEY (season, submission_id)
);
CREATE INDEX IF NOT EXISTS dojo_archive_user ON dojo_archive (user_id);
",
    // 10
    r"
-- Community benchmarks (bring your own benchmark, see server/dojo/benchmarks.rs). The formula
-- as normalized DIMACS, its GBD hash and a hash its scrambled copies share (dojo::community).
-- user_id goes when the author deletes their account; benchmarks that are in a season's draw
-- stay (licensed CC BY 4.0, published with the season), all others go
CREATE TABLE IF NOT EXISTS dojo_benchmarks (
    id          INTEGER PRIMARY KEY,
    user_id     INTEGER,
    family      TEXT NOT NULL,
    description TEXT NOT NULL,
    -- 'SAT', 'UNSAT' or NULL: what the author expects
    claimed     TEXT,
    -- fetched from the Global Benchmark Database (1) or uploaded (0)
    from_gbd    INTEGER NOT NULL DEFAULT 0,
    cnf         BLOB NOT NULL,
    hash        TEXT NOT NULL,
    shape       TEXT NOT NULL,
    vars        INTEGER NOT NULL,
    clauses     INTEGER NOT NULL,
    ip_hash     TEXT,
    -- queued, checking (at the worker), verified (waits for a moderator), refused (by the
    -- worker), accepted (in the pool), rejected (by a moderator)
    state       TEXT NOT NULL,
    -- why it was refused (dojo::community::Refusal::id, wrong_claim, failed)
    reason      TEXT,
    -- the certain answer, a model of satisfiable ones (JSON) and how long splr took
    answer      TEXT,
    model       TEXT,
    solve_ms    INTEGER,
    job_token   TEXT,
    attempts    INTEGER NOT NULL DEFAULT 0,
    license     TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    -- when a moderator accepted it: it can be drawn in seasons that start later
    accepted_at INTEGER
);
CREATE UNIQUE INDEX IF NOT EXISTS dojo_benchmark_hash ON dojo_benchmarks (hash);
CREATE INDEX IF NOT EXISTS dojo_benchmark_shape ON dojo_benchmarks (shape);
CREATE INDEX IF NOT EXISTS dojo_benchmark_user ON dojo_benchmarks (user_id, created_at);
CREATE INDEX IF NOT EXISTS dojo_benchmark_network ON dojo_benchmarks (ip_hash, created_at);
CREATE INDEX IF NOT EXISTS dojo_benchmark_state ON dojo_benchmarks (state, created_at);

-- The benchmarks drawn from the pool for a season's hidden set, fixed when the season starts
-- (dojo::community::select), in set order
CREATE TABLE IF NOT EXISTS dojo_season_pool (
    season       INTEGER NOT NULL,
    position     INTEGER NOT NULL,
    benchmark_id INTEGER NOT NULL,
    PRIMARY KEY (season, position)
);
-- dojo::community::pool_digest of the season's draw, '' for none, NULL: the season started
-- before there was a pool (it has none)
ALTER TABLE dojo_seasons ADD COLUMN pool_digest TEXT;
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
    migrate(&connect(&db).map_err(|e| e.to_string())?).await?;
    DB.set(db)
        .map_err(|_| "database initialized twice".to_string())
}

/// A local SQLite file, switched to WAL mode (it stays in it, the mode is kept in the file).
///
/// In SQLite's default rollback journal mode a read in progress blocks every write from
/// committing, and a write about to commit blocks every new read: requests waited for each
/// other, and on a busy server long enough to fail with "database is locked". In WAL mode reads
/// and writes don't wait for each other, only writes for another write, which never takes long
/// (no transaction waits for anything but the database). The writes go to `<path>-wal` first,
/// with `<path>-shm` as its index: a copy of the database needs all three files, or the server
/// stopped first (the last connection writes everything back and removes them).
///
/// Turso isn't a local file, it has a storage of its own and no journal mode to set.
pub async fn open_local(path: &str) -> Result<Database, String> {
    let err = |e: libsql::Error| format!("could not open {path}: {e}");
    let db = Builder::new_local(path).build().await.map_err(err)?;
    let conn = db.connect().map_err(err)?;
    // Switching waits for other processes on the file, like the browser tests' reads
    conn.busy_timeout(BUSY_TIMEOUT).map_err(err)?;
    let mut rows = conn
        .query("PRAGMA journal_mode = WAL", ())
        .await
        .map_err(err)?;
    let mode = match rows.next().await.map_err(err)? {
        Some(row) => row.get::<String>(0).map_err(err)?,
        None => String::new(),
    };
    if !mode.eq_ignore_ascii_case("wal") {
        // In memory, or a file system without shared memory
        eprintln!("database: {path} stays in journal mode {mode}, WAL isn't available there");
    }
    Ok(db)
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

/// Removes a local database file of the tests and its WAL and journal files: a fresh file next to
/// an old `-wal` would get the old one's writes.
#[cfg(test)]
pub fn remove_local(path: &std::path::Path) {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut file = path.as_os_str().to_owned();
        file.push(suffix);
        let _ = std::fs::remove_file(file);
    }
}

/// A database file of the tests in the temp directory, removed with its `-wal`, `-shm` and
/// `-journal` files when this is dropped, also when the test panics.
///
/// The temp directory is often a tmpfs (RAM): the tests used to leave every file behind,
/// hundreds a day.
///
/// Drop it after the connections to the file (declare it before them, or as a struct's last
/// field): SQLite may still write to the files while closing.
#[cfg(test)]
#[derive(Debug)]
pub struct TempDb(std::path::PathBuf);

#[cfg(test)]
impl TempDb {
    /// A fresh file, `hashi-test-<pid>-<name>-<n>.db`: unique per process and call, tests run in
    /// parallel.
    pub fn new(name: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = test_temp_path(&format!("{name}-{n}.db"));
        // Left over from a process with the same id
        remove_local(&path);
        Self(path)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.0
    }

    /// The path as [`open_local`] takes it.
    pub fn as_str(&self) -> &str {
        self.0.to_str().expect("a temp path in UTF-8")
    }
}

#[cfg(test)]
impl Drop for TempDb {
    fn drop(&mut self) {
        remove_local(&self.0);
    }
}

/// A connection to a fresh, migrated database file of the tests, which is removed when this is
/// dropped. Derefs to the [`Connection`], so tests use it like one.
#[cfg(test)]
#[derive(Debug)]
pub struct TestConn {
    conn: Connection,
    /// After the connection: fields are dropped in order.
    file: TempDb,
}

#[cfg(test)]
impl TestConn {
    /// A connection to a fresh [`TempDb`] named after `name`, with every migration applied.
    pub async fn new(name: &str) -> Self {
        let file = TempDb::new(name);
        let database = open_local(file.as_str()).await.unwrap();
        let conn = database.connect().unwrap();
        migrate(&conn).await.unwrap();
        Self { conn, file }
    }
}

#[cfg(test)]
impl std::ops::Deref for TestConn {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        &self.conn
    }
}

/// What the names of the tests' temp files start with, followed by the process id.
#[cfg(test)]
const TEST_PREFIX: &str = "hashi-test-";

/// `name` in the temp directory, prefixed with `hashi-test-<pid>-`. The first call in a process
/// removes what test processes that no longer run left there (see [`sweep_test_files`]).
#[cfg(test)]
pub fn test_temp_path(name: &str) -> std::path::PathBuf {
    static SWEPT: std::sync::Once = std::sync::Once::new();
    let dir = std::env::temp_dir();
    SWEPT.call_once(|| sweep_test_files(&dir));
    dir.join(format!("{TEST_PREFIX}{}-{name}", std::process::id()))
}

/// Removes the files and directories in `dir` of test processes that no longer run: what a
/// killed run (Ctrl-C, a timeout) left behind, and the HTTP tests' site, which lives as long as
/// its process (see `router::tests::setup`). Only where `/proc` tells which processes run; the
/// files of running ones (other test binaries, other checkouts) stay.
#[cfg(test)]
fn sweep_test_files(dir: &std::path::Path) {
    let proc = std::path::Path::new("/proc");
    if !proc.join("self").exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.strip_prefix(TEST_PREFIX))
            .and_then(|rest| rest.split('-').next())
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        if proc.join(pid.to_string()).exists() {
            continue;
        }
        let _ = if entry.file_type().is_ok_and(|t| t.is_dir()) {
            std::fs::remove_dir_all(entry.path())
        } else {
            std::fs::remove_file(entry.path())
        };
    }
}

/// A connection for the current request.
pub fn conn() -> Result<Connection, ServerFnError> {
    connect(
        DB.get()
            .ok_or_else(|| crate::model::ApiError::Internal.fail())?,
    )
}

/// How long a write waits for another one to finish.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// A new connection to `db`, which waits for other writes to finish.
///
/// A local file is locked by every write: without a busy timeout a second request writing at the
/// same moment failed at once with "database is locked" (Turso queues writes itself, there the
/// timeout does nothing). Reads don't wait for writes, the file is in WAL mode ([`open_local`]).
///
/// SQLite doesn't wait where waiting could deadlock: a transaction that read first and then
/// writes fails at once if another write is in progress. A transaction that writes has to begin
/// with its write, or with `TransactionBehavior::Immediate`.
pub fn connect(db: &Database) -> Result<Connection, ServerFnError> {
    let conn = db.connect().map_err(db_err)?;
    conn.busy_timeout(BUSY_TIMEOUT).map_err(db_err)?;
    Ok(conn)
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
