//! The SAT Dojo on the website: submissions, the leaderboard and talking to the worker.
//!
//! A submission is stored with its WebAssembly module and queued. A background task sends
//! queued submissions to the worker (`DOJO_URL`, which starts on demand), the worker reports
//! progress and the score to `/dojo/report`. Runs that go quiet are queued again.
//!
//! Settings: `DOJO_URL` (worker, unset: nothing is evaluated), `DOJO_TOKEN` (shared secret),
//! `DOJO_SECRET` (seed of the hidden sets), `DOJO_SEASON` (number of the current season; a
//! new season re-evaluates every submission on a new hidden set).

use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Multipart, Query};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use dojo::{DEFAULT_BUDGET, MAX_WASM_BYTES, Rejection, Report, Score};
use leptos::prelude::ServerFnError;
use libsql::{Connection, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::auth::{self, User};
use super::db::{self, db_err};
use super::now_ms;
use crate::model::{DojoBoard, DojoEntry, DojoMe, DojoRow, DojoState, DojoSubmission, MS_PER_DAY};

/// Submissions per account and day (house entries don't count).
pub const PER_DAY: u32 = 3;
const NAME_MAX: usize = 40;
const DESCRIPTION_MAX: usize = 600;
const URL_MAX: usize = 200;
/// A running evaluation that hasn't reported for this long is queued again.
const STALE_MS: i64 = 15 * 60 * 1000;
const MAX_ATTEMPTS: i64 = 3;
/// Evaluations sent to the worker at the same time.
const MAX_RUNNING: i64 = 2;
const BOARD_SIZE: usize = 100;

pub fn season() -> u32 {
    std::env::var("DOJO_SEASON").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
}

/// The secret seed of a season's hidden set.
pub fn test_seed(season: u32) -> String {
    let secret = std::env::var("DOJO_SECRET").unwrap_or_else(|_| "hashi-dojo-local".into());
    let digest = Sha256::digest(format!("{secret}:season:{season}").as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn token() -> Option<String> {
    std::env::var("DOJO_TOKEN").ok().filter(|t| !t.is_empty())
}

/* ------------------------------------------------------------------ */
/*  Reading                                                            */
/* ------------------------------------------------------------------ */

fn score_of(json: Option<String>) -> Option<Score> {
    json.and_then(|s| serde_json::from_str(&s).ok())
}

/// The best submission of every author this season, ranked.
async fn ranked(conn: &Connection, season: u32) -> Result<Vec<DojoEntry>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT s.id, s.name, COALESCE(u.username, u.name, 'Hashi'), s.house, s.score, s.created_at, s.user_id
             FROM dojo_submissions s LEFT JOIN users u ON u.id = s.user_id
             WHERE s.state = 'done' AND s.season = ?1 AND s.wrong = 0 AND s.solved > 0
             ORDER BY s.par2 ASC, s.solved DESC, s.created_at ASC",
            params![season],
        )
        .await
        .map_err(db_err)?;
    let mut seen = std::collections::HashSet::new();
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let house = row.get::<i64>(3).map_err(db_err)? != 0;
        let id: i64 = row.get(0).map_err(db_err)?;
        let user: Option<i64> = row.get(6).map_err(db_err)?;
        // House entries all stay, everyone else with their best
        if !house && !seen.insert(user) {
            continue;
        }
        let Some(score) = score_of(row.get(4).map_err(db_err)?) else { continue };
        entries.push(DojoEntry {
            rank: entries.len() as u32 + 1,
            id,
            name: row.get(1).map_err(db_err)?,
            author: row.get(2).map_err(db_err)?,
            house,
            score,
            created_at: row.get(5).map_err(db_err)?,
        });
        if entries.len() == BOARD_SIZE {
            break;
        }
    }
    Ok(entries)
}

pub async fn board(conn: &Connection) -> Result<DojoBoard, ServerFnError> {
    let season = season();
    let entries = ranked(conn, season).await?;
    let mut rows = conn
        .query("SELECT COUNT(*) FROM dojo_submissions WHERE state IN ('queued', 'running')", ())
        .await
        .map_err(db_err)?;
    let queued: i64 = match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err)?,
        None => 0,
    };
    Ok(DojoBoard {
        season,
        formulas: dojo::SET_SIZE,
        budget: DEFAULT_BUDGET,
        entries,
        queued: queued as u32,
    })
}

pub async fn submission(conn: &Connection, id: i64, me: Option<i64>) -> Result<Option<DojoSubmission>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT s.id, s.name, s.description, s.source_url, COALESCE(u.username, u.name, 'Hashi'), s.house,
                    s.created_at, s.state, s.season, s.progress, s.total, s.score, s.error, s.size, s.sha256, s.user_id
             FROM dojo_submissions s LEFT JOIN users u ON u.id = s.user_id WHERE s.id = ?1",
            params![id],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    let season: u32 = row.get::<i64>(8).map_err(db_err)? as u32;
    let rank = ranked(conn, season).await?.into_iter().find(|e| e.id == id).map(|e| e.rank);
    let user: Option<i64> = row.get(15).map_err(db_err)?;
    let error: Option<String> = row.get(12).map_err(db_err)?;
    Ok(Some(DojoSubmission {
        id,
        name: row.get(1).map_err(db_err)?,
        description: row.get(2).map_err(db_err)?,
        source_url: row.get(3).map_err(db_err)?,
        author: row.get(4).map_err(db_err)?,
        house: row.get::<i64>(5).map_err(db_err)? != 0,
        created_at: row.get(6).map_err(db_err)?,
        state: DojoState::from_id(&row.get::<String>(7).map_err(db_err)?),
        season,
        progress: row.get::<i64>(9).map_err(db_err)? as u32,
        total: row.get::<i64>(10).map_err(db_err)? as u32,
        score: score_of(row.get(11).map_err(db_err)?),
        rejection: error.and_then(|e| serde_json::from_str(&e).ok()),
        rank,
        size: row.get(13).map_err(db_err)?,
        sha256: row.get(14).map_err(db_err)?,
        mine: user.is_some() && user == me,
    }))
}

async fn submitted_today(conn: &Connection, user: i64) -> Result<u32, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM dojo_submissions WHERE user_id = ?1 AND created_at > ?2",
            params![user, now_ms() - MS_PER_DAY],
        )
        .await
        .map_err(db_err)?;
    Ok(match rows.next().await.map_err(db_err)? {
        Some(row) => row.get::<i64>(0).map_err(db_err)? as u32,
        None => 0,
    })
}

pub async fn me(conn: &Connection, user: Option<&User>) -> Result<DojoMe, ServerFnError> {
    let Some(user) = user else {
        return Ok(DojoMe { registered: false, left_today: 0, submissions: Vec::new() });
    };
    let mut rows = conn
        .query(
            "SELECT id, name, state, created_at, score FROM dojo_submissions
             WHERE user_id = ?1 ORDER BY created_at DESC LIMIT 20",
            params![user.id],
        )
        .await
        .map_err(db_err)?;
    let mut submissions = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        submissions.push(DojoRow {
            id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            state: DojoState::from_id(&row.get::<String>(2).map_err(db_err)?),
            created_at: row.get(3).map_err(db_err)?,
            score: score_of(row.get(4).map_err(db_err)?),
        });
    }
    Ok(DojoMe {
        registered: user.username.is_some(),
        left_today: PER_DAY.saturating_sub(submitted_today(conn, user.id).await?),
        submissions,
    })
}

/* ------------------------------------------------------------------ */
/*  Submitting                                                         */
/* ------------------------------------------------------------------ */

/// Why a submission was refused, as a code for the page (see `dojo.submit.errors` in locales).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitError {
    SignIn,
    Name,
    Description,
    Url,
    Size,
    NotWasm,
    Duplicate,
    Limit,
    Server,
}

impl SubmitError {
    pub fn code(self) -> &'static str {
        match self {
            SubmitError::SignIn => "sign_in",
            SubmitError::Name => "name",
            SubmitError::Description => "description",
            SubmitError::Url => "url",
            SubmitError::Size => "size",
            SubmitError::NotWasm => "not_wasm",
            SubmitError::Duplicate => "duplicate",
            SubmitError::Limit => "limit",
            SubmitError::Server => "server",
        }
    }
}

#[derive(Debug, Default)]
pub struct Form {
    pub name: String,
    pub description: String,
    pub source_url: String,
    pub wasm: Vec<u8>,
}

impl Form {
    fn check(&self) -> Result<(), SubmitError> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > NAME_MAX || name.chars().any(char::is_control) {
            return Err(SubmitError::Name);
        }
        if self.description.chars().count() > DESCRIPTION_MAX {
            return Err(SubmitError::Description);
        }
        let url = self.source_url.trim();
        if !url.is_empty() && (!url.starts_with("https://") || url.len() > URL_MAX || url.contains(char::is_whitespace)) {
            return Err(SubmitError::Url);
        }
        if self.wasm.len() > MAX_WASM_BYTES {
            return Err(SubmitError::Size);
        }
        // The worker checks the rest
        if !self.wasm.starts_with(b"\0asm\x01\0\0\0") {
            return Err(SubmitError::NotWasm);
        }
        Ok(())
    }
}

/// Stores a submission and queues it. `user` is `None` for house entries.
pub async fn submit(conn: &Connection, user: Option<&User>, form: Form) -> Result<i64, SubmitError> {
    form.check()?;
    let server = |e: libsql::Error| {
        eprintln!("dojo submit: {e}");
        SubmitError::Server
    };
    if let Some(user) = user {
        if user.username.is_none() {
            return Err(SubmitError::SignIn);
        }
        let today = submitted_today(conn, user.id).await.map_err(|_| SubmitError::Server)?;
        if today >= PER_DAY {
            return Err(SubmitError::Limit);
        }
    }
    let sha256: String = Sha256::digest(&form.wasm).iter().map(|b| format!("{b:02x}")).collect();
    let mut rows = conn
        .query("SELECT 1 FROM dojo_submissions WHERE sha256 = ?1", params![sha256.clone()])
        .await
        .map_err(server)?;
    if rows.next().await.map_err(server)?.is_some() {
        return Err(SubmitError::Duplicate);
    }
    let now = now_ms();
    let url = form.source_url.trim();
    conn.execute(
        "INSERT INTO dojo_submissions
            (user_id, house, name, description, source_url, wasm, sha256, size, created_at, updated_at, state, season)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, 'queued', ?10)",
        params![
            user.map(|u| u.id),
            user.is_none() as i64,
            form.name.trim().to_string(),
            form.description.trim().to_string(),
            (!url.is_empty()).then(|| url.to_string()),
            form.wasm.clone(),
            sha256,
            form.wasm.len() as i64,
            now,
            season()
        ],
    )
    .await
    .map_err(server)?;
    let id = conn.last_insert_rowid();
    // Don't wait for the next round of the dispatcher
    tokio::spawn(async {
        let _ = dispatch().await;
    });
    Ok(id)
}

/// `POST /dojo/submit`, a multipart form (works without JavaScript). Redirects to the
/// submission, or back to the form with an error code.
pub async fn submit_handler(headers: HeaderMap, mut multipart: Multipart) -> Response {
    let mut form = Form::default();
    let mut prefix = String::new();
    let mut too_large = false;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                too_large = true;
                break;
            }
        };
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "wasm" => match field.bytes().await {
                Ok(bytes) => form.wasm = bytes.to_vec(),
                Err(_) => too_large = true,
            },
            _ => {
                let text = field.text().await.unwrap_or_default();
                match name.as_str() {
                    "name" => form.name = text,
                    "description" => form.description = text,
                    "source_url" => form.source_url = text,
                    // The language prefix to redirect to, e.g. "/de"
                    "prefix" if text.len() <= 8 && text.chars().all(|c| c == '/' || c == '-' || c.is_ascii_alphabetic()) => {
                        prefix = text
                    }
                    _ => {}
                }
            }
        }
    }
    let result = async {
        if too_large {
            return Err(SubmitError::Size);
        }
        let conn = db::conn().map_err(|_| SubmitError::Server)?;
        let user = auth::user_from_headers(&conn, &headers)
            .await
            .map_err(|_| SubmitError::Server)?
            .ok_or(SubmitError::SignIn)?;
        submit(&conn, Some(&user), form).await
    }
    .await;
    match result {
        Ok(id) => Redirect::to(&format!("{prefix}/dojo/{id}")).into_response(),
        Err(e) => Redirect::to(&format!("{prefix}/dojo?error={}#submit", e.code())).into_response(),
    }
}

fn authorized(headers: &HeaderMap) -> bool {
    let Some(token) = token() else { return false };
    let expected = format!("Bearer {token}");
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.len() == expected.len() && v.bytes().zip(expected.bytes()).fold(0, |acc, (a, b)| acc | (a ^ b)) == 0
        })
}

/// `GET /dojo/board.json`: the leaderboard for scripts and papers.
pub async fn board_json() -> Response {
    let board = match db::conn() {
        Ok(conn) => board(&conn).await,
        Err(e) => Err(e),
    };
    match board {
        Ok(board) => (
            [(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
            axum::Json(board),
        )
            .into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

#[derive(Deserialize)]
pub struct HouseQuery {
    name: String,
    #[serde(default)]
    description: String,
}

/// `POST /dojo/house?name=` with the token: adds one of our own solvers to the leaderboard.
pub async fn house_handler(headers: HeaderMap, Query(q): Query<HouseQuery>, wasm: Bytes) -> Response {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let form = Form {
        name: q.name,
        description: q.description,
        source_url: format!("{}/tree/main/dojo/house", crate::seo::REPO_URL),
        wasm: wasm.to_vec(),
    };
    let Ok(conn) = db::conn() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    match submit(&conn, None, form).await {
        Ok(id) => (StatusCode::CREATED, id.to_string()).into_response(),
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, e.code()).into_response(),
    }
}

/* ------------------------------------------------------------------ */
/*  The worker                                                         */
/* ------------------------------------------------------------------ */

/// Applies a report from the worker. Reports for another season are stale and ignored.
pub async fn apply(conn: &Connection, report: &Report) -> Result<(), libsql::Error> {
    let now = now_ms();
    match report {
        Report::Progress { id, season, done, total } => {
            conn.execute(
                "UPDATE dojo_submissions SET state = 'running', progress = ?1, total = ?2, updated_at = ?3
                 WHERE id = ?4 AND season = ?5 AND state IN ('queued', 'running')",
                params![*done, *total, now, *id, *season],
            )
            .await?;
        }
        Report::Finished { id, season, score } => {
            let json = serde_json::to_string(score).unwrap_or_default();
            conn.execute(
                "UPDATE dojo_submissions SET state = 'done', progress = ?1, total = ?1, score = ?2, par2 = ?3,
                    solved = ?4, wrong = ?5, error = NULL, updated_at = ?6
                 WHERE id = ?7 AND season = ?8",
                params![score.total, json, score.par2, score.solved, score.wrong, now, *id, *season],
            )
            .await?;
        }
        Report::Rejected { id, season, reason } => {
            let json = serde_json::to_string(reason).unwrap_or_default();
            conn.execute(
                "UPDATE dojo_submissions SET state = 'rejected', error = ?1, updated_at = ?2 WHERE id = ?3 AND season = ?4",
                params![json, now, *id, *season],
            )
            .await?;
        }
        Report::Failed { id, season, message } => {
            eprintln!("dojo #{id}: worker failed: {message}");
            conn.execute(
                "UPDATE dojo_submissions SET state = 'queued', updated_at = ?1 WHERE id = ?2 AND season = ?3",
                params![now, *id, *season],
            )
            .await?;
        }
    }
    Ok(())
}

/// `POST /dojo/report` from the worker.
pub async fn report_handler(headers: HeaderMap, axum::Json(report): axum::Json<Report>) -> StatusCode {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED;
    }
    let Ok(conn) = db::conn() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    match apply(&conn, &report).await {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(e) => {
            eprintln!("dojo report: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// Housekeeping before sending work: a new season re-evaluates everything, evaluations that
/// went quiet are queued again (or given up after a few attempts).
pub async fn housekeeping(conn: &Connection) -> Result<(), libsql::Error> {
    let now = now_ms();
    conn.execute(
        "UPDATE dojo_submissions SET state = 'queued', season = ?1, progress = 0, updated_at = ?2, attempts = 0
         WHERE season != ?1 AND state IN ('done', 'queued', 'running')",
        params![season(), now],
    )
    .await?;
    conn.execute(
        "UPDATE dojo_submissions SET state = CASE WHEN attempts >= ?1 THEN 'failed' ELSE 'queued' END, updated_at = ?2
         WHERE state = 'running' AND updated_at < ?3",
        params![MAX_ATTEMPTS, now, now - STALE_MS],
    )
    .await?;
    Ok(())
}

/// Sends queued submissions to the worker.
pub async fn dispatch() -> Result<(), String> {
    let (Some(url), Some(token)) = (std::env::var("DOJO_URL").ok().filter(|u| !u.is_empty()), token()) else {
        return Ok(());
    };
    static BUSY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let Ok(_guard) = BUSY.try_lock() else { return Ok(()) };
    let conn = db::conn().map_err(|e| e.to_string())?;
    housekeeping(&conn).await.map_err(|e| e.to_string())?;
    let client = reqwest::Client::new();
    loop {
        let mut rows = conn
            .query(
                "SELECT
                    (SELECT COUNT(*) FROM dojo_submissions WHERE state = 'running'),
                    (SELECT id FROM dojo_submissions WHERE state = 'queued' ORDER BY house DESC, created_at LIMIT 1)",
                (),
            )
            .await
            .map_err(|e| e.to_string())?;
        let Some(row) = rows.next().await.map_err(|e| e.to_string())? else { return Ok(()) };
        let running: i64 = row.get(0).map_err(|e| e.to_string())?;
        let Some(id) = row.get::<Option<i64>>(1).map_err(|e| e.to_string())? else { return Ok(()) };
        if running >= MAX_RUNNING {
            return Ok(());
        }
        let mut rows = conn
            .query("SELECT wasm, season FROM dojo_submissions WHERE id = ?1", params![id])
            .await
            .map_err(|e| e.to_string())?;
        let Some(row) = rows.next().await.map_err(|e| e.to_string())? else { return Ok(()) };
        let wasm: Vec<u8> = row.get(0).map_err(|e| e.to_string())?;
        let season = row.get::<i64>(1).map_err(|e| e.to_string())? as u32;
        let seed = test_seed(season);
        // The worker machine may have to start first
        let sent = client
            .post(format!("{}/jobs?id={id}&season={season}&seed={seed}", url.trim_end_matches('/')))
            .bearer_auth(&token)
            .body(wasm)
            .timeout(Duration::from_secs(120))
            .send()
            .await;
        let now = now_ms();
        match sent {
            Ok(r) if r.status() == reqwest::StatusCode::ACCEPTED => {
                conn.execute(
                    "UPDATE dojo_submissions SET state = 'running', attempts = attempts + 1, updated_at = ?1 WHERE id = ?2",
                    params![now, id],
                )
                .await
                .map_err(|e| e.to_string())?;
            }
            Ok(r) if r.status() == reqwest::StatusCode::UNPROCESSABLE_ENTITY => {
                let body = r.bytes().await.unwrap_or_default();
                let reason = serde_json::from_slice(&body).unwrap_or(Rejection::Invalid(String::new()));
                apply(&conn, &Report::Rejected { id, season, reason }).await.map_err(|e| e.to_string())?;
            }
            Ok(r) => return Err(format!("worker answered {}", r.status())),
            Err(e) => return Err(format!("worker unreachable: {e}")),
        }
    }
}

/// Runs [`dispatch`] every half minute while the server is up.
pub fn start_dispatcher() {
    tokio::spawn(async {
        loop {
            if let Err(e) = dispatch().await {
                eprintln!("dojo dispatch: {e}");
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use dojo::{Answer, Family, Outcome, RunResult};

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
        User { id: conn.last_insert_rowid(), username: registered.then(|| name.into()), name: name.into(), created_at: 0 }
    }

    fn form(name: &str, extra: u8) -> Form {
        Form { name: name.into(), wasm: vec![0, b'a', b's', b'm', 1, 0, 0, 0, extra], ..Default::default() }
    }

    fn score(par2: f64, wrong: u32) -> Score {
        let results = [RunResult {
            name: "x".into(),
            family: Family::Hashi,
            expected: Answer::Sat,
            outcome: if wrong > 0 { Outcome::Wrong } else { Outcome::Solved(Answer::Sat) },
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

        assert_eq!(submit(&conn, Some(&guest), form("x", 0)).await, Err(SubmitError::SignIn));
        assert_eq!(submit(&conn, Some(&alice), form("", 0)).await, Err(SubmitError::Name));
        let bad_url = Form { source_url: "http://x".into(), ..form("x", 0) };
        assert_eq!(submit(&conn, Some(&alice), bad_url).await, Err(SubmitError::Url));
        let not_wasm = Form { wasm: b"hello".to_vec(), ..form("x", 0) };
        assert_eq!(submit(&conn, Some(&alice), not_wasm).await, Err(SubmitError::NotWasm));

        let a1 = submit(&conn, Some(&alice), form("first", 1)).await.unwrap();
        assert_eq!(submit(&conn, Some(&alice), form("again", 1)).await, Err(SubmitError::Duplicate));
        let a2 = submit(&conn, Some(&alice), form("second", 2)).await.unwrap();
        let a3 = submit(&conn, Some(&alice), form("third", 3)).await.unwrap();
        assert_eq!(submit(&conn, Some(&alice), form("fourth", 4)).await, Err(SubmitError::Limit));
        let house = submit(&conn, None, form("Hashi CDCL", 5)).await.unwrap();

        apply(&conn, &Report::Progress { id: a1, season, done: 3, total: 34 }).await.unwrap();
        let s = submission(&conn, a1, Some(alice.id)).await.unwrap().unwrap();
        assert_eq!((s.state, s.progress, s.total, s.mine), (DojoState::Running, 3, 34, true));

        apply(&conn, &Report::Finished { id: a1, season, score: score(2.0, 0) }).await.unwrap();
        apply(&conn, &Report::Finished { id: a2, season, score: score(1.0, 0) }).await.unwrap();
        apply(&conn, &Report::Finished { id: a3, season, score: score(0.5, 1) }).await.unwrap();
        apply(&conn, &Report::Finished { id: house, season, score: score(1.5, 0) }).await.unwrap();
        // Stale seasons are ignored
        apply(&conn, &Report::Finished { id: a1, season: season + 1, score: score(0.1, 0) }).await.unwrap();

        let board = board(&conn).await.unwrap();
        let names: Vec<_> = board.entries.iter().map(|e| (e.rank, e.name.as_str())).collect();
        // Alice's best (the wrong one is disqualified), then the house entry
        assert_eq!(names, [(1, "second"), (2, "Hashi CDCL")]);
        assert_eq!(submission(&conn, a2, None).await.unwrap().unwrap().rank, Some(1));
        assert_eq!(submission(&conn, a1, None).await.unwrap().unwrap().rank, None);

        let me = me(&conn, Some(&alice)).await.unwrap();
        assert_eq!((me.registered, me.left_today, me.submissions.len()), (true, 0, 3));

        apply(&conn, &Report::Rejected { id: house, season, reason: Rejection::NoStart }).await.unwrap();
        let s = submission(&conn, house, None).await.unwrap().unwrap();
        assert_eq!((s.state, s.rejection), (DojoState::Rejected, Some(Rejection::NoStart)));
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
        assert_eq!(submission(&conn, id, None).await.unwrap().unwrap().state, DojoState::Queued);
        conn.execute(
            "UPDATE dojo_submissions SET state = 'running', attempts = 3, updated_at = 0 WHERE id = ?1",
            params![id],
        )
        .await
        .unwrap();
        housekeeping(&conn).await.unwrap();
        assert_eq!(submission(&conn, id, None).await.unwrap().unwrap().state, DojoState::Failed);
    }
}
