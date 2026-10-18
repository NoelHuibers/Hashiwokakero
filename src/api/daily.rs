//! The daily puzzle: starting, hints and checks, submitting, leaderboards and the archive.

use common::Puzzle;
use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use crate::model::{AllTimeEntry, DailyInfo, Leaderboard, MoveEvent, SubmitResult};
#[cfg(feature = "ssr")]
use crate::server::{auth, daily, db};

/// Today's daily, with the puzzle only if the current user already started it.
#[server(input = Json)]
pub async fn daily_today() -> Result<DailyInfo, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn).await?;
    daily::info(&conn, user.map(|u| u.id), daily::today()).await
}

/// Starts today's daily for the current user (a guest is created if needed).
#[server(input = Json)]
pub async fn start_daily(device: Option<String>) -> Result<DailyInfo, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::ensure_user(&conn).await?;
    daily::start(&conn, user.id, daily::today(), device.as_deref()).await
}

#[cfg(feature = "ssr")]
async fn signed_in() -> Result<(libsql::Connection, auth::User), ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn)
        .await?
        .ok_or_else(|| crate::model::ApiError::SessionExpired.fail())?;
    Ok((conn, user))
}

/// One correct bridge, +30 s.
#[server(input = Json)]
pub async fn daily_hint(day: i64, bridges: Vec<u8>) -> Result<Option<(usize, u8)>, ServerFnError> {
    let (conn, user) = signed_in().await?;
    daily::hint(&conn, user.id, day, bridges).await
}

/// Bridges that don't belong to the solution, +10 s.
#[server(input = Json)]
pub async fn daily_check(day: i64, bridges: Vec<u8>) -> Result<Vec<usize>, ServerFnError> {
    let (conn, user) = signed_in().await?;
    daily::check(&conn, user.id, day, bridges).await
}

/// The solution. The attempt is no longer ranked.
#[server(input = Json)]
pub async fn daily_reveal(day: i64) -> Result<Vec<u8>, ServerFnError> {
    let (conn, user) = signed_in().await?;
    daily::reveal(&conn, user.id, day).await
}

#[server(input = Json)]
pub async fn submit_daily(
    day: i64,
    bridges: Vec<u8>,
    moves: u32,
    // Missing from pages loaded before move logs existed: then the result isn't ranked
    #[server(default)] log: Vec<MoveEvent>,
) -> Result<SubmitResult, ServerFnError> {
    let (conn, user) = signed_in().await?;
    daily::submit(&conn, user.id, day, bridges, moves, log).await
}

/// The ranking of a day, today if `day` is `None`.
#[server(input = Json)]
pub async fn daily_leaderboard(day: Option<i64>) -> Result<Leaderboard, ServerFnError> {
    let conn = db::conn()?;
    let me = auth::current_user(&conn).await?.map(|u| u.id);
    let today = daily::today();
    let day = day.unwrap_or(today).min(today);
    daily::leaderboard(&conn, day, me).await
}

#[server(input = Json)]
pub async fn all_time_leaderboard() -> Result<Vec<AllTimeEntry>, ServerFnError> {
    daily::all_time(&db::conn()?).await
}

/// An earlier daily, to play it in practice mode.
#[server(input = Json)]
pub async fn archived_daily(day: i64) -> Result<Option<Puzzle>, ServerFnError> {
    daily::archived(&db::conn()?, day).await
}
