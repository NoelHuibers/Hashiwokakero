//! The state of today's daily on the page: the server's puzzle info, the clocks and the ranking.

use common::{GenerateParams, Puzzle};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{daily_leaderboard, daily_today};
use crate::components::stamps::announce;
use crate::model::{
    AttemptInfo, DailyInfo, Flag, Leaderboard, MoveEvent, SubmitResult, daily_difficulty,
    date_string,
};
use crate::state::{Game, Source, Status, error_toast};

/// The move log of the running daily, kept in the browser so a reload doesn't lose it.
pub(super) const LOG_KEY: &str = "hashi.daily.log";

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub(super) struct SavedLog {
    pub(super) day: i64,
    pub(super) events: Vec<MoveEvent>,
}

fn finished(info: &DailyInfo) -> bool {
    info.attempt.as_ref().is_some_and(AttemptInfo::finished)
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

/// Minutes the browser's clock is behind UTC (`getTimezoneOffset`), -120 in Berlin in summer.
#[expect(
    clippy::cast_possible_truncation,
    reason = "time zone offsets are a few hundred minutes"
)]
pub(super) fn browser_utc_offset() -> i32 {
    js_sys::Date::new_0().get_timezone_offset() as i32
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
    /// A quiet [`Self::refresh`] is on its way.
    pub(super) refreshing: StoredValue<bool>,
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
                .is_some_and(AttemptInfo::finished)
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
        self.show(info);
    }

    /// Shows the server's state of today's daily. A board that still plays a daily the server
    /// has finished (in another tab) gets the finished board, locked. It never unlocks here:
    /// a submit may be on its way, see [`Self::submit_failed`].
    fn show(&self, info: DailyInfo) {
        announce(&info.stamps);
        if let Some(puzzle) = &info.puzzle {
            let status = self.game.status.get_untracked();
            let loaded = status != Status::Empty
                && !(finished(&info) && status == Status::Playing)
                && self.game.board.with_untracked(|b| b.puzzle == *puzzle);
            if !loaded {
                self.load_puzzle(&info, puzzle);
            }
        }
        self.info.set(Some(info));
    }

    /// After a failed submit, with today's daily as the server sees it now (`None` if it
    /// couldn't be asked). The finish may be saved despite the error (one after the update, or
    /// another tab was first), so the board only unlocks if the server says the daily still
    /// runs. Otherwise it stays solved and locked, a reload submits again.
    pub(super) fn submit_failed(&self, server: Option<DailyInfo>) {
        let Some(info) = server else { return };
        if !finished(&info) {
            self.game.status.set(Status::Playing);
        }
        self.show(info);
    }

    /// Fetches today's daily again, quietly, unless it's finished: back on the page after
    /// playing it in another tab.
    pub(super) fn refresh(&self) {
        let done = self
            .info
            .with_untracked(|i| i.as_ref().is_some_and(finished));
        if done || self.refreshing.get_value() {
            return;
        }
        self.refreshing.set_value(true);
        let this = *self;
        spawn_local(async move {
            if let Ok(info) = daily_today().await {
                this.apply(info);
            }
            this.refreshing.set_value(false);
        });
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
                Err(e) => error_toast(e),
            }
        });
        self.refresh_ranking();
    }

    pub(super) fn update_attempt(&self, f: impl FnOnce(&mut AttemptInfo)) {
        self.info.update(|i| {
            if let Some(a) = i.as_mut().and_then(|i| i.attempt.as_mut()) {
                f(a);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use common::{Difficulty, Island, Pos};

    use super::*;

    /// Two islands with one bridge between them, the solution is `[1]`.
    fn puzzle() -> Puzzle {
        let island = |x| Island {
            pos: Pos { x, y: 0 },
            required: 1,
        };
        Puzzle::new(3, 1, vec![island(0), island(2)])
    }

    /// The move log of a running daily in the browser (`hashi.daily.log`): a different format
    /// loses the log, and a daily submitted without it isn't ranked.
    #[test]
    fn saved_logs_still_load() {
        let saved = SavedLog {
            day: 20_745,
            events: vec![
                MoveEvent {
                    t: 0,
                    edge: 0,
                    count: 1,
                    tap: true,
                },
                MoveEvent {
                    t: 1_250,
                    edge: 0,
                    count: 2,
                    tap: false,
                },
            ],
        };
        let json = serde_json::to_string(&saved).unwrap();
        crate::golden::check(
            "saved_log.txt",
            &json,
            "the move logs saved in the browsers wouldn't load any more: running dailies lose \
             their log and aren't ranked",
        );
        let loaded: SavedLog = serde_json::from_str(&json).unwrap();
        assert_eq!((loaded.day, loaded.events), (saved.day, saved.events));
    }

    /// Runs `test` on the daily page of a fresh tab (signals need an owner).
    fn tab(test: impl FnOnce(&Daily)) {
        Owner::new().with(|| test(&daily()));
    }

    fn daily() -> Daily {
        Daily {
            game: Game::new("test.daily"),
            info: RwSignal::new(None),
            skew: RwSignal::new(0),
            now: RwSignal::new(0),
            result: RwSignal::new(None),
            ranking: RwSignal::new(None),
            busy: RwSignal::new(false),
            refreshing: StoredValue::new(false),
        }
    }

    /// Today's daily as the server sends it, finished or still running.
    fn info(finished: bool) -> DailyInfo {
        DailyInfo {
            day: 100,
            difficulty: Difficulty::Easy,
            puzzle: Some(puzzle()),
            attempt: Some(AttemptInfo {
                started_at: 1_000,
                finished_at: finished.then_some(61_000),
                hints: 0,
                checks: 0,
                revealed: false,
                time_ms: finished.then_some(60_000),
                bridges: finished.then(|| vec![1]),
                flag: None,
            }),
            server_now: 70_000,
            finishers: 1,
            stamps: vec![],
        }
    }

    /// A board of today's daily in this tab: playing with `bridges`, or solved and won.
    fn board(daily: &Daily, bridges: Vec<u8>, status: Status) {
        daily
            .game
            .set_puzzle(puzzle(), None, Some(true), Source::Custom);
        daily.game.bridges.set(bridges);
        daily.game.status.set(status);
    }

    /// Whether a tap on the only bridge changes the board.
    fn editable(daily: &Daily) -> bool {
        let before = daily.game.bridges.get_untracked();
        daily.game.cycle(0, -1);
        daily.game.bridges.get_untracked() != before
    }

    #[test]
    fn a_daily_finished_in_another_tab_locks_this_one() {
        tab(|d| {
            board(d, vec![0], Status::Playing);
            d.show(info(true));
            assert_eq!(d.game.status.get_untracked(), Status::Won);
            assert_eq!(d.game.bridges.get_untracked(), vec![1]);
            assert!(!editable(d));
        });
    }

    #[test]
    fn a_refresh_during_the_submit_keeps_the_board_locked() {
        tab(|d| {
            // Solved here, the server hasn't saved the finish yet
            board(d, vec![1], Status::Won);
            d.show(info(false));
            assert_eq!(d.game.status.get_untracked(), Status::Won);
            assert!(!editable(d));
        });
    }

    #[test]
    fn a_failed_submit_the_server_saved_keeps_the_board_locked() {
        tab(|d| {
            board(d, vec![1], Status::Won);
            d.submit_failed(Some(info(true)));
            assert_eq!(d.game.status.get_untracked(), Status::Won);
            assert!(d.finished());
            assert!(!editable(d));
        });
    }

    #[test]
    fn a_failed_submit_keeps_the_board_locked_if_the_server_cant_tell() {
        tab(|d| {
            board(d, vec![1], Status::Won);
            d.submit_failed(None);
            assert_eq!(d.game.status.get_untracked(), Status::Won);
            assert!(!editable(d));
        });
    }

    #[test]
    fn a_failed_submit_unlocks_the_board_while_the_daily_runs() {
        tab(|d| {
            board(d, vec![1], Status::Won);
            d.submit_failed(Some(info(false)));
            assert_eq!(d.game.status.get_untracked(), Status::Playing);
            assert!(!d.finished());
            assert!(editable(d));
        });
    }
}
