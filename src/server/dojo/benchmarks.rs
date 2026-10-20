//! Community benchmarks ("bring your own benchmark"), like the SAT Competition's.
//!
//! - Registered players submit a formula (an upload, or a hash of the Global Benchmark Database
//!   that we fetch from <https://benchmark-database.de/file/HASH>) with a family, a description,
//!   an optional expected answer and a licence (CC BY 4.0), under the same account-age and
//!   daily limits as solvers. Duplicates are refused by GBD hash and by a hash scrambled copies
//!   share (`dojo::community`).
//! - The worker verifies it (`POST {DOJO_URL}/benchmarks`): too easy if a plain DPLL solver
//!   settles it at once, verified if splr settles it with a checked model or a proof the
//!   verified checker accepts. Its verdict comes back to `/dojo/benchmark-report` with the
//!   check's token.
//! - A moderator accepts verified ones into the pool (`/admin`).
//! - When a season starts, its hidden set draws a few from the pool (`dojo::community::select`,
//!   seeded with the season's secret seed and the pool's hashes) and the draw is fixed for the
//!   season (`dojo_season_pool`): later acceptances wait for the next season. The worker gets the
//!   draw (`PUT {DOJO_URL}/pools/<digest>`) before the season's first job.
//! - Authors are credited in the guide, on this page and in the season archive.

use std::time::Duration;

use axum::extract::Multipart;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use dojo::Answer;
use dojo::community::{
    self, BenchmarkReport, Formula, FormulaError, Origin, PoolEntry, PoolInstance, Verdict,
};
use leptos::prelude::ServerFnError;
use libsql::params::IntoParams;
use libsql::{Connection, TransactionBehavior, params};

use super::{DESCRIPTION_MAX, MAX_ATTEMPTS, MIN_ACCOUNT_AGE_MS, PER_DAY, STALE_MS, test_seed};
use crate::model::{
    AdminBenchmark, BenchmarkAction, BenchmarkState, CommunityCredit, CommunitySummary,
    DojoBenchmark, DojoBenchmarks, MS_PER_DAY,
};
use crate::server::auth::{self, User};
use crate::server::db::{self, db_err};
use crate::server::{hex, now_ms};

/// Benchmarks waiting for or in verification before new ones are refused.
const MAX_QUEUED: i64 = 40;
const FAMILY_MAX: usize = 40;
/// Largest download from GBD (xz-compressed).
const MAX_DOWNLOAD: usize = 16 << 20;

/// Why a benchmark was refused, as a code for the page (see `dojo.community.errors` in locales).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenchmarkError {
    SignIn,
    AccountAge,
    Limit,
    NetworkLimit,
    Busy,
    /// Neither a file nor a GBD hash, or both.
    Source,
    /// GBD doesn't have it, or sent something else.
    Gbd,
    Invalid,
    TooLarge,
    Duplicate,
    Family,
    Description,
    License,
    Server,
}

impl BenchmarkError {
    pub fn code(self) -> &'static str {
        match self {
            Self::SignIn => "sign_in",
            Self::AccountAge => "account_age",
            Self::Limit => "limit",
            Self::NetworkLimit => "network_limit",
            Self::Busy => "busy",
            Self::Source => "source",
            Self::Gbd => "gbd",
            Self::Invalid => "invalid",
            Self::TooLarge => "too_large",
            Self::Duplicate => "duplicate",
            Self::Family => "family",
            Self::Description => "description",
            Self::License => "license",
            Self::Server => "server",
        }
    }
}

/// The form, with the formula already read (and fetched from GBD).
#[derive(Debug, Default)]
pub struct BenchmarkForm {
    pub family: String,
    pub description: String,
    pub claimed: Option<Answer>,
    pub license: bool,
    /// DIMACS, maybe xz-compressed.
    pub formula: Vec<u8>,
    pub from_gbd: bool,
    /// The hashed network it came from.
    pub network: Option<String>,
}

/// A family name: lowercase letters, digits and dashes.
fn family_ok(family: &str) -> bool {
    !family.is_empty()
        && family.len() <= FAMILY_MAX
        && family
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !family.starts_with('-')
}

/// A GBD instance hash: 32 hex digits.
pub(super) fn gbd_hash_ok(hash: &str) -> bool {
    hash.len() == 32 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

/// xz-compressed (like GBD's files) or plain, at most [`community::MAX_BYTES`] of text.
fn decompress(bytes: &[u8]) -> Result<String, BenchmarkError> {
    const XZ: &[u8] = &[0xfd, b'7', b'z', b'X', b'Z', 0];
    let text = if bytes.starts_with(XZ) {
        /// Fails once more than the limit comes out.
        struct Capped(Vec<u8>);
        impl std::io::Write for Capped {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                if self.0.len() + buf.len() > community::MAX_BYTES {
                    return Err(std::io::Error::other("too large"));
                }
                self.0.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut out = Capped(Vec::new());
        match lzma_rs::xz_decompress(&mut std::io::Cursor::new(bytes), &mut out) {
            Ok(()) => out.0,
            Err(lzma_rs::error::Error::IoError(_)) => return Err(BenchmarkError::TooLarge),
            Err(_) => return Err(BenchmarkError::Invalid),
        }
    } else {
        if bytes.len() > community::MAX_BYTES {
            return Err(BenchmarkError::TooLarge);
        }
        bytes.to_vec()
    };
    String::from_utf8(text).map_err(|_| BenchmarkError::Invalid)
}

/// Where GBD's files are (`DOJO_GBD_URL` for tests).
fn gbd_url() -> String {
    crate::server::env_var("DOJO_GBD_URL").unwrap_or_else(|| "https://benchmark-database.de".into())
}

/// The file the GBD at `base` has under `hash` (xz-compressed).
pub(super) async fn fetch_gbd(base: &str, hash: &str) -> Result<Vec<u8>, BenchmarkError> {
    if !gbd_hash_ok(hash) {
        return Err(BenchmarkError::Gbd);
    }
    let url = format!("{}/file/{}", base.trim_end_matches('/'), hash);
    let response = reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|_| BenchmarkError::Gbd)?;
    if !response.status().is_success() {
        return Err(BenchmarkError::Gbd);
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_DOWNLOAD as u64)
    {
        return Err(BenchmarkError::TooLarge);
    }
    let bytes = response.bytes().await.map_err(|_| BenchmarkError::Gbd)?;
    if bytes.len() > MAX_DOWNLOAD {
        return Err(BenchmarkError::TooLarge);
    }
    Ok(bytes.to_vec())
}

async fn one(conn: &Connection, sql: &str, args: impl IntoParams) -> Result<i64, libsql::Error> {
    let mut rows = conn.query(sql, args).await?;
    match rows.next().await? {
        Some(row) => row.get(0),
        None => Ok(0),
    }
}

/// Stores a benchmark and queues it for verification. `gbd`: the hash it was fetched under,
/// which it has to have.
pub(super) async fn submit(
    conn: &Connection,
    user: &User,
    form: BenchmarkForm,
    gbd: Option<&str>,
) -> Result<i64, BenchmarkError> {
    let server = |e: libsql::Error| {
        eprintln!("dojo benchmark: {e}");
        BenchmarkError::Server
    };
    if user.username.is_none() {
        return Err(BenchmarkError::SignIn);
    }
    if user.created_at > now_ms() - MIN_ACCOUNT_AGE_MS {
        return Err(BenchmarkError::AccountAge);
    }
    let family = form.family.trim().to_lowercase();
    if !family_ok(&family) {
        return Err(BenchmarkError::Family);
    }
    let description = form.description.trim();
    if description.chars().count() > DESCRIPTION_MAX
        || description
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\r')
    {
        return Err(BenchmarkError::Description);
    }
    if crate::server::names::blocked(&family).is_some() {
        return Err(BenchmarkError::Family);
    }
    if !form.license {
        return Err(BenchmarkError::License);
    }
    let text = decompress(&form.formula)?;
    let formula = Formula::parse(&text).map_err(|e| match e {
        FormulaError::TooLarge => BenchmarkError::TooLarge,
        FormulaError::Invalid(_) | FormulaError::Empty => BenchmarkError::Invalid,
    })?;
    let hash = community::gbd_hash(&formula);
    if gbd.is_some_and(|wanted| !wanted.eq_ignore_ascii_case(&hash)) {
        return Err(BenchmarkError::Gbd);
    }
    let shape = community::shape_hash(&formula);
    // Quotas, duplicates and the insert in one write transaction, across replicas too
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(server)?;
    let today = one(
        &tx,
        "SELECT COUNT(*) FROM dojo_benchmarks WHERE user_id = ?1 AND created_at > ?2",
        params![user.id, now_ms() - MS_PER_DAY],
    )
    .await
    .map_err(server)?;
    if today >= i64::from(PER_DAY) {
        return Err(BenchmarkError::Limit);
    }
    if let Some(network) = &form.network {
        let from_network = one(
            &tx,
            "SELECT COUNT(*) FROM dojo_benchmarks WHERE ip_hash = ?1",
            params![network.clone()],
        )
        .await
        .map_err(server)?;
        if from_network >= i64::from(PER_DAY) {
            return Err(BenchmarkError::NetworkLimit);
        }
    }
    let queued = one(
        &tx,
        "SELECT COUNT(*) FROM dojo_benchmarks WHERE state IN ('queued', 'checking') AND ?1 = ?1",
        params![0],
    )
    .await
    .map_err(server)?;
    if queued >= MAX_QUEUED {
        return Err(BenchmarkError::Busy);
    }
    let known = one(
        &tx,
        "SELECT COUNT(*) FROM dojo_benchmarks WHERE hash = ?1 OR shape = ?2",
        params![hash.clone(), shape.clone()],
    )
    .await
    .map_err(server)?;
    if known > 0 {
        return Err(BenchmarkError::Duplicate);
    }
    let now = now_ms();
    tx.execute(
        "INSERT INTO dojo_benchmarks (user_id, family, description, claimed, from_gbd, cnf, hash, shape, vars,
            clauses, ip_hash, state, license, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'queued', ?12, ?13, ?13)",
        params![
            user.id,
            family,
            description.to_string(),
            form.claimed.map(Answer::id),
            i64::from(form.from_gbd),
            formula.dimacs().into_bytes(),
            hash,
            shape,
            formula.vars,
            crate::server::count(formula.clauses.len()),
            form.network.clone(),
            community::LICENSE,
            now
        ],
    )
    .await
    .map_err(server)?;
    let id = tx.last_insert_rowid();
    tx.commit().await.map_err(server)?;
    tokio::spawn(async {
        let _ = super::dispatch().await;
    });
    Ok(id)
}

/// `POST /dojo/benchmarks/submit`, a multipart form (works without JavaScript). Redirects back to the
/// page, with the new benchmark or an error code.
pub async fn submit_handler(headers: HeaderMap, mut multipart: Multipart) -> Response {
    let mut form = BenchmarkForm::default();
    let mut gbd = String::new();
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
        if name == "cnf" {
            match field.bytes().await {
                Ok(bytes) => form.formula = bytes.to_vec(),
                Err(_) => too_large = true,
            }
            continue;
        }
        let text = field.text().await.unwrap_or_default();
        match name.as_str() {
            "family" => form.family = text,
            "description" => form.description = text,
            "gbd" => gbd = text.trim().to_lowercase(),
            "claimed" => form.claimed = Answer::from_id(&text),
            "license" => form.license = !text.is_empty(),
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
    let result = async {
        if too_large {
            return Err(BenchmarkError::TooLarge);
        }
        let conn = db::conn().map_err(|_| BenchmarkError::Server)?;
        let user = auth::user_from_headers(&conn, &headers)
            .await
            .map_err(|_| BenchmarkError::Server)?
            .ok_or(BenchmarkError::SignIn)?;
        // Checked before the download, which takes a while
        if user.username.is_none() {
            return Err(BenchmarkError::SignIn);
        }
        let hash = match (form.formula.is_empty(), gbd.is_empty()) {
            (false, true) => None,
            (true, false) => {
                form.formula = fetch_gbd(&gbd_url(), &gbd).await?;
                form.from_gbd = true;
                Some(gbd.as_str())
            }
            _ => return Err(BenchmarkError::Source),
        };
        form.network = crate::server::client_ip(&headers)
            .map(|ip| crate::server::network_hash(&ip, crate::model::day_of(now_ms())));
        submit(&conn, &user, form, hash).await
    }
    .await;
    match result {
        Ok(id) => {
            Redirect::to(&format!("{prefix}/dojo/benchmarks?submitted={id}#mine")).into_response()
        }
        Err(e) => Redirect::to(&format!(
            "{prefix}/dojo/benchmarks?error={}#submit",
            e.code()
        ))
        .into_response(),
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "takes what a row's column gives"
)]
fn answer_of(text: Option<String>) -> Option<Answer> {
    text.as_deref().and_then(Answer::from_id)
}

fn u32_of(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The accepted benchmarks with their authors (empty: deleted account), oldest first.
pub(super) async fn pool(conn: &Connection) -> Result<Vec<CommunityCredit>, ServerFnError> {
    credits(
        conn,
        "WHERE b.state = 'accepted' AND ?1 = ?1 ORDER BY b.accepted_at, b.id",
        0,
    )
    .await
}

/// The benchmarks drawn for `season`, in set order.
pub(super) async fn drawn(
    conn: &Connection,
    season: u32,
) -> Result<Vec<CommunityCredit>, ServerFnError> {
    credits(
        conn,
        "JOIN dojo_season_pool p ON p.benchmark_id = b.id WHERE p.season = ?1 ORDER BY p.position",
        i64::from(season),
    )
    .await
}

async fn credits(
    conn: &Connection,
    filter: &str,
    arg: i64,
) -> Result<Vec<CommunityCredit>, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT b.id, b.family, COALESCE(u.name, ''), b.answer, b.vars, b.clauses, b.hash
                 FROM dojo_benchmarks b LEFT JOIN users u ON u.id = b.user_id {filter}"
            ),
            params![arg],
        )
        .await
        .map_err(db_err)?;
    let mut credits = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let Some(answer) = answer_of(row.get(3).map_err(db_err)?) else {
            continue;
        };
        credits.push(CommunityCredit {
            id: row.get(0).map_err(db_err)?,
            family: row.get(1).map_err(db_err)?,
            author: row.get(2).map_err(db_err)?,
            answer,
            vars: u32_of(row.get(4).map_err(db_err)?),
            clauses: u32_of(row.get(5).map_err(db_err)?),
            hash: row.get(6).map_err(db_err)?,
        });
    }
    Ok(credits)
}

/// The pool in short: how many, and who, by how many they contributed.
pub(super) async fn summary(conn: &Connection) -> Result<CommunitySummary, ServerFnError> {
    let pool = pool(conn).await?;
    let mut authors: Vec<(String, usize)> = Vec::new();
    for credit in pool.iter().filter(|c| !c.author.is_empty()) {
        match authors.iter_mut().find(|(a, _)| *a == credit.author) {
            Some((_, n)) => *n += 1,
            None => authors.push((credit.author.clone(), 1)),
        }
    }
    authors.sort_by_key(|a| std::cmp::Reverse(a.1));
    Ok(CommunitySummary {
        benchmarks: u32::try_from(pool.len()).unwrap_or(u32::MAX),
        authors: authors.into_iter().map(|(a, _)| a).collect(),
        per_set: u32::try_from(community::PER_SET).unwrap_or(u32::MAX),
    })
}

/// The page's data: the player's benchmarks and the pool.
pub async fn page(conn: &Connection, user: Option<&User>) -> Result<DojoBenchmarks, ServerFnError> {
    let pool = pool(conn).await?;
    let per_set = u32::try_from(community::PER_SET).unwrap_or(u32::MAX);
    let Some(user) = user else {
        return Ok(DojoBenchmarks {
            registered: false,
            left_today: 0,
            mine: Vec::new(),
            pool,
            per_set,
        });
    };
    let mut rows = conn
        .query(
            "SELECT id, family, description, state, reason, answer, vars, clauses, hash, created_at
             FROM dojo_benchmarks WHERE user_id = ?1 ORDER BY created_at DESC, id DESC LIMIT 50",
            params![user.id],
        )
        .await
        .map_err(db_err)?;
    let mut mine = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        mine.push(DojoBenchmark {
            id: row.get(0).map_err(db_err)?,
            family: row.get(1).map_err(db_err)?,
            description: row.get(2).map_err(db_err)?,
            state: BenchmarkState::from_id(&row.get::<String>(3).map_err(db_err)?),
            reason: row.get(4).map_err(db_err)?,
            answer: answer_of(row.get(5).map_err(db_err)?),
            vars: u32_of(row.get(6).map_err(db_err)?),
            clauses: u32_of(row.get(7).map_err(db_err)?),
            hash: row.get(8).map_err(db_err)?,
            created_at: row.get(9).map_err(db_err)?,
        });
    }
    let today = one(
        conn,
        "SELECT COUNT(*) FROM dojo_benchmarks WHERE user_id = ?1 AND created_at > ?2",
        params![user.id, now_ms() - MS_PER_DAY],
    )
    .await
    .map_err(db_err)?;
    Ok(DojoBenchmarks {
        registered: user.username.is_some(),
        left_today: PER_DAY.saturating_sub(u32_of(today)),
        mine,
        pool,
        per_set,
    })
}

/// Applies the worker's verdict on a benchmark being checked, whatever its token.
#[cfg(test)]
pub(super) async fn apply(
    conn: &Connection,
    report: &BenchmarkReport,
) -> Result<(), libsql::Error> {
    apply_for_check(conn, report, None).await.map(|_| ())
}

/// [`apply`] for the check with `token`, compared in the same statement that writes: a check
/// requeued or sent again between authorization and this write keeps its own verdict. Whether
/// the verdict was taken.
pub(super) async fn apply_for_check(
    conn: &Connection,
    report: &BenchmarkReport,
    token: Option<&str>,
) -> Result<bool, libsql::Error> {
    let now = now_ms();
    let changed = match &report.verdict {
        Verdict::Verified {
            answer,
            model,
            solve_ms,
        } => {
            // An author who expected the other answer gets told, it doesn't go to the pool
            conn.execute(
                "UPDATE dojo_benchmarks SET
                    state = CASE WHEN claimed IS NOT NULL AND claimed != ?2 THEN 'refused' ELSE 'verified' END,
                    reason = CASE WHEN claimed IS NOT NULL AND claimed != ?2 THEN 'wrong_claim' ELSE NULL END,
                    answer = ?2, model = ?3, solve_ms = ?4, job_token = NULL, updated_at = ?5
                 WHERE id = ?1 AND state = 'checking' AND (?6 IS NULL OR job_token = ?6)",
                params![
                    report.id,
                    answer.id(),
                    model
                        .as_ref()
                        .map(|m| serde_json::to_string(m).unwrap_or_default()),
                    i64::try_from(*solve_ms).unwrap_or(i64::MAX),
                    now,
                    token
                ],
            )
            .await?
        }
        Verdict::Refused { reason } => {
            conn.execute(
                "UPDATE dojo_benchmarks SET state = 'refused', reason = ?2, job_token = NULL, updated_at = ?3
                 WHERE id = ?1 AND state = 'checking' AND (?4 IS NULL OR job_token = ?4)",
                params![report.id, reason.id(), now, token],
            )
            .await?
        }
        Verdict::Failed { message } => {
            eprintln!("dojo benchmark #{}: worker failed: {message}", report.id);
            conn.execute(
                "UPDATE dojo_benchmarks SET state = CASE WHEN attempts >= ?2 THEN 'refused' ELSE 'queued' END,
                    reason = CASE WHEN attempts >= ?2 THEN 'failed' ELSE NULL END, job_token = NULL,
                    updated_at = ?3
                 WHERE id = ?1 AND state = 'checking' AND (?4 IS NULL OR job_token = ?4)",
                params![report.id, MAX_ATTEMPTS, now, token],
            )
            .await?
        }
    };
    Ok(changed > 0)
}

/// Whether the request carries the token of the check of benchmark `id`.
async fn check_authorized(
    conn: &Connection,
    id: i64,
    headers: &HeaderMap,
) -> Result<bool, StatusCode> {
    let expected: Option<String> = match conn
        .query(
            "SELECT job_token FROM dojo_benchmarks WHERE id = ?1",
            params![id],
        )
        .await
    {
        Ok(mut rows) => match rows.next().await {
            Ok(Some(row)) => row.get(0).ok().flatten(),
            _ => None,
        },
        Err(_) => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    Ok(match (expected, given) {
        (Some(expected), Some(given)) => {
            expected.len() == given.len()
                && expected
                    .bytes()
                    .zip(given.bytes())
                    .fold(0, |acc, (a, b)| acc | (a ^ b))
                    == 0
        }
        _ => false,
    })
}

/// `POST /dojo/benchmark-report` from the worker.
pub async fn report_handler(
    headers: HeaderMap,
    axum::Json(report): axum::Json<BenchmarkReport>,
) -> StatusCode {
    let Ok(conn) = db::conn() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    match check_authorized(&conn, report.id, &headers).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::UNAUTHORIZED,
        Err(status) => return status,
    }
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match apply_for_check(&conn, &report, given).await {
        Ok(true) => StatusCode::NO_CONTENT,
        // Not at the worker any more (requeued, sent again, decided): the worker stops retrying
        Ok(false) => StatusCode::CONFLICT,
        Err(e) => {
            eprintln!("dojo benchmark report: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// Checks that went quiet are queued again, or refused after a few attempts.
pub(super) async fn housekeeping(conn: &Connection, now: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE dojo_benchmarks SET state = CASE WHEN attempts >= ?1 THEN 'refused' ELSE 'queued' END,
            reason = CASE WHEN attempts >= ?1 THEN 'failed' ELSE NULL END, job_token = NULL,
            updated_at = ?2
         WHERE state = 'checking' AND updated_at < ?3",
        params![MAX_ATTEMPTS, now, now - STALE_MS],
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Sends the next queued benchmark to the worker, one check at a time.
pub(super) async fn dispatch_to(
    conn: &Connection,
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<(), String> {
    let err = |e: libsql::Error| e.to_string();
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    let report_token = hex(&bytes);
    // One statement claims the next one if none is at the worker, before it is sent: a verdict
    // can't come before the claim, and replicas can't both send one
    let mut rows = conn
        .query(
            "UPDATE dojo_benchmarks SET state = 'checking', job_token = ?1, attempts = attempts + 1,
                updated_at = ?2
             WHERE id = (SELECT id FROM dojo_benchmarks WHERE state = 'queued' ORDER BY created_at, id LIMIT 1)
               AND NOT EXISTS (SELECT 1 FROM dojo_benchmarks WHERE state = 'checking')
             RETURNING id, cnf",
            params![report_token.clone(), now_ms()],
        )
        .await
        .map_err(err)?;
    let Some(row) = rows.next().await.map_err(err)? else {
        return Ok(());
    };
    let id: i64 = row.get(0).map_err(err)?;
    let cnf: Vec<u8> = row.get(1).map_err(err)?;
    drop(rows);
    let sent = client
        .post(format!(
            "{}/benchmarks?id={id}&report_token={report_token}&rules={}",
            url.trim_end_matches('/'),
            dojo::RULES_VERSION
        ))
        .bearer_auth(token)
        .body(cnf)
        .timeout(Duration::from_secs(120))
        .send()
        .await;
    let failed = match sent {
        // The verdict may be in already: nothing to change
        Ok(r) if r.status() == StatusCode::ACCEPTED => return Ok(()),
        Ok(r) => format!("worker answered {} to a benchmark", r.status()),
        Err(e) => format!("worker unreachable: {e}"),
    };
    // Never at the worker: queued again without using up an attempt
    conn.execute(
        "UPDATE dojo_benchmarks SET state = 'queued', job_token = NULL, attempts = attempts - 1, updated_at = ?3
         WHERE id = ?1 AND state = 'checking' AND job_token = ?2",
        params![id, report_token, now_ms()],
    )
    .await
    .map_err(err)?;
    Err(failed)
}

/// Draws the benchmarks of a season that just started from the pool, once: every benchmark
/// accepted before now, see [`community::select`]. Records the draw and its digest.
pub(super) async fn draw(conn: &Connection, season: u32, now: i64) -> Result<(), String> {
    let err = |e: libsql::Error| e.to_string();
    let undrawn = one(
        conn,
        "SELECT COUNT(*) FROM dojo_seasons WHERE season = ?1 AND pool_digest IS NULL",
        params![season],
    )
    .await
    .map_err(err)?;
    if undrawn == 0 {
        return Ok(());
    }
    let mut rows = conn
        .query(
            "SELECT id, user_id, family, answer, hash FROM dojo_benchmarks
             WHERE state = 'accepted' AND accepted_at <= ?1 ORDER BY id",
            params![now],
        )
        .await
        .map_err(err)?;
    let mut ids = Vec::new();
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.map_err(err)? {
        let id: i64 = row.get(0).map_err(err)?;
        let Some(answer) = answer_of(row.get(3).map_err(err)?) else {
            continue;
        };
        // Deleted accounts count as authors of their own
        let author = match row.get::<Option<i64>>(1).map_err(err)? {
            Some(user) => format!("user {user}"),
            None => format!("benchmark {id}"),
        };
        ids.push(id);
        entries.push(PoolEntry {
            hash: row.get(4).map_err(err)?,
            author,
            family: row.get(2).map_err(err)?,
            answer,
        });
    }
    drop(rows);
    let picked = community::select(&test_seed(season), &entries, community::PER_SET);
    let digest = if picked.is_empty() {
        String::new()
    } else {
        community::digest_of(
            picked
                .iter()
                .map(|&i| (ids[i], entries[i].hash.as_str(), entries[i].answer)),
        )
    };
    let tx = conn.transaction().await.map_err(err)?;
    // Only the first time: a season's draw never changes
    let fresh = tx
        .execute(
            "UPDATE dojo_seasons SET pool_digest = ?2 WHERE season = ?1 AND pool_digest IS NULL",
            params![season, digest],
        )
        .await
        .map_err(err)?;
    if fresh == 1 {
        for (position, &i) in picked.iter().enumerate() {
            tx.execute(
                "INSERT OR REPLACE INTO dojo_season_pool (season, position, benchmark_id) VALUES (?1, ?2, ?3)",
                params![season, crate::server::count(position), ids[i]],
            )
            .await
            .map_err(err)?;
        }
    }
    tx.commit().await.map_err(err)
}

/// The digest of a season's draw, empty for none.
pub(super) async fn digest(conn: &Connection, season: u32) -> Result<String, String> {
    let mut rows = conn
        .query(
            "SELECT COALESCE(pool_digest, '') FROM dojo_seasons WHERE season = ?1",
            params![season],
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(match rows.next().await.map_err(|e| e.to_string())? {
        Some(row) => row.get(0).map_err(|e| e.to_string())?,
        None => String::new(),
    })
}

/// The benchmarks drawn for a season, as the worker needs them.
pub(super) async fn instances(conn: &Connection, season: u32) -> Result<Vec<PoolInstance>, String> {
    let err = |e: libsql::Error| e.to_string();
    let mut rows = conn
        .query(
            "SELECT b.id, b.hash, b.family, COALESCE(u.name, ''), b.answer, b.cnf, b.model
             FROM dojo_season_pool p JOIN dojo_benchmarks b ON b.id = p.benchmark_id
             LEFT JOIN users u ON u.id = b.user_id
             WHERE p.season = ?1 ORDER BY p.position",
            params![season],
        )
        .await
        .map_err(err)?;
    let mut pool = Vec::new();
    while let Some(row) = rows.next().await.map_err(err)? {
        let model: Option<String> = row.get(6).map_err(err)?;
        pool.push(PoolInstance {
            origin: Origin {
                benchmark: row.get(0).map_err(err)?,
                hash: row.get(1).map_err(err)?,
                family: row.get(2).map_err(err)?,
                author: row.get(3).map_err(err)?,
            },
            answer: answer_of(row.get(4).map_err(err)?)
                .ok_or("a drawn benchmark without answer")?,
            dimacs: String::from_utf8_lossy(&row.get::<Vec<u8>>(5).map_err(err)?).into_owned(),
            model: model.and_then(|m| serde_json::from_str(&m).ok()),
        });
    }
    Ok(pool)
}

/// Makes sure the worker has the draw of `season` before a job of it: asks, and sends it if not.
/// Returns its digest, empty for none.
pub(super) async fn ensure_pool(
    conn: &Connection,
    client: &reqwest::Client,
    url: &str,
    token: &str,
    season: u32,
) -> Result<String, String> {
    let digest = digest(conn, season).await?;
    if digest.is_empty() {
        return Ok(digest);
    }
    let pool_url = format!("{}/pools/{digest}", url.trim_end_matches('/'));
    let has = client
        .get(&pool_url)
        .bearer_auth(token)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| format!("worker unreachable: {e}"))?;
    if has.status() == StatusCode::OK {
        return Ok(digest);
    }
    let pool = instances(conn, season).await?;
    let sent = client
        .put(&pool_url)
        .bearer_auth(token)
        .header(header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_vec(&pool).map_err(|e| e.to_string())?)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| format!("worker unreachable: {e}"))?;
    if sent.status().is_success() {
        Ok(digest)
    } else {
        Err(format!("worker refused the pool: {}", sent.status()))
    }
}

/// Every benchmark for moderators, the ones waiting for them first.
pub async fn admin_list(conn: &Connection) -> Result<Vec<AdminBenchmark>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT b.id, b.family, b.description, COALESCE(u.name, ''), b.state, b.reason, b.claimed, b.answer,
                b.vars, b.clauses, b.solve_ms, b.from_gbd, b.hash, b.created_at
             FROM dojo_benchmarks b LEFT JOIN users u ON u.id = b.user_id
             ORDER BY b.state != 'verified', b.created_at DESC LIMIT 200",
            (),
        )
        .await
        .map_err(db_err)?;
    let mut list = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        list.push(AdminBenchmark {
            id: row.get(0).map_err(db_err)?,
            family: row.get(1).map_err(db_err)?,
            description: row.get(2).map_err(db_err)?,
            author: row.get(3).map_err(db_err)?,
            state: BenchmarkState::from_id(&row.get::<String>(4).map_err(db_err)?),
            reason: row.get(5).map_err(db_err)?,
            claimed: answer_of(row.get(6).map_err(db_err)?),
            answer: answer_of(row.get(7).map_err(db_err)?),
            vars: u32_of(row.get(8).map_err(db_err)?),
            clauses: u32_of(row.get(9).map_err(db_err)?),
            solve_ms: row
                .get::<Option<i64>>(10)
                .map_err(db_err)?
                .and_then(|ms| u64::try_from(ms).ok()),
            from_gbd: row.get::<i64>(11).map_err(db_err)? != 0,
            hash: row.get(12).map_err(db_err)?,
            created_at: row.get(13).map_err(db_err)?,
        });
    }
    Ok(list)
}

/// A moderator's decision. Accepting needs a verified benchmark; a deleted benchmark that a
/// season drew stays (its set is published), only rejected for later seasons.
pub async fn admin_action(
    conn: &Connection,
    id: i64,
    action: BenchmarkAction,
) -> Result<(), ServerFnError> {
    let now = now_ms();
    let sql = match action {
        BenchmarkAction::Accept => {
            "UPDATE dojo_benchmarks SET state = 'accepted', reason = NULL, accepted_at = ?2, updated_at = ?2
             WHERE id = ?1 AND state IN ('verified', 'rejected') AND answer IS NOT NULL"
        }
        BenchmarkAction::Reject => {
            "UPDATE dojo_benchmarks SET state = 'rejected', reason = 'moderation', accepted_at = NULL,
                job_token = NULL, updated_at = ?2
             WHERE id = ?1"
        }
        BenchmarkAction::Requeue => {
            "UPDATE dojo_benchmarks SET state = 'queued', reason = NULL, attempts = 0, accepted_at = NULL,
                job_token = NULL, updated_at = ?2 WHERE id = ?1"
        }
        BenchmarkAction::Delete => {
            "DELETE FROM dojo_benchmarks WHERE id = ?1 AND ?2 = ?2
                AND id NOT IN (SELECT benchmark_id FROM dojo_season_pool)"
        }
    };
    let changed = conn.execute(sql, params![id, now]).await.map_err(db_err)?;
    if changed == 0 && action == BenchmarkAction::Delete {
        // Drawn by a season: kept, out of the pool
        conn.execute(
            "UPDATE dojo_benchmarks SET state = 'rejected', reason = 'moderation', accepted_at = NULL, updated_at = ?2
             WHERE id = ?1",
            params![id, now],
        )
        .await
        .map_err(db_err)?;
    }
    if action == BenchmarkAction::Requeue {
        tokio::spawn(async {
            let _ = super::dispatch().await;
        });
    }
    Ok(())
}
