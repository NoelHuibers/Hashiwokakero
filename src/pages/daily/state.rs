//! The state of today's daily on the page: the server's puzzle info, the clocks and the ranking.

use common::{GenerateParams, Puzzle};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{daily_leaderboard, daily_today};
use crate::lang::error_text;
use crate::model::{
    DailyInfo, Flag, Leaderboard, MoveEvent, SubmitResult, daily_difficulty, date_string,
};
use crate::state::{Game, Source, Status, ToastKind, toast};

/// The move log of the running daily, kept in the browser so a reload doesn't lose it.
pub(super) const LOG_KEY: &str = "hashi.daily.log";

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub(super) struct SavedLog {
    pub(super) day: i64,
    pub(super) events: Vec<MoveEvent>,
}

fn params_for(day: i64) -> GenerateParams {
    let difficulty = daily_difficulty(day);
    let (width, height) = difficulty.size();
    GenerateParams {
        width,
        height,
        difficulty,
        seed: 0,
    }
}

/// The browser clock, unix time in ms.
#[expect(
    clippy::cast_possible_truncation,
    reason = "`Date.now()` is a whole number of ms, far inside the i64 range"
)]
pub(super) fn browser_now() -> i64 {
    js_sys::Date::now() as i64
}

#[derive(Clone, Copy, Debug)]
pub struct Daily {
    pub(super) game: Game,
    pub(super) info: RwSignal<Option<DailyInfo>>,
    /// Server clock minus client clock.
    pub(super) skew: RwSignal<i64>,
    /// Client clock, ticking while the page is open.
    pub(super) now: RwSignal<i64>,
    pub(super) result: RwSignal<Option<SubmitResult>>,
    pub(super) ranking: RwSignal<Option<Leaderboard>>,
    pub(super) busy: RwSignal<bool>,
}

impl Daily {
    pub(super) fn server_now(&self) -> i64 {
        self.now.get() + self.skew.get()
    }

    pub(super) fn day(&self) -> Option<i64> {
        self.info.with_untracked(|i| i.as_ref().map(|i| i.day))
    }

    pub(super) fn started(&self) -> bool {
        self.info
            .with(|i| i.as_ref().is_some_and(|i| i.attempt.is_some()))
    }

    pub(super) fn finished(&self) -> bool {
        self.info.with(|i| {
            i.as_ref()
                .and_then(|i| i.attempt.as_ref())
                .is_some_and(crate::model::AttemptInfo::finished)
        })
    }

    /// Milliseconds since the start on the server clock, without penalties: the move log's time.
    pub(super) fn clock_ms(&self) -> u32 {
        let now = self.server_now();
        let started = self.info.with_untracked(|i| {
            i.as_ref()
                .and_then(|i| i.attempt.as_ref())
                .map(|a| a.started_at)
        });
        started.map_or(0, |s| u32::try_from((now - s).max(0)).unwrap_or(u32::MAX))
    }

    /// Why the result isn't ranked, if it isn't.
    pub(super) fn flag(&self) -> Option<Flag> {
        self.result
            .with(|r| r.as_ref().and_then(|r| r.flag))
            .or_else(|| {
                self.info.with(|i| {
                    i.as_ref()
                        .and_then(|i| i.attempt.as_ref())
                        .and_then(|a| a.flag)
                })
            })
    }

    /// Running time including penalties, or the final time.
    pub(super) fn elapsed_ms(&self) -> i64 {
        let now = self.server_now();
        self.info.with(|i| {
            let Some(a) = i.as_ref().and_then(|i| i.attempt.as_ref()) else {
                return 0;
            };
            match (a.time_ms, a.finished_at) {
                (Some(t), _) => t,
                (None, Some(end)) => end - a.started_at + a.penalty_ms(),
                _ => now - a.started_at + a.penalty_ms(),
            }
        })
    }

    pub(super) fn apply(&self, info: DailyInfo) {
        self.skew.set(info.server_now - browser_now());
        if let Some(puzzle) = &info.puzzle {
            let loaded = self.game.status.get_untracked() != Status::Empty
                && self.game.board.with_untracked(|b| b.puzzle == *puzzle);
            if !loaded {
                self.load_puzzle(&info, puzzle);
            }
        }
        self.info.set(Some(info));
    }

    fn load_puzzle(&self, info: &DailyInfo, puzzle: &Puzzle) {
        let game = self.game;
        let source = Source::Daily {
            date: date_string(info.day),
            params: params_for(info.day),
        };
        game.set_puzzle(puzzle.clone(), None, Some(true), source);
        match &info.attempt {
            Some(a) if a.finished() => {
                if let Some(bridges) = &a.bridges {
                    game.bridges.set(bridges.clone());
                }
                game.status.set(if a.revealed {
                    Status::Revealed
                } else {
                    Status::Won
                });
            }
            _ => {
                game.restore_progress(puzzle);
            }
        }
    }

    pub(super) fn refresh_ranking(&self) {
        let ranking = self.ranking;
        spawn_local(async move {
            if let Ok(lb) = daily_leaderboard(None).await {
                ranking.set(Some(lb));
            }
        });
    }

    pub(super) fn reload(&self) {
        let this = *self;
        spawn_local(async move {
            match daily_today().await {
                Ok(info) => this.apply(info),
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
        });
        self.refresh_ranking();
    }

    pub(super) fn update_attempt(&self, f: impl FnOnce(&mut crate::model::AttemptInfo)) {
        self.info.update(|i| {
            if let Some(a) = i.as_mut().and_then(|i| i.attempt.as_mut()) {
                f(a);
            }
        });
    }
}
