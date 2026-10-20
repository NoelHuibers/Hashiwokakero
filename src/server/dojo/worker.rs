//! Talking to the worker: sending queued submissions, applying its reports, housekeeping.

use std::time::Duration;

use axum::http::{HeaderMap, StatusCode, header};
use dojo::{RULES_VERSION, Rejection, Report};
use libsql::{Connection, params};

use super::{MAX_ATTEMPTS, MAX_RUNNING, STALE_MS, code_hash, season, seasons, test_seed, token};
use crate::server::db;
use crate::server::{hex, now_ms};

/// Applies a report from the worker. Reports for another season are stale and ignored.
pub async fn apply(conn: &Connection, report: &Report) -> Result<(), libsql::Error> {
    apply_for_job(conn, report, None).await.map(|_| ())
}

/// Token comparison is repeated inside the UPDATE: a requeue between authorization and this
/// statement must not let a previous evaluation change the replacement job.
pub(super) async fn apply_for_job(
    conn: &Connection,
    report: &Report,
    token: Option<&str>,
) -> Result<bool, libsql::Error> {
    let now = now_ms();
    let changed = match report {
        Report::Progress {
            id,
            season,
            done,
            total,
        } => {
            conn.execute(
                "UPDATE dojo_submissions SET state = 'running', progress = MAX(progress, ?1), total = ?2, updated_at = ?3
                 WHERE id = ?4 AND season = ?5 AND state IN ('queued', 'running')
                   AND (?6 IS NULL OR job_token = ?6)",
                params![*done, *total, now, *id, *season, token],
            )
            .await?
        }
        Report::Finished {
            id,
            season,
            score,
            fingerprint,
            results,
            rules_version,
        } => {
            let json = serde_json::to_string(score).unwrap_or_default();
            let fingerprint = serde_json::to_string(fingerprint).unwrap_or_default();
            let results = serde_json::to_string(results).unwrap_or_default();
            let changed = conn
                .execute(
                    "UPDATE dojo_submissions SET state = 'done', job_token = NULL, progress = ?1, total = ?1,
                    score = ?2, par2 = ?3, solved = ?4, wrong = ?5, error = NULL, updated_at = ?6,
                    fingerprint = ?9, similar_to = NULL, results = ?10, rules_version = ?11
                 WHERE id = ?7 AND season = ?8 AND state IN ('queued', 'running')
                   AND (?12 IS NULL OR job_token = ?12)",
                    params![
                        score.total,
                        json,
                        score.par2,
                        score.solved,
                        score.wrong,
                        now,
                        *id,
                        *season,
                        fingerprint,
                        results,
                        *rules_version,
                        token
                    ],
                )
                .await?;
            if changed > 0 {
                mark_copies(conn, *id).await?;
                crate::server::daily::invalidate_profiles();
                let mut rows = conn
                    .query(
                        "SELECT user_id FROM dojo_submissions WHERE id = ?1",
                        params![*id],
                    )
                    .await?;
                if let Some(row) = rows.next().await?
                    && let Some(user) = row.get::<Option<i64>>(0)?
                    && let Err(e) = crate::server::daily::sync_stamps(conn, user, false).await
                {
                    eprintln!("dojo #{id}: stamp reconciliation: {e}");
                }
            }
            changed
        }
        Report::Rejected { id, season, reason } => conn.execute(
            "UPDATE dojo_submissions SET state = 'rejected', job_token = NULL, error = ?1, updated_at = ?2
             WHERE id = ?3 AND season = ?4 AND state IN ('queued', 'running')
               AND (?5 IS NULL OR job_token = ?5)",
            params![serde_json::to_string(reason).unwrap_or_default(), now, *id, *season, token],
        ).await?,
        Report::Failed { id, season, message } => {
            eprintln!("dojo #{id}: worker failed: {message}");
            conn.execute(
                "UPDATE dojo_submissions SET state = CASE WHEN attempts >= ?4 THEN 'failed' ELSE 'queued' END,
                    job_token = NULL, updated_at = ?1
                 WHERE id = ?2 AND season = ?3 AND state IN ('queued', 'running')
                   AND (?5 IS NULL OR job_token = ?5)",
                params![now, *id, *season, MAX_ATTEMPTS, token],
            ).await?
        }
    };
    Ok(changed > 0)
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

/// Whether the request carries the token of the job of submission `id`: the worker holds no key
/// to other submissions. `Some` with the submission's season when it does.
pub(super) async fn job_authorized(
    conn: &Connection,
    id: i64,
    headers: &HeaderMap,
) -> Result<Option<u32>, StatusCode> {
    let (expected, season): (Option<String>, u32) = match conn
        .query(
            "SELECT job_token, season FROM dojo_submissions WHERE id = ?1",
            params![id],
        )
        .await
    {
        Ok(mut rows) => match rows.next().await {
            Ok(Some(row)) => (row.get(0).ok().flatten(), row.get(1).unwrap_or_default()),
            _ => (None, 0),
        },
        Err(_) => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };
    let valid = match (expected, bearer(headers)) {
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
    Ok(valid.then_some(season))
}

/// The bearer token of a request.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

/// `POST /dojo/report` from the worker.
pub async fn report_handler(
    headers: HeaderMap,
    axum::Json(report): axum::Json<Report>,
) -> StatusCode {
    let Ok(conn) = db::conn() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    match job_authorized(&conn, report.id(), &headers).await {
        Ok(Some(_)) => {}
        Ok(None) => return StatusCode::UNAUTHORIZED,
        Err(status) => return status,
    }
    // The token goes into the UPDATE again: a requeue between the check above and the write
    // must not let the previous evaluation change the next one
    let given = bearer(&headers);
    match apply_for_job(&conn, &report, given).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::CONFLICT,
        Err(e) => {
            eprintln!("dojo report: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// Housekeeping before sending work.
///
/// A new season freezes the old one and re-evaluates everything (see [`seasons`]), so do newer
/// rules, evaluations that went quiet are queued again (or given up after a few attempts).
pub async fn housekeeping(conn: &Connection) -> Result<(), String> {
    housekeeping_at(conn, season()).await
}

/// [`housekeeping`] with `current` as the current season.
pub(super) async fn housekeeping_at(conn: &Connection, current: u32) -> Result<(), String> {
    let err = |e: libsql::Error| e.to_string();
    let now = now_ms();
    seasons::freeze_past(conn, current, now).await?;
    // Later seasons (DOJO_SEASON went back): evaluated again, nothing archived or revealed
    conn.execute(
        "UPDATE dojo_submissions SET state = 'queued', season = ?1, progress = 0, updated_at = ?2, attempts = 0,
            fingerprint = NULL, similar_to = NULL, results = NULL, score = NULL, par2 = NULL,
            solved = NULL, wrong = 0, error = NULL, job_token = NULL
         WHERE season != ?1 AND state IN ('done', 'queued', 'running')",
        params![current, now],
    )
    .await
    .map_err(err)?;
    seasons::open(conn, current, now).await?;
    seasons::requeue_old_rules(conn, current, now).await?;
    // Code hashes of submissions from before they existed
    let mut rows = conn
        .query(
            "SELECT id, wasm FROM dojo_submissions WHERE code_hash IS NULL LIMIT 20",
            (),
        )
        .await
        .map_err(err)?;
    let mut missing = Vec::new();
    while let Some(row) = rows.next().await.map_err(err)? {
        missing.push((
            row.get::<i64>(0).map_err(err)?,
            row.get::<Vec<u8>>(1).map_err(err)?,
        ));
    }
    for (id, wasm) in missing {
        conn.execute(
            "UPDATE dojo_submissions SET code_hash = ?2 WHERE id = ?1",
            params![id, code_hash(&wasm).unwrap_or_default()],
        )
        .await
        .map_err(err)?;
    }
    conn.execute(
        "UPDATE dojo_submissions SET state = CASE WHEN attempts >= ?1 THEN 'failed' ELSE 'queued' END, job_token = NULL, updated_at = ?2
         WHERE state = 'running' AND updated_at < ?3",
        params![MAX_ATTEMPTS, now, now - STALE_MS],
    )
    .await
    .map_err(err)?;
    Ok(())
}

/// Housekeeping, then sends queued submissions to the worker if there is one. Without one
/// seasons are still frozen and opened on time.
pub async fn dispatch() -> Result<(), String> {
    static BUSY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let Ok(_guard) = BUSY.try_lock() else {
        return Ok(());
    };
    let conn = db::conn().map_err(|e| e.to_string())?;
    match (
        std::env::var("DOJO_URL").ok().filter(|u| !u.is_empty()),
        token(),
    ) {
        (Some(url), Some(token)) => dispatch_to(&conn, &url, &token).await,
        _ => housekeeping(&conn).await,
    }
}

/// [`dispatch`] to the worker at `url`, authenticated with `token`.
pub(super) async fn dispatch_to(conn: &Connection, url: &str, token: &str) -> Result<(), String> {
    housekeeping(conn).await?;
    let client = reqwest::Client::new();
    loop {
        let report_token = new_job_token()?;
        // One SQL statement claims the row and checks the global running count. This works
        // across server replicas, and the claim exists before any worker can report back.
        let mut rows = conn
            .query(
                "UPDATE dojo_submissions SET state = 'running', job_token = ?1,
                attempts = attempts + 1, updated_at = ?2
             WHERE id = (SELECT id FROM dojo_submissions WHERE state = 'queued'
                         ORDER BY house DESC, created_at, id LIMIT 1)
               AND (SELECT COUNT(*) FROM dojo_submissions WHERE state = 'running') < ?3
             RETURNING id, wasm, season",
                params![report_token.clone(), now_ms(), MAX_RUNNING],
            )
            .await
            .map_err(|e| e.to_string())?;
        let Some(row) = rows.next().await.map_err(|e| e.to_string())? else {
            return Ok(());
        };
        let id: i64 = row.get(0).map_err(|e| e.to_string())?;
        let wasm: Vec<u8> = row.get(1).map_err(|e| e.to_string())?;
        let season: u32 = row.get(2).map_err(|e| e.to_string())?;
        drop(rows);
        let seed = test_seed(season);
        // The set of this season, unless the site has it from this generator already. Without
        // an answer the next job asks again
        let export = if seasons::wants_set(conn, season).await.unwrap_or(false) {
            "&export=1"
        } else {
            ""
        };
        // The worker machine may have to start first
        let sent = client
            .post(format!(
                "{}/jobs?id={id}&season={season}&seed={seed}&report_token={report_token}&rules={RULES_VERSION}{export}",
                url.trim_end_matches('/')
            ))
            .bearer_auth(token)
            .body(wasm)
            .timeout(Duration::from_secs(120))
            .send()
            .await;
        match sent {
            // The worker may already have finished. Never overwrite its callback on acceptance.
            Ok(r) if r.status() == StatusCode::ACCEPTED => {}
            Ok(r) if r.status() == StatusCode::UNPROCESSABLE_ENTITY => {
                let body = r.bytes().await.unwrap_or_default();
                let reason =
                    serde_json::from_slice(&body).unwrap_or(Rejection::Invalid(String::new()));
                apply_for_job(
                    conn,
                    &Report::Rejected { id, season, reason },
                    Some(&report_token),
                )
                .await
                .map_err(|e| e.to_string())?;
            }
            other => {
                let message = match other {
                    Ok(r) => format!("worker answered {}", r.status()),
                    Err(e) => format!("worker unreachable: {e}"),
                };
                // Not taken (worker down, busy or on other rules): queued again without using
                // up an attempt, unless the job was replaced meanwhile
                conn.execute(
                    "UPDATE dojo_submissions SET state = 'queued', job_token = NULL, attempts = attempts - 1,
                        updated_at = ?1
                     WHERE id = ?2 AND state = 'running' AND job_token = ?3",
                    params![now_ms(), id, report_token.clone()],
                )
                .await
                .map_err(|e| e.to_string())?;
                return Err(message);
            }
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

fn new_job_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(hex(&bytes))
}
