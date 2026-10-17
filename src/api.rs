//! Server functions. Solving and generating run on the server, in the backend crate.

use common::{GenerateParams, GeneratedPuzzle, Puzzle, SolveReport, SolverConfig};
use leptos::prelude::*;
use leptos::server_fn::codec::Json;

/// Upper bound for a single solver run, whatever the client asks for.
#[cfg(feature = "ssr")]
const MAX_TIMEOUT_MS: u32 = 20_000;

/// Runs CPU bound backend work off the async runtime. The fly.io VM has a single shared CPU,
/// so only a couple of jobs run at the same time and the rest queue up.
#[cfg(feature = "ssr")]
async fn run_blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, ServerFnError> {
    use tokio::sync::Semaphore;
    static PERMITS: Semaphore = Semaphore::const_new(2);

    let _permit = PERMITS.acquire().await.map_err(ServerFnError::new)?;
    tokio::task::spawn_blocking(job)
        .await
        .map_err(ServerFnError::new)?
        .map_err(ServerFnError::new)
}

/// Deserializing skips `Puzzle::new`, which keeps the islands in reading order.
#[cfg(feature = "ssr")]
fn normalized(puzzle: Puzzle) -> Puzzle {
    Puzzle::new(puzzle.width, puzzle.height, puzzle.islands)
}

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
