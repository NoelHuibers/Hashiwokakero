//! Server functions. Solving and generating run on the server, in the backend crate; users and
//! daily results live in the database (see `server`).

use common::{GenerateParams, GeneratedPuzzle, Puzzle, SolveReport, SolverConfig};
use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use crate::model::{
    AllTimeEntry, DailyInfo, DojoBoard, DojoMe, DojoSubmission, Leaderboard, SubmitResult, UserInfo, UserStats,
};
#[cfg(feature = "ssr")]
use crate::server::{auth, daily, db, dojo, run_blocking};

/// Upper bound for a single solver run, whatever the client asks for.
#[cfg(feature = "ssr")]
const MAX_TIMEOUT_MS: u32 = 20_000;

/// Deserializing skips `Puzzle::new`, which keeps the islands in reading order.
#[cfg(feature = "ssr")]
fn normalized(puzzle: Puzzle) -> Puzzle {
    Puzzle::new(puzzle.width, puzzle.height, puzzle.islands)
}

/* ------------------------------------------------------------------ */
/*  Puzzles and solvers                                                */
/* ------------------------------------------------------------------ */

/// Generates a new puzzle with a unique solution.
#[server(input = Json)]
pub async fn generate_puzzle(params: GenerateParams) -> Result<GeneratedPuzzle, ServerFnError> {
    run_blocking(move || Ok(backend::api::generate(&params))).await
}

/// Encodes the puzzle as CNF and solves it with the chosen SAT solver.
/// With `unique` the solver additionally checks whether the solution is the only one.
#[server(input = Json)]
pub async fn solve_puzzle(
    puzzle: Puzzle,
    config: SolverConfig,
    unique: bool,
) -> Result<SolveReport, ServerFnError> {
    let puzzle = normalized(puzzle);
    let config = SolverConfig {
        timeout_ms: config.timeout_ms.clamp(100, MAX_TIMEOUT_MS),
        ..config
    };
    run_blocking(move || {
        if unique {
            backend::api::solve_unique(&puzzle, &config)
        } else {
            backend::api::solve(&puzzle, &config)
        }
    })
    .await
}

/// The CNF encoding of the puzzle in DIMACS format.
#[server(input = Json)]
pub async fn puzzle_cnf(puzzle: Puzzle) -> Result<String, ServerFnError> {
    let puzzle = normalized(puzzle);
    run_blocking(move || backend::api::to_dimacs(&puzzle)).await
}

/* ------------------------------------------------------------------ */
/*  Users                                                              */
/* ------------------------------------------------------------------ */

#[server(input = Json)]
pub async fn current_user() -> Result<Option<UserInfo>, ServerFnError> {
    let conn = db::conn()?;
    Ok(auth::current_user(&conn).await?.map(|u| u.info()))
}

/// Creates an account. A guest keeps their results and streak.
#[server(input = Json)]
pub async fn register(username: String, password: String) -> Result<UserInfo, ServerFnError> {
    let conn = db::conn()?;
    Ok(auth::register(&conn, username.trim(), password).await?.info())
}

#[server(input = Json)]
pub async fn login(username: String, password: String) -> Result<UserInfo, ServerFnError> {
    let conn = db::conn()?;
    Ok(auth::login(&conn, username.trim(), password).await?.info())
}

#[server(input = Json)]
pub async fn logout() -> Result<(), ServerFnError> {
    auth::logout(&db::conn()?).await
}

/// Statistics of the current user, `None` without a session.
#[server(input = Json)]
pub async fn my_stats() -> Result<Option<UserStats>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => Ok(Some(daily::stats(&conn, user.id).await?)),
        None => Ok(None),
    }
}

/* ------------------------------------------------------------------ */
/*  Daily puzzle                                                       */
/* ------------------------------------------------------------------ */

/// Today's daily, with the puzzle only if the current user already started it.
#[server(input = Json)]
pub async fn daily_today() -> Result<DailyInfo, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn).await?;
    daily::info(&conn, user.map(|u| u.id), daily::today()).await
}

/// Starts today's daily for the current user (a guest is created if needed).
#[server(input = Json)]
pub async fn start_daily() -> Result<DailyInfo, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::ensure_user(&conn).await?;
    daily::start(&conn, user.id, daily::today()).await
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
pub async fn submit_daily(day: i64, bridges: Vec<u8>, moves: u32) -> Result<SubmitResult, ServerFnError> {
    let (conn, user) = signed_in().await?;
    daily::submit(&conn, user.id, day, bridges, moves).await
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

/* ------------------------------------------------------------------ */
/*  SAT Dojo                                                           */
/* ------------------------------------------------------------------ */

/// The leaderboard of the current season.
#[server(input = Json)]
pub async fn load_dojo_board() -> Result<DojoBoard, ServerFnError> {
    dojo::board(&db::conn()?).await
}

/// One submission with its score, `None` if there is no such submission.
#[server(input = Json)]
pub async fn load_dojo_submission(id: i64) -> Result<Option<DojoSubmission>, ServerFnError> {
    let conn = db::conn()?;
    let me = auth::current_user(&conn).await?.map(|u| u.id);
    dojo::submission(&conn, id, me).await
}

/// The current user's submissions and how many they can still make today.
#[server(input = Json)]
pub async fn load_dojo_me() -> Result<DojoMe, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn).await?;
    dojo::me(&conn, user.as_ref()).await
}
