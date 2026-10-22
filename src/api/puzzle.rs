//! Puzzles and solvers: generating, solving and the CNF encoding.

use common::{GenerateParams, GeneratedPuzzle, Puzzle, SolveReport, SolverConfig};
use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;

#[cfg(feature = "ssr")]
use crate::server::{auth, daily, db, run_blocking};

/// Refuses to solve today's daily for anyone who hasn't finished it: the leaderboard is about
/// solving it yourself. The same for an open puzzle of one of the player's groups.
#[cfg(feature = "ssr")]
async fn not_the_daily(puzzle: &Puzzle) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn).await?.map(|u| u.id);
    if daily::locked(&conn, user, puzzle).await? {
        return Err(crate::model::ApiError::DailyLocked.fail());
    }
    if crate::server::groups::kadai::locked(&conn, user, puzzle, crate::server::now_ms()).await? {
        return Err(crate::model::ApiError::PuzzleLocked.fail());
    }
    Ok(())
}

/// The puzzle in reading order, see `backend::api::player_puzzle`.
#[cfg(feature = "ssr")]
fn normalized(puzzle: Puzzle) -> Result<Puzzle, ServerFnError> {
    backend::api::player_puzzle(puzzle).map_err(|_| crate::model::ApiError::WrongPuzzle.fail())
}

/// Generates a new puzzle with a unique solution.
#[server(input = Json, client = HashiClient)]
pub async fn generate_puzzle(params: GenerateParams) -> Result<GeneratedPuzzle, ServerFnError> {
    run_blocking(move || Ok(backend::api::generate(&params))).await
}

/// Encodes the puzzle as CNF and solves it with the chosen SAT solver.
/// With `unique` the solver additionally checks whether the solution is the only one.
#[server(input = Json, client = HashiClient)]
pub async fn solve_puzzle(
    puzzle: Puzzle,
    config: SolverConfig,
    unique: bool,
) -> Result<SolveReport, ServerFnError> {
    let puzzle = normalized(puzzle)?;
    not_the_daily(&puzzle).await?;
    // The same limits as the app's offline practice (app/src-tauri/offline)
    let config = backend::api::player_config(config);
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
#[server(input = Json, client = HashiClient)]
pub async fn puzzle_cnf(puzzle: Puzzle) -> Result<String, ServerFnError> {
    let puzzle = normalized(puzzle)?;
    not_the_daily(&puzzle).await?;
    run_blocking(move || backend::api::to_dimacs(&puzzle)).await
}
