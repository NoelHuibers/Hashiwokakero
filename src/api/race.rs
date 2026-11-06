//! Races (勝負 shōbu): a group's members race each other live on the same fresh puzzle. Opening
//! and joining a lobby, starting it, playing it, and the page's poll.

use common::Difficulty;
use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{GroupRaces, MoveEvent, RaceView, ReplayData, SharedLink};
#[cfg(feature = "ssr")]
use crate::server::{auth, blocks, db, groups::race, now_ms, shares};

#[cfg(feature = "ssr")]
async fn signed_in() -> Result<(libsql::Connection, auth::User), ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn)
        .await?
        .ok_or_else(|| crate::model::ApiError::SessionExpired.fail())?;
    Ok((conn, user))
}

/// Opens a race in a group with the size and difficulty of a preset of Play, the player its
/// host. Returns its id.
#[server(input = Json, client = HashiClient)]
pub async fn open_group_race(group: i64, difficulty: Difficulty) -> Result<i64, ServerFnError> {
    let (conn, user) = signed_in().await?;
    race::open(&conn, group, user.id, difficulty, now_ms()).await
}

/// Opens a race in a group on the shared puzzle of the link `code` (`/p/<code>`), the player its
/// host. Returns its id. The racers' results go on the puzzle's board through that link.
#[server(input = Json, client = HashiClient)]
pub async fn open_shared_race(group: i64, code: String) -> Result<i64, ServerFnError> {
    let (conn, user) = signed_in().await?;
    race::open_shared(&conn, group, user.id, &code, now_ms()).await
}

/// The races of a group page: the open one and the last ones.
#[server(input = Json, client = HashiClient)]
pub async fn load_group_races(group: i64) -> Result<GroupRaces, ServerFnError> {
    let (conn, user) = signed_in().await?;
    let races = race::list(&conn, group, user.id, now_ms()).await?;
    blocks::hidden(&conn, Some(user.id), races).await
}

/// The race's page asks every second or two: the race, `None` if it isn't there (any more) or of
/// a group the visitor isn't in. Notes that the player is still there and takes their `progress`
/// (percent of the solution's bridges on their board, for the others' bars). `puzzle`: the page
/// doesn't have the puzzle yet, it comes once the race started.
///
/// Polled for now (the site runs on more than one machine, the state is in the database). The
/// answer is the whole race in one value ([`RaceView`]), so a live connection (WebSocket or SSE,
/// issue #203) would only change how it gets to the page, not what it carries.
#[server(input = Json, client = HashiClient)]
pub async fn poll_group_race(
    id: i64,
    progress: Option<u8>,
    puzzle: bool,
) -> Result<Option<RaceView>, ServerFnError> {
    let conn = db::conn()?;
    let Some(user) = auth::current_user(&conn).await? else {
        return Ok(None);
    };
    // The names of blocked players are hidden in the same query
    race::poll(&conn, id, user.id, progress, puzzle, now_ms()).await
}

/// Joins a race's lobby.
#[server(input = Json, client = HashiClient)]
pub async fn join_group_race(id: i64) -> Result<RaceView, ServerFnError> {
    let (conn, user) = signed_in().await?;
    race::join(&conn, id, user.id, now_ms()).await
}

/// Leaves a race's lobby; the host calls it off.
#[server(input = Json, client = HashiClient)]
pub async fn leave_group_race(id: i64) -> Result<(), ServerFnError> {
    let (conn, user) = signed_in().await?;
    race::leave(&conn, id, user.id, now_ms()).await
}

/// The host starts the race: the countdown runs, the puzzle comes with the start.
#[server(input = Json, client = HashiClient)]
pub async fn start_group_race(id: i64) -> Result<RaceView, ServerFnError> {
    let (conn, user) = signed_in().await?;
    race::start(&conn, id, user.id, now_ms).await
}

/// One correct bridge, at the price of a hint.
#[server(input = Json, client = HashiClient)]
pub async fn group_race_hint(
    id: i64,
    bridges: Vec<u8>,
) -> Result<Option<(usize, u8)>, ServerFnError> {
    let (conn, user) = signed_in().await?;
    race::hint(&conn, id, user.id, bridges, now_ms()).await
}

/// Bridges that don't belong to the solution, at the price of a check.
#[server(input = Json, client = HashiClient)]
pub async fn group_race_check(id: i64, bridges: Vec<u8>) -> Result<Vec<usize>, ServerFnError> {
    let (conn, user) = signed_in().await?;
    race::check(&conn, id, user.id, bridges, now_ms()).await
}

/// Gives up the race.
#[server(input = Json, client = HashiClient)]
pub async fn give_up_group_race(id: i64) -> Result<RaceView, ServerFnError> {
    let (conn, user) = signed_in().await?;
    race::give_up(&conn, id, user.id, now_ms()).await
}

/// The solved board, the moves and the move log: the race comes back with the time and place.
#[server(input = Json, client = HashiClient)]
pub async fn submit_group_race(
    id: i64,
    bridges: Vec<u8>,
    moves: u32,
    log: Vec<MoveEvent>,
) -> Result<RaceView, ServerFnError> {
    let (conn, user) = signed_in().await?;
    let finished = race::finish(&conn, id, user.id, bridges, moves, log, now_ms()).await?;
    // Through someone's link (a race on a shared puzzle): their sharing stamps
    if finished.reached.is_some() {
        shares::reached(finished.reached, user.id);
    }
    Ok(finished.view)
}

/// Shares the puzzle of a race that is over as a link of the player's (`/p/<code>`). The first
/// link puts the racers' results on its board as the times to beat.
#[server(input = Json, client = HashiClient)]
pub async fn share_group_race(id: i64) -> Result<SharedLink, ServerFnError> {
    let (conn, user) = signed_in().await?;
    let code = race::share(&conn, id, user.id, now_ms()).await?;
    Ok(SharedLink {
        code,
        user: user.info(),
    })
}

/// A new lobby with the settings of a race that is over, or the one opened for it already:
/// joined either way. Returns its id.
#[server(input = Json, client = HashiClient)]
pub async fn rematch_group_race(id: i64) -> Result<i64, ServerFnError> {
    let (conn, user) = signed_in().await?;
    race::rematch(&conn, id, user.id, now_ms()).await
}

/// Everyone's moves of a race that is over, for the replay theatre. Locked before.
#[server(input = Json, client = HashiClient)]
pub async fn load_race_replay(id: i64) -> Result<ReplayData, ServerFnError> {
    let (conn, user) = signed_in().await?;
    let data = race::replay(&conn, id, user.id, now_ms()).await?;
    blocks::hidden(&conn, Some(user.id), data).await
}
