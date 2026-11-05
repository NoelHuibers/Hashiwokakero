//! A race on its page: what the server sent last, the clocks, the player's board, the progress
//! that goes with the next request and the move log.

use common::{Board, Puzzle};
use leptos::prelude::*;

use crate::model::{
    ClockSync, MoveEvent, PROGRESS_CAP, ProgressReport, RaceOutcome, RacePhase, RaceView,
    poll_delay_ms, progress, total_bridges,
};
use crate::state::{Game, MoveLog, Source, Status};

/// The board of the race being played, kept in the browser like the daily's.
pub const GAME_KEY: &str = "hashi.race.v1";
/// Its move log.
const LOG_KEY: &str = "hashi.race.log";

#[derive(Clone, Copy, Debug)]
pub struct RaceState {
    pub game: Game,
    /// The race as the server sent it last, `None` while loading. Keeps the puzzle once it came,
    /// later answers don't repeat it.
    pub view: RwSignal<Option<RaceView>>,
    /// The browser's estimate of the server's clock.
    pub sync: RwSignal<ClockSync>,
    /// Browser clock, ticking.
    pub now: RwSignal<i64>,
    /// A request of the player is on its way.
    pub busy: RwSignal<bool>,
    /// The submit of the solved board failed: it waits for the player's retry.
    pub failed: RwSignal<bool>,
    /// Solved in this tab just now: the petals.
    pub won_here: RwSignal<bool>,
    /// The race whose puzzle is on the player's board.
    loaded: RwSignal<Option<i64>>,
    report: StoredValue<ProgressReport>,
    log: MoveLog,
}

impl RaceState {
    pub fn new(game: Game) -> Self {
        Self {
            game,
            view: RwSignal::new(None),
            sync: RwSignal::new(ClockSync::default()),
            now: RwSignal::new(0),
            busy: RwSignal::new(false),
            failed: RwSignal::new(false),
            won_here: RwSignal::new(false),
            loaded: RwSignal::new(None),
            report: StoredValue::new(ProgressReport::default()),
            log: MoveLog::new(LOG_KEY),
        }
    }

    pub fn id(&self) -> Option<i64> {
        self.view.with_untracked(|v| v.as_ref().map(|v| v.id))
    }

    /// The server's clock now.
    pub fn server_now(&self) -> i64 {
        let now = self.now.get();
        self.sync.with(|s| s.server(now))
    }

    pub fn phase(&self) -> Option<RacePhase> {
        let now = self.server_now();
        self.view.with(|v| {
            v.as_ref()
                .map(|v| RacePhase::at(now, v.starts_at, v.ended_at))
        })
    }

    /// The viewer's outcome, if they joined.
    pub fn mine(&self) -> Option<RaceOutcome> {
        self.view
            .with(|v| v.as_ref().and_then(|v| v.mine()).map(|p| p.outcome))
    }

    /// The player's board has this race's puzzle.
    pub fn has_board(&self) -> bool {
        let id = self.view.with(|v| v.as_ref().map(|v| v.id));
        id.is_some() && self.loaded.get() == id
    }

    /// The viewer races now: joined, the race runs on the server's clock, not done.
    pub fn racing(&self) -> bool {
        self.phase() == Some(RacePhase::Running) && self.mine() == Some(RaceOutcome::Racing)
    }

    /// Takes a race the server sent: the request went at `sent` and the answer came at
    /// `received`, on the browser's clock.
    pub fn apply(&self, view: RaceView, sent: i64, received: i64) {
        self.sync.update(|s| {
            s.sample(sent, received, view.server_now);
        });
        self.show(view);
    }

    /// Shows what the server sent. The puzzle goes on the player's board once it came (with the
    /// bridges placed before a reload); a player who is done gets the board locked, and once the
    /// race is over one who didn't solve it sees the solution there.
    pub fn show(&self, mut view: RaceView) {
        if self.id() != Some(view.id) {
            // Another race in the same page: the rematch
            self.report.set_value(ProgressReport::default());
            self.won_here.set(false);
            self.failed.set(false);
        } else if view.puzzle.is_none() {
            view.puzzle = self
                .view
                .with_untracked(|v| v.as_ref().and_then(|v| v.puzzle.clone()));
        }
        let game = self.game;
        let joined = view.joined();
        if let Some(puzzle) = view.puzzle.as_ref().filter(|_| joined)
            && self.loaded.get_untracked() != Some(view.id)
        {
            game.set_puzzle(
                puzzle.clone(),
                None,
                Some(true),
                Source::Imported(String::new()),
            );
            game.restore_progress(puzzle);
            self.loaded.set(Some(view.id));
        }
        if self.loaded.get_untracked() == Some(view.id) {
            match view.mine().map(|p| p.outcome) {
                Some(RaceOutcome::Solved { .. }) => game.status.set(Status::Won),
                Some(RaceOutcome::GaveUp | RaceOutcome::Out) => {
                    if let Some(solution) = &view.solution {
                        game.bridges.set(solution.clone());
                    }
                    game.status.set(Status::Revealed);
                }
                Some(RaceOutcome::Racing) | None => {}
            }
        }
        self.view.set(Some(view));
    }

    /// Whether the next request asks for the puzzle: the race started (the player's), or it's
    /// over (everyone's), and it isn't here yet.
    pub fn wants_puzzle(&self) -> bool {
        self.view.with_untracked(|v| {
            v.as_ref().is_some_and(|v| {
                v.puzzle.is_none() && v.starts_at.is_some() && (v.joined() || v.ended_at.is_some())
            })
        })
    }

    /// Ms on the race's clock, without penalties: the move log's time.
    pub fn clock_ms(&self) -> u32 {
        let now = self.server_now();
        let start = self
            .view
            .with_untracked(|v| v.as_ref().and_then(|v| v.starts_at));
        start.map_or(0, |s| u32::try_from((now - s).max(0)).unwrap_or(u32::MAX))
    }

    pub fn penalty_ms(&self) -> i64 {
        self.view.with(|v| {
            v.as_ref().map_or(0, |v| {
                v.mine().map_or(0, |p| v.penalties.total(p.hints, p.checks))
            })
        })
    }

    /// The viewer's time with penalties: running, or the final one.
    pub fn elapsed_ms(&self) -> i64 {
        match self.mine() {
            Some(RaceOutcome::Solved { time_ms, .. }) => time_ms,
            Some(RaceOutcome::Racing) if self.phase() == Some(RacePhase::Running) => {
                i64::from(self.clock_ms()) + self.penalty_ms()
            }
            _ => 0,
        }
    }

    /// The solved board can go to the server: once, not while something else is on its way,
    /// and after a failed submit only when the player says so.
    pub fn ready_to_submit(&self) -> bool {
        self.game.status.get() == Status::Won
            && self.has_board()
            && self.racing()
            && !self.busy.get()
            && !self.failed.get()
    }

    /// How far the player's board is, the share of the solution's bridges on it (in percent),
    /// at most [`PROGRESS_CAP`]: only the server says it's solved.
    pub fn progress(&self) -> u8 {
        if !self.has_board() {
            return 0;
        }
        let total = self.game.board.with(|b: &Board| total_bridges(&b.puzzle));
        let done = self.game.bridges.with(|b| progress(b, total));
        u8::try_from(done).unwrap_or(0).min(PROGRESS_CAP)
    }

    /// The progress to send with the next request at `now` (browser clock), if it changed and
    /// the last went long enough ago; only while racing.
    pub fn take_progress(&self, now: i64) -> Option<u8> {
        if !untrack(|| self.racing() && self.has_board()) {
            return None;
        }
        let value = untrack(|| self.progress());
        self.report
            .try_update_value(|r| r.take(now, value))
            .flatten()
    }

    /// The request with the progress failed: it goes again with the next one.
    pub fn progress_failed(&self) {
        self.report.update_value(ProgressReport::failed);
    }

    /// How long until the next request, at the server time now.
    pub fn next_poll_ms(&self) -> u32 {
        let now = untrack(|| self.server_now());
        let has = untrack(|| {
            self.view
                .with(|v| v.as_ref().is_some_and(|v| v.puzzle.is_some()))
        });
        self.view
            .with_untracked(|v| v.as_ref().map_or(2_000, |v| poll_delay_ms(v, now, has)))
    }

    /// The saved move log of this race.
    pub fn log(&self) -> Vec<MoveEvent> {
        self.log.events(self.loaded.get_untracked())
    }

    /// Logs the bridge changes since the last call, while the player races.
    pub fn record_moves(&self) {
        let puzzle = self.loaded.get();
        let running = untrack(|| self.racing());
        self.log
            .record(&self.game, puzzle, running, || self.clock_ms());
    }

    /// The puzzle on the player's board.
    pub fn puzzle(&self) -> Option<Puzzle> {
        self.view
            .with(|v| v.as_ref().and_then(|v| v.puzzle.clone()))
    }
}

#[cfg(test)]
mod tests {
    use common::{Difficulty, Island, Pos};

    use super::*;
    use crate::model::{COUNTDOWN_MS, HelpPenalties, RacePlayer};

    /// Two islands with one bridge between them, the solution is `[1]`.
    fn puzzle() -> Puzzle {
        let island = |x| Island {
            pos: Pos { x, y: 0 },
            required: 1,
        };
        Puzzle::new(3, 1, vec![island(0), island(2)])
    }

    /// A 2 next to a 2: two bridges, a double one.
    fn double() -> Puzzle {
        let island = |x| Island {
            pos: Pos { x, y: 0 },
            required: 2,
        };
        Puzzle::new(3, 1, vec![island(0), island(2)])
    }

    const START: i64 = 100_000;

    fn player(user_id: i64, outcome: RaceOutcome) -> RacePlayer {
        RacePlayer {
            user_id,
            name: format!("p{user_id}"),
            guest: false,
            colour: 0,
            joined_at: 0,
            progress: 0,
            away: false,
            hints: 1,
            checks: 1,
            flag: None,
            outcome,
        }
    }

    /// The race as the server sends it to player 2 at `server_now`: started at [`START`] if
    /// `started`, with the puzzle once it runs.
    fn view(mine: RaceOutcome, server_now: i64) -> RaceView {
        let running = server_now >= START;
        RaceView {
            id: 7,
            group_id: 1,
            group_name: "Club".into(),
            me: 2,
            host: Some(1),
            difficulty: Difficulty::Easy,
            penalties: HelpPenalties::of(Difficulty::Easy),
            created_at: 0,
            starts_at: Some(START),
            ended_at: None,
            players: vec![player(1, RaceOutcome::Racing), player(2, mine)],
            puzzle: running.then(puzzle),
            solution: None,
            rematch: None,
            rematch_open: false,
            server_now,
        }
    }

    fn tab(test: impl FnOnce(&RaceState)) {
        Owner::new().with(|| test(&RaceState::new(Game::new("test.race"))));
    }

    #[test]
    fn the_countdown_has_no_board_the_start_brings_it() {
        tab(|r| {
            r.now.set(START - COUNTDOWN_MS);
            let at = START - COUNTDOWN_MS;
            r.apply(view(RaceOutcome::Racing, at), at, at);
            assert_eq!(r.phase(), Some(RacePhase::Countdown));
            assert!(!r.has_board() && !r.racing());
            // Asked for, it comes with the start
            assert!(r.wants_puzzle());
            assert_eq!(r.game.status.get_untracked(), Status::Empty);
            // The next request comes right at the start
            r.now.set(START - 300);
            assert_eq!(r.next_poll_ms(), 350);
            r.now.set(START);
            r.apply(view(RaceOutcome::Racing, START), START, START);
            assert!(r.has_board() && r.racing());
            assert_eq!(r.game.status.get_untracked(), Status::Playing);
            assert!(!r.wants_puzzle());
        });
    }

    #[test]
    fn the_puzzle_stays_when_later_answers_leave_it_out() {
        tab(|r| {
            r.apply(view(RaceOutcome::Racing, START), START, START);
            r.game.cycle(0, 1);
            let mut later = view(RaceOutcome::Racing, START + 1_000);
            later.puzzle = None;
            r.apply(later, START + 1_000, START + 1_000);
            assert_eq!(r.puzzle(), Some(puzzle()));
            // The board isn't loaded again: the bridge stays
            assert_eq!(r.game.bridges.get_untracked(), vec![1]);
        });
    }

    #[test]
    fn the_clock_runs_on_the_servers_time_with_penalties() {
        tab(|r| {
            // The browser is 5 s behind the server; the answer took 200 ms
            r.apply(
                view(RaceOutcome::Racing, START),
                START - 5_100,
                START - 4_900,
            );
            assert_eq!(r.sync.get_untracked().skew, 5_000);
            r.now.set(START - 5_000 + 60_000);
            assert_eq!(r.clock_ms(), 60_000);
            let penalties = HelpPenalties::of(Difficulty::Easy);
            assert_eq!(r.penalty_ms(), penalties.hint_ms + penalties.check_ms);
            assert_eq!(r.elapsed_ms(), 60_000 + r.penalty_ms());
            r.show(view(
                RaceOutcome::Solved {
                    time_ms: 90_000,
                    place: Some(1),
                },
                START + 61_000,
            ));
            assert_eq!(r.elapsed_ms(), 90_000);
            assert_eq!(r.game.status.get_untracked(), Status::Won);
            assert!(!r.racing());
        });
    }

    #[test]
    fn a_solved_board_goes_to_the_server_once() {
        tab(|r| {
            r.now.set(START);
            r.apply(view(RaceOutcome::Racing, START), START, START);
            r.game.cycle(0, 1);
            r.game.status.set(Status::Won);
            assert!(r.ready_to_submit());
            r.busy.set(true);
            assert!(!r.ready_to_submit());
            r.busy.set(false);
            r.failed.set(true);
            assert!(!r.ready_to_submit());
            r.failed.set(false);
            // Over before it went: nothing to submit
            let mut over = view(RaceOutcome::Out, START + 10_000);
            over.ended_at = Some(START + 10_000);
            over.solution = Some(vec![1]);
            r.now.set(START + 10_000);
            r.apply(over, START + 10_000, START + 10_000);
            assert!(!r.ready_to_submit());
            // The solution on the board, locked
            assert_eq!(r.game.status.get_untracked(), Status::Revealed);
            assert_eq!(r.game.bridges.get_untracked(), vec![1]);
        });
    }

    #[test]
    fn watchers_have_no_board_and_send_no_progress() {
        tab(|r| {
            let mut watching = view(RaceOutcome::Racing, START);
            watching.me = 9;
            watching.puzzle = None;
            r.now.set(START);
            r.apply(watching.clone(), START, START);
            assert!(!r.has_board() && !r.racing() && !r.wants_puzzle());
            assert_eq!(r.take_progress(START), None);
            assert_eq!(r.next_poll_ms(), 1_000);
            // Once it's over, the puzzle for the results
            watching.ended_at = Some(START + 5_000);
            r.apply(watching, START + 5_000, START + 5_000);
            assert!(r.wants_puzzle());
        });
    }

    #[test]
    fn progress_goes_with_a_request_once_a_second_while_racing() {
        tab(|r| {
            let mut started = view(RaceOutcome::Racing, START);
            started.puzzle = Some(double());
            r.now.set(START);
            r.apply(started, START, START);
            assert_eq!(r.progress(), 0);
            assert_eq!(r.take_progress(START), Some(0));
            assert_eq!(r.take_progress(START + 2_000), None, "unchanged");
            r.game.cycle(0, 1);
            assert_eq!(r.progress(), 50);
            assert_eq!(r.take_progress(START + 2_000), Some(50));
            r.game.cycle(0, 1);
            // Solved on the board, but only the server says so
            assert_eq!(r.progress(), PROGRESS_CAP);
            assert_eq!(r.take_progress(START + 2_500), None, "too soon");
            assert_eq!(r.take_progress(START + 3_000), Some(PROGRESS_CAP));
            // A failed request: it goes again
            r.progress_failed();
            assert_eq!(r.take_progress(START + 3_100), Some(PROGRESS_CAP));
        });
    }

    #[test]
    fn moves_are_logged_on_the_races_clock_while_racing() {
        tab(|r| {
            r.now.set(START);
            r.apply(view(RaceOutcome::Racing, START), START, START);
            r.record_moves();
            r.now.set(START + 2_500);
            r.game.cycle(0, 1);
            r.record_moves();
            assert_eq!(
                r.log(),
                [MoveEvent {
                    t: 2_500,
                    edge: 0,
                    count: 1,
                    tap: true
                }]
            );
            // Done: nothing more is logged
            r.show(view(RaceOutcome::GaveUp, START + 3_000));
            r.game.status.set(Status::Playing);
            r.game.cycle(0, 1);
            r.record_moves();
            assert_eq!(r.log().len(), 1);
        });
    }

    #[test]
    fn a_rematch_on_the_same_page_starts_afresh() {
        tab(|r| {
            r.now.set(START);
            r.apply(view(RaceOutcome::Racing, START), START, START);
            r.won_here.set(true);
            let _ = r.take_progress(START);
            let mut next = view(RaceOutcome::Racing, START);
            next.id = 8;
            next.starts_at = None;
            next.puzzle = None;
            r.apply(next, START, START);
            assert!(!r.won_here.get_untracked() && !r.has_board());
            assert_eq!(r.phase(), Some(RacePhase::Lobby));
        });
    }
}
