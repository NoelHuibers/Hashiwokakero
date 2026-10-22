//! A group puzzle on its page: the server's page data, the clocks and the move log.

use common::{Board, Puzzle};
use leptos::prelude::*;

use crate::model::{AttemptInfo, MoveEvent, MyPuzzle, PuzzlePage};
use crate::state::{Game, Source, Status, load, store};

/// The board of the group puzzle being played, kept in the browser like the daily's.
pub const GAME_KEY: &str = "hashi.kadai.v1";
/// Its move log, kept in the browser so a reload doesn't lose it.
pub(super) const LOG_KEY: &str = "hashi.kadai.log";

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub(super) struct SavedLog {
    pub(super) puzzle: i64,
    pub(super) events: Vec<MoveEvent>,
}

/// The last board the log saw, with the puzzle and game it belongs to, so loading another one
/// is never logged as moves.
#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    puzzle: Option<i64>,
    epoch: u32,
    bridges: Vec<u8>,
    moves: u32,
}

/// What the page shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The viewer's to start: the cover.
    Cover,
    Playing,
    Solved,
    GaveUp,
    /// The author's view: the solution, how everyone does.
    Author,
    /// Closed before the viewer finished: the final board.
    Closed,
}

impl Mode {
    pub fn of(page: &PuzzlePage) -> Self {
        match page.summary.mine {
            MyPuzzle::Author => Self::Author,
            MyPuzzle::Solved { .. } => Self::Solved,
            MyPuzzle::GaveUp => Self::GaveUp,
            MyPuzzle::Fresh | MyPuzzle::Playing if !page.summary.open => Self::Closed,
            MyPuzzle::Fresh => Self::Cover,
            MyPuzzle::Playing => Self::Playing,
        }
    }

    /// The viewer's own board is shown, the game: playing or after.
    pub fn played(self) -> bool {
        matches!(self, Self::Playing | Self::Solved | Self::GaveUp)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Play {
    pub game: Game,
    /// The page as the server sent it last, `None` while loading.
    pub page: RwSignal<Option<PuzzlePage>>,
    /// Server clock minus browser clock.
    pub skew: RwSignal<i64>,
    /// Browser clock, ticking while the clock runs.
    pub now: RwSignal<i64>,
    /// A request is on its way.
    pub busy: RwSignal<bool>,
    /// The submit of the solved board failed: it waits for the player's retry.
    pub failed: RwSignal<bool>,
    /// Solved in this tab just now: the petals.
    pub won_here: RwSignal<bool>,
    pub(super) logged: StoredValue<Option<Snapshot>>,
}

impl Play {
    pub fn new(game: Game) -> Self {
        Self {
            game,
            page: RwSignal::new(None),
            skew: RwSignal::new(0),
            now: RwSignal::new(0),
            busy: RwSignal::new(false),
            failed: RwSignal::new(false),
            won_here: RwSignal::new(false),
            logged: StoredValue::new(None),
        }
    }

    pub fn id(&self) -> Option<i64> {
        self.page
            .with_untracked(|p| p.as_ref().map(|p| p.summary.id))
    }

    pub fn mode(&self) -> Option<Mode> {
        self.page.with(|p| p.as_ref().map(Mode::of))
    }

    fn attempt<T>(&self, f: impl FnOnce(&AttemptInfo) -> T) -> Option<T> {
        self.page
            .with(|p| p.as_ref().and_then(|p| p.attempt.as_ref()).map(f))
    }

    pub fn started(&self) -> bool {
        self.attempt(|_| ()).is_some()
    }

    pub fn finished(&self) -> bool {
        self.attempt(AttemptInfo::finished).unwrap_or(false)
    }

    pub fn server_now(&self) -> i64 {
        self.now.get() + self.skew.get()
    }

    /// Ms since the start on the server's clock, without penalties: the move log's time.
    pub fn clock_ms(&self) -> u32 {
        let now = self.server_now();
        let started = self.page.with_untracked(|p| {
            p.as_ref()
                .and_then(|p| p.attempt.as_ref())
                .map(|a| a.started_at)
        });
        started.map_or(0, |s| u32::try_from((now - s).max(0)).unwrap_or(u32::MAX))
    }

    /// The running time with penalties, or the final time.
    pub fn elapsed_ms(&self) -> i64 {
        let now = self.server_now();
        self.attempt(|a| match (a.time_ms, a.finished_at) {
            (Some(t), _) => t,
            (None, Some(end)) => end - a.started_at + a.penalty_ms(),
            _ if a.revealed => 0,
            _ => now - a.started_at + a.penalty_ms(),
        })
        .unwrap_or(0)
    }

    pub fn penalty_ms(&self) -> i64 {
        self.attempt(AttemptInfo::penalty_ms).unwrap_or(0)
    }

    /// Shows what the server sent. The viewer's board is loaded once the clock runs: a running
    /// one keeps (or gets back from the browser) the bridges placed so far, a finished one shows
    /// how it ended, locked.
    pub fn show(&self, page: PuzzlePage) {
        if let (Some(puzzle), Some(attempt)) = (&page.puzzle, &page.attempt) {
            let game = self.game;
            let ended = if attempt.revealed {
                Some(Status::Revealed)
            } else if attempt.finished_at.is_some() {
                Some(Status::Won)
            } else {
                None
            };
            let loaded = game.status.get_untracked() != Status::Empty
                && game.board.with_untracked(|b| b.puzzle == *puzzle)
                && self.id() == Some(page.summary.id);
            match ended {
                Some(status) => {
                    if !loaded {
                        self.load(puzzle, &page);
                    }
                    if let Some(bridges) = &attempt.bridges {
                        game.bridges.set(bridges.clone());
                    }
                    game.status.set(status);
                }
                None if !loaded => {
                    self.load(puzzle, &page);
                    // Solved before a reload: it goes to the server again
                    game.restore_progress(puzzle);
                }
                None => {}
            }
        }
        self.page.set(Some(page));
    }

    fn load(&self, puzzle: &Puzzle, page: &PuzzlePage) {
        let title = page.summary.title.clone().unwrap_or_default();
        self.game
            .set_puzzle(puzzle.clone(), None, Some(true), Source::Imported(title));
        self.failed.set(false);
    }

    /// The solved board can go to the server: once, not while something else is on its way,
    /// and after a failed submit only when the player says so.
    pub fn ready_to_submit(&self) -> bool {
        self.game.status.get() == Status::Won
            && self.started()
            && !self.finished()
            && !self.busy.get()
            && !self.failed.get()
    }

    /// The saved move log of this puzzle.
    pub fn log(&self) -> Vec<MoveEvent> {
        let id = self.id();
        load::<SavedLog>(LOG_KEY)
            .filter(|l| Some(l.puzzle) == id)
            .map(|l| l.events)
            .unwrap_or_default()
    }

    /// Logs the bridge changes since the last call, while the clock runs: a click changes one
    /// bridge and counts a move, hints, undo and restart don't.
    pub fn record_moves(&self) {
        let current = Snapshot {
            puzzle: self.page.with(|p| p.as_ref().map(|p| p.summary.id)),
            epoch: self.game.epoch.get(),
            bridges: self.game.bridges.get(),
            moves: self.game.moves.get_untracked(),
        };
        let previous = self.logged.get_value();
        self.logged.set_value(Some(current.clone()));
        let (Some(before), Some(puzzle)) = (previous, current.puzzle) else {
            return;
        };
        if before.puzzle != current.puzzle
            || before.epoch != current.epoch
            || before.bridges.len() != current.bridges.len()
            || !untrack(|| self.started())
            || untrack(|| self.finished())
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
        store(LOG_KEY, &saved);
    }

    /// Islands with all their bridges, and how many there are.
    pub fn islands(&self) -> (usize, usize) {
        self.game.board.with(|b: &Board| {
            self.game.bridges.with(|br| {
                let degrees = b.check(br).degrees;
                let done = b
                    .puzzle
                    .islands
                    .iter()
                    .zip(&degrees)
                    .filter(|(i, d)| i.required == **d)
                    .count();
                (done, b.puzzle.islands.len())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use common::{Island, Pos};

    use super::*;
    use crate::model::{CHECK_PENALTY_MS, HINT_PENALTY_MS, PuzzleSummary, TodayEntry, TodayState};

    /// Two islands with one bridge between them, the solution is `[1]`.
    fn puzzle() -> Puzzle {
        let island = |x| Island {
            pos: Pos { x, y: 0 },
            required: 1,
        };
        Puzzle::new(3, 1, vec![island(0), island(2)])
    }

    fn summary(mine: MyPuzzle, open: bool) -> PuzzleSummary {
        PuzzleSummary {
            id: 7,
            group_id: 1,
            title: Some("Pair".into()),
            author: None,
            width: 3,
            height: 1,
            islands: 2,
            created_at: 0,
            closes_at: 1_000_000,
            open,
            pips: vec![],
            mine,
            board: None,
            winner: None,
        }
    }

    /// The page as the server sends it: started at 1 s, finished at 61 s or still running.
    fn page(mine: MyPuzzle) -> PuzzlePage {
        let finished = matches!(mine, MyPuzzle::Solved { .. });
        let started = !matches!(mine, MyPuzzle::Fresh | MyPuzzle::Author);
        PuzzlePage {
            group_id: 1,
            group_name: "Club".into(),
            me: 2,
            summary: summary(mine, true),
            puzzle: started.then(puzzle),
            solution: None,
            attempt: started.then(|| AttemptInfo {
                attempt_id: String::new(),
                started_at: 1_000,
                finished_at: finished.then_some(61_000),
                hints: 0,
                checks: 0,
                revealed: mine == MyPuzzle::GaveUp,
                time_ms: finished.then_some(60_000),
                bridges: (finished || mine == MyPuzzle::GaveUp).then(|| vec![1]),
                flag: None,
            }),
            board: vec![TodayEntry {
                user_id: 2,
                name: "bram".into(),
                guest: false,
                colour: 0,
                stamp: None,
                state: TodayState::Waiting,
            }],
            board_open: false,
            stats: None,
            server_now: 70_000,
        }
    }

    fn tab(test: impl FnOnce(&Play)) {
        Owner::new().with(|| test(&Play::new(Game::new("test.kadai"))));
    }

    fn editable(play: &Play) -> bool {
        let before = play.game.bridges.get_untracked();
        play.game.cycle(0, 1);
        play.game.bridges.get_untracked() != before
    }

    #[test]
    fn the_page_shows_what_the_viewer_stands_for() {
        let solved = MyPuzzle::Solved {
            time_ms: 60_000,
            rank: Some(1),
        };
        assert_eq!(Mode::of(&page(MyPuzzle::Fresh)), Mode::Cover);
        assert_eq!(Mode::of(&page(MyPuzzle::Playing)), Mode::Playing);
        assert_eq!(Mode::of(&page(solved)), Mode::Solved);
        assert_eq!(Mode::of(&page(MyPuzzle::GaveUp)), Mode::GaveUp);
        assert_eq!(Mode::of(&page(MyPuzzle::Author)), Mode::Author);
        // Closed before finishing: the final board, whether started or not
        for mine in [MyPuzzle::Fresh, MyPuzzle::Playing] {
            let mut closed = page(mine);
            closed.summary.open = false;
            assert_eq!(Mode::of(&closed), Mode::Closed);
        }
        let mut closed = page(solved);
        closed.summary.open = false;
        assert_eq!(Mode::of(&closed), Mode::Solved);
        assert!(Mode::Playing.played() && Mode::GaveUp.played());
        assert!(!Mode::Author.played() && !Mode::Cover.played());
    }

    #[test]
    fn before_the_start_there_is_no_board_and_no_clock() {
        tab(|p| {
            p.show(page(MyPuzzle::Fresh));
            p.now.set(70_000);
            assert_eq!(p.game.status.get_untracked(), Status::Empty);
            assert!(!p.started() && !p.finished());
            assert_eq!((p.clock_ms(), p.elapsed_ms()), (0, 0));
            assert!(!p.ready_to_submit());
        });
    }

    #[test]
    fn a_running_puzzle_plays_on_the_servers_clock() {
        tab(|p| {
            let mut running = page(MyPuzzle::Playing);
            let a = running.attempt.as_mut().unwrap();
            a.hints = 1;
            a.checks = 2;
            p.show(running);
            assert_eq!(p.game.status.get_untracked(), Status::Playing);
            assert!(editable(p));
            p.now.set(60_000);
            p.skew.set(5_000);
            assert_eq!(p.clock_ms(), 64_000);
            assert_eq!(
                p.elapsed_ms(),
                64_000 + HINT_PENALTY_MS + 2 * CHECK_PENALTY_MS
            );
            assert_eq!(p.penalty_ms(), HINT_PENALTY_MS + 2 * CHECK_PENALTY_MS);
        });
    }

    #[test]
    fn a_finished_puzzle_shows_its_board_locked() {
        tab(|p| {
            p.show(page(MyPuzzle::Playing));
            p.show(page(MyPuzzle::Solved {
                time_ms: 60_000,
                rank: Some(1),
            }));
            assert_eq!(p.game.status.get_untracked(), Status::Won);
            assert_eq!(p.game.bridges.get_untracked(), vec![1]);
            assert!(!editable(p));
            assert_eq!(p.elapsed_ms(), 60_000);
            assert!(!p.ready_to_submit());
        });
        tab(|p| {
            p.show(page(MyPuzzle::GaveUp));
            assert_eq!(p.game.status.get_untracked(), Status::Revealed);
            assert_eq!(p.game.bridges.get_untracked(), vec![1]);
            assert!(!editable(p));
        });
    }

    #[test]
    fn a_solved_board_goes_to_the_server_once() {
        tab(|p| {
            p.show(page(MyPuzzle::Playing));
            p.game.cycle(0, 1);
            p.game.status.set(Status::Won);
            assert!(p.ready_to_submit());
            p.busy.set(true);
            assert!(!p.ready_to_submit());
            p.busy.set(false);
            // A failed submit waits for the retry
            p.failed.set(true);
            assert!(!p.ready_to_submit());
            // A page with another solved board loads it
            p.failed.set(false);
            p.show(page(MyPuzzle::Solved {
                time_ms: 60_000,
                rank: Some(1),
            }));
            assert!(!p.ready_to_submit());
        });
    }

    #[test]
    fn clicks_are_logged_on_the_server_clock_hints_and_undo_arent_taps() {
        tab(|p| {
            p.show(page(MyPuzzle::Playing));
            p.now.set(3_500);
            p.record_moves();
            p.game.cycle(0, 1);
            p.record_moves();
            p.game.undo();
            p.record_moves();
            p.game.set_edges(&[(0, 1)]);
            p.record_moves();
            let events = p.log();
            assert_eq!(
                events,
                [
                    MoveEvent {
                        t: 2_500,
                        edge: 0,
                        count: 1,
                        tap: true
                    },
                    MoveEvent {
                        t: 2_500,
                        edge: 0,
                        count: 0,
                        tap: false
                    },
                    MoveEvent {
                        t: 2_500,
                        edge: 0,
                        count: 1,
                        tap: false
                    },
                ]
            );
            // The clock ticking adds nothing
            p.now.set(9_000);
            p.record_moves();
            assert_eq!(p.log().len(), 3);
        });
    }

    #[test]
    fn another_puzzle_has_a_log_of_its_own() {
        tab(|p| {
            p.show(page(MyPuzzle::Playing));
            p.record_moves();
            p.game.cycle(0, 1);
            p.record_moves();
            assert_eq!(p.log().len(), 1);
            let mut other = page(MyPuzzle::Playing);
            other.summary.id = 8;
            p.show(other);
            assert_eq!(p.log(), []);
        });
    }
}
