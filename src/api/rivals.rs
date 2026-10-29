//! Rivals: marking players to compare yourself with, and the comparisons.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{HeadToHead, RivalsBoard, RivalsToday};
#[cfg(feature = "ssr")]
use crate::server::{auth, blocks, daily, db};

/// The ids of the current user's rivals, none without a session.
#[server(input = Json, client = HashiClient)]
pub async fn my_rivals() -> Result<Vec<i64>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => daily::rivals(&conn, user.id).await,
        None => Ok(Vec::new()),
    }
}

/// Marks a player as a rival (`on`) or not any more, and returns the rivals then. A visitor
/// without a session becomes a guest, like on the daily's start.
#[server(input = Json, client = HashiClient)]
pub async fn set_rival(player: i64, on: bool) -> Result<Vec<i64>, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::ensure_user(&conn).await?;
    daily::set_rival(&conn, user.id, player, on).await
}

/// How the current user did against their rivals today, `None` until they finished today's
/// daily (or without a session). Not `rivals_today`: its struct would clash with the model type.
#[server(input = Json, client = HashiClient)]
pub async fn todays_rivals() -> Result<Option<RivalsToday>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => {
            let duels = daily::rival_duels(&conn, user.id, daily::today()).await?;
            blocks::hidden(&conn, Some(user.id), duels).await
        }
        None => Ok(None),
    }
}

/// The current user and their rivals compared, `None` without a session.
#[server(input = Json, client = HashiClient)]
pub async fn load_rivals_board() -> Result<Option<RivalsBoard>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => {
            let board = daily::rivals_board(&conn, user.id).await?;
            blocks::hidden(&conn, Some(user.id), Some(board)).await
        }
        None => Ok(None),
    }
}

/// The current user's head-to-head record against `player`, if that is one of their rivals.
#[server(input = Json, client = HashiClient)]
pub async fn rival_record(player: i64) -> Result<Option<HeadToHead>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => daily::rival_record(&conn, user.id, player).await,
        None => Ok(None),
    }
}
