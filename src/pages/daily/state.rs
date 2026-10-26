//! The state of today's daily on the page: the server's puzzle info, the clocks and the ranking.

use common::{GenerateParams, Puzzle};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::recovery::{Fetch, Phase, Submission, error_feedback};
use crate::api::{daily_leaderboard, daily_today};
use crate::components::stamps::announce;
use crate::model::{
    AttemptInfo, DailyInfo, Flag, Leaderboard, MoveEvent, SubmitResult, daily_difficulty,
    date_string,
};
use crate::state::{Game, Source, Status, load, store};

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
    /// What this tab's submit got back, `None` for a daily finished before the page was opened.
    pub(super) result: RwSignal<Option<SubmitResult>>,
    /// The player put the win card of [`Self::result`] away to look at the board.
    pub(super) admired: RwSignal<bool>,
    pub(super) ranking: RwSignal<Option<Leaderboard>>,
    pub(super) busy: RwSignal<bool>,
    pub(super) metadata: Fetch,
    pub(super) starting: Fetch,
    pub(super) ranking_load: Fetch,
    pub(super) submission: RwSignal<Phase>,
    pub(super) submit_payload: StoredValue<Option<Submission>>,
    pub(super) logged: StoredValue<Option<MoveSnapshot>>,
    #[cfg(feature = "browser")]
    pub(super) log_clock: StoredValue<Option<super::clock::MoveClock>>,
    /// A quiet [`Self::refresh`] is on its way.
    pub(super) refreshing: StoredValue<bool>,
}

/// Quiet fetches may overlap a purchase, identity change, day change or page disposal.
/// Compare the full captured info too: even a same-attempt counter update makes it stale.
#[derive(Clone, Debug)]
pub(super) struct RefreshGuard {
    info: Option<DailyInfo>,
    user: Option<i64>,
    epoch: u32,
    revision: u64,
}

impl RefreshGuard {
    pub(super) fn new(daily: &Daily) -> Self {
        Self {
            info: daily.info.get_untracked(),
            user: daily.actions.user(),
            epoch: daily.game.epoch.get_untracked(),
            revision: daily.actions.refresh_revision(),
        }
    }

    pub(super) fn current(&self, daily: &Daily) -> bool {
        daily.info.try_with_untracked(|i| *i == self.info) == Some(true)
            && daily.game.epoch.try_get_untracked() == Some(self.epoch)
            && daily.actions.user() == self.user
            && daily.actions.refresh_revision() == self.revision
    }
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
        #[cfg(feature = "browser")]
        if let Some(t) = self
            .log_clock
            .with_value(|c| c.as_ref().map(|c| c.at(super::clock::monotonic_now())))
        {
            return t;
        }
        #[cfg(feature = "browser")]
        let now = browser_now() + self.skew.get_untracked();
        #[cfg(not(feature = "browser"))]
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
        let t = t.max(saved.events.last().map_or(0, |e| e.t));
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
        let new_day = self.day() != Some(info.day);
        #[cfg(feature = "browser")]
        self.log_clock.update_value(|clock| {
            let Some(attempt) = &info.attempt else {
                *clock = None;
                return;
            };
            let elapsed =
                u32::try_from((info.server_now - attempt.started_at).max(0)).unwrap_or(u32::MAX);
            let now = super::clock::monotonic_now();
            match clock {
                Some(c) if c.attempt == attempt.attempt_id => c.observe(elapsed, now),
                _ => {
                    *clock = Some(super::clock::MoveClock::new(
                        attempt.attempt_id.clone(),
                        elapsed,
                        now,
                    ));
                }
            }
        });
        self.skew.set(info.server_now - browser_now());
        self.show(info);
        if new_day {
            self.result.set(None);
            self.ranking.set(None);
            self.refresh_ranking();
        }
    }

    /// Shows the server's state of today's daily. A board that still plays a daily the server
    /// has finished (in another tab) gets the finished board, locked. It never unlocks here:
    /// a submit may be on its way, see the owned submission recovery.
    pub(super) fn show(&self, info: DailyInfo) {
        announce(&info.stamps);
        if let Some(puzzle) = &info.puzzle {
            let status = self.game.status.get_untracked();
            let loaded = status != Status::Empty
                && !(finished(&info)
                    && status
                        != if info.attempt.as_ref().is_some_and(|a| a.revealed) {
                            Status::Revealed
                        } else {
                            Status::Won
                        })
                && self.game.board.with_untracked(|b| b.puzzle == *puzzle);
            if !loaded {
                self.load_puzzle(&info, puzzle);
            }
        }
        self.info.set(Some(info));
    }

    /// Whether the solved board can go to the server now: once per win, never while a submit
    /// or a paid action is on its way, and after a failed one only when the player says so.
    pub(super) fn ready_to_submit(&self) -> bool {
        self.game.status.get() == Status::Won
            && self.started()
            && self.submission.get() == Phase::Idle
            && !self.finished()
            && !self.actions.locked()
            && !self.busy.get_untracked()
    }

    /// The server took the solved board: `end` is the finish on the server's clock.
    pub(super) fn submitted(&self, result: SubmitResult, end: i64) {
        self.update_attempt(|a| {
            a.finished_at = Some(end);
            a.time_ms = Some(result.time_ms);
        });
        self.admired.set(false);
        self.result.set(Some(result));
    }

    /// The win card over the board: right after this tab's submit, until the player puts it
    /// away. A daily finished before the page was opened only has the result card.
    pub(super) fn celebrating(&self) -> bool {
        self.result.with(Option::is_some)
            && self.game.status.get() == Status::Won
            && !self.admired.get()
    }

    /// Fetches today's daily again, quietly, unless it's finished: back on the page after
    /// playing it in another tab.
    /// Focus refreshes stop on failure; only a named manual retry clears the error.
    /// Unconfirmed solved boards can only be reconciled by the owned submission flow.
    pub(super) fn refresh(&self) {
        if self.finished()
            || self.info.with_untracked(Option::is_none)
            || self.game.status.get_untracked() == Status::Won
            || self.busy.get_untracked()
            || self.actions.locked()
            || self.metadata.phase.get_untracked() == Phase::Error
        {
            return;
        }
        self.fetch_metadata(false);
    }

    pub(super) fn retry_metadata(&self) {
        self.fetch_metadata(false);
    }

    /// A loaded attempt with a local solution belongs to the submission recovery flow.
    /// A shared solved board without metadata must still be allowed to discover its attempt.
    pub(super) fn unconfirmed(&self) -> bool {
        self.started() && self.game.status.get() == Status::Won && !self.finished()
    }

    fn fetch_metadata(&self, replace: bool) {
        if !replace && untrack(|| self.unconfirmed()) {
            return;
        }
        let Some(request) = self.metadata.begin(replace) else {
            return;
        };
        self.refreshing.set_value(true);
        let this = *self;
        let guard = RefreshGuard::new(self);
        spawn_local(async move {
            let response = daily_today().await;
            if !this.metadata.current(request) {
                return;
            }
            if guard.current(&this) {
                match response {
                    Ok(info) => {
                        this.metadata.finish(request, false);
                        this.apply(info);
                    }
                    Err(error) => {
                        this.metadata.finish(request, true);
                        error_feedback(&error);
                    }
                }
            } else {
                this.metadata.finish(request, false);
            }
            this.refreshing.try_set_value(false);
        });
    }

    fn load_puzzle(&self, info: &DailyInfo, puzzle: &Puzzle) {
        let game = self.game;
        let source = Source::Daily {
            date: date_string(info.day),
            params: params_for(info.day),
        };
        game.set_puzzle(puzzle.clone(), None, Some(true), source);
        // A result or a failed submit belongs to the board before
        self.result.set(None);
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
                if game.restore_progress(puzzle)
                    && game
                        .board
                        .with_untracked(|b| game.bridges.with_untracked(|br| b.check(br).solved))
                {
                    // A reload cannot prove whether this solved board reached the server.
                    // Restore it locked with explicit recovery, rather than editing or auto-send.
                    game.status.set(Status::Won);
                    self.submission.set(Phase::Error);
                }
            }
        }
    }

    pub(super) fn refresh_ranking(&self) {
        // A saved finish supersedes an older pending ranking response.
        self.fetch_ranking(true);
    }

    pub(super) fn retry_ranking(&self) {
        self.fetch_ranking(false);
    }

    fn fetch_ranking(&self, replace: bool) {
        let Some(request) = self.ranking_load.begin(replace) else {
            return;
        };
        let this = *self;
        let user = self.actions.user();
        let day = self.day();
        spawn_local(async move {
            let response = daily_leaderboard(day).await;
            if !this.ranking_load.current(request) {
                return;
            }
            // Ranking is independently owned: metadata/counter updates do not invalidate it.
            if this.actions.user() == user && (day.is_none() || this.day() == day) {
                match response {
                    Ok(lb) => {
                        if this.ranking_day_matches(lb.day) {
                            this.ranking.set(Some(lb));
                            this.ranking_load.finish(request, false);
                        } else {
                            this.ranking_load.finish(request, true);
                        }
                    }
                    Err(error) => {
                        this.ranking_load.finish(request, true);
                        error_feedback(&error);
                    }
                }
            } else {
                this.ranking_load.finish(request, false);
            }
        });
    }

    fn ranking_day_matches(&self, day: i64) -> bool {
        self.day().is_none_or(|current| current == day)
    }

    pub(super) fn reload(&self) {
        // Old metadata belongs to the preceding identity/day. Discover the new attempt before
        // any solved-board effect can dispatch under the new authenticated user.
        self.info.set(None);
        // Synchronize the cleared day/changed identity before capturing a metadata guard.
        // The reactive scope effect may otherwise run later and invalidate this new request.
        untrack(|| self.actions.sync(self));
        self.busy.set(false);
        self.starting.cancel();
        self.submission.set(Phase::Idle);
        self.submit_payload.set_value(None);
        self.fetch_metadata(true);
        self.ranking.set(None);
        self.fetch_ranking(true);
    }

    /// The day of the header: `rendered`, the server's day the page was rendered with, until the
    /// daily is loaded, then the loaded daily's, and the last one while a new day loads. So the
    /// server's page has its heading before the daily is loaded, and it doesn't change when the
    /// daily comes (it's the same day) or flash away when a new day starts.
    pub(super) fn header_day(&self, rendered: Option<i64>) -> Memo<Option<i64>> {
        let info = self.info;
        Memo::new(move |before: Option<&Option<i64>>| {
            info.with(|i| i.as_ref().map(|i| i.day))
                .or_else(|| before.copied().flatten())
                .or(rendered)
        })
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
    fn an_initial_ranking_must_match_the_day_metadata_established() {
        tab(|d| {
            assert!(d.ranking_day_matches(99));
            d.info.set(Some(info(false)));
            assert!(!d.ranking_day_matches(99));
            assert!(d.ranking_day_matches(100));
            d.info.update(|i| i.as_mut().unwrap().day = 101);
            assert!(!d.ranking_day_matches(100));
            assert!(d.ranking_day_matches(101));
        });
    }

    #[test]
    fn refresh_replies_reject_newer_counters_days_boards_and_disposed_owners() {
        tab(|d| {
            d.info.set(Some(info(false)));
            let before = RefreshGuard::new(d);
            assert!(before.current(d));
            d.update_attempt(|a| a.hints = 2);
            assert!(!before.current(d)); // old daily_today cannot roll back accepted penalties
            let current = RefreshGuard::new(d);
            d.info.update(|i| i.as_mut().unwrap().day += 1);
            assert!(!current.current(d));
            let current = RefreshGuard::new(d);
            d.game.epoch.update(|e| *e += 1);
            assert!(!current.current(d));
        });
        let owner = Owner::new();
        let (d, guard) = owner.with(|| {
            let d = daily();
            let guard = RefreshGuard::new(&d);
            (d, guard)
        });
        owner.cleanup();
        assert!(!guard.current(&d));
        d.refreshing.try_set_value(false); // completing a quiet fetch is also disposal-safe
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
            admired: RwSignal::new(false),
            ranking: RwSignal::new(None),
            busy: RwSignal::new(false),
            metadata: Fetch::new(),
            starting: Fetch::new(),
            ranking_load: Fetch::new(),
            submission: RwSignal::new(Phase::Idle),
            submit_payload: StoredValue::new(None),
            refreshing: StoredValue::new(false),
            logged: StoredValue::new(None),
            #[cfg(feature = "browser")]
            log_clock: StoredValue::new(None),
        }
    }

    /// Today's daily as the server sends it, finished or still running.
    fn info(finished: bool) -> DailyInfo {
        DailyInfo {
            day: 100,
            difficulty: Difficulty::Easy,
            puzzle: Some(puzzle()),
            attempt: Some(AttemptInfo {
                attempt_id: String::new(),
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

    /// What this tab's submit got back.
    fn result(stamps: Vec<crate::model::Stamp>) -> SubmitResult {
        SubmitResult {
            time_ms: 60_000,
            rank: 1,
            finishers: 1,
            flag: None,
            stamps,
        }
    }

    #[test]
    fn the_win_card_shows_after_this_tabs_submit_until_its_put_away() {
        tab(|d| {
            board(d, vec![1], Status::Won);
            d.show(info(false));
            assert!(!d.celebrating());
            d.submitted(result(vec![crate::model::Stamp::First]), 61_000);
            assert!(d.celebrating());
            assert!(d.finished() && !d.ready_to_submit());
            assert_eq!(d.elapsed_ms(), 60_000);
            d.admired.set(true);
            assert!(!d.celebrating());
            // The result card still has the result, and the board stays solved
            assert!(d.result.with_untracked(Option::is_some));
            assert_eq!(d.game.status.get_untracked(), Status::Won);
            assert_eq!(d.game.bridges.get_untracked(), vec![1]);
        });
    }

    #[test]
    fn a_daily_finished_before_the_page_opened_has_no_win_card() {
        tab(|d| {
            d.show(info(true));
            assert_eq!(d.game.status.get_untracked(), Status::Won);
            assert!(!d.celebrating());
            assert!(!d.ready_to_submit());
        });
    }

    #[test]
    fn the_next_daily_drops_the_win_card_of_the_last_one() {
        tab(|d| {
            board(d, vec![1], Status::Won);
            d.show(info(false));
            d.submitted(result(vec![]), 61_000);
            assert!(d.celebrating());
            // The next day, already finished in another tab
            let mut tomorrow = info(true);
            tomorrow.day = 101;
            tomorrow.puzzle = Some(Puzzle::new(
                3,
                1,
                vec![
                    Island {
                        pos: Pos { x: 0, y: 0 },
                        required: 2,
                    },
                    Island {
                        pos: Pos { x: 2, y: 0 },
                        required: 2,
                    },
                ],
            ));
            attempt(&mut tomorrow).bridges = Some(vec![2]);
            d.show(tomorrow);
            assert_eq!(d.game.status.get_untracked(), Status::Won);
            assert!(!d.celebrating());
        });
    }

    /// The attempt of today's daily, to change a field of it.
    fn attempt(info: &mut DailyInfo) -> &mut AttemptInfo {
        info.attempt.as_mut().unwrap()
    }

    #[test]
    fn a_fresh_tab_restores_an_unconfirmed_solved_daily_locked_for_retry() {
        tab(|d| {
            // The board as the last tab saved it
            let before = Game::new("test.daily");
            before.set_puzzle(puzzle(), None, Some(true), Source::Custom);
            before.cycle(0, 1);
            before.save();

            d.show(info(false));
            assert_eq!(d.game.status.get_untracked(), Status::Won);
            assert_eq!(d.submission.get_untracked(), Phase::Error);
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

    #[test]
    fn the_header_shows_the_rendered_day_until_the_daily_is_loaded() {
        tab(|d| {
            let day = d.header_day(Some(99));
            assert_eq!(day.get_untracked(), Some(99));
            // Loaded just after midnight, a day after the page was rendered
            d.show(info(false));
            assert_eq!(day.get_untracked(), Some(100));
            // A new day loads: the header keeps the day it shows until it's there
            d.info.set(None);
            assert_eq!(day.get_untracked(), Some(100));
            let mut tomorrow = info(false);
            tomorrow.day = 101;
            d.info.set(Some(tomorrow));
            assert_eq!(day.get_untracked(), Some(101));
        });
    }

    /// Opened in the browser, without a day from the server: no header until the daily is loaded.
    #[test]
    fn without_a_rendered_day_the_header_waits_for_the_daily() {
        tab(|d| {
            let day = d.header_day(None);
            assert_eq!(day.get_untracked(), None);
            d.show(info(false));
            assert_eq!(day.get_untracked(), Some(100));
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
