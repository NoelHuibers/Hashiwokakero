//! Shared puzzles (`/p/<code>`): sharing a puzzle as a link, its page, and a solve of it.

use common::Puzzle;
use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{SharedLink, SharedPuzzlePage, SharedSolve, SharedSubmit};
#[cfg(feature = "ssr")]
use crate::server::{auth, blocks, db, now_ms, shares};

/// Shares a puzzle as a link, for the current player (a guest is made if needed). `solve`: how
/// they solved it on Play, their result on its board; `None` from the workshop. The server proves
/// the puzzle unique itself, see `server::shares::share`.
#[server(input = Json, client = HashiClient)]
pub async fn share_puzzle(
    puzzle: Puzzle,
    solve: Option<SharedSolve>,
) -> Result<SharedLink, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::ensure_user(&conn).await?;
    let code = shares::share(&conn, user.id, puzzle, solve, now_ms()).await?;
    Ok(SharedLink {
        code,
        user: user.info(),
    })
}

/// The page of a link, `None` for a code that isn't one.
#[server(input = Json, client = HashiClient)]
pub async fn load_shared_puzzle(code: String) -> Result<Option<SharedPuzzlePage>, ServerFnError> {
    let conn = db::conn()?;
    let viewer = auth::current_user(&conn).await?.map(|u| u.id);
    let page = shares::page(&conn, &code, viewer).await?;
    blocks::hidden(&conn, viewer, page).await
}

/// A solve of the puzzle of a link, for the current player (a guest is made if needed). Only
/// their first solve of a puzzle counts.
#[server(input = Json, client = HashiClient)]
pub async fn submit_shared_puzzle(
    code: String,
    solve: SharedSolve,
) -> Result<SharedSubmit, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::ensure_user(&conn).await?;
    let (page, counted) = shares::submit(&conn, &code, user.id, &solve, now_ms()).await?;
    Ok(SharedSubmit {
        page: blocks::hidden(&conn, Some(user.id), page).await?,
        counted,
        user: user.info(),
    })
}
