//! The admin dashboard (`/admin`, for the accounts in `HASHI_ADMINS`) and the same tools for
//! scripts (`HASHI_ADMIN_TOKEN` as bearer token):
//!
//! - `GET /admin/attempt?user=&day=` shows an attempt with its move log
//! - `POST /admin/flag?user=&day=&flag=review` takes a result off the board, without `flag`
//!   it goes back on

use axum::extract::{Multipart, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use leptos::prelude::ServerFnError;
use libsql::{Connection, params};
use serde::Deserialize;

use super::auth::{self, User};
use super::db::db_err;
use super::{daily, db, dojo, names};
use crate::model::{
    AdminAttempt, AdminOverview, AdminSubmission, ApiError, DojoState, Flag, FlaggedName,
    MoveEvent, NameKind, SubmissionAction, neutral_name, summarize_log,
};

/// How `HASHI_ADMINS` (comma separated) lists an admin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listed {
    /// By user id (`2`): the account is the admin, whatever its name.
    Id,
    /// By username, the older way (case doesn't matter). Such an admin keeps their name: freed, it
    /// would make whoever registers it next an admin.
    Name,
}

/// Whether the user is an admin: a registered account listed in `HASHI_ADMINS`.
pub fn is_admin(user: &User) -> bool {
    listing(user).is_some()
}

/// How `HASHI_ADMINS` lists the user, `None` if they are no admin. Guests never are.
pub fn listing(user: &User) -> Option<Listed> {
    let username = user.username.as_deref()?;
    listed(&admins(), user.id, username)
}

/// Whether `HASHI_ADMINS` lists this username (not an id).
pub fn admin_username(name: &str) -> bool {
    entries(&admins()).any(|a| a.parse::<i64>().is_err() && a.eq_ignore_ascii_case(name))
}

fn admins() -> String {
    super::env_var("HASHI_ADMINS").unwrap_or_default()
}

fn entries(admins: &str) -> impl Iterator<Item = &str> {
    admins.split(',').map(str::trim).filter(|a| !a.is_empty())
}

/// An entry that is a number is an id, never a username: "123" doesn't make the player who
/// registers the username 123 an admin.
fn listed(admins: &str, id: i64, username: &str) -> Option<Listed> {
    let mut by_name = false;
    for entry in entries(admins) {
        match entry.parse::<i64>() {
            Ok(listed) if listed == id => return Some(Listed::Id),
            Ok(_) => {}
            Err(_) => by_name |= entry.eq_ignore_ascii_case(username),
        }
    }
    by_name.then_some(Listed::Name)
}

/// The signed in admin, or `Forbidden`.
pub async fn require(conn: &Connection) -> Result<User, ServerFnError> {
    match auth::current_user(conn).await? {
        Some(user) if is_admin(&user) => Ok(user),
        _ => Err(ApiError::Forbidden.fail()),
    }
}

async fn count(conn: &Connection, sql: &str, day: i64) -> Result<i64, ServerFnError> {
    let mut rows = conn.query(sql, params![day]).await.map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err),
        None => Ok(0),
    }
}

pub async fn overview(conn: &Connection) -> Result<AdminOverview, ServerFnError> {
    let day = daily::today();
    Ok(AdminOverview {
        users: count(
            conn,
            "SELECT COUNT(*) FROM users WHERE deleted_at IS NULL AND ?1 = ?1",
            day,
        )
        .await?,
        registered: count(
            conn,
            "SELECT COUNT(*) FROM users WHERE username IS NOT NULL AND ?1 = ?1",
            day,
        )
        .await?,
        day,
        started: count(conn, "SELECT COUNT(*) FROM attempts WHERE day = ?1", day).await?,
        finished: count(
            conn,
            "SELECT COUNT(*) FROM attempts WHERE day = ?1 AND finished_at IS NOT NULL AND revealed = 0",
            day,
        )
        .await?,
        flagged: count(
            conn,
            "SELECT COUNT(*) FROM attempts WHERE day = ?1 AND flag IS NOT NULL",
            day,
        )
        .await?,
        dojo_pending: count(
            conn,
            // Evaluations under way, and community benchmarks waiting for a moderator
            "SELECT (SELECT COUNT(*) FROM dojo_submissions WHERE state IN ('queued', 'running'))
                + (SELECT COUNT(*) FROM dojo_benchmarks WHERE state = 'verified') + ?1 - ?1",
            day,
        )
        .await?,
        dojo_total: count(conn, "SELECT COUNT(*) FROM dojo_submissions WHERE ?1 = ?1", day).await?,
        messages: count(conn, "SELECT COUNT(*) FROM contact_messages WHERE ?1 = ?1", day).await?,
        names: super::count(flagged_names(conn).await?.len()),
    })
}

/// Names of players and Dojo solvers the list of blocked names flags. Guests have generated
/// names, players renamed by an admin a neutral one.
pub async fn flagged_names(conn: &Connection) -> Result<Vec<FlaggedName>, ServerFnError> {
    let mut out = Vec::new();
    let mut rows = conn
        .query(
            "SELECT id, name, username FROM users
             WHERE username IS NOT NULL AND deleted_at IS NULL ORDER BY id",
            (),
        )
        .await
        .map_err(db_err)?;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let id: i64 = row.get(0).map_err(db_err)?;
        let name: String = row.get(1).map_err(db_err)?;
        let username: String = row.get(2).map_err(db_err)?;
        if name == neutral_name(id) {
            continue;
        }
        let admin = listed(&admins(), id, &username).is_some();
        let word = names::blocked(&name).or_else(|| names::reserved(&name).filter(|_| !admin));
        if let Some(word) = word {
            out.push(FlaggedName {
                kind: NameKind::Player,
                id,
                name,
                word: word.into(),
            });
        }
    }
    let mut rows = conn
        .query("SELECT id, name FROM dojo_submissions ORDER BY id", ())
        .await
        .map_err(db_err)?;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let name: String = row.get(1).map_err(db_err)?;
        if let Some(word) = names::blocked(&name) {
            out.push(FlaggedName {
                kind: NameKind::Solver,
                id: row.get(0).map_err(db_err)?,
                name,
                word: word.into(),
            });
        }
    }
    Ok(out)
}

/// Gives a player the neutral name, on every board. `None` if there is no such player.
///
/// The username stays, so they can still sign in, and they can choose a new name right away (it
/// doesn't count as their monthly change).
pub async fn rename_player(conn: &Connection, user: i64) -> Result<Option<String>, ServerFnError> {
    let name = neutral_name(user);
    let changed = conn
        .execute(
            "UPDATE users SET name = ?2, renamed_at = NULL WHERE id = ?1 AND deleted_at IS NULL",
            params![user, name.clone()],
        )
        .await
        .map_err(db_err)?;
    Ok((changed != 0).then_some(name))
}

/// Every attempt of a day, finished ones fastest first, with a summary of the move log.
pub async fn attempts(conn: &Connection, day: i64) -> Result<Vec<AdminAttempt>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT a.user_id, u.name, u.username, a.started_at, a.finished_at, a.time_ms, a.hints,
                    a.checks, a.moves, a.revealed, a.flag, a.log
             FROM attempts a JOIN users u ON u.id = a.user_id
             WHERE a.day = ?1
             ORDER BY a.time_ms IS NULL, a.time_ms, a.started_at",
            params![day],
        )
        .await
        .map_err(db_err)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let log: Option<String> = row.get(11).map_err(db_err)?;
        let log: Vec<MoveEvent> = log
            .and_then(|l| serde_json::from_str(&l).ok())
            .unwrap_or_default();
        out.push(AdminAttempt {
            user_id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            username: row.get(2).map_err(db_err)?,
            started_at: row.get(3).map_err(db_err)?,
            finished_at: row.get(4).map_err(db_err)?,
            time_ms: row.get(5).map_err(db_err)?,
            hints: row.get(6).map_err(db_err)?,
            checks: row.get(7).map_err(db_err)?,
            moves: row.get(8).map_err(db_err)?,
            revealed: row.get::<i64>(9).map_err(db_err)? != 0,
            flag: row
                .get::<Option<String>>(10)
                .map_err(db_err)?
                .and_then(|f| Flag::from_id(&f)),
            log: summarize_log(&log),
        });
    }
    Ok(out)
}

/// The full move log of an attempt.
pub async fn log(conn: &Connection, user: i64, day: i64) -> Result<Vec<MoveEvent>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT log FROM attempts WHERE user_id = ?1 AND day = ?2",
            params![user, day],
        )
        .await
        .map_err(db_err)?;
    let log: Option<String> = match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err)?,
        None => None,
    };
    Ok(log
        .and_then(|l| serde_json::from_str(&l).ok())
        .unwrap_or_default())
}

pub async fn submissions(conn: &Connection) -> Result<Vec<AdminSubmission>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT s.id, s.name, COALESCE(u.name, 'Hashi'), s.house, s.state, s.season,
                    s.solved, s.total, s.par2, s.wrong, s.similar_to, s.created_at
             FROM dojo_submissions s LEFT JOIN users u ON u.id = s.user_id
             ORDER BY s.created_at DESC LIMIT 200",
            (),
        )
        .await
        .map_err(db_err)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        out.push(AdminSubmission {
            id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            author: row.get(2).map_err(db_err)?,
            house: row.get::<i64>(3).map_err(db_err)? != 0,
            state: DojoState::from_id(&row.get::<String>(4).map_err(db_err)?),
            season: row.get(5).map_err(db_err)?,
            solved: row.get(6).map_err(db_err)?,
            total: row.get(7).map_err(db_err)?,
            par2: row.get(8).map_err(db_err)?,
            wrong: row.get(9).map_err(db_err)?,
            similar_to: row.get(10).map_err(db_err)?,
            created_at: row.get(11).map_err(db_err)?,
        });
    }
    Ok(out)
}

pub async fn submission_action(
    conn: &Connection,
    id: i64,
    action: SubmissionAction,
) -> Result<(), ServerFnError> {
    let sql = match action {
        SubmissionAction::Requeue => {
            "UPDATE dojo_submissions SET state = 'queued', progress = 0, attempts = 0, similar_to = NULL,
                fingerprint = NULL WHERE id = ?1"
        }
        SubmissionAction::ClearCopy => "UPDATE dojo_submissions SET similar_to = NULL WHERE id = ?1",
        SubmissionAction::Delete => "DELETE FROM dojo_submissions WHERE id = ?1",
    };
    conn.execute(sql, params![id]).await.map_err(db_err)?;
    if action == SubmissionAction::Delete {
        // Copies of it are originals now
        conn.execute(
            "UPDATE dojo_submissions SET similar_to = NULL WHERE similar_to = ?1",
            params![id],
        )
        .await
        .map_err(db_err)?;
    }
    if action == SubmissionAction::Requeue {
        tokio::spawn(async {
            let _ = dojo::dispatch().await;
        });
    }
    Ok(())
}

/// `POST /admin/house`, a form on the dashboard: adds one of our own solvers to the Dojo.
pub async fn house_form(headers: HeaderMap, mut multipart: Multipart) -> Response {
    let mut form = dojo::Form::default();
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name().unwrap_or_default() {
            "wasm" => form.wasm = field.bytes().await.map(|b| b.to_vec()).unwrap_or_default(),
            "name" => form.name = field.text().await.unwrap_or_default(),
            "description" => form.description = field.text().await.unwrap_or_default(),
            _ => {}
        }
    }
    form.source_url = format!("{}/tree/main/dojo/house", crate::seo::REPO_URL);
    let Ok(conn) = db::conn() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let admin = auth::user_from_headers(&conn, &headers)
        .await
        .ok()
        .flatten()
        .is_some_and(|u| is_admin(&u));
    if !admin {
        return StatusCode::NOT_FOUND.into_response();
    }
    match dojo::submit(&conn, None, form).await {
        Ok(id) => Redirect::to(&format!("/admin?tab=dojo&added={id}")).into_response(),
        Err(e) => Redirect::to(&format!("/admin?tab=dojo&error={}", e.code())).into_response(),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn user(username: Option<&str>) -> User {
        User {
            id: 1,
            username: username.map(String::from),
            name: "x".into(),
            created_at: 0,
            renamed_at: None,
        }
    }

    #[test]
    fn admins_by_username() {
        assert_eq!(listed("Noel", 7, "noel"), Some(Listed::Name));
        assert_eq!(listed(" alice , Noel ", 7, "NOEL"), Some(Listed::Name));
        assert_eq!(listed("Noel", 7, "noelle"), None);
        // An empty list has no admins, guests have no username
        assert_eq!(listed("", 7, ""), None);
        assert_eq!(listed(",", 7, "noel"), None);
        assert!(is_admin(&user(Some("HTTP_admin"))));
        assert!(!is_admin(&user(None)));
        assert!(admin_username("http_ADMIN"));
    }

    #[test]
    fn admins_by_id() {
        assert_eq!(listed("2", 2, "noel"), Some(Listed::Id));
        assert_eq!(listed("Noel, 2", 2, "noel"), Some(Listed::Id));
        assert_eq!(listed(" 5 ,2 ", 2, "anyone"), Some(Listed::Id));
        assert_eq!(listed("2", 3, "noel"), None);
        // A number is an id, never the username of that name
        assert_eq!(listed("123", 7, "123"), None);
        assert!(!admin_username("900001"));
    }
}
