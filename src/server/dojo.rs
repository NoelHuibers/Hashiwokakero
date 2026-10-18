//! The SAT Dojo on the website: submissions, the leaderboard and talking to the worker.
//!
//! A submission is stored with its WebAssembly module and queued. A background task sends
//! queued submissions to the worker (`DOJO_URL`, which starts on demand), the worker reports
//! progress and the score to `/dojo/report`. Runs that go quiet are queued again.
//!
//! Settings: `DOJO_URL` (worker, unset: nothing is evaluated), `DOJO_TOKEN` (authenticates us
//! to the worker; its reports carry a per-job token instead), `HASHI_ADMIN_TOKEN` (house entries),
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
use super::{count, hex, now_ms};
use crate::model::{DojoBoard, DojoEntry, DojoMe, DojoRow, DojoState, DojoSubmission, MS_PER_DAY};

/// Submissions per account and day (house entries don't count).
pub const PER_DAY: u32 = 3;
/// Accounts have to be this old to submit.
const MIN_ACCOUNT_AGE_MS: i64 = MS_PER_DAY;
/// Submissions waiting or running before new ones are refused.
const MAX_PENDING: i64 = 40;
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
    std::env::var("DOJO_SEASON")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
}

/// The secret seed of a season's hidden set.
pub fn test_seed(season: u32) -> String {
    let secret = std::env::var("DOJO_SECRET").unwrap_or_else(|_| "hashi-dojo-local".into());
    let digest = Sha256::digest(format!("{secret}:season:{season}").as_bytes());
    hex(&digest)
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
               AND s.similar_to IS NULL
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
        let Some(score) = score_of(row.get(4).map_err(db_err)?) else {
            continue;
        };
        entries.push(DojoEntry {
            rank: u32::try_from(entries.len() + 1).unwrap_or(u32::MAX),
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
        .query(
            "SELECT COUNT(*) FROM dojo_submissions WHERE state IN ('queued', 'running')",
            (),
        )
        .await
        .map_err(db_err)?;
    let queued: u32 = match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err)?,
        None => 0,
    };
    Ok(DojoBoard {
        season,
        formulas: dojo::SET_SIZE,
        budget: DEFAULT_BUDGET,
        entries,
        queued,
    })
}

pub async fn submission(
    conn: &Connection,
    id: i64,
    me: Option<i64>,
) -> Result<Option<DojoSubmission>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT s.id, s.name, s.description, s.source_url, COALESCE(u.username, u.name, 'Hashi'), s.house,
                    s.created_at, s.state, s.season, s.progress, s.total, s.score, s.error, s.size, s.sha256, s.user_id,
                    s.similar_to
             FROM dojo_submissions s LEFT JOIN users u ON u.id = s.user_id WHERE s.id = ?1",
            params![id],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    let season: u32 = row.get(8).map_err(db_err)?;
    let rank = ranked(conn, season)
        .await?
        .into_iter()
        .find(|e| e.id == id)
        .map(|e| e.rank);
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
        progress: row.get(9).map_err(db_err)?,
        total: row.get(10).map_err(db_err)?,
        score: score_of(row.get(11).map_err(db_err)?),
        rejection: error.and_then(|e| serde_json::from_str(&e).ok()),
        rank,
        size: row.get(13).map_err(db_err)?,
        sha256: row.get(14).map_err(db_err)?,
        similar_to: row.get(16).map_err(db_err)?,
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
        Some(row) => row.get::<u32>(0).map_err(db_err)?,
        None => 0,
    })
}

pub async fn me(conn: &Connection, user: Option<&User>) -> Result<DojoMe, ServerFnError> {
    let Some(user) = user else {
        return Ok(DojoMe {
            registered: false,
            left_today: 0,
            submissions: Vec::new(),
        });
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
    /// The account was created less than a day ago.
    AccountAge,
    /// Enough submissions from this network today, whatever the account.
    NetworkLimit,
    /// The queue is full, try again later.
    Busy,
    Server,
}

impl SubmitError {
    pub fn code(self) -> &'static str {
        match self {
            Self::SignIn => "sign_in",
            Self::Name => "name",
            Self::Description => "description",
            Self::Url => "url",
            Self::Size => "size",
            Self::NotWasm => "not_wasm",
            Self::Duplicate => "duplicate",
            Self::Limit => "limit",
            Self::AccountAge => "account_age",
            Self::NetworkLimit => "network_limit",
            Self::Busy => "busy",
            Self::Server => "server",
        }
    }
}

#[derive(Debug, Default)]
pub struct Form {
    pub name: String,
    pub description: String,
    pub source_url: String,
    pub wasm: Vec<u8>,
    /// The hashed network the upload came from (`None` for house entries and local runs).
    pub network: Option<String>,
}

impl Form {
    fn check(&self) -> Result<(), SubmitError> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > NAME_MAX || name.chars().any(char::is_control)
        {
            return Err(SubmitError::Name);
        }
        if self.description.chars().count() > DESCRIPTION_MAX {
            return Err(SubmitError::Description);
        }
        let url = self.source_url.trim();
        if !url.is_empty()
            && (!url.starts_with("https://")
                || url.len() > URL_MAX
                || url.contains(char::is_whitespace))
        {
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

/// SHA-256 of the module without its custom sections (names, debug info, anything appended
/// there): the same code renamed or padded hashes the same. `None` if it isn't WebAssembly.
pub fn code_hash(wasm: &[u8]) -> Option<String> {
    let mut hasher = Sha256::new();
    for payload in wasmparser::Parser::new(0).parse_all(wasm) {
        let payload = payload.ok()?;
        if let Some((id, range)) = payload.as_section()
            && id != 0
        {
            hasher.update([id]);
            hasher.update(wasm.get(range)?);
        }
    }
    Some(hex(&hasher.finalize()))
}

/// Stores a submission and queues it. `user` is `None` for house entries.
pub async fn submit(
    conn: &Connection,
    user: Option<&User>,
    form: Form,
) -> Result<i64, SubmitError> {
    form.check()?;
    let server = |e: libsql::Error| {
        eprintln!("dojo submit: {e}");
        SubmitError::Server
    };
    if let Some(user) = user {
        if user.username.is_none() {
            return Err(SubmitError::SignIn);
        }
        // Throwaway accounts made to get around the limits have to wait a day
        if user.created_at > now_ms() - MIN_ACCOUNT_AGE_MS {
            return Err(SubmitError::AccountAge);
        }
        let today = submitted_today(conn, user.id)
            .await
            .map_err(|_| SubmitError::Server)?;
        if today >= PER_DAY {
            return Err(SubmitError::Limit);
        }
        if let Some(network) = &form.network {
            let mut rows = conn
                .query(
                    "SELECT COUNT(*) FROM dojo_submissions WHERE ip_hash = ?1",
                    params![network.clone()],
                )
                .await
                .map_err(server)?;
            let from_network: i64 = match rows.next().await.map_err(server)? {
                Some(row) => row.get(0).map_err(server)?,
                None => 0,
            };
            if from_network >= i64::from(PER_DAY) {
                return Err(SubmitError::NetworkLimit);
            }
        }
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM dojo_submissions WHERE state IN ('queued', 'running')",
                (),
            )
            .await
            .map_err(server)?;
        let pending: i64 = match rows.next().await.map_err(server)? {
            Some(row) => row.get(0).map_err(server)?,
            None => 0,
        };
        if pending >= MAX_PENDING {
            return Err(SubmitError::Busy);
        }
    }
    let sha256 = hex(&Sha256::digest(&form.wasm));
    // Renaming, padding or adding custom sections doesn't make a different solver
    let code = code_hash(&form.wasm).ok_or(SubmitError::NotWasm)?;
    let mut rows = conn
        .query(
            "SELECT 1 FROM dojo_submissions WHERE sha256 = ?1 OR code_hash = ?2",
            params![sha256.clone(), code.clone()],
        )
        .await
        .map_err(server)?;
    if rows.next().await.map_err(server)?.is_some() {
        return Err(SubmitError::Duplicate);
    }
    let now = now_ms();
    let url = form.source_url.trim();
    conn.execute(
        "INSERT INTO dojo_submissions
            (user_id, house, name, description, source_url, wasm, sha256, size, created_at, updated_at, state, season,
             code_hash, ip_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, 'queued', ?10, ?11, ?12)",
        params![
            user.map(|u| u.id),
            i64::from(user.is_none()),
            form.name.trim().to_string(),
            form.description.trim().to_string(),
            (!url.is_empty()).then(|| url.to_string()),
            form.wasm.clone(),
            sha256,
            count(form.wasm.len()),
            now,
            season(),
            code,
            form.network.clone()
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
        if name.as_str() == "wasm" {
            match field.bytes().await {
                Ok(bytes) => form.wasm = bytes.to_vec(),
                Err(_) => too_large = true,
            }
        } else {
            let text = field.text().await.unwrap_or_default();
            match name.as_str() {
                "name" => form.name = text,
                "description" => form.description = text,
                "source_url" => form.source_url = text,
                // The language prefix to redirect to, e.g. "/de"
                "prefix"
                    if text.len() <= 8
                        && text
                            .chars()
                            .all(|c| c == '/' || c == '-' || c.is_ascii_alphabetic()) =>
                {
                    prefix = text;
                }
                _ => {}
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
        form.network = super::client_ip(&headers)
            .map(|ip| super::network_hash(&ip, crate::model::day_of(now_ms())));
        submit(&conn, Some(&user), form).await
    }
    .await;
    match result {
        Ok(id) => Redirect::to(&format!("{prefix}/dojo/{id}")).into_response(),
        Err(e) => Redirect::to(&format!("{prefix}/dojo?error={}#submit", e.code())).into_response(),
    }
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

#[derive(Debug, Deserialize)]
pub struct HouseQuery {
    name: String,
    #[serde(default)]
    description: String,
}

/// `POST /dojo/house?name=` with `HASHI_ADMIN_TOKEN`: adds one of our own solvers to the
/// leaderboard.
pub async fn house_handler(
    headers: HeaderMap,
    Query(q): Query<HouseQuery>,
    wasm: Bytes,
) -> Response {
    // The admin token, not the worker's: nothing the worker holds can add entries
    if !super::bearer_ok(&headers, "HASHI_ADMIN_TOKEN") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let form = Form {
        name: q.name,
        description: q.description,
        source_url: format!("{}/tree/main/dojo/house", crate::seo::REPO_URL),
        wasm: wasm.to_vec(),
        network: None,
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
        Report::Progress {
            id,
            season,
            done,
            total,
        } => {
            conn.execute(
                "UPDATE dojo_submissions SET state = 'running', progress = ?1, total = ?2, updated_at = ?3
                 WHERE id = ?4 AND season = ?5 AND state IN ('queued', 'running')",
                params![*done, *total, now, *id, *season],
            )
            .await?;
        }
        Report::Finished {
            id,
            season,
            score,
            fingerprint,
        } => {
            let json = serde_json::to_string(score).unwrap_or_default();
            let fingerprint = serde_json::to_string(fingerprint).unwrap_or_default();
            conn.execute(
                "UPDATE dojo_submissions SET state = 'done', progress = ?1, total = ?1, score = ?2, par2 = ?3,
                    solved = ?4, wrong = ?5, error = NULL, updated_at = ?6, fingerprint = ?9, similar_to = NULL
                 WHERE id = ?7 AND season = ?8",
                params![
                    score.total,
                    json,
                    score.par2,
                    score.solved,
                    score.wrong,
                    now,
                    *id,
                    *season,
                    fingerprint
                ],
            )
            .await?;
            mark_copies(conn, *id).await?;
        }
        Report::Rejected { id, season, reason } => {
            let json = serde_json::to_string(reason).unwrap_or_default();
            conn.execute(
                "UPDATE dojo_submissions SET state = 'rejected', error = ?1, updated_at = ?2 WHERE id = ?3 AND season = ?4",
                params![json, now, *id, *season],
            )
            .await?;
        }
        Report::Failed {
            id,
            season,
            message,
        } => {
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

/// Compares a finished submission with everyone else's of the same season: of two that behave
/// the same on every formula (see [`dojo::similar`]), only the earlier one is ranked.
async fn mark_copies(conn: &Connection, id: i64) -> Result<(), libsql::Error> {
    let mut rows = conn
        .query(
            "SELECT s.id, s.user_id, s.created_at, s.fingerprint, s.similar_to
             FROM dojo_submissions s JOIN dojo_submissions me ON me.id = ?1
             WHERE s.season = me.season AND s.state = 'done' AND s.fingerprint IS NOT NULL",
            params![id],
        )
        .await?;
    let mut all = Vec::new();
    while let Some(row) = rows.next().await? {
        let fingerprint: String = row.get(3)?;
        all.push((
            row.get::<i64>(0)?,
            row.get::<Option<i64>>(1)?,
            row.get::<i64>(2)?,
            serde_json::from_str::<Vec<u64>>(&fingerprint).unwrap_or_default(),
            row.get::<Option<i64>>(4)?,
        ));
    }
    let Some(me) = all.iter().find(|s| s.0 == id).cloned() else {
        return Ok(());
    };
    for other in all
        .iter()
        .filter(|o| o.0 != id && (o.1 != me.1 || o.1.is_none()))
    {
        if !dojo::similar(&me.3, &other.3) {
            continue;
        }
        // The later one is the copy
        let (copy, original) = if (me.2, me.0) > (other.2, other.0) {
            (me.0, other.0)
        } else {
            (other.0, me.0)
        };
        conn.execute(
            "UPDATE dojo_submissions SET similar_to = ?2 WHERE id = ?1 AND similar_to IS NULL",
            params![copy, original],
        )
        .await?;
    }
    Ok(())
}

/// `POST /dojo/report` from the worker.
pub async fn report_handler(
    headers: HeaderMap,
    axum::Json(report): axum::Json<Report>,
) -> StatusCode {
    let Ok(conn) = db::conn() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    // Only with the token of this very job: the worker holds no key to other submissions
    let expected: Option<String> = match conn
        .query(
            "SELECT job_token FROM dojo_submissions WHERE id = ?1",
            params![report.id()],
        )
        .await
    {
        Ok(mut rows) => match rows.next().await {
            Ok(Some(row)) => row.get(0).ok().flatten(),
            _ => None,
        },
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE,
    };
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let valid = match (expected, given) {
        (Some(expected), Some(given)) => {
            expected.len() == given.len()
                && expected
                    .bytes()
                    .zip(given.bytes())
                    .fold(0, |acc, (a, b)| acc | (a ^ b))
                    == 0
        }
        _ => false,
    };
    if !valid {
        return StatusCode::UNAUTHORIZED;
    }
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
        "UPDATE dojo_submissions SET state = 'queued', season = ?1, progress = 0, updated_at = ?2, attempts = 0,
            fingerprint = NULL, similar_to = NULL
         WHERE season != ?1 AND state IN ('done', 'queued', 'running')",
        params![season(), now],
    )
    .await?;
    // Code hashes of submissions from before they existed
    let mut rows = conn
        .query(
            "SELECT id, wasm FROM dojo_submissions WHERE code_hash IS NULL LIMIT 20",
            (),
        )
        .await?;
    let mut missing = Vec::new();
    while let Some(row) = rows.next().await? {
        missing.push((row.get::<i64>(0)?, row.get::<Vec<u8>>(1)?));
    }
    for (id, wasm) in missing {
        conn.execute(
            "UPDATE dojo_submissions SET code_hash = ?2 WHERE id = ?1",
            params![id, code_hash(&wasm).unwrap_or_default()],
        )
        .await?;
    }
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
    static BUSY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let (Some(url), Some(token)) = (
        std::env::var("DOJO_URL").ok().filter(|u| !u.is_empty()),
        token(),
    ) else {
        return Ok(());
    };
    let Ok(_guard) = BUSY.try_lock() else {
        return Ok(());
    };
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
        let Some(row) = rows.next().await.map_err(|e| e.to_string())? else {
            return Ok(());
        };
        let running: i64 = row.get(0).map_err(|e| e.to_string())?;
        let Some(id) = row.get::<Option<i64>>(1).map_err(|e| e.to_string())? else {
            return Ok(());
        };
        if running >= MAX_RUNNING {
            return Ok(());
        }
        let mut rows = conn
            .query(
                "SELECT wasm, season FROM dojo_submissions WHERE id = ?1",
                params![id],
            )
            .await
            .map_err(|e| e.to_string())?;
        let Some(row) = rows.next().await.map_err(|e| e.to_string())? else {
            return Ok(());
        };
        let wasm: Vec<u8> = row.get(0).map_err(|e| e.to_string())?;
        let season = row.get::<u32>(1).map_err(|e| e.to_string())?;
        let seed = test_seed(season);
        // Reports about this job have to carry this token, it's good for nothing else
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
        let report_token = hex(&bytes);
        conn.execute(
            "UPDATE dojo_submissions SET job_token = ?2 WHERE id = ?1",
            params![id, report_token.clone()],
        )
        .await
        .map_err(|e| e.to_string())?;
        // The worker machine may have to start first
        let sent = client
            .post(format!(
                "{}/jobs?id={id}&season={season}&seed={seed}&report_token={report_token}",
                url.trim_end_matches('/')
            ))
            .bearer_auth(&token)
            .body(wasm)
            .timeout(Duration::from_secs(120))
            .send()
            .await;
        let now = now_ms();
        match sent {
            Ok(r) if r.status() == StatusCode::ACCEPTED => {
                conn.execute(
                    "UPDATE dojo_submissions SET state = 'running', attempts = attempts + 1, updated_at = ?1 WHERE id = ?2",
                    params![now, id],
                )
                .await
                .map_err(|e| e.to_string())?;
            }
            Ok(r) if r.status() == StatusCode::UNPROCESSABLE_ENTITY => {
                let body = r.bytes().await.unwrap_or_default();
                let reason =
                    serde_json::from_slice(&body).unwrap_or(Rejection::Invalid(String::new()));
                apply(&conn, &Report::Rejected { id, season, reason })
                    .await
                    .map_err(|e| e.to_string())?;
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
        let path =
            std::env::temp_dir().join(format!("hashi-dojo-{name}-{}.db", std::process::id()));
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
}
