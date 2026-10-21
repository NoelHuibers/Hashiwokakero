//! Replays (再生): the move logs of a day for the replay theatre, a group's or one's own.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::ReplayData;
#[cfg(feature = "ssr")]
use crate::server::{auth, daily, db, replay};

/// The replay of `day`: a group's (`group`, for its members), or the player's own solve
/// (`None`). `None` for a group the visitor isn't in (or that doesn't exist), and for a visitor
/// without a user. Today's stays [`ReplayData::locked`] until they finished it.
#[server(input = Json, client = HashiClient)]
pub async fn load_replay(
    group: Option<i64>,
    day: i64,
) -> Result<Option<ReplayData>, ServerFnError> {
    let conn = db::conn()?;
    let Some(user) = auth::current_user(&conn).await? else {
        return Ok(None);
    };
    let today = daily::today();
    let Some(group) = group else {
        return replay::own_replay(&conn, &user, day, today).await.map(Some);
    };
    match replay::group_replay(&conn, group, user.id, day, today).await {
        Ok(data) => Ok(Some(data)),
        Err(e)
            if crate::model::ApiError::from_message(&e.to_string())
                == Some(crate::model::ApiError::GroupNotFound) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
