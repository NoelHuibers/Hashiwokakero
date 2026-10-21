//! Hanko 判子: seals pressed on each other's daily results in a group, and the ones got.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{HankoKind, HankoMark, HankoReceived, UnreadHanko};
#[cfg(feature = "ssr")]
use crate::server::{auth, daily, db, groups};

/// Presses a seal of `kind` on `target`'s result of `day` in a group (`on`), or takes it back.
/// Returns the seals on that day's board of the group.
#[server(input = Json, client = HashiClient)]
pub async fn set_hanko(
    group: i64,
    target: i64,
    day: i64,
    kind: HankoKind,
    on: bool,
) -> Result<Vec<HankoMark>, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn)
        .await?
        .ok_or_else(|| crate::model::ApiError::SessionExpired.fail())?;
    groups::hanko::press(&conn, group, user.id, target, day, kind, on, daily::today()).await
}

/// The seals the player got lately, in one group or all of theirs (`None`), newest first.
/// Opens them: they don't count as unread any more.
#[server(input = Json, client = HashiClient)]
pub async fn open_hanko(group: Option<i64>) -> Result<Vec<HankoReceived>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => groups::hanko::received(&conn, user.id, group, daily::today()).await,
        None => Ok(Vec::new()),
    }
}

/// How many seals the player got and didn't see yet, per group, for the unread dot.
#[server(input = Json, client = HashiClient)]
pub async fn unread_hanko_counts() -> Result<Vec<UnreadHanko>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => groups::hanko::unread(&conn, user.id, daily::today()).await,
        None => Ok(Vec::new()),
    }
}
