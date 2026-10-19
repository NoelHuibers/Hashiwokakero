//! Talking to the worker: sending queued submissions, applying its reports, housekeeping.

use std::time::Duration;

use axum::http::{HeaderMap, StatusCode, header};
use dojo::{Rejection, Report};
use libsql::{Connection, params};

use super::{MAX_ATTEMPTS, MAX_RUNNING, STALE_MS, code_hash, season, test_seed, token};
use crate::server::db;
use crate::server::{hex, now_ms};

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
    dispatch_to(&conn, &url, &token).await
}

/// [`dispatch`] to the worker at `url`, authenticated with `token`.
pub(super) async fn dispatch_to(conn: &Connection, url: &str, token: &str) -> Result<(), String> {
    housekeeping(conn).await.map_err(|e| e.to_string())?;
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
            .bearer_auth(token)
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
                apply(conn, &Report::Rejected { id, season, reason })
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
