//! The admin dashboard. All of these check that the user is an admin.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use crate::model::{
    AdminAttempt, AdminOverview, AdminSubmission, Flag, MoveEvent, SubmissionAction,
};
#[cfg(feature = "ssr")]
use crate::server::{admin, daily, db};

#[server(input = Json)]
pub async fn load_admin_overview() -> Result<AdminOverview, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::overview(&conn).await
}

/// Every attempt of a day (today if `None`), with move log summaries.
#[server(input = Json)]
pub async fn admin_attempts(day: Option<i64>) -> Result<(i64, Vec<AdminAttempt>), ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    let day = day.unwrap_or_else(daily::today);
    Ok((day, admin::attempts(&conn, day).await?))
}

#[server(input = Json)]
pub async fn admin_log(user: i64, day: i64) -> Result<Vec<MoveEvent>, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::log(&conn, user, day).await
}

/// Takes a result off the board (`Some`) or puts it back (`None`).
#[server(input = Json)]
pub async fn admin_flag(user: i64, day: i64, flag: Option<Flag>) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    daily::set_flag(&conn, user, day, flag).await.map(|_| ())
}

#[server(input = Json)]
pub async fn admin_submissions() -> Result<Vec<AdminSubmission>, ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::submissions(&conn).await
}

#[server(input = Json)]
pub async fn update_admin_submission(
    id: i64,
    action: SubmissionAction,
) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    admin::require(&conn).await?;
    admin::submission_action(&conn, id, action).await
}
