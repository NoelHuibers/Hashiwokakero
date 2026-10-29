//! Group puzzles (課題 kadai): posting a puzzle to a group, the group page's list, and playing
//! one like a daily.

use common::Puzzle;
use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{GroupPuzzles, MoveEvent, PuzzlePage};
#[cfg(feature = "ssr")]
use crate::server::{auth, blocks, db, groups::kadai, now_ms};

#[cfg(feature = "ssr")]
async fn signed_in() -> Result<(libsql::Connection, auth::User), ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn)
        .await?
        .ok_or_else(|| crate::model::ApiError::SessionExpired.fail())?;
    Ok((conn, user))
}

/// Posts a puzzle made in the workshop to one of the player's groups, with an optional title.
/// The server proves it unique itself. Returns the puzzle's id.
#[server(input = Json, client = HashiClient)]
pub async fn post_group_puzzle(
    group: i64,
    puzzle: Puzzle,
    title: String,
) -> Result<i64, ServerFnError> {
    let (conn, user) = signed_in().await?;
    kadai::post(&conn, group, user.id, puzzle, &title, now_ms()).await
}

/// The puzzles of a group, for its page. Not `group_puzzles`: its generated struct would clash
/// with the model type.
#[server(input = Json, client = HashiClient)]
pub async fn load_group_puzzles(group: i64) -> Result<GroupPuzzles, ServerFnError> {
    let (conn, user) = signed_in().await?;
    let list = kadai::list(&conn, group, user.id, now_ms()).await?;
    blocks::hidden(&conn, Some(user.id), list).await
}

/// A group puzzle's page, `None` for one that isn't there (any more) or of a group the visitor
/// isn't in.
#[server(input = Json, client = HashiClient)]
pub async fn load_group_puzzle(id: i64) -> Result<Option<PuzzlePage>, ServerFnError> {
    use crate::model::ApiError;
    let conn = db::conn()?;
    let Some(user) = auth::current_user(&conn).await? else {
        return Ok(None);
    };
    match kadai::page(&conn, id, user.id, now_ms()).await {
        Ok(page) => blocks::hidden(&conn, Some(user.id), Some(page)).await,
        Err(e)
            if matches!(
                ApiError::from_message(&e.to_string()),
                Some(ApiError::GroupNotFound | ApiError::PuzzleClosed)
            ) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// Starts the clock of a group puzzle (once), the page comes back with the puzzle.
#[server(input = Json, client = HashiClient)]
pub async fn start_group_puzzle(id: i64) -> Result<PuzzlePage, ServerFnError> {
    let (conn, user) = signed_in().await?;
    let page = kadai::start(&conn, id, user.id, now_ms()).await?;
    blocks::hidden(&conn, Some(user.id), page).await
}

/// One correct bridge, +30 s.
#[server(input = Json, client = HashiClient)]
pub async fn group_puzzle_hint(
    id: i64,
    bridges: Vec<u8>,
) -> Result<Option<(usize, u8)>, ServerFnError> {
    let (conn, user) = signed_in().await?;
    kadai::hint(&conn, id, user.id, bridges, now_ms()).await
}

/// Bridges that don't belong to the solution, +10 s.
#[server(input = Json, client = HashiClient)]
pub async fn group_puzzle_check(id: i64, bridges: Vec<u8>) -> Result<Vec<usize>, ServerFnError> {
    let (conn, user) = signed_in().await?;
    kadai::check(&conn, id, user.id, bridges, now_ms()).await
}

/// Gives up: the page comes back with the solution on the board, unranked.
#[server(input = Json, client = HashiClient)]
pub async fn group_puzzle_reveal(id: i64) -> Result<PuzzlePage, ServerFnError> {
    let (conn, user) = signed_in().await?;
    let page = kadai::reveal(&conn, id, user.id, now_ms()).await?;
    blocks::hidden(&conn, Some(user.id), page).await
}

/// The solved board, the moves and the move log: the page comes back with the time and place.
#[server(input = Json, client = HashiClient)]
pub async fn submit_group_puzzle(
    id: i64,
    bridges: Vec<u8>,
    moves: u32,
    log: Vec<MoveEvent>,
) -> Result<PuzzlePage, ServerFnError> {
    let (conn, user) = signed_in().await?;
    let page = kadai::submit(&conn, id, user.id, bridges, moves, log, now_ms()).await?;
    blocks::hidden(&conn, Some(user.id), page).await
}

/// The author takes back an open puzzle.
#[server(input = Json, client = HashiClient)]
pub async fn withdraw_group_puzzle(id: i64) -> Result<(), ServerFnError> {
    let (conn, user) = signed_in().await?;
    kadai::withdraw(&conn, id, user.id, now_ms()).await
}

/// The author has seen the applause of their closed puzzle.
#[server(input = Json, client = HashiClient)]
pub async fn seen_group_puzzle_kudos(id: i64) -> Result<(), ServerFnError> {
    let (conn, user) = signed_in().await?;
    kadai::seen_kudos(&conn, id, user.id, now_ms()).await
}

/// The replay of a group puzzle for the theatre, `None` like [`load_group_puzzle`]. Locked until
/// the viewer is done with it.
#[server(input = Json, client = HashiClient)]
pub async fn load_puzzle_replay(
    id: i64,
) -> Result<Option<crate::model::ReplayData>, ServerFnError> {
    use crate::model::ApiError;
    let conn = db::conn()?;
    let Some(user) = auth::current_user(&conn).await? else {
        return Ok(None);
    };
    match kadai::replay(&conn, id, user.id, now_ms()).await {
        Ok(data) => blocks::hidden(&conn, Some(user.id), Some(data)).await,
        Err(e)
            if matches!(
                ApiError::from_message(&e.to_string()),
                Some(ApiError::GroupNotFound | ApiError::PuzzleClosed)
            ) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
