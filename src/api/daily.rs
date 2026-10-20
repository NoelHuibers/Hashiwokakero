//! The daily puzzle: starting, hints and checks, submitting, leaderboards and the archive.

use common::Puzzle;
use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{DailyInfo, HallOfFame, Leaderboard, MonthBoard, MoveEvent, SubmitResult};
#[cfg(feature = "ssr")]
use crate::server::{auth, daily, db};

/// Today's daily, with the puzzle only if the current user already started it.
#[server(input = Json, client = HashiClient)]
pub async fn daily_today() -> Result<DailyInfo, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn).await?;
    daily::info(&conn, user.map(|u| u.id), daily::today()).await
}

/// Starts today's daily for the current user (a guest is created if needed).
#[server(input = Json, client = HashiClient)]
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
#[server(input = Json, client = HashiClient)]
pub async fn daily_hint(day: i64, bridges: Vec<u8>) -> Result<Option<(usize, u8)>, ServerFnError> {
    let (conn, user) = signed_in().await?;
    daily::hint(&conn, user.id, day, bridges).await
}

/// Bridges that don't belong to the solution, +10 s.
#[server(input = Json, client = HashiClient)]
pub async fn daily_check(day: i64, bridges: Vec<u8>) -> Result<Vec<usize>, ServerFnError> {
    let (conn, user) = signed_in().await?;
    daily::check(&conn, user.id, day, bridges).await
}

/// A paid action intent. The older hint/check endpoints keep their wire formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PaidAction {
    Hint,
    Check,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PaidOutcome {
    Hint(Option<(usize, u8)>),
    Check(Vec<usize>),
}

/// The original outcome and the current authoritative penalty counters, including on replay.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PaidReply {
    pub outcome: PaidOutcome,
    pub hints: i64,
    pub checks: i64,
}

/// An unresolved purchase saved before dispatch. Its original identity survives guest transfer.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PaidIntent {
    /// Older stored intents without this field can only use read-only receipt recovery.
    #[serde(default)]
    pub attempt_id: String,
    pub user: i64,
    pub day: i64,
    pub started_at: i64,
    pub action: PaidAction,
    pub bridges: Vec<u8>,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PaidRecovery {
    pub intent: PaidIntent,
    pub reply: PaidReply,
}

/// Read-only: recover only an exact receipt now owned by this authenticated attempt. Missing
/// receipts never become purchases, including after an unrelated identity switch or conflict.
#[server(input = Json, client = HashiClient)]
pub async fn daily_recover_actions(
    day: i64,
    started_at: i64,
    intents: Vec<PaidIntent>,
    expected_user: i64,
) -> Result<Option<PaidRecovery>, ServerFnError> {
    let (conn, user) = signed_in().await?;
    if user.id != expected_user {
        return Err(crate::model::ApiError::SessionExpired.into());
    }
    daily::recover_actions(&conn, user.id, day, started_at, &intents).await
}

/// Retries must reuse the same ID, action and bridges. A fresh ID expresses a fresh purchase.
#[server(input = Json, client = HashiClient)]
pub async fn daily_paid_action(
    day: i64,
    bridges: Vec<u8>,
    action: PaidAction,
    request_id: String,
    expected_user: i64,
    expected_attempt: String,
) -> Result<PaidReply, ServerFnError> {
    let (conn, user) = signed_in().await?;
    // Cookies are shared across tabs. A sign-in elsewhere must not charge another player's
    // attempt while this page still displays the previous identity. This is an assertion,
    // never a substitute for the authenticated user.
    if user.id != expected_user {
        return Err(crate::model::ApiError::SessionExpired.into());
    }
    daily::paid_action(
        &conn,
        user.id,
        day,
        bridges,
        action,
        &request_id,
        &expected_attempt,
    )
    .await
}

/// The solution. The attempt is no longer ranked.
#[server(input = Json, client = HashiClient)]
pub async fn daily_reveal(day: i64) -> Result<Vec<u8>, ServerFnError> {
    let (conn, user) = signed_in().await?;
    daily::reveal(&conn, user.id, day).await
}

#[server(input = Json, client = HashiClient)]
pub async fn submit_daily(
    day: i64,
    bridges: Vec<u8>,
    moves: u32,
    // Missing from pages loaded before move logs existed: then the result isn't ranked
    #[server(default)] log: Vec<MoveEvent>,
    // The browser's time zone, for the night owl and lunch break stamps (not stored)
    #[server(default)] utc_offset: Option<i32>,
) -> Result<SubmitResult, ServerFnError> {
    let (conn, user) = signed_in().await?;
    daily::submit(&conn, user.id, day, bridges, moves, log, utc_offset).await
}

/// The ranking of a day, today if `day` is `None`.
#[server(input = Json, client = HashiClient)]
pub async fn daily_leaderboard(day: Option<i64>) -> Result<Leaderboard, ServerFnError> {
    let conn = db::conn()?;
    let me = auth::current_user(&conn).await?.map(|u| u.id);
    let today = daily::today();
    let day = day.unwrap_or(today).min(today);
    daily::leaderboard(&conn, day, me).await
}

/// The all time rankings: most solved, most medals, longest streaks.
#[server(input = Json, client = HashiClient)]
pub async fn load_hall_of_fame() -> Result<HallOfFame, ServerFnError> {
    daily::hall_of_fame(&db::conn()?).await
}

/// The points race of a month (see `model::month_of`), the running one if `month` is `None`.
#[server(input = Json, client = HashiClient)]
pub async fn month_leaderboard(month: Option<i64>) -> Result<MonthBoard, ServerFnError> {
    let conn = db::conn()?;
    let me = auth::current_user(&conn).await?.map(|u| u.id);
    daily::month_board(&conn, month, me).await
}

/// An earlier daily, to play it in practice mode.
#[server(input = Json, client = HashiClient)]
pub async fn archived_daily(day: i64) -> Result<Option<Puzzle>, ServerFnError> {
    daily::archived(&db::conn()?, day).await
}
