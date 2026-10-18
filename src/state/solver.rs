//! Async actions: new puzzles, the SAT solver, hints, checks and revealing the solution.

use std::time::Duration;

use common::{GenerateParams, Puzzle, SolveOutcome, SolveReport, SolverConfig};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::game::{Game, Source, Status};
use super::settings::Settings;
use super::toasts::{ToastKind, toast};
use crate::api::{generate_puzzle, solve_puzzle};
use crate::i18n::td_string;

pub fn new_puzzle(game: Game, params: GenerateParams, source: Source) {
    if game.loading.get_untracked() {
        return;
    }
    game.loading.set(true);
    spawn_local(async move {
        match generate_puzzle(params).await {
            Ok(generated) => game.set_puzzle(
                generated.puzzle,
                Some(generated.solution),
                Some(generated.unique),
                source,
            ),
            Err(e) => toast(ToastKind::Error, move |l| crate::lang::error_text(l, &e)),
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
) -> Option<Vec<u8>> {
    busy.set(true);
    let result = solve_puzzle(puzzle, config, unique).await;
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
            toast(ToastKind::Error, move |l| crate::lang::error_text(l, &e));
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
    )
    .await;
    if game.epoch.get_untracked() != epoch {
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
        if game.epoch.get_untracked() != epoch || !game.is_playing() {
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
        if game.epoch.get_untracked() != epoch {
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
        if game.epoch.get_untracked() != epoch {
            return;
        }
        if game.status.get_untracked() == Status::Playing {
            game.status.set(Status::Revealed);
        }
        let current = game.bridges.get_untracked();
        // Tear down wrong bridges at once, then build the missing ones one after another
        game.bridges.update(|b| {
            for (e, slot) in b.iter_mut().enumerate() {
                if *slot > solution[e] {
                    *slot = solution[e];
                }
            }
        });
        let missing: Vec<usize> = (0..current.len())
            .filter(|&e| current[e] < solution[e])
            .collect();
        let step = (2400 / missing.len().max(1) as u64).clamp(12, 90);
        for (k, edge) in missing.into_iter().enumerate() {
            let value = solution[edge];
            set_timeout(
                move || {
                    if game.epoch.get_untracked() == epoch {
                        game.bridges.update(|b| b[edge] = value);
                    }
                },
                Duration::from_millis(step * k as u64),
            );
        }
    });
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
