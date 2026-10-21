//! Async actions: new puzzles, the SAT solver, hints, checks and revealing the solution.

use std::time::Duration;

use common::{GenerateParams, Puzzle, SolveOutcome, SolveReport, SolverConfig};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::game::{Game, Source, Status};
use super::settings::Settings;
use super::toasts::{ToastKind, error_toast, toast};
use crate::api::{generate_puzzle, solve_puzzle};
use crate::i18n::td_string;

pub fn new_puzzle(game: Game, params: GenerateParams, source: Source) {
    if game.loading.get_untracked() {
        return;
    }
    let epoch = game.epoch.get_untracked();
    game.loading.set(true);
    spawn_local(async move {
        let generated = generate_puzzle(params).await;
        if game.epoch.try_get_untracked() != Some(epoch) {
            return;
        }
        match generated {
            Ok(generated) => game.set_puzzle(
                generated.puzzle,
                Some(generated.solution),
                Some(generated.unique),
                source,
            ),
            Err(e) => error_toast(e),
        }
        game.loading.set(false);
    });
}

/// Runs the SAT solver and stores its report. Returns the solution if one was found.
pub async fn run_solver(
    puzzle: Puzzle,
    config: SolverConfig,
    unique: bool,
    report: RwSignal<Option<SolveReport>>,
    busy: RwSignal<bool>,
    epoch: RwSignal<u32>,
) -> Option<Vec<u8>> {
    let request = epoch.get_untracked();
    busy.set(true);
    let result = solve_puzzle(puzzle, config, unique).await;
    // Do this before touching report, busy or toasts. The caller's check after this function
    // returned was too late to stop an old result overwriting a replacement puzzle.
    if !current_request(epoch, request) {
        return None;
    }
    busy.set(false);
    match result {
        Ok(r) => {
            let solution = match &r.outcome {
                SolveOutcome::Solved { bridges, .. } => Some(bridges.clone()),
                SolveOutcome::Unsatisfiable => {
                    toast(ToastKind::Error, |l| {
                        td_string!(l, common.toast.unsat).to_string()
                    });
                    None
                }
                SolveOutcome::Timeout => {
                    toast(ToastKind::Error, |l| {
                        td_string!(l, common.toast.timeout).to_string()
                    });
                    None
                }
            };
            report.set(Some(r));
            solution
        }
        Err(e) => {
            error_toast(e);
            None
        }
    }
}

async fn game_solver(game: &Game, settings: Settings, unique: bool) -> Option<Vec<u8>> {
    let epoch = game.epoch.get_untracked();
    let solution = run_solver(
        game.puzzle(),
        settings.solver.get_untracked(),
        unique,
        game.report,
        game.solving,
        game.epoch,
    )
    .await;
    if game.epoch.try_get_untracked() != Some(epoch) {
        return None;
    }
    if let Some(SolveOutcome::Solved {
        unique: Some(u), ..
    }) = game
        .report
        .with_untracked(|r| r.as_ref().map(|r| r.outcome.clone()))
    {
        game.unique.set(Some(u));
    }
    if game.solution.get_untracked().is_none() {
        game.solution.set(solution.clone());
    }
    solution
}

async fn ensure_solution(game: &Game, settings: Settings) -> Option<Vec<u8>> {
    match game.solution.get_untracked() {
        Some(s) => Some(s),
        None => game_solver(game, settings, false).await,
    }
}

/// Places (or corrects) one bridge of the solution.
pub fn hint(game: Game, settings: Settings) {
    if !game.is_playing() {
        return;
    }
    spawn_local(async move {
        let epoch = game.epoch.get_untracked();
        let Some(solution) = ensure_solution(&game, settings).await else {
            return;
        };
        if game.epoch.try_get_untracked() != Some(epoch) || !game.is_playing() {
            return;
        }
        let current = game.bridges.get_untracked();
        let target = game
            .board
            .with_untracked(|b| crate::model::pick_hint(b, &current, &solution))
            .map(|(edge, _)| edge);
        match target {
            Some(edge) => {
                game.set_edges(&[(edge, solution[edge])]);
                game.hints.update(|h| *h += 1);
                game.hint.set(Some(edge));
                set_timeout(
                    move || {
                        if game.hint.get_untracked() == Some(edge) {
                            game.hint.set(None);
                        }
                    },
                    Duration::from_millis(3000),
                );
            }
            None => toast(ToastKind::Info, |l| {
                td_string!(l, common.toast.nothing_to_hint).to_string()
            }),
        }
    });
}

/// Flashes bridges that are not part of the solution.
pub fn check(game: Game, settings: Settings) {
    spawn_local(async move {
        let epoch = game.epoch.get_untracked();
        let Some(solution) = ensure_solution(&game, settings).await else {
            return;
        };
        if game.epoch.try_get_untracked() != Some(epoch) {
            return;
        }
        let wrong = game
            .bridges
            .with_untracked(|b| crate::model::wrong_edges(b, &solution));
        if wrong.is_empty() {
            crate::audio::play(crate::audio::Sfx::Correct);
            toast(ToastKind::Success, |l| {
                td_string!(l, common.toast.all_correct).to_string()
            });
        } else {
            let count = wrong.len();
            toast(ToastKind::Error, move |l| {
                td_string!(l, common.toast.wrong_bridges, count)
            });
            game.wrong.set(wrong);
            set_timeout(move || game.wrong.set(vec![]), Duration::from_millis(2400));
        }
    });
}

/// Solves the puzzle with the configured SAT solver and builds the solution bridge by bridge.
pub fn reveal(game: Game, settings: Settings) {
    spawn_local(async move {
        let epoch = game.epoch.get_untracked();
        let Some(solution) = game_solver(&game, settings, false).await else {
            return;
        };
        if game.epoch.try_get_untracked() != Some(epoch) {
            return;
        }
        if game.status.get_untracked() == Status::Playing {
            game.status.set(Status::Revealed);
        }
        let plan = game
            .bridges
            .with_untracked(|current| RevealPlan::new(current, &solution));
        game.bridges.set(plan.torn_down);
        let step = plan.step_ms;
        for (k, edge) in plan.missing.into_iter().enumerate() {
            let value = solution[edge];
            set_timeout(
                move || {
                    if game.epoch.try_get_untracked() == Some(epoch) {
                        game.bridges.update(|b| b[edge] = value);
                    }
                },
                Duration::from_millis(step * k as u64),
            );
        }
    });
}

/// How [`reveal`] gets from the player's bridges to the solution: wrong bridges are torn down at
/// once, then the missing ones are built one after another, `step_ms` apart.
#[derive(Debug, PartialEq, Eq)]
struct RevealPlan {
    /// The bridges with every one too many taken away.
    torn_down: Vec<u8>,
    /// The edges still short of the solution, in order.
    missing: Vec<usize>,
    step_ms: u64,
}

impl RevealPlan {
    fn new(current: &[u8], solution: &[u8]) -> Self {
        let torn_down = current
            .iter()
            .enumerate()
            .map(|(e, &have)| have.min(solution[e]))
            .collect();
        let missing: Vec<usize> = (0..current.len())
            .filter(|&e| current[e] < solution[e])
            .collect();
        // About 2.4 s for the whole board, but neither a blur nor a crawl
        let step_ms = (2400 / missing.len().max(1) as u64).clamp(12, 90);
        Self {
            torn_down,
            missing,
            step_ms,
        }
    }
}

/// Asks the solver whether the solution is unique.
pub fn check_unique(game: Game, settings: Settings) {
    spawn_local(async move {
        if game_solver(&game, settings, true).await.is_some() {
            match game.unique.get_untracked() {
                Some(true) => toast(ToastKind::Success, |l| {
                    td_string!(l, common.toast.unique).to_string()
                }),
                Some(false) => toast(ToastKind::Info, |l| {
                    td_string!(l, common.toast.not_unique).to_string()
                }),
                None => {}
            }
        }
    });
}

fn current_request(epoch: RwSignal<u32>, request: u32) -> bool {
    epoch.try_get_untracked() == Some(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reveal_tears_down_wrong_bridges_and_builds_the_missing_ones() {
        let plan = RevealPlan::new(&[2, 0, 1, 1, 0], &[1, 2, 1, 0, 1]);
        assert_eq!(plan.torn_down, [1, 0, 1, 0, 0]);
        assert_eq!(plan.missing, [1, 4]);
    }

    #[test]
    fn a_solved_board_has_nothing_to_build() {
        let plan = RevealPlan::new(&[1, 2, 0], &[1, 2, 0]);
        assert_eq!(plan.torn_down, [1, 2, 0]);
        assert_eq!(plan.missing, Vec::<usize>::new());
    }

    /// Building the solution takes about 2.4 s, but on a big board no bridge comes faster
    /// than every 12 ms, and on a small one none slower than every 90 ms.
    #[test]
    fn the_solution_is_built_in_about_two_seconds() {
        let step = |n: usize| RevealPlan::new(&vec![0; n], &vec![1; n]).step_ms;
        assert_eq!(step(1), 90);
        assert_eq!(step(26), 90);
        assert_eq!(step(30), 80);
        assert_eq!(step(100), 24);
        assert_eq!(step(200), 12);
        assert_eq!(step(1000), 12);
    }

    #[test]
    fn obsolete_or_disposed_requests_cannot_publish() {
        Owner::new().with(|| {
            let epoch = RwSignal::new(0);
            assert!(current_request(epoch, 0));
            epoch.set(1);
            assert!(!current_request(epoch, 0));
            assert!(current_request(epoch, 1));
            epoch.dispose();
            assert!(!current_request(epoch, 1));
        });
    }
}
