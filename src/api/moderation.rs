//! Reporting a player to us, and blocking players.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{BlockedPlayer, ReportReason};
#[cfg(feature = "ssr")]
use crate::server::{auth, blocks, db, reports};

/// Reports a player to the admins, with a note (at most `MAX_REPORT_NOTE` characters, the rest
/// is cut). Reporting the same player again updates the open report. A visitor without a
/// session becomes a guest, like on the daily's start.
#[server(input = Json, client = HashiClient)]
pub async fn report_player(
    player: i64,
    reason: ReportReason,
    note: String,
) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    let user = auth::ensure_user(&conn).await?;
    reports::report(&conn, user.id, player, reason, &note)
        .await
        .map(drop)
}

/// Blocks a player (`on`) or unblocks them, and returns the players blocked then. A visitor
/// without a session becomes a guest.
#[server(input = Json, client = HashiClient)]
pub async fn block_player(player: i64, on: bool) -> Result<Vec<BlockedPlayer>, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::ensure_user(&conn).await?;
    blocks::set(&conn, user.id, player, on).await
}

/// The players the current user blocked, none without a session.
#[server(input = Json, client = HashiClient)]
pub async fn my_blocks() -> Result<Vec<BlockedPlayer>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => blocks::list(&conn, user.id).await,
        None => Ok(Vec::new()),
    }
}
