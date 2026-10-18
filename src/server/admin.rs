//! Reviewing daily results: `HASHI_ADMIN_TOKEN` as bearer token.
//!
//! - `GET /admin/attempt?user=&day=` shows an attempt with its move log
//! - `POST /admin/flag?user=&day=&flag=review` takes a result off the board, without `flag`
//!   it goes back on

use axum::extract::Query;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use libsql::params;
use serde::Deserialize;

use super::{daily, db};
use crate::model::Flag;

const TOKEN: &str = "HASHI_ADMIN_TOKEN";

#[derive(Debug, Deserialize)]
pub struct AttemptQuery {
    user: i64,
    day: i64,
    #[serde(default)]
    flag: Option<String>,
}

pub async fn attempt(headers: HeaderMap, Query(q): Query<AttemptQuery>) -> Response {
    if !super::bearer_ok(&headers, TOKEN) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(conn) = db::conn() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let row = async {
        let mut rows = conn
            .query(
                "SELECT started_at, finished_at, time_ms, hints, checks, revealed, flag, moves, log
                 FROM attempts WHERE user_id = ?1 AND day = ?2",
                params![q.user, q.day],
            )
            .await?;
        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        let log: Option<String> = row.get(8)?;
        Ok::<_, libsql::Error>(Some(serde_json::json!({
            "started_at": row.get::<i64>(0)?,
            "finished_at": row.get::<Option<i64>>(1)?,
            "time_ms": row.get::<Option<i64>>(2)?,
            "hints": row.get::<i64>(3)?,
            "checks": row.get::<i64>(4)?,
            "revealed": row.get::<i64>(5)? != 0,
            "flag": row.get::<Option<String>>(6)?,
            "moves": row.get::<Option<i64>>(7)?,
            "log": log.and_then(|l| serde_json::from_str::<serde_json::Value>(&l).ok()),
        })))
    }
    .await;
    match row {
        Ok(Some(json)) => axum::Json(json).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            eprintln!("admin attempt: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn flag(headers: HeaderMap, Query(q): Query<AttemptQuery>) -> Response {
    if !super::bearer_ok(&headers, TOKEN) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let flag = match q.flag.as_deref() {
        None | Some("") => None,
        Some(id) => match Flag::from_id(id) {
            Some(f) => Some(f),
            None => return (StatusCode::BAD_REQUEST, "unknown flag").into_response(),
        },
    };
    let Ok(conn) = db::conn() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match daily::set_flag(&conn, q.user, q.day, flag).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
