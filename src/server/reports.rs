//! Reports of players to the admins.
//!
//! Anyone with a session (guests too) reports another player's name, their cheating or something
//! else, with a note if they like. The admins look at every report within a day
//! (`/admin?tab=reports`): they rename the player, ban them, or dismiss the report. A mail tells
//! us at once, if SMTP is set up (`contact`); a report never fails because of the mail.
//!
//! One open report per reporter and player: reporting again updates it (without another mail).
//! Reports per reporter and per day are limited, and all of them per day (and per network by
//! `limits::admission`). Resolved reports go after half a year, all reports of and by a player
//! with their account (`auth::erase`).

#[cfg(test)]
mod tests;

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::db::db_err;
use super::now_ms;
use crate::model::{
    AdminReport, ApiError, MAX_REPORT_NOTE, MS_PER_DAY, ReportOutcome, ReportReason,
};

/// Reports (or updates of one) a player sends in a day, and all players together.
const PER_REPORTER: i64 = 10;
const PER_DAY: i64 = 200;
/// Resolved reports are kept this long, for a player reported again.
const KEEP_DAYS: i64 = 180;

/// A report as sent, after the checks.
#[derive(Debug)]
struct Sent {
    id: i64,
    /// Not an update of an open one.
    new: bool,
    name: String,
    reporter_name: String,
}

/// `reporter` reports `player`. Returns the report's id.
pub async fn report(
    conn: &Connection,
    reporter: i64,
    player: i64,
    reason: ReportReason,
    note: &str,
) -> Result<i64, ServerFnError> {
    let note: String = note
        .trim()
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(MAX_REPORT_NOTE)
        .collect();
    let sent = store(conn, reporter, player, reason, &note, now_ms()).await?;
    let id = sent.id;
    // Once per report: updates don't mail again, they'd make a lot of mail
    if !sent.new {
        return Ok(id);
    }
    let subject = format!("Meldung #{id}: {} (#{player})", sent.name);
    let body = format!(
        "Meldung #{id} über Spieler #{player} „{}“\nGrund: {}\nVon: {} (#{reporter})\n\n{}\n\nhttps://hashi.rs/admin?tab=reports",
        sent.name,
        reason.id(),
        sent.reporter_name,
        if note.is_empty() { "-" } else { &note },
    );
    // The player doesn't wait for the mail server, the report is safe in the database
    tokio::spawn(async move {
        if let Err(e) = super::contact::notify(&subject, &body).await {
            eprintln!("reports: mailing #{id} failed: {e}");
        }
    });
    Ok(id)
}

/// Stores the report, or updates the reporter's open one of the player.
async fn store(
    conn: &Connection,
    reporter: i64,
    player: i64,
    reason: ReportReason,
    note: &str,
    now: i64,
) -> Result<Sent, ServerFnError> {
    if reporter == player {
        return Err(ApiError::NoSuchPlayer.into());
    }
    let name = {
        let mut rows = conn
            .query(
                "SELECT name FROM users WHERE id = ?1 AND deleted_at IS NULL",
                params![player],
            )
            .await
            .map_err(db_err)?;
        match rows.next().await.map_err(db_err)? {
            Some(row) => row.get::<String>(0).map_err(db_err)?,
            None => return Err(ApiError::NoSuchPlayer.into()),
        }
    };
    let since = now - MS_PER_DAY;
    let mut rows = conn
        .query(
            "SELECT (SELECT COUNT(*) FROM reports WHERE reporter_id = ?1 AND updated_at > ?2),
                    (SELECT COUNT(*) FROM reports WHERE updated_at > ?2),
                    (SELECT name FROM users WHERE id = ?1)",
            params![reporter, since],
        )
        .await
        .map_err(db_err)?;
    let (mine, all, reporter_name) = match rows.next().await.map_err(db_err)? {
        Some(row) => (
            row.get::<i64>(0).map_err(db_err)?,
            row.get::<i64>(1).map_err(db_err)?,
            row.get::<Option<String>>(2).map_err(db_err)?,
        ),
        None => (0, 0, None),
    };
    drop(rows);
    if mine >= PER_REPORTER || all >= PER_DAY {
        return Err(ApiError::TooManyReports.into());
    }
    // One statement: two reports at once can't both insert
    let mut rows = conn
        .query(
            "INSERT INTO reports (reporter_id, player_id, reason, note, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT (reporter_id, player_id) WHERE resolved_at IS NULL
             DO UPDATE SET reason = excluded.reason, note = excluded.note, name = excluded.name,
                 updated_at = excluded.updated_at
             RETURNING id, created_at = updated_at",
            params![reporter, player, reason.id(), note, name.clone(), now],
        )
        .await
        .map_err(db_err)?;
    let (id, new) = match rows.next().await.map_err(db_err)? {
        Some(row) => (
            row.get(0).map_err(db_err)?,
            row.get::<i64>(1).map_err(db_err)? != 0,
        ),
        None => return Err(ApiError::Internal.into()),
    };
    Ok(Sent {
        id,
        new,
        name,
        reporter_name: reporter_name.unwrap_or_default(),
    })
}

/// The open reports, the one waiting longest first.
pub async fn open(conn: &Connection) -> Result<Vec<AdminReport>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT r.id, r.player_id, COALESCE(p.name, ''), r.name, p.username,
                    p.banned_at IS NOT NULL, r.reason, r.note, r.reporter_id,
                    COALESCE(u.name, ''), r.created_at, r.updated_at
             FROM reports r
             LEFT JOIN users p ON p.id = r.player_id
             LEFT JOIN users u ON u.id = r.reporter_id
             WHERE r.resolved_at IS NULL
             ORDER BY r.updated_at, r.id",
            (),
        )
        .await
        .map_err(db_err)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let reason: String = row.get(6).map_err(db_err)?;
        out.push(AdminReport {
            id: row.get(0).map_err(db_err)?,
            player: row.get(1).map_err(db_err)?,
            player_name: row.get(2).map_err(db_err)?,
            reported_name: row.get(3).map_err(db_err)?,
            player_username: row.get(4).map_err(db_err)?,
            banned: row.get::<Option<i64>>(5).map_err(db_err)?.unwrap_or(0) != 0,
            reason: ReportReason::from_id(&reason).unwrap_or(ReportReason::Other),
            note: row.get(7).map_err(db_err)?,
            reporter: row.get(8).map_err(db_err)?,
            reporter_name: row.get(9).map_err(db_err)?,
            created_at: row.get(10).map_err(db_err)?,
            updated_at: row.get(11).map_err(db_err)?,
        });
    }
    Ok(out)
}

/// How many reports wait for an admin.
pub async fn open_count(conn: &Connection) -> Result<i64, ServerFnError> {
    let mut rows = conn
        .query("SELECT COUNT(*) FROM reports WHERE resolved_at IS NULL", ())
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err),
        None => Ok(0),
    }
}

/// Marks a report as dealt with. A ban resolves every open report of the player, a new name
/// every open report of their name. Returns how many reports were resolved.
pub async fn resolve(
    conn: &Connection,
    id: i64,
    outcome: ReportOutcome,
) -> Result<u64, ServerFnError> {
    let also = match outcome {
        ReportOutcome::Banned => "1",
        ReportOutcome::Renamed => "reason = 'name'",
        ReportOutcome::Dismissed => "0",
    };
    conn.execute(
        &format!(
            "UPDATE reports SET resolved_at = ?2, resolution = ?3
             WHERE resolved_at IS NULL AND (id = ?1 OR ({also}
                 AND player_id = (SELECT player_id FROM reports WHERE id = ?1)))"
        ),
        params![id, now_ms(), outcome.id()],
    )
    .await
    .map_err(db_err)
}

/// Deletes resolved reports older than half a year.
pub async fn delete_old(conn: &Connection) -> Result<(), ServerFnError> {
    conn.execute(
        "DELETE FROM reports WHERE resolved_at < ?1",
        params![now_ms() - KEEP_DAYS * MS_PER_DAY],
    )
    .await
    .map_err(db_err)?;
    Ok(())
}

/// For `auth::erase`, `?1` the user: the reports they sent and those about them.
pub const ERASE: [&str; 1] = ["DELETE FROM reports WHERE reporter_id = ?1 OR player_id = ?1"];

/// For `auth::merge_guest`: `?1` the account, `?2` the guest who played in the browser before.
///
/// The account takes over the guest's reports, sent and got. Nobody reports themselves, and
/// where both reported the same player, the account's open report stays.
pub const MERGE_GUEST: [&str; 4] = [
    "DELETE FROM reports WHERE (reporter_id = ?1 AND player_id = ?2)
        OR (reporter_id = ?2 AND player_id = ?1)",
    "UPDATE OR IGNORE reports SET reporter_id = ?1 WHERE reporter_id = ?2",
    "UPDATE OR IGNORE reports SET player_id = ?1 WHERE player_id = ?2",
    "DELETE FROM reports WHERE reporter_id = ?2 OR player_id = ?2",
];
