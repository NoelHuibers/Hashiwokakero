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
use crate::state::{Game, Source, Status, error_toast, load, store};

/// The move log of the running daily, kept in the browser so a reload doesn't lose it.
pub(super) const LOG_KEY: &str = "hashi.daily.log";

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub(super) struct SavedLog {
    pub(super) day: i64,
    pub(super) events: Vec<MoveEvent>,
}

/// The last observed board, including its identity so loading another puzzle never logs moves.
#[derive(Clone, Debug)]
pub(super) struct MoveSnapshot {
    day: Option<i64>,
    epoch: u32,
    bridges: Vec<u8>,
    moves: u32,
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
    pub(super) actions: super::actions::Actions,
    pub(super) game: Game,
    pub(super) info: RwSignal<Option<DailyInfo>>,
    /// Server clock minus client clock.
    pub(super) skew: RwSignal<i64>,
    /// Client clock, ticking while the page is open.
    pub(super) now: RwSignal<i64>,
    pub(super) result: RwSignal<Option<SubmitResult>>,
    pub(super) ranking: RwSignal<Option<Leaderboard>>,
    pub(super) busy: RwSignal<bool>,
    pub(super) logged: StoredValue<Option<MoveSnapshot>>,
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

    /// Log bridge changes, whether called by the reactive observer or just before submitting.
    /// Only the board and attempt are reactive dependencies; sampling the clock must not make
    /// the 100 ms display timer read, serialize and rewrite the whole saved log.
    pub(super) fn record_moves(&self) {
        let current = MoveSnapshot {
            day: self.info.with(|i| i.as_ref().map(|i| i.day)),
            epoch: self.game.epoch.get(),
            bridges: self.game.bridges.get(),
            moves: self.game.moves.get_untracked(),
        };
        let previous = self.logged.get_value();
        self.logged.set_value(Some(current.clone()));
        let Some(before) = previous else { return };
        let Some(day) = current.day else { return };
        if before.day != current.day
            || before.epoch != current.epoch
            || before.bridges.len() != current.bridges.len()
            || !self.started()
            || self.finished()
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
        let t = untrack(|| self.clock_ms());
        let mut saved = load::<SavedLog>(LOG_KEY)
            .filter(|l| l.day == day)
            .unwrap_or_default();
        saved.day = day;
        for edge in changed {
            saved.events.push(MoveEvent {
                t,
                edge: u16::try_from(edge).unwrap_or(u16::MAX),
                count: current.bridges[edge],
                tap,
            });
        }
        store(LOG_KEY, &saved);
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
    use crate::model::{CHECK_PENALTY_MS, HINT_PENALTY_MS};

    /// Two islands with one bridge between them, the solution is `[1]`.
    fn puzzle() -> Puzzle {
        let island = |x| Island {
            pos: Pos { x, y: 0 },
            required: 1,
        };
        Puzzle::new(3, 1, vec![island(0), island(2)])
    }

    #[test]
    fn the_final_move_is_flushed_once_before_the_observer_runs() {
        tab(|d| {
            board(d, vec![0], Status::Playing);
            d.info.set(Some(info(false)));
            d.now.set(2_500);
            d.record_moves();
            d.game.cycle(0, 1);
            d.game.status.set(Status::Won);
            // Submit flushes first, the board observer runs afterward.
            d.record_moves();
            d.record_moves();
            let saved = load::<SavedLog>(LOG_KEY).unwrap();
            assert_eq!(saved.day, 100);
            assert_eq!(
                saved.events,
                vec![MoveEvent {
                    t: 1_500,
                    edge: 0,
                    count: 1,
                    tap: true
                }]
            );
            // Clock ticks and an attempt refresh cannot add events or rewrite their time.
            d.now.set(10_000);
            d.skew.set(500);
            d.update_attempt(|a| a.checks += 1);
            d.record_moves();
            assert_eq!(load::<SavedLog>(LOG_KEY).unwrap().events, saved.events);
        });
    }

    #[test]
    fn undo_redo_and_clear_log_changes_without_counting_taps() {
        tab(|d| {
            board(d, vec![0], Status::Playing);
            d.info.set(Some(info(false)));
            d.now.set(2_000);
            d.record_moves();
            d.game.cycle(0, 1);
            d.record_moves();
            d.game.undo();
            d.record_moves();
            d.game.redo();
            d.record_moves();
            d.game.restart();
            d.record_moves();
            let events = load::<SavedLog>(LOG_KEY).unwrap().events;
            assert_eq!(
                events.iter().map(|e| e.count).collect::<Vec<_>>(),
                [1, 0, 1, 0]
            );
            assert_eq!(
                events.iter().map(|e| e.tap).collect::<Vec<_>>(),
                [true, false, false, false]
            );
            assert_eq!(d.game.moves.get_untracked(), 1);
        });
    }

    #[test]
    fn loading_another_puzzle_or_day_cannot_create_move_events() {
        tab(|d| {
            board(d, vec![0], Status::Playing);
            d.info.set(Some(info(false)));
            d.record_moves();
            // Same edge count, different epoch: loading/restoring bridges is not a move.
            board(d, vec![1], Status::Playing);
            d.record_moves();
            assert!(load::<SavedLog>(LOG_KEY).is_none());
            // Even reusing the same board across a day change is not a move.
            d.info.update(|i| i.as_mut().unwrap().day += 1);
            d.game.bridges.set(vec![0]);
            d.record_moves();
            assert!(load::<SavedLog>(LOG_KEY).is_none());
            d.game.cycle(0, 1);
            d.record_moves();
            let saved = load::<SavedLog>(LOG_KEY).unwrap();
            assert_eq!(saved.day, 101);
            assert_eq!(saved.events.len(), 1);
        });
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
            actions: super::super::actions::Actions::new(crate::state::Auth {
                user: RwSignal::new(None),
                loaded: RwSignal::new(true),
            }),
            info: RwSignal::new(None),
            skew: RwSignal::new(0),
            now: RwSignal::new(0),
            result: RwSignal::new(None),
            ranking: RwSignal::new(None),
            busy: RwSignal::new(false),
            refreshing: StoredValue::new(false),
            logged: StoredValue::new(None),
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

    /// The attempt of today's daily, to change a field of it.
    fn attempt(info: &mut DailyInfo) -> &mut AttemptInfo {
        info.attempt.as_mut().unwrap()
    }

    #[test]
    fn a_fresh_tab_picks_up_the_running_daily_where_it_was_left() {
        tab(|d| {
            // The board as the last tab saved it
            let before = Game::new("test.daily");
            before.set_puzzle(puzzle(), None, Some(true), Source::Custom);
            before.cycle(0, 1);
            before.save();

            d.show(info(false));
            assert_eq!(d.game.status.get_untracked(), Status::Playing);
            assert_eq!(d.game.bridges.get_untracked(), vec![1]);
            assert_eq!(d.game.moves.get_untracked(), 1);
            assert_eq!(
                d.game.source.get_untracked(),
                Source::Daily {
                    date: date_string(100),
                    params: params_for(100),
                }
            );
            assert_eq!(d.game.unique.get_untracked(), Some(true));
            assert!(d.started() && !d.finished());
            assert_eq!(d.day(), Some(100));
        });
    }

    #[test]
    fn a_daily_given_up_shows_the_solution_locked() {
        tab(|d| {
            let mut given_up = info(false);
            attempt(&mut given_up).revealed = true;
            attempt(&mut given_up).bridges = Some(vec![1]);
            d.show(given_up);
            assert_eq!(d.game.status.get_untracked(), Status::Revealed);
            assert_eq!(d.game.bridges.get_untracked(), vec![1]);
            assert!(d.finished());
            assert!(!editable(d));
        });
    }

    /// The puzzle is only sent once the clock runs.
    #[test]
    fn before_the_start_there_is_no_board_and_no_clock() {
        tab(|d| {
            let mut waiting = info(false);
            waiting.puzzle = None;
            waiting.attempt = None;
            d.show(waiting);
            d.now.set(70_000);
            assert_eq!(d.game.status.get_untracked(), Status::Empty);
            assert!(!d.started() && !d.finished());
            assert_eq!((d.clock_ms(), d.elapsed_ms()), (0, 0));
            assert_eq!(d.day(), Some(100));
        });
    }

    /// Both clocks go by the server's time, the shown one adds the penalties.
    #[test]
    fn the_clock_runs_on_server_time() {
        tab(|d| {
            let mut running = info(false);
            attempt(&mut running).hints = 1;
            attempt(&mut running).checks = 2;
            d.show(running);
            // The browser's clock is 5 s behind the server's
            d.now.set(60_000);
            d.skew.set(5_000);
            assert_eq!(d.server_now(), 65_000);
            assert_eq!(d.clock_ms(), 64_000);
            assert_eq!(
                d.elapsed_ms(),
                64_000 + HINT_PENALTY_MS + 2 * CHECK_PENALTY_MS
            );
            // A clock still before the start doesn't run backwards
            d.skew.set(-60_000);
            assert_eq!(d.clock_ms(), 0);
        });
    }

    #[test]
    fn a_finished_daily_shows_its_final_time() {
        tab(|d| {
            let mut done = info(true);
            d.show(done.clone());
            d.now.set(1_000_000);
            assert_eq!(d.elapsed_ms(), 60_000);
            // Without the server's final time: from start to finish, with the penalties
            attempt(&mut done).time_ms = None;
            attempt(&mut done).hints = 1;
            d.info.set(Some(done));
            assert_eq!(d.elapsed_ms(), 60_000 + HINT_PENALTY_MS);
        });
    }

    /// Why a result isn't ranked: the submit's answer, or else what the server said before.
    #[test]
    fn the_latest_word_on_a_flag_counts() {
        tab(|d| {
            assert_eq!(d.flag(), None);
            let mut flagged = info(true);
            attempt(&mut flagged).flag = Some(Flag::Pace);
            d.show(flagged);
            assert_eq!(d.flag(), Some(Flag::Pace));
            let result = |flag| SubmitResult {
                time_ms: 60_000,
                rank: 0,
                finishers: 2,
                flag,
                stamps: vec![],
            };
            d.result.set(Some(result(Some(Flag::SecondAccount))));
            assert_eq!(d.flag(), Some(Flag::SecondAccount));
            d.result.set(Some(result(None)));
            assert_eq!(d.flag(), Some(Flag::Pace));
        });
    }

    #[test]
    fn only_a_started_daily_has_an_attempt_to_update() {
        tab(|d| {
            d.update_attempt(|a| a.hints += 1);
            assert_eq!(d.info.get_untracked(), None);
            d.show(info(false));
            d.update_attempt(|a| a.hints += 1);
            d.update_attempt(|a| a.checks += 1);
            let a = d.info.get_untracked().unwrap().attempt.unwrap();
            assert_eq!((a.hints, a.checks), (1, 1));
        });
    }

    /// Back on the page after playing in another tab: a finished daily can't change any more,
    /// so it isn't fetched again.
    #[test]
    fn a_finished_daily_is_not_fetched_again() {
        tab(|d| {
            d.show(info(true));
            d.refresh();
            assert!(!d.refreshing.get_value());
        });
    }
}
