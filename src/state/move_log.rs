//! The move log of a timed game against the server's clock (a group puzzle, a race), kept in the
//! browser so a reload doesn't lose it, and sent with the solved board (`model::verify_log`).

use leptos::prelude::*;

use super::{Game, load, store};
use crate::model::MoveEvent;

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SavedLog {
    puzzle: i64,
    events: Vec<MoveEvent>,
}

/// The last board the log saw, with the puzzle and game it belongs to, so loading another one
/// is never logged as moves.
#[derive(Clone, Debug)]
struct Snapshot {
    puzzle: Option<i64>,
    epoch: u32,
    bridges: Vec<u8>,
    moves: u32,
}

/// The log of the game played on a page, under the local storage `key`, for one puzzle (by its id)
/// at a time.
#[derive(Clone, Copy, Debug)]
pub struct MoveLog {
    key: &'static str,
    logged: StoredValue<Option<Snapshot>>,
}

impl MoveLog {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            logged: StoredValue::new(None),
        }
    }

    /// The saved log of `puzzle`.
    pub fn events(&self, puzzle: Option<i64>) -> Vec<MoveEvent> {
        load::<SavedLog>(self.key)
            .filter(|l| Some(l.puzzle) == puzzle)
            .map(|l| l.events)
            .unwrap_or_default()
    }

    /// Logs the bridge changes of `game` since the last call, while the clock of `puzzle` runs
    /// (`running`), at `t` ms on its clock: a click changes one bridge and counts a move, hints,
    /// undo and restart don't. Tracks the board and its epoch, call it in an effect.
    pub fn record(&self, game: &Game, puzzle: Option<i64>, running: bool, t: impl FnOnce() -> u32) {
        let current = Snapshot {
            puzzle,
            epoch: game.epoch.get(),
            bridges: game.bridges.get(),
            moves: game.moves.get_untracked(),
        };
        let previous = self.logged.get_value();
        self.logged.set_value(Some(current.clone()));
        let (Some(before), Some(puzzle)) = (previous, current.puzzle) else {
            return;
        };
        if before.puzzle != current.puzzle
            || before.epoch != current.epoch
            || before.bridges.len() != current.bridges.len()
            || !running
        {
            return;
        }
        let changed: Vec<usize> = (0..current.bridges.len())
            .filter(|&e| before.bridges[e] != current.bridges[e])
            .collect();
        if changed.is_empty() {
            return;
        }
        let tap = changed.len() == 1 && current.moves > before.moves;
        let t = untrack(t);
        let mut saved = load::<SavedLog>(self.key)
            .filter(|l| l.puzzle == puzzle)
            .unwrap_or_default();
        saved.puzzle = puzzle;
        for edge in changed {
            saved.events.push(MoveEvent {
                t,
                edge: u16::try_from(edge).unwrap_or(u16::MAX),
                count: current.bridges[edge],
                tap,
            });
        }
        store(self.key, &saved);
    }
}
