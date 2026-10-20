//! The game being played: puzzle, bridges, undo and redo, saved in the browser.

use common::{Board, Difficulty, GenerateParams, Puzzle, SolveReport};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::browser::{load, store};
use crate::i18n::{Locale, td_string};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Generated(GenerateParams),
    Daily {
        date: String,
        params: GenerateParams,
    },
    Custom,
    Imported(String),
}

impl Source {
    pub fn title(&self, locale: Locale) -> String {
        match self {
            Self::Generated(p) => format!(
                "{} {}×{}",
                crate::lang::difficulty_name(locale, p.difficulty),
                p.width,
                p.height
            ),
            Self::Daily { date, .. } => td_string!(locale, common.source.daily, date),
            Self::Custom => td_string!(locale, common.source.custom).to_string(),
            Self::Imported(name) => td_string!(locale, common.source.imported, name),
        }
    }

    pub fn difficulty(&self) -> Option<Difficulty> {
        match self {
            Self::Generated(p) | Self::Daily { params: p, .. } => Some(p.difficulty),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Nothing loaded yet.
    Empty,
    Playing,
    /// Solved by the player.
    Won,
    /// The SAT solver showed the solution.
    Revealed,
}

#[derive(Serialize, Deserialize)]
struct SavedGame {
    puzzle: Puzzle,
    bridges: Vec<u8>,
    solution: Option<Vec<u8>>,
    unique: Option<bool>,
    source: Source,
    status: Status,
    elapsed: u32,
    moves: u32,
    hints: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Game {
    pub board: RwSignal<Board>,
    pub bridges: RwSignal<Vec<u8>>,
    pub undo_stack: RwSignal<Vec<Vec<u8>>>,
    pub redo_stack: RwSignal<Vec<Vec<u8>>>,
    /// Solution from the generator or the SAT solver.
    pub solution: RwSignal<Option<Vec<u8>>>,
    pub unique: RwSignal<Option<bool>>,
    pub source: RwSignal<Source>,
    pub status: RwSignal<Status>,
    pub elapsed: RwSignal<u32>,
    pub moves: RwSignal<u32>,
    pub hints: RwSignal<u32>,
    pub wrong: RwSignal<Vec<usize>>,
    pub hint: RwSignal<Option<usize>>,
    pub loading: RwSignal<bool>,
    /// Incremented for every new puzzle, so late async results can be dropped.
    pub epoch: RwSignal<u32>,
    /// Solver stats of the last run on this puzzle.
    pub report: RwSignal<Option<SolveReport>>,
    pub solving: RwSignal<bool>,
    restored: StoredValue<bool>,
    /// Local storage key of the saved game.
    key: &'static str,
}

impl Game {
    pub fn new(key: &'static str) -> Self {
        let board = Board::new(Puzzle::empty(7, 7));
        Self {
            bridges: RwSignal::new(board.empty_bridges()),
            board: RwSignal::new(board),
            undo_stack: RwSignal::new(vec![]),
            redo_stack: RwSignal::new(vec![]),
            solution: RwSignal::new(None),
            unique: RwSignal::new(None),
            source: RwSignal::new(Source::Custom),
            status: RwSignal::new(Status::Empty),
            elapsed: RwSignal::new(0),
            moves: RwSignal::new(0),
            hints: RwSignal::new(0),
            wrong: RwSignal::new(vec![]),
            hint: RwSignal::new(None),
            loading: RwSignal::new(false),
            epoch: RwSignal::new(0),
            report: RwSignal::new(None),
            solving: RwSignal::new(false),
            restored: StoredValue::new(false),
            key,
        }
    }

    /// Restores the saved game once (browser only). Returns whether a game is loaded.
    pub fn ensure_restored(&self) -> bool {
        if !self.restored.get_value() {
            self.restored.set_value(true);
            self.restore();
        }
        self.status.get_untracked() != Status::Empty
    }

    pub fn set_puzzle(
        &self,
        puzzle: Puzzle,
        solution: Option<Vec<u8>>,
        unique: Option<bool>,
        source: Source,
    ) {
        let board = Board::new(puzzle);
        let solution = solution.filter(|s| s.len() == board.edges.len() && board.check(s).solved);
        self.bridges.set(board.empty_bridges());
        self.board.set(board);
        self.solution.set(solution);
        self.unique.set(unique);
        self.source.set(source);
        self.status.set(Status::Playing);
        self.elapsed.set(0);
        self.moves.set(0);
        self.hints.set(0);
        self.undo_stack.set(vec![]);
        self.redo_stack.set(vec![]);
        self.wrong.set(vec![]);
        self.hint.set(None);
        self.report.set(None);
        self.solving.set(false);
        self.loading.set(false);
        self.epoch.update(|e| *e += 1);
    }

    pub fn is_playing(&self) -> bool {
        self.status.get_untracked() == Status::Playing
    }

    /// Applies bridge changes as one undoable step.
    pub fn set_edges(&self, changes: &[(usize, u8)]) {
        let before = self.bridges.get_untracked();
        if changes.iter().all(|&(e, v)| before.get(e) == Some(&v)) {
            return;
        }
        self.undo_stack.update(|u| {
            u.push(before);
            if u.len() > 500 {
                u.remove(0);
            }
        });
        self.redo_stack.update(Vec::clear);
        self.bridges.update(|b| {
            for &(e, v) in changes {
                if let Some(slot) = b.get_mut(e) {
                    *slot = v;
                }
            }
        });
        self.wrong.set(vec![]);
    }

    /// Adds (+1) or removes (-1) a bridge, cycling between 0, 1 and 2.
    pub fn cycle(&self, edge: usize, delta: i8) {
        if !self.is_playing() {
            return;
        }
        let current = self
            .bridges
            .with_untracked(|b| b.get(edge).copied().unwrap_or(0));
        let next = (current.cast_signed() + delta)
            .rem_euclid(3)
            .cast_unsigned();
        self.set_edges(&[(edge, next)]);
        self.moves.update(|m| *m += 1);
        self.hint.set(None);
    }

    pub fn undo(&self) {
        if !self.is_playing() {
            return;
        }
        if let Some(prev) = self.undo_stack.try_update(Vec::pop).flatten() {
            let current = self.bridges.get_untracked();
            self.redo_stack.update(|r| r.push(current));
            self.bridges.set(prev);
            self.wrong.set(vec![]);
        }
    }

    pub fn redo(&self) {
        if !self.is_playing() {
            return;
        }
        if let Some(next) = self.redo_stack.try_update(Vec::pop).flatten() {
            let current = self.bridges.get_untracked();
            self.undo_stack.update(|u| u.push(current));
            self.bridges.set(next);
            self.wrong.set(vec![]);
        }
    }

    /// Removes all bridges and starts over.
    pub fn restart(&self) {
        let empty = self.board.with_untracked(Board::empty_bridges);
        if self.status.get_untracked() == Status::Playing {
            let changes: Vec<(usize, u8)> = (0..empty.len()).map(|e| (e, 0)).collect();
            self.set_edges(&changes);
        } else {
            self.undo_stack.set(vec![]);
            self.redo_stack.set(vec![]);
            self.bridges.set(empty);
            self.status.set(Status::Playing);
            self.elapsed.set(0);
            self.moves.set(0);
            self.hints.set(0);
        }
        self.hint.set(None);
    }

    pub fn puzzle(&self) -> Puzzle {
        self.board.with_untracked(|b| b.puzzle.clone())
    }

    pub(crate) fn save(&self) {
        if self.status.get() == Status::Empty {
            return;
        }
        let saved = SavedGame {
            puzzle: self.board.with(|b| b.puzzle.clone()),
            bridges: self.bridges.get(),
            solution: self.solution.get(),
            unique: self.unique.get(),
            source: self.source.get(),
            status: self.status.get(),
            elapsed: self.elapsed.get(),
            moves: self.moves.get(),
            hints: self.hints.get(),
        };
        store(self.key, &saved);
    }

    /// Restores the last game from local storage.
    fn restore(&self) -> bool {
        let Some(saved) = load::<SavedGame>(self.key) else {
            return false;
        };
        if saved.puzzle.validate().is_err() || saved.bridges.iter().any(|&b| b > 2) {
            return false;
        }
        let board = Board::new(saved.puzzle);
        if saved.bridges.len() != board.edges.len() || board.puzzle.islands.is_empty() {
            return false;
        }
        self.set_puzzle(board.puzzle, saved.solution, saved.unique, saved.source);
        self.bridges.set(saved.bridges);
        self.status.set(saved.status);
        self.elapsed.set(saved.elapsed);
        self.moves.set(saved.moves);
        self.hints.set(saved.hints);
        true
    }
}

impl Game {
    /// Restores the saved bridges if they belong to `puzzle` (daily progress of today).
    pub fn restore_progress(&self, puzzle: &Puzzle) -> bool {
        let Some(saved) = load::<SavedGame>(self.key) else {
            return false;
        };
        if saved.puzzle != *puzzle
            || saved.bridges.len() != self.bridges.with_untracked(Vec::len)
            || saved.bridges.iter().any(|&b| b > 2)
        {
            return false;
        }
        self.bridges.set(saved.bridges);
        self.moves.set(saved.moves);
        true
    }
}

/// The daily puzzle, separate from the practice game so neither overwrites the other.
#[derive(Clone, Copy, Debug)]
pub struct DailyGame(pub Game);

#[cfg(test)]
mod tests {
    use common::Difficulty;
    use proptest::prelude::*;
    use proptest::test_runner::{RngSeed, contextualize_config};

    use super::*;

    /// The same cases on every run, so CI never fails at random. `PROPTEST_RNG_SEED` (other
    /// cases) and `PROPTEST_CASES` (more of them) search further.
    fn config(cases: u32) -> ProptestConfig {
        contextualize_config(ProptestConfig {
            cases,
            rng_seed: RngSeed::Fixed(0x4a5e),
            failure_persistence: None,
            ..ProptestConfig::default()
        })
    }

    /// A game being played on a row of islands with `n` bridges between them.
    fn playing(n: usize) -> Game {
        let width = u8::try_from(n + 1).unwrap();
        let mut puzzle = Puzzle::empty(width, 1);
        for x in 0..width {
            puzzle.set(common::Pos::new(x, 0), 2);
        }
        let game = Game::new("test.game");
        game.set_puzzle(puzzle, None, None, Source::Custom);
        assert_eq!(game.board.with_untracked(|b| b.edges.len()), n);
        game
    }

    proptest! {
        #![proptest_config(config(128))]

        /// Undo steps back through every click in reverse, redo replays them, a new click
        /// after undoing drops what could be redone, and a restart is one undoable step.
        #[test]
        fn undo_and_redo_retrace_the_moves(
            n in 1..8usize,
            clicks in prop::collection::vec((0..8usize, prop::bool::ANY), 1..40),
            back in 0..45usize,
            new_click in 0..8usize,
        ) {
            Owner::new().with(|| {
                let game = playing(n);
                let mut states = vec![game.bridges.get_untracked()];
                for &(edge, add) in &clicks {
                    game.cycle(edge % n, if add { 1 } else { -1 });
                    let bridges = game.bridges.get_untracked();
                    prop_assert!(bridges.len() == n && bridges.iter().all(|&b| b <= 2));
                    // Every click changes its bridge
                    prop_assert_ne!(states.last(), Some(&bridges));
                    states.push(bridges);
                }
                prop_assert_eq!(game.moves.get_untracked() as usize, clicks.len());
                let back = back.min(clicks.len() + 2);
                for _ in 0..back {
                    game.undo();
                }
                let at = clicks.len().saturating_sub(back);
                prop_assert_eq!(&game.bridges.get_untracked(), &states[at]);
                for _ in 0..back {
                    game.redo();
                }
                prop_assert_eq!(&game.bridges.get_untracked(), states.last().unwrap());

                // A click after undoing starts a new branch: nothing to redo
                game.undo();
                game.cycle(new_click % n, 1);
                let branch = game.bridges.get_untracked();
                game.redo();
                prop_assert_eq!(&game.bridges.get_untracked(), &branch);
                prop_assert!(game.redo_stack.with_untracked(Vec::is_empty));

                game.restart();
                prop_assert_eq!(game.bridges.get_untracked(), vec![0; n]);
                // (Restarting an empty board changes nothing, so it isn't a step)
                if branch.iter().any(|&b| b > 0) {
                    game.undo();
                    prop_assert_eq!(game.bridges.get_untracked(), branch);
                }
                Ok(())
            })?;
        }
    }

    /// The title above the board names where the puzzle came from, in the player's language.
    #[test]
    fn sources_have_titles() {
        use crate::i18n::Locale;
        let params = GenerateParams {
            width: 10,
            height: 7,
            difficulty: Difficulty::Hard,
            seed: 1,
        };
        let daily = Source::Daily {
            date: "Mon 19 Oct 2026".into(),
            params,
        };
        assert_eq!(Source::Generated(params).title(Locale::en), "Hard 10×7");
        assert!(daily.title(Locale::en).contains("Mon 19 Oct 2026"));
        assert!(
            Source::Imported("x.txt".into())
                .title(Locale::de)
                .contains("x.txt")
        );
        assert_ne!(Source::Custom.title(Locale::ja), "");
        assert_eq!(daily.difficulty(), Some(Difficulty::Hard));
        assert_eq!(Source::Custom.difficulty(), None);
    }

    /// Only the last 500 steps can be undone, and a finished game takes no more clicks.
    #[test]
    fn undo_history_is_bounded() {
        Owner::new().with(|| {
            let game = playing(1);
            for _ in 0..600 {
                game.cycle(0, 1);
            }
            assert_eq!(game.undo_stack.with_untracked(Vec::len), 500);
            game.set_edges(&[(0, game.bridges.get_untracked()[0])]);
            assert_eq!(
                game.undo_stack.with_untracked(Vec::len),
                500,
                "no change, no step"
            );
            game.status.set(Status::Won);
            let before = game.bridges.get_untracked();
            game.cycle(0, 1);
            game.undo();
            assert_eq!(game.bridges.get_untracked(), before);
            // Restarting a finished game starts it over without history
            game.restart();
            assert_eq!(game.status.get_untracked(), Status::Playing);
            assert_eq!(game.bridges.get_untracked(), vec![0]);
            assert!(game.undo_stack.with_untracked(Vec::is_empty));
            assert_eq!(game.moves.get_untracked(), 0);
        });
    }

    /// The saved game (`hashi.game.v2`, `hashi.daily.v1`) of every browser. A different format
    /// loses the game in progress of everyone after a deploy.
    #[test]
    fn saved_games_still_load() {
        let params = GenerateParams {
            width: 3,
            height: 3,
            difficulty: Difficulty::Hard,
            seed: 42,
        };
        let saved = |source, status| SavedGame {
            puzzle: crate::golden::puzzle(),
            bridges: vec![2, 0],
            solution: Some(vec![2, 1]),
            unique: Some(true),
            source,
            status,
            elapsed: 61,
            moves: 4,
            hints: 1,
        };
        let games = [
            saved(Source::Generated(params), Status::Playing),
            saved(
                Source::Daily {
                    date: "Mon 19 Oct 2026".into(),
                    params,
                },
                Status::Won,
            ),
            saved(Source::Custom, Status::Revealed),
            saved(Source::Imported("test1.txt".into()), Status::Empty),
        ];
        let lines: Vec<String> = games
            .iter()
            .map(|game| {
                let json = serde_json::to_string(game).unwrap();
                let loaded: SavedGame = serde_json::from_str(&json).unwrap();
                assert_eq!(serde_json::to_string(&loaded).unwrap(), json);
                json
            })
            .collect();
        crate::golden::check(
            "saved_games.txt",
            &lines.join("\n"),
            "the games saved in the browsers wouldn't load any more: everybody loses the puzzle \
             they are playing",
        );
    }

    /// A game played a bit: two bridges, a hint, a minute on the clock.
    fn played(key: &'static str) -> Game {
        let game = Game::new(key);
        let params = GenerateParams {
            width: 3,
            height: 3,
            difficulty: Difficulty::Easy,
            seed: 7,
        };
        game.set_puzzle(
            crate::golden::puzzle(),
            Some(vec![2, 1]),
            Some(true),
            Source::Generated(params),
        );
        game.cycle(0, 1);
        game.cycle(1, 1);
        game.hints.set(1);
        game.elapsed.set(61);
        game
    }

    #[test]
    fn a_saved_game_comes_back_in_a_new_tab() {
        Owner::new().with(|| {
            let game = played("test.reload");
            game.save();

            let tab = Game::new("test.reload");
            assert!(tab.ensure_restored());
            assert_eq!(tab.puzzle(), game.puzzle());
            assert_eq!(tab.bridges.get_untracked(), vec![1, 1]);
            assert_eq!(tab.solution.get_untracked(), Some(vec![2, 1]));
            assert_eq!(tab.unique.get_untracked(), Some(true));
            assert_eq!(tab.source.get_untracked(), game.source.get_untracked());
            assert_eq!(tab.status.get_untracked(), Status::Playing);
            assert_eq!(
                (
                    tab.elapsed.get_untracked(),
                    tab.moves.get_untracked(),
                    tab.hints.get_untracked()
                ),
                (61, 2, 1)
            );
            // The undo history stays behind
            assert!(tab.undo_stack.with_untracked(Vec::is_empty));
            tab.undo();
            assert_eq!(tab.bridges.get_untracked(), vec![1, 1]);
        });
    }

    #[test]
    fn a_finished_game_comes_back_finished() {
        Owner::new().with(|| {
            let game = played("test.finished");
            game.status.set(Status::Revealed);
            game.save();

            let tab = Game::new("test.finished");
            assert!(tab.ensure_restored());
            assert_eq!(tab.status.get_untracked(), Status::Revealed);
            assert!(!tab.is_playing());
        });
    }

    /// Restoring happens once per tab: later saves (of the same game) don't reset the board.
    #[test]
    fn a_game_is_restored_only_once() {
        Owner::new().with(|| {
            let game = played("test.once");
            game.save();
            let tab = Game::new("test.once");
            assert!(tab.ensure_restored());
            tab.cycle(0, 1);
            game.cycle(1, 1);
            game.save();
            assert!(tab.ensure_restored());
            assert_eq!(tab.bridges.get_untracked(), vec![2, 1]);
        });
    }

    #[test]
    fn without_a_saved_game_nothing_is_loaded() {
        Owner::new().with(|| {
            let tab = Game::new("test.nothing");
            assert!(!tab.ensure_restored());
            assert_eq!(tab.status.get_untracked(), Status::Empty);
        });
    }

    /// The empty board shown before anything is loaded never replaces a saved game.
    #[test]
    fn an_empty_board_is_not_saved() {
        Owner::new().with(|| {
            played("test.empty").save();
            Game::new("test.empty").save();
            assert!(Game::new("test.empty").ensure_restored());
        });
    }

    /// What doesn't fit (another version, a damaged entry) is ignored rather than shown broken.
    #[test]
    fn saved_games_that_dont_fit_are_ignored() {
        Owner::new().with(|| {
            let saved = |puzzle, bridges| SavedGame {
                puzzle,
                bridges,
                solution: None,
                unique: None,
                source: Source::Custom,
                status: Status::Playing,
                elapsed: 0,
                moves: 0,
                hints: 0,
            };
            store("test.bad", &saved(crate::golden::puzzle(), vec![1, 1, 1]));
            assert!(!Game::new("test.bad").ensure_restored());
            store("test.bad", &saved(Puzzle::empty(3, 3), vec![]));
            assert!(!Game::new("test.bad").ensure_restored());
            crate::state::browser::store_raw("test.bad", "{\"puzzle\":");
            assert!(!Game::new("test.bad").ensure_restored());
        });
    }

    /// The daily keeps only the bridges and the moves of the saved game, and only on the same
    /// puzzle: the clock and the result come from the server.
    #[test]
    fn progress_is_restored_onto_the_same_puzzle_only() {
        Owner::new().with(|| {
            let game = played("test.progress");
            game.status.set(Status::Won);
            game.save();

            let tab = Game::new("test.progress");
            tab.set_puzzle(crate::golden::puzzle(), None, None, Source::Custom);
            assert!(tab.restore_progress(&crate::golden::puzzle()));
            assert_eq!(tab.bridges.get_untracked(), vec![1, 1]);
            assert_eq!(tab.moves.get_untracked(), 2);
            assert_eq!(tab.status.get_untracked(), Status::Playing);
            assert_eq!(
                (tab.elapsed.get_untracked(), tab.hints.get_untracked()),
                (0, 0)
            );

            let mut other = crate::golden::puzzle();
            other.set(common::Pos::new(0, 2), 1);
            let tab = Game::new("test.progress");
            tab.set_puzzle(other.clone(), None, None, Source::Custom);
            assert!(!tab.restore_progress(&other));
            assert!(tab.bridges.with_untracked(|b| b.iter().all(|&n| n == 0)));

            assert!(!Game::new("test.no.progress").restore_progress(&other));
        });
    }

    /// A solution that doesn't fit the puzzle (a stale one) is dropped, and every new puzzle
    /// starts a new epoch, so late solver results for the old one are ignored.
    #[test]
    fn a_new_puzzle_starts_over() {
        Owner::new().with(|| {
            let game = played("test.new");
            let epoch = game.epoch.get_untracked();
            game.wrong.set(vec![0]);
            game.set_puzzle(crate::golden::puzzle(), Some(vec![1]), None, Source::Custom);
            assert_eq!(game.solution.get_untracked(), None);
            assert_eq!(game.epoch.get_untracked(), epoch + 1);
            assert_eq!(game.bridges.get_untracked(), vec![0, 0]);
            assert_eq!(game.moves.get_untracked(), 0);
            assert!(game.wrong.with_untracked(Vec::is_empty));
            assert!(game.undo_stack.with_untracked(Vec::is_empty));
        });
    }
}
