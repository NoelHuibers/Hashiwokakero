//! Client state shared by all pages: the current game, the editor, solver settings and toasts.

use std::time::Duration;

use common::{
    Board, Difficulty, GenerateParams, Puzzle, SolveOutcome, SolveReport, SolverConfig,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::api::{generate_puzzle, solve_puzzle};
use crate::i18n::{Locale, td_string};

/* ------------------------------------------------------------------ */
/*  Toasts                                                             */
/* ------------------------------------------------------------------ */

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

/// The text of a toast in a given language, so it follows the language picker.
pub type ToastText = std::sync::Arc<dyn Fn(Locale) -> String + Send + Sync>;

#[derive(Clone)]
pub struct Toast {
    pub id: u32,
    pub kind: ToastKind,
    pub text: ToastText,
}

#[derive(Clone, Copy)]
pub struct Toasts(pub RwSignal<Vec<Toast>>);

impl Toasts {
    pub fn push(&self, kind: ToastKind, text: impl Fn(Locale) -> String + Send + Sync + 'static) {
        let list = self.0;
        let id = list.with_untracked(|l| l.iter().map(|t| t.id + 1).max().unwrap_or(0));
        list.update(|l| {
            l.push(Toast {
                id,
                kind,
                text: std::sync::Arc::new(text),
            });
            if l.len() > 3 {
                l.remove(0);
            }
        });
        set_timeout(
            move || list.update(|l| l.retain(|t| t.id != id)),
            Duration::from_millis(3600),
        );
    }
}

/// Shows a toast, e.g. `toast(ToastKind::Info, |l| td_string!(l, common.toast.copied).to_string())`.
pub fn toast(kind: ToastKind, text: impl Fn(Locale) -> String + Send + Sync + 'static) {
    if let Some(t) = use_context::<Toasts>() {
        t.push(kind, text);
    }
}

/* ------------------------------------------------------------------ */
/*  Game                                                               */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    Generated(GenerateParams),
    Daily { date: String, params: GenerateParams },
    Custom,
    Imported(String),
}

impl Source {
    pub fn title(&self, locale: Locale) -> String {
        match self {
            Source::Generated(p) => format!(
                "{} {}×{}",
                crate::lang::difficulty_name(locale, p.difficulty),
                p.width,
                p.height
            ),
            Source::Daily { date, .. } => td_string!(locale, common.source.daily, date).to_string(),
            Source::Custom => td_string!(locale, common.source.custom).to_string(),
            Source::Imported(name) => td_string!(locale, common.source.imported, name).to_string(),
        }
    }

    pub fn difficulty(&self) -> Option<Difficulty> {
        match self {
            Source::Generated(p) | Source::Daily { params: p, .. } => Some(p.difficulty),
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

const GAME_KEY: &str = "hashi.game.v2";
const DAILY_KEY: &str = "hashi.daily.v1";
const EDITOR_KEY: &str = "hashi.editor.v2";
const SOLVER_KEY: &str = "hashi.solver.v2";
const THEME_KEY: &str = "hashi.theme";

#[derive(Clone, Copy)]
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
        Game {
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
        let solution = solution.filter(|s| s.len() == board.edges.len());
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
        let current = self.bridges.with_untracked(|b| b.get(edge).copied().unwrap_or(0));
        let next = (current as i8 + delta).rem_euclid(3) as u8;
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
        if self.status.get_untracked() != Status::Playing {
            self.undo_stack.set(vec![]);
            self.redo_stack.set(vec![]);
            self.bridges.set(empty);
            self.status.set(Status::Playing);
            self.elapsed.set(0);
            self.moves.set(0);
            self.hints.set(0);
        } else {
            let changes: Vec<(usize, u8)> = (0..empty.len()).map(|e| (e, 0)).collect();
            self.set_edges(&changes);
        }
        self.hint.set(None);
    }

    pub fn puzzle(&self) -> Puzzle {
        self.board.with_untracked(|b| b.puzzle.clone())
    }

    fn save(&self) {
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
        let board = Board::new(saved.puzzle);
        if saved.bridges.len() != board.edges.len() || board.puzzle.islands.is_empty() {
            return false;
        }
        self.set_puzzle(board.puzzle.clone(), saved.solution, saved.unique, saved.source);
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
        if saved.puzzle != *puzzle || saved.bridges.len() != self.bridges.with_untracked(Vec::len) {
            return false;
        }
        self.bridges.set(saved.bridges);
        self.moves.set(saved.moves);
        true
    }
}

/// The daily puzzle, separate from the practice game so neither overwrites the other.
#[derive(Clone, Copy)]
pub struct DailyGame(pub Game);

/// The signed in user (or guest), loaded after hydration.
#[derive(Clone, Copy)]
pub struct Auth {
    pub user: RwSignal<Option<crate::model::UserInfo>>,
    pub loaded: RwSignal<bool>,
}

impl Auth {
    pub fn refresh(&self) {
        let auth = *self;
        spawn_local(async move {
            match crate::api::current_user().await {
                Ok(user) => auth.user.set(user),
                Err(e) => leptos::logging::warn!("could not load the user: {e}"),
            }
            auth.loaded.set(true);
        });
    }
}

/* ------------------------------------------------------------------ */
/*  Editor                                                             */
/* ------------------------------------------------------------------ */

#[derive(Clone, Copy)]
pub struct Editor {
    pub puzzle: RwSignal<Puzzle>,
    /// Bridges of the last solver run, shown on top of the edited puzzle.
    pub solution: RwSignal<Option<Vec<u8>>>,
    pub report: RwSignal<Option<SolveReport>>,
    pub solving: RwSignal<bool>,
}

impl Editor {
    pub fn new() -> Self {
        Editor {
            puzzle: RwSignal::new(Puzzle::empty(8, 8)),
            solution: RwSignal::new(None),
            report: RwSignal::new(None),
            solving: RwSignal::new(false),
        }
    }

    pub fn edit(&self, f: impl FnOnce(&mut Puzzle)) {
        self.puzzle.update(f);
        self.solution.set(None);
        self.report.set(None);
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

/* ------------------------------------------------------------------ */
/*  Settings                                                           */
/* ------------------------------------------------------------------ */

#[derive(Clone, Copy)]
pub struct Settings {
    pub solver: RwSignal<SolverConfig>,
    pub difficulty: RwSignal<Difficulty>,
    pub dark: RwSignal<bool>,
}

impl Settings {
    pub fn new() -> Self {
        Settings {
            solver: RwSignal::new(SolverConfig::default()),
            difficulty: RwSignal::new(Difficulty::Medium),
            dark: RwSignal::new(false),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

/// Creates all state and wires up persistence, the timer and win detection.
pub fn provide_state() {
    let game = Game::new(GAME_KEY);
    let daily = Game::new(DAILY_KEY);
    let editor = Editor::new();
    let settings = Settings::new();
    let auth = Auth {
        user: RwSignal::new(None),
        loaded: RwSignal::new(false),
    };
    provide_context(game);
    provide_context(DailyGame(daily));
    provide_context(auth);
    provide_context(editor);
    provide_context(settings);
    provide_context(Toasts(RwSignal::new(vec![])));

    // Everything below only runs in the browser
    Effect::new(move |first: Option<()>| {
        if first.is_some() {
            return;
        }
        if let Some(puzzle) = load::<Puzzle>(EDITOR_KEY) {
            editor.puzzle.set(puzzle);
        }
        if let Some(config) = load::<SolverConfig>(SOLVER_KEY) {
            settings.solver.set(config.normalized());
        }
        settings.dark.set(
            document()
                .document_element()
                .is_some_and(|el| el.class_list().contains("dark")),
        );
        auth.refresh();
    });

    Effect::new(move |_| game.save());
    Effect::new(move |_| daily.save());
    Effect::new(move |_| store(EDITOR_KEY, &editor.puzzle.get()));
    Effect::new(move |_| store(SOLVER_KEY, &settings.solver.get()));
    Effect::new(move |prev: Option<bool>| {
        let dark = settings.dark.get();
        if prev.is_some() {
            if let Some(el) = document().document_element() {
                let _ = if dark {
                    el.class_list().add_1("dark")
                } else {
                    el.class_list().remove_1("dark")
                };
            }
            store_raw(THEME_KEY, if dark { "dark" } else { "light" });
        }
        dark
    });

    // Win detection
    for g in [game, daily] {
        Effect::new(move |_| {
            let solved = g.board.with(|b| g.bridges.with(|br| b.check(br).solved));
            if solved && g.status.get_untracked() == Status::Playing {
                g.status.set(Status::Won);
            }
        });
    }

    // Timer, paused while the tab is hidden
    Effect::new(move |first: Option<()>| {
        if first.is_some() {
            return;
        }
        set_interval(
            move || {
                let visible = document().visibility_state() == web_sys::VisibilityState::Visible;
                if visible && game.status.get_untracked() == Status::Playing && !game.loading.get_untracked() {
                    game.elapsed.update(|e| *e += 1);
                }
            },
            Duration::from_secs(1),
        );
    });
}

/* ------------------------------------------------------------------ */
/*  Async actions                                                      */
/* ------------------------------------------------------------------ */

pub fn random_seed() -> u64 {
    let a = (js_sys::Math::random() * 4_294_967_296.0) as u64;
    let b = js_sys::Date::now() as u64;
    (a << 32) ^ b
}

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
                    toast(ToastKind::Error, |l| td_string!(l, common.toast.unsat).to_string());
                    None
                }
                SolveOutcome::Timeout => {
                    toast(ToastKind::Error, |l| td_string!(l, common.toast.timeout).to_string());
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

async fn game_solver(game: Game, settings: Settings, unique: bool) -> Option<Vec<u8>> {
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
    if let Some(SolveOutcome::Solved { unique: Some(u), .. }) =
        game.report.with_untracked(|r| r.as_ref().map(|r| r.outcome.clone()))
    {
        game.unique.set(Some(u));
    }
    if game.solution.get_untracked().is_none() {
        game.solution.set(solution.clone());
    }
    solution
}

async fn ensure_solution(game: Game, settings: Settings) -> Option<Vec<u8>> {
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
        let Some(solution) = ensure_solution(game, settings).await else { return };
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
            None => toast(ToastKind::Info, |l| td_string!(l, common.toast.nothing_to_hint).to_string()),
        }
    });
}

/// Flashes bridges that are not part of the solution.
pub fn check(game: Game, settings: Settings) {
    spawn_local(async move {
        let epoch = game.epoch.get_untracked();
        let Some(solution) = ensure_solution(game, settings).await else { return };
        if game.epoch.get_untracked() != epoch {
            return;
        }
        let wrong = game
            .bridges
            .with_untracked(|b| crate::model::wrong_edges(b, &solution));
        if wrong.is_empty() {
            toast(ToastKind::Success, |l| td_string!(l, common.toast.all_correct).to_string());
        } else {
            let count = wrong.len();
            toast(ToastKind::Error, move |l| {
                td_string!(l, common.toast.wrong_bridges, count).to_string()
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
        let Some(solution) = game_solver(game, settings, false).await else { return };
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
        let missing: Vec<usize> = (0..current.len()).filter(|&e| current[e] < solution[e]).collect();
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
        if game_solver(game, settings, true).await.is_some() {
            match game.unique.get_untracked() {
                Some(true) => toast(ToastKind::Success, |l| td_string!(l, common.toast.unique).to_string()),
                Some(false) => toast(ToastKind::Info, |l| td_string!(l, common.toast.not_unique).to_string()),
                None => {}
            }
        }
    });
}

/* ------------------------------------------------------------------ */
/*  Browser helpers                                                    */
/* ------------------------------------------------------------------ */

fn storage() -> Option<web_sys::Storage> {
    window().local_storage().ok().flatten()
}

pub fn load<T: DeserializeOwned>(key: &str) -> Option<T> {
    let raw = storage()?.get_item(key).ok()??;
    serde_json::from_str(&raw).ok()
}

pub fn store<T: Serialize>(key: &str, value: &T) {
    if let (Some(s), Ok(json)) = (storage(), serde_json::to_string(value)) {
        let _ = s.set_item(key, &json);
    }
}

fn store_raw(key: &str, value: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(key, value);
    }
}

/// Offers `content` as a file download.
pub fn download(filename: &str, content: &str, mime: &str) {
    use wasm_bindgen::JsCast;
    let parts = js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(content));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options) else { return };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else { return };
    if let Ok(link) = document().create_element("a")
        && let Ok(link) = link.dyn_into::<web_sys::HtmlAnchorElement>()
    {
        link.set_href(&url);
        link.set_download(filename);
        link.click();
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

pub fn format_time(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}
