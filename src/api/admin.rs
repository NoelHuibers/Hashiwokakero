//! The admin dashboard. All of these check that the user is an admin.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{
    AdminAttempt, AdminBenchmark, AdminOverview, AdminReport, AdminSubmission, BanOutcome,
    BannedPlayer, BenchmarkAction, ContactMessage, FeedbackKind, FeedbackStatus, Flag, FlaggedName,
    MoveEvent, ReportOutcome, SubmissionAction,
};
#[cfg(feature = "ssr")]
use crate::server::{admin, contact, daily, db, reports};

#[server(input = Json, client = HashiClient)]
pub async fn load_admin_overview() -> Result<AdminOverview, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::overview(&conn).await
}

/// Every attempt of a day (today if `None`), with move log summaries.
#[server(input = Json, client = HashiClient)]
pub async fn admin_attempts(day: Option<i64>) -> Result<(i64, Vec<AdminAttempt>), ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    let day = day.unwrap_or_else(daily::today);
    Ok((day, admin::attempts(&conn, day).await?))
}

#[server(input = Json, client = HashiClient)]
pub async fn admin_log(user: i64, day: i64) -> Result<Vec<MoveEvent>, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::log(&conn, user, day).await
}

/// Takes a result off the board (`Some`) or puts it back (`None`).
#[server(input = Json, client = HashiClient)]
pub async fn admin_flag(user: i64, day: i64, flag: Option<Flag>) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    daily::set_flag(&conn, user, day, flag).await.map(|_| ())
}

#[server(input = Json, client = HashiClient)]
pub async fn admin_submissions() -> Result<Vec<AdminSubmission>, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::submissions(&conn).await
}

#[server(input = Json, client = HashiClient)]
pub async fn update_admin_submission(
    id: i64,
    action: SubmissionAction,
) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::submission_action(&conn, id, action).await
}

/// Community benchmarks, the ones waiting for a moderator first.
#[server(input = Json, client = HashiClient)]
pub async fn admin_benchmarks() -> Result<Vec<AdminBenchmark>, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    crate::server::dojo::admin_benchmarks(&conn).await
}

#[server(input = Json, client = HashiClient)]
pub async fn update_admin_benchmark(id: i64, action: BenchmarkAction) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    crate::server::dojo::benchmark_action(&conn, id, action).await
}

/// Messages of the feedback form with `status` and of `kind` (`None`: any), newest first.
#[server(input = Json, client = HashiClient)]
pub async fn admin_messages(
    #[server(default)] before: Option<i64>,
    #[server(default)] status: Option<FeedbackStatus>,
    #[server(default)] kind: Option<FeedbackKind>,
) -> Result<Vec<ContactMessage>, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    contact::page(&conn, before, status, kind).await
}

/// Marks a message as answered or closed, or new again (see `contact::set_status`). `false` if
/// there is no such message.
#[server(input = Json, client = HashiClient)]
pub async fn admin_message_status(id: i64, status: FeedbackStatus) -> Result<bool, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    contact::set_status(&conn, id, status).await
}

/// Posts message `id` as a GitHub issue with the admin's `title` and `body` (see
/// `issues::create`), returns its number. Fails without `GITHUB_FEEDBACK_TOKEN`.
#[server(input = Json, client = HashiClient)]
pub async fn admin_create_issue(
    id: i64,
    title: String,
    body: String,
) -> Result<i64, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    let github = crate::server::github::GitHub::from_env()
        .ok_or_else(|| ServerFnError::new("GITHUB_FEEDBACK_TOKEN isn't set"))?;
    crate::server::issues::create(&conn, &github, id, &title, &body).await
}

/// Deletes a message of the feedback form, spam say.
#[server(input = Json, client = HashiClient)]
pub async fn delete_admin_message(id: i64) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    contact::delete(&conn, id).await
}

/// Names on the boards the list of blocked names flags.
#[server(input = Json, client = HashiClient)]
pub async fn admin_names() -> Result<Vec<FlaggedName>, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::flagged_names(&conn).await
}

/// Gives a player a neutral name ("Player 17"), `None` if there is no such player.
#[server(input = Json, client = HashiClient)]
pub async fn admin_rename_player(id: i64) -> Result<Option<String>, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::rename_player(&conn, id).await
}

/// Bans a player (see `server::admin::ban`), `reason` is only for the admins.
#[server(input = Json, client = HashiClient)]
pub async fn admin_ban_player(id: i64, reason: String) -> Result<BanOutcome, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::ban(&conn, id, &reason).await
}

/// Lifts a ban, `false` if the player wasn't banned.
#[server(input = Json, client = HashiClient)]
pub async fn admin_unban_player(id: i64) -> Result<bool, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::unban(&conn, id).await
}

/// The banned players, the latest ban first.
#[server(input = Json, client = HashiClient)]
pub async fn admin_banned_players() -> Result<Vec<BannedPlayer>, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::banned_players(&conn).await
}

/// The reports of players nobody dealt with yet, the one waiting longest first.
#[server(input = Json, client = HashiClient)]
pub async fn admin_reports() -> Result<Vec<AdminReport>, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    reports::open(&conn).await
}

/// Marks a report as dealt with, after renaming or banning the player (with the functions
/// above) or to dismiss it. Returns how many reports that resolved (see `reports::resolve`).
#[server(input = Json, client = HashiClient)]
pub async fn admin_resolve_report(id: i64, outcome: ReportOutcome) -> Result<u64, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    reports::resolve(&conn, id, outcome).await
}
