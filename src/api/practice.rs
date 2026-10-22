//! Generating and solving for practice, the editor and the lab, in the app on the phone itself.
//!
//! In the app the phone computes, without the network (the offline practice, `native`); on the
//! website the server functions of `puzzle.rs` do. The app's commands run the same `backend` functions with the same
//! limits, so the answers are the same: a seed gives the same puzzle either way.
//!
//! The server refuses to solve today's daily for players who haven't finished it
//! (`not_the_daily`). The phone checks the daily this device saved for today
//! ([`daily_locked`]), as far as it can. It can't enforce it: a player who never opened today's
//! daily on the phone, or who runs any other solver, gets the daily solved. On the website the
//! same is true of every solver outside it. The group puzzles' lock isn't repeated on the phone:
//! it doesn't know which of them are still open.

use common::{GenerateParams, GeneratedPuzzle, Puzzle, SolveReport, SolverConfig};
use leptos::prelude::*;
#[cfg(any(feature = "csr", test))]
use leptos::server_fn::ServerFn;

#[cfg(any(feature = "csr", test))]
use super::{GeneratePuzzle, PuzzleCnf, SolvePuzzle};
use super::{generate_puzzle, puzzle_cnf, solve_puzzle};
#[cfg(any(feature = "csr", test))]
use crate::model::{ApiError, canonical, date_string};
#[cfg(any(feature = "csr", test))]
use crate::state::{Source, Status};

/// Generates a new puzzle with a unique solution, see `generate_puzzle`.
pub async fn generate(params: GenerateParams) -> Result<GeneratedPuzzle, ServerFnError> {
    #[cfg(feature = "csr")]
    if route(super::native::available(), None, None, 0)? == Route::Phone {
        return super::native::call(command::<GeneratePuzzle>(), &GeneratePuzzle { params }).await;
    }
    generate_puzzle(params).await
}

/// Solves the puzzle with the chosen SAT solver, see `solve_puzzle`.
pub async fn solve(
    puzzle: Puzzle,
    config: SolverConfig,
    unique: bool,
) -> Result<SolveReport, ServerFnError> {
    #[cfg(feature = "csr")]
    if on_the_phone(&puzzle)? {
        let args = SolvePuzzle {
            puzzle,
            config,
            unique,
        };
        return super::native::call(command::<SolvePuzzle>(), &args).await;
    }
    solve_puzzle(puzzle, config, unique).await
}

/// The CNF encoding of the puzzle in DIMACS format, see `puzzle_cnf`.
pub async fn cnf(puzzle: Puzzle) -> Result<String, ServerFnError> {
    #[cfg(feature = "csr")]
    if on_the_phone(&puzzle)? {
        return super::native::call(command::<PuzzleCnf>(), &PuzzleCnf { puzzle }).await;
    }
    puzzle_cnf(puzzle).await
}

/// Where a puzzle is solved: on the phone, unless it's today's unfinished daily.
#[cfg(feature = "csr")]
fn on_the_phone(puzzle: &Puzzle) -> Result<bool, ServerFnError> {
    let native = super::native::available();
    // Only read when it matters
    let daily = native.then(crate::state::saved_daily).flatten();
    #[expect(
        clippy::cast_possible_truncation,
        reason = "milliseconds since 1970 fit an i64"
    )]
    let today = crate::model::day_of(js_sys::Date::now() as i64);
    Ok(route(native, Some(puzzle), daily.as_ref(), today)? == Route::Phone)
}

#[cfg(any(feature = "csr", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// The app's commands.
    Phone,
    /// The server functions.
    Server,
}

/// Where a computation runs: on the phone when the page runs in the app (`native`, Tauri's
/// commands are there), else on the server, which checks the daily itself. An error for
/// solving a `puzzle` that is [`daily_locked`], given the `daily` this device saved and `today`.
#[cfg(any(feature = "csr", test))]
fn route(
    native: bool,
    puzzle: Option<&Puzzle>,
    daily: Option<&(Puzzle, Source, Status)>,
    today: i64,
) -> Result<Route, ServerFnError> {
    if !native {
        return Ok(Route::Server);
    }
    if puzzle.is_some_and(|p| daily_locked(p, daily, today)) {
        return Err(ApiError::DailyLocked.fail());
    }
    Ok(Route::Phone)
}

/// Whether `puzzle` is today's daily, however it is turned, and the daily this device saved
/// (`hashi.daily.v1`) isn't finished, as the server's `daily::locked` decides for the player.
/// Without a saved daily of `today` the phone doesn't know today's daily: not locked.
#[cfg(any(feature = "csr", test))]
fn daily_locked(puzzle: &Puzzle, daily: Option<&(Puzzle, Source, Status)>, today: i64) -> bool {
    let Some((saved, Source::Daily { date, .. }, status)) = daily else {
        return false;
    };
    *date == date_string(today)
        && !matches!(status, Status::Won | Status::Revealed)
        && canonical(saved) == canonical(puzzle)
}

/// The app's command for the server function `F`: the same name.
#[cfg(any(feature = "csr", test))]
fn command<F: ServerFn>() -> &'static str {
    F::PATH.strip_prefix("/api/").unwrap_or(F::PATH)
}

#[cfg(test)]
mod tests {
    use common::{Difficulty, Island, Pos};

    use super::*;

    const TODAY: i64 = 20_000;

    fn puzzle() -> Puzzle {
        let island = |x, y, required| Island {
            pos: Pos { x, y },
            required,
        };
        Puzzle::new(
            4,
            3,
            vec![
                island(0, 0, 2),
                island(3, 0, 3),
                island(3, 2, 1),
                island(0, 2, 2),
            ],
        )
    }

    fn daily(date: i64, status: Status) -> (Puzzle, Source, Status) {
        let params = GenerateParams {
            width: 4,
            height: 3,
            difficulty: Difficulty::Easy,
            seed: 1,
        };
        let source = Source::Daily {
            date: date_string(date),
            params,
        };
        (puzzle(), source, status)
    }

    fn locked(puzzle: &Puzzle, daily: &(Puzzle, Source, Status)) -> bool {
        daily_locked(puzzle, Some(daily), TODAY)
    }

    /// Today's daily saved on this device and not finished: not on the phone, however the
    /// puzzle is turned. Once won or revealed, or on another day, it is.
    #[test]
    fn the_unfinished_daily_of_today_is_locked() {
        let turned = {
            let p = puzzle();
            let islands = p
                .islands
                .iter()
                .map(|i| Island {
                    pos: Pos {
                        x: p.width - 1 - i.pos.x,
                        y: i.pos.y,
                    },
                    required: i.required,
                })
                .collect();
            Puzzle::new(p.width, p.height, islands)
        };
        assert_ne!(turned, puzzle());
        for status in [Status::Playing, Status::Empty] {
            assert!(locked(&puzzle(), &daily(TODAY, status)));
            assert!(locked(&turned, &daily(TODAY, status)));
        }
        for status in [Status::Won, Status::Revealed] {
            assert!(!locked(&puzzle(), &daily(TODAY, status)));
        }
        assert!(!locked(&puzzle(), &daily(TODAY - 1, Status::Playing)));
        let mut other = puzzle();
        other.islands[0].required = 1;
        assert!(!locked(&other, &daily(TODAY, Status::Playing)));
        // A practice game isn't a daily, nor is nothing saved
        let practice = (puzzle(), Source::Custom, Status::Playing);
        assert!(!locked(&puzzle(), &practice));
        assert!(!daily_locked(&puzzle(), None, TODAY));
    }

    /// The website's build, or the app's bundle in a browser without Tauri: the server, which
    /// checks the daily itself. In the app the phone, except for the locked daily, which gets
    /// the server's error.
    #[test]
    fn the_phone_computes_in_the_app_only() {
        let saved = daily(TODAY, Status::Playing);
        let (today, mut other) = (puzzle(), puzzle());
        other.islands[0].required = 1;
        let route = |native, puzzle| route(native, puzzle, Some(&saved), TODAY);
        assert_eq!(route(false, None).unwrap(), Route::Server);
        assert_eq!(route(false, Some(&today)).unwrap(), Route::Server);
        assert_eq!(route(true, None).unwrap(), Route::Phone);
        let error = route(true, Some(&today)).unwrap_err();
        assert_eq!(
            ApiError::from_message(&error.to_string()),
            Some(ApiError::DailyLocked)
        );
        assert_eq!(route(true, Some(&other)).unwrap(), Route::Phone);
    }

    /// The commands of `app/src-tauri` and the server functions have the same names, and the
    /// commands take the server functions' request bodies (`golden/files/requests.txt`, which
    /// the tests of `app/src-tauri/offline` feed to them).
    #[test]
    fn the_commands_are_the_server_functions() {
        assert_eq!(command::<GeneratePuzzle>(), "generate_puzzle");
        assert_eq!(command::<SolvePuzzle>(), "solve_puzzle");
        assert_eq!(command::<PuzzleCnf>(), "puzzle_cnf");
        let requests = include_str!("../golden/files/requests.txt");
        let body = |command: &str| {
            let path = format!("/api/{command} ");
            requests
                .lines()
                .find_map(|l| l.strip_prefix(&path))
                .unwrap()
                .to_string()
        };
        let generate: GeneratePuzzle = serde_json::from_str(&body("generate_puzzle")).unwrap();
        assert_eq!(
            serde_json::to_string(&generate).unwrap(),
            body("generate_puzzle")
        );
        let solve: SolvePuzzle = serde_json::from_str(&body("solve_puzzle")).unwrap();
        assert_eq!(serde_json::to_string(&solve).unwrap(), body("solve_puzzle"));
        let cnf: PuzzleCnf = serde_json::from_str(&body("puzzle_cnf")).unwrap();
        assert_eq!(serde_json::to_string(&cnf).unwrap(), body("puzzle_cnf"));
    }
}
