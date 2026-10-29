//! A group puzzle (課題 kadai), `/groups/:id/puzzles/:puzzle`.
//!
//! Played like a daily, with the clock on the server, hints and checks at the daily's prices and
//! a ranking within the group. Its author watches instead, and once it's closed everyone sees the
//! final board.

pub mod author;
pub mod cover;
pub mod ranking;
pub mod replay;
pub mod result;
pub mod state;

use std::time::Duration;

use common::Board;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::{
    group_puzzle_check, group_puzzle_hint, group_puzzle_reveal, load_group_puzzle,
    submit_group_puzzle,
};
use crate::components::board::{BoardMode, BoardView};
use crate::components::celebration::{Petals, Stamp};
use crate::components::layout::use_localized;
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::progress::Progress;
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::prefix;
use crate::model::{
    CHECK_PENALTY_MS, HINT_PENALTY_MS, PuzzlePage, PuzzleSummary, TimeLeft, format_duration,
    is_blocked_name, time_left,
};
use crate::pages::daily::Stat;
use crate::pages::play::islands_percent;
use crate::seo::title_text;
use crate::state::{Auth, Game, Status, ToastKind, error_toast, toast};

use author::{AuthorCard, SummaryCard};
use cover::Cover;
use ranking::RankingCard;
use result::ResultCard;
use state::{GAME_KEY, Mode, Play};

/// A puzzle's name: its title, or whose it is. One of a player the viewer blocked has neither
/// (the server leaves its title out, see `server::blocks`).
pub fn puzzle_title(locale: Locale, summary: &PuzzleSummary) -> String {
    match (&summary.title, &summary.author) {
        (Some(title), _) => title.clone(),
        (None, Some(author)) if !is_blocked_name(&author.name) => {
            td_string!(locale, kadai.untitled, name = author.name.clone())
        }
        (None, _) => td_string!(locale, kadai.untitled_former).to_string(),
    }
}

/// Who made it.
pub fn byline(locale: Locale, summary: &PuzzleSummary) -> String {
    match &summary.author {
        Some(author) if is_blocked_name(&author.name) => {
            td_string!(locale, kadai.by_blocked).to_string()
        }
        Some(author) => td_string!(locale, kadai.by, name = author.name.clone()),
        None => td_string!(locale, kadai.by_former).to_string(),
    }
}

/// "5d left", or "closed".
pub fn left_text(locale: Locale, now: i64, closes_at: i64) -> String {
    match time_left(now, closes_at) {
        TimeLeft::Days(count) => td_string!(locale, kadai.left.days, count),
        TimeLeft::Hours(count) => td_string!(locale, kadai.left.hours, count),
        TimeLeft::Minutes(count) => td_string!(locale, kadai.left.minutes, count),
        TimeLeft::Closed => td_string!(locale, kadai.left.closed).to_string(),
    }
}

/// The path of a group puzzle's page (without the language).
pub fn puzzle_path(group: i64, puzzle: i64) -> String {
    format!("/groups/{group}/puzzles/{puzzle}")
}

/// The browser clock, unix time in ms.
#[expect(
    clippy::cast_possible_truncation,
    reason = "`Date.now()` is a whole number of ms, far inside the i64 range"
)]
fn browser_now() -> i64 {
    js_sys::Date::now() as i64
}

impl Play {
    /// Shows a page the server sent, with its clock.
    fn apply(&self, page: PuzzlePage) {
        self.skew.set(page.server_now - browser_now());
        self.show(page);
    }

    fn update_attempt(&self, f: impl FnOnce(&mut crate::model::AttemptInfo)) {
        self.page.update(|p| {
            if let Some(a) = p.as_mut().and_then(|p| p.attempt.as_mut()) {
                f(a);
            }
        });
    }

    /// Sends the solved board with the move log.
    fn submit(self) {
        let Some(id) = self.id() else { return };
        // The last click may not be logged yet
        self.record_moves();
        self.busy.set(true);
        self.failed.set(false);
        let bridges = self.game.bridges.get_untracked();
        let moves = self.game.moves.get_untracked();
        let log = self.log();
        spawn_local(async move {
            let reply = submit_group_puzzle(id, bridges, moves, log).await;
            if self.busy.try_set(false).is_some() || self.id() != Some(id) {
                return;
            }
            match reply {
                Ok(page) => {
                    self.won_here.set(true);
                    self.apply(page);
                }
                Err(e) => {
                    self.failed.set(true);
                    error_toast(e);
                }
            }
        });
    }
}

/// A group puzzle that isn't there (any more), or of a group the visitor isn't in.
#[component]
fn Missing() -> impl IntoView {
    let i18n = use_i18n();
    let groups = use_localized("/groups");
    view! {
        <div class="mx-auto flex max-w-xl flex-col items-center gap-4 py-16 text-center" data-testid="kadai-missing">
            <Stamp kanji="迷" class="size-20 text-5xl" />
            <h1 class="text-2xl font-black">{t!(i18n, kadai.page.missing)}</h1>
            <p class="text-muted-foreground">{t!(i18n, kadai.page.missing_hint)}</p>
            <A href=groups attr:class="rounded-md bg-primary px-4 py-2 font-medium text-primary-foreground">
                {t!(i18n, groups.page.back)}
            </A>
        </div>
    }
}

/// What the board of the page shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shown {
    Cover,
    Solution,
    Game,
}

/// The board of the page: the cover, the viewer's game, or the solution for the author and
/// after the close (with the bridge that took the most help marked).
#[component]
fn BoardArea(play: Play) -> impl IntoView {
    let game = play.game;
    let solution_board = Memo::new(move |_| {
        play.page.with(|p| {
            p.as_ref()
                .and_then(|p| p.puzzle.clone().zip(p.solution.clone()))
                .map(|(puzzle, solution)| (Board::new(puzzle), solution))
        })
    });
    let hardest = Signal::derive(move || {
        play.page.with(|p| {
            p.as_ref()
                .and_then(|p| p.stats.as_ref())
                .and_then(|s| s.hardest)
                .map(|h| h.edge)
        })
    });
    // Re-rendered only when what it shows changes, not with every hint or check
    let shown = Memo::new(move |_| {
        let mode = play.mode()?;
        let closed = play
            .page
            .with(|p| p.as_ref().is_some_and(|p| !p.summary.open));
        Some(match mode {
            Mode::Cover => Shown::Cover,
            // Closed: the solution for everyone, with the bridge that took the most help marked
            Mode::Author | Mode::Closed => Shown::Solution,
            _ if closed => Shown::Solution,
            Mode::Playing | Mode::Solved | Mode::GaveUp => Shown::Game,
        })
    });
    move || match shown.get()? {
        Shown::Cover => Some(view! { <Cover play /> }.into_any()),
        Shown::Solution => solution_board.get().map(|(board, solution)| {
            let board = Signal::stored(board);
            let bridges = Signal::stored(solution);
            view! {
                <div data-testid="kadai-solution">
                    <BoardView board bridges mode=BoardMode::View hint=hardest framed=true />
                </div>
            }
            .into_any()
        }),
        Shown::Game => Some(
            view! {
                <div class="relative">
                    <BoardView
                        board=game.board
                        bridges=game.bridges
                        mode=BoardMode::Play
                        on_cycle=Callback::new(move |(edge, delta)| game.cycle(edge, delta))
                        wrong=game.wrong
                        hint=game.hint
                        framed=true
                    />
                    <Show when=move || play.won_here.get() && game.status.get() == Status::Won>
                        <Petals />
                    </Show>
                </div>
            }
            .into_any(),
        ),
    }
}

/// Undo, redo, clear, the paid helps and giving up, while the clock runs.
#[component]
fn Controls(play: Play) -> impl IntoView {
    let i18n = use_i18n();
    let game = play.game;
    let playing = Signal::derive(move || {
        play.mode() == Some(Mode::Playing)
            && game.status.get() == Status::Playing
            && !play.busy.get()
    });
    let hint = move |_| {
        let Some(id) = play.id() else { return };
        play.busy.set(true);
        let bridges = game.bridges.get_untracked();
        let epoch = game.epoch.get_untracked();
        spawn_local(async move {
            let reply = group_puzzle_hint(id, bridges).await;
            if play.busy.try_set(false).is_some() || game.epoch.try_get_untracked() != Some(epoch) {
                return;
            }
            match reply {
                Ok(Some((edge, value))) => {
                    play.update_attempt(|a| a.hints += 1);
                    game.set_edges(&[(edge, value)]);
                    game.hints.update(|h| *h += 1);
                    game.hint.set(Some(edge));
                    set_timeout(
                        move || {
                            if game.hint.try_get_untracked() == Some(Some(edge)) {
                                game.hint.set(None);
                            }
                        },
                        Duration::from_millis(3000),
                    );
                }
                Ok(None) => toast(ToastKind::Info, |l| {
                    td_string!(l, common.toast.nothing_to_hint).to_string()
                }),
                Err(e) => error_toast(e),
            }
        });
    };
    let check = move |_| {
        let Some(id) = play.id() else { return };
        play.busy.set(true);
        let bridges = game.bridges.get_untracked();
        let epoch = game.epoch.get_untracked();
        spawn_local(async move {
            let reply = group_puzzle_check(id, bridges).await;
            if play.busy.try_set(false).is_some() || game.epoch.try_get_untracked() != Some(epoch) {
                return;
            }
            match reply {
                Ok(wrong) => {
                    play.update_attempt(|a| a.checks += 1);
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
                        set_timeout(
                            move || {
                                game.wrong.try_set(vec![]);
                            },
                            Duration::from_millis(2400),
                        );
                    }
                }
                Err(e) => error_toast(e),
            }
        });
    };
    let give_up = move |_| {
        let Some(id) = play.id() else { return };
        let sure = window()
            .confirm_with_message(t_string!(i18n, kadai.page.give_up_confirm))
            .unwrap_or(false);
        if !sure {
            return;
        }
        play.busy.set(true);
        spawn_local(async move {
            let reply = group_puzzle_reveal(id).await;
            if play.busy.try_set(false).is_some() || play.id() != Some(id) {
                return;
            }
            match reply {
                Ok(page) => play.apply(page),
                Err(e) => error_toast(e),
            }
        });
    };
    view! {
        <div class="grid grid-cols-3 gap-2 sm:flex sm:flex-wrap" aria-busy=move || play.busy.get().to_string()>
            <Button
                variant=ButtonVariant::Outline
                attr:disabled=move || !playing.get() || game.undo_stack.with(Vec::is_empty)
                on:click=move |_| game.undo()
            >
                "↶ " {t!(i18n, common.game.undo)}
            </Button>
            <Button
                variant=ButtonVariant::Outline
                attr:disabled=move || !playing.get() || game.redo_stack.with(Vec::is_empty)
                on:click=move |_| game.redo()
            >
                {t!(i18n, common.game.redo)} " ↷"
            </Button>
            <Button variant=ButtonVariant::Ghost attr:disabled=move || !playing.get() on:click=move |_| game.restart()>
                {t!(i18n, common.game.clear)}
            </Button>
            <div class="contents sm:ms-auto sm:flex sm:gap-2">
                <Button
                    variant=ButtonVariant::Secondary
                    attr:disabled=move || !playing.get()
                    attr:data-tooltip=format!("+{} s", CHECK_PENALTY_MS / 1000)
                    on:click=check
                >
                    {t!(i18n, common.game.check)} <span class="hidden sm:inline">" 確認"</span> <span class="text-xs opacity-70">"+10s"</span>
                </Button>
                <Button
                    variant=ButtonVariant::Secondary
                    attr:disabled=move || !playing.get()
                    attr:data-tooltip=format!("+{} s", HINT_PENALTY_MS / 1000)
                    on:click=hint
                >
                    {t!(i18n, common.game.hint)} <span class="hidden sm:inline">" 助言"</span> <span class="text-xs opacity-70">"+30s"</span>
                </Button>
                <Button variant=ButtonVariant::Ghost attr:disabled=move || !playing.get() on:click=give_up>
                    {t!(i18n, kadai.page.give_up)}
                </Button>
            </div>
        </div>
    }
}

/// The page of a group puzzle.
#[component]
pub fn KadaiPage() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let params = use_params_map();
    let group_id =
        Memo::new(move |_| params.with(|p| p.get("id").and_then(|id| id.parse::<i64>().ok())));
    let puzzle_id =
        Memo::new(move |_| params.with(|p| p.get("puzzle").and_then(|id| id.parse::<i64>().ok())));
    let game = Game::new(GAME_KEY);
    let play = Play::new(game);
    // `Some(false)`: no such puzzle, or not a member
    let found = RwSignal::new(None::<bool>);

    // Load the puzzle, again when the visitor changes
    Effect::new(move |_| {
        let (group, id) = (group_id.get(), puzzle_id.get());
        let _ = auth.user.with(|u| u.as_ref().map(|u| u.id));
        if !auth.loaded.get() {
            return;
        }
        let Some(id) = id else {
            found.set(Some(false));
            return;
        };
        spawn_local(async move {
            match load_group_puzzle(id).await {
                Ok(Some(page)) if Some(page.group_id) == group => {
                    play.apply(page);
                    found.try_set(Some(true));
                }
                Ok(_) => {
                    found.try_set(Some(false));
                }
                Err(e) => error_toast(e),
            }
        });
    });

    // The clock (browser only), and the game kept in the browser while it's played
    Effect::new(move |first: Option<()>| {
        if first.is_some() {
            return;
        }
        play.now.set(browser_now());
        if let Ok(handle) = set_interval_with_handle(
            move || {
                play.now.try_set(browser_now());
            },
            Duration::from_millis(100),
        ) {
            on_cleanup(move || handle.clear());
        }
    });
    Effect::new(move |_| {
        if play.mode() == Some(Mode::Playing) {
            game.save();
        }
    });
    // Solved, the sounds of hints and wrong bridges, like every game
    Effect::new(move |_| {
        let solved = game
            .board
            .with(|b| game.bridges.with(|br| b.check(br).solved));
        if solved && game.status.get_untracked() == Status::Playing {
            game.status.set(Status::Won);
            crate::audio::play(crate::audio::Sfx::Win);
        }
    });
    Effect::new(move |_| {
        if game.hint.get().is_some() {
            crate::audio::play(crate::audio::Sfx::Hint);
        }
    });
    Effect::new(move |_| {
        if game.wrong.with(|w| !w.is_empty()) {
            crate::audio::play(crate::audio::Sfx::Wrong);
        }
    });
    Effect::new(move |_| play.record_moves());
    Effect::new(move |_| {
        if play.ready_to_submit() {
            untrack(|| play.submit());
        }
    });

    let back = move || {
        let p = prefix(i18n.get_locale());
        match group_id.get() {
            Some(id) => format!("{p}/groups/{id}"),
            None => format!("{p}/groups"),
        }
    };
    let back_text = move || {
        play.page
            .with(|p| p.as_ref().map(|p| p.group_name.clone()))
            .unwrap_or_else(|| t_string!(i18n, groups.page.back).to_string())
    };
    let title = move || {
        let page = play.page.with(|p| {
            p.as_ref()
                .map(|p| puzzle_title(i18n.get_locale(), &p.summary))
        });
        format!(
            "{} · Hashi",
            page.unwrap_or_else(|| t_string!(i18n, kadai.page.title).to_string())
        )
    };

    view! {
        <Title text=title_text(title) />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto flex max-w-7xl flex-col gap-5 px-4 py-6 sm:px-6">
            <A href=back exact=true attr:class="w-fit text-sm text-primary hover:underline">
                "← " {back_text}
            </A>
            {move || match found.get() {
                None => view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                Some(false) => view! { <Missing /> }.into_any(),
                Some(true) => view! { <PuzzleView play /> }.into_any(),
            }}
        </section>
    }
}

/// The header with the puzzle's name, author, size and time left.
#[component]
fn Header(play: Play) -> impl IntoView {
    let i18n = use_i18n();
    let summary = Memo::new(move |_| play.page.with(|p| p.as_ref().map(|p| p.summary.clone())));
    // Changes once a minute at most, not with every tick of the clock
    let left = Memo::new(move |_| {
        let closes_at = summary.with(|s| s.as_ref().map(|s| s.closes_at))?;
        Some(left_text(i18n.get_locale(), play.server_now(), closes_at))
    });
    move || {
        summary.get().map(|s| {
            let locale = i18n.get_locale();
            let open = s.open;
            view! {
                <div class="flex min-w-0 items-center gap-3">
                    <span class="hanko grid size-14 shrink-0 rotate-[-4deg] place-items-center rounded-lg font-pixel text-3xl">
                        "課"
                    </span>
                    <div class="min-w-0">
                        <p class="font-pixel text-sm text-primary">"課題 · kadai"</p>
                        <h1 class="break-words text-2xl font-black sm:text-3xl" data-testid="kadai-title">
                            {puzzle_title(locale, &s)}
                        </h1>
                        <div class="mt-1 flex flex-wrap items-center gap-1.5 text-sm text-muted-foreground">
                            <span>{byline(locale, &s)}</span>
                            <Badge variant=BadgeVariant::Muted>
                                {format!("{}×{}", s.width, s.height)}
                            </Badge>
                            <Badge variant=if open { BadgeVariant::Default } else { BadgeVariant::Muted }>
                                {move || left.get()}
                            </Badge>
                        </div>
                    </div>
                </div>
            }
        })
    }
}

/// The puzzle loaded: board, controls and the cards beside it.
#[component]
fn PuzzleView(play: Play) -> impl IntoView {
    let i18n = use_i18n();
    let game = play.game;
    let islands = Memo::new(move |_| play.islands());
    let played = Signal::derive(move || play.mode().is_some_and(Mode::played));
    view! {
        <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]" data-testid="kadai-page" data-mode=move || format!("{:?}", play.mode())>
            <div class="flex min-w-0 flex-col gap-4">
                <div class="flex min-h-16 flex-wrap items-end justify-between gap-3">
                    <Header play />
                    <Show when=move || played.get()>
                        <div class="flex gap-2">
                            <Stat
                                label=Signal::derive(move || t_string!(i18n, common.game.time).to_string())
                                jp="時間"
                                value=Signal::derive(move || format_duration(play.elapsed_ms()))
                            />
                            // Only known while playing here: a reload doesn't bring them back
                            <Show when=move || play.mode() == Some(Mode::Playing)>
                                <Stat label=Signal::derive(move || t_string!(i18n, common.game.moves).to_string()) jp="手" value=Signal::derive(move || game.moves.get().to_string()) />
                            </Show>
                            <Stat
                                label=Signal::derive(move || t_string!(i18n, daily.penalty).to_string())
                                jp="罰"
                                value=Signal::derive(move || format!("+{}s", play.penalty_ms() / 1000))
                            />
                        </div>
                    </Show>
                </div>
                <BoardArea play />
                <Show when=move || play.mode() == Some(Mode::Playing)>
                    <div class="flex flex-col gap-2">
                        <div class="flex items-center justify-between text-xs text-muted-foreground">
                            <span>
                                <span class="font-pixel text-foreground">
                                    {move || { let (done, all) = islands.get(); format!("{done} / {all}") }}
                                </span>
                                " " {t!(i18n, common.game.islands_complete)}
                            </span>
                        </div>
                        <Progress value=Signal::derive(move || { let (done, all) = islands.get(); islands_percent(done, all) }) class="h-2.5" />
                    </div>
                    <Show when=move || play.failed.get()>
                        <div role="status" class="flex flex-wrap items-center gap-3 rounded-lg border border-warning/40 bg-warning-light px-3 py-2 text-sm text-warning-dark">
                            <span>{t!(i18n, kadai.page.submit_failed)}</span>
                            <Button variant=ButtonVariant::Outline attr:disabled=move || play.busy.get() on:click=move |_| play.submit()>
                                {t!(i18n, kadai.page.submit_retry)}
                            </Button>
                        </div>
                    </Show>
                    <Controls play />
                </Show>
                <Show when=move || play.mode() == Some(Mode::Closed)>
                    <p class="text-sm text-muted-foreground">{t!(i18n, kadai.page.closed_text)}</p>
                </Show>
            </div>
            <aside class="flex min-w-0 flex-col gap-4">
                <Show when=move || matches!(play.mode(), Some(Mode::Solved | Mode::GaveUp))>
                    <ResultCard play />
                </Show>
                <Show when=move || play.mode() == Some(Mode::Author)>
                    <AuthorCard play />
                </Show>
                <RankingCard play />
                <Show when=move || play.mode() != Some(Mode::Author) && play.page.with(|p| p.as_ref().is_some_and(|p| !p.summary.open))>
                    <SummaryCard play />
                </Show>
            </aside>
        </div>
    }
}

// Rendering on the server needs its features
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use common::Puzzle;

    use super::*;
    use crate::i18n::Locale;
    use crate::model::{
        AttemptInfo, FinalBoard, Hardest, MyPuzzle, Pip, PuzzleAuthor, PuzzleStats, TodayEntry,
        TodayState,
    };

    fn puzzle() -> Puzzle {
        Puzzle::from_grid(&[vec![2, 0, 3], vec![0, 0, 0], vec![0, 0, 1]])
    }

    fn entry(user_id: i64, name: &str, state: TodayState) -> TodayEntry {
        TodayEntry {
            user_id,
            name: name.into(),
            guest: false,
            colour: u8::try_from(user_id).unwrap(),
            stamp: None,
            state,
        }
    }

    /// The page of the small puzzle for bram (2), aiko's (1), chen (3) solved it.
    fn page(mine: MyPuzzle, open: bool) -> PuzzlePage {
        let author = mine == MyPuzzle::Author;
        let started = !matches!(mine, MyPuzzle::Fresh | MyPuzzle::Author);
        let finished = matches!(mine, MyPuzzle::Solved { .. } | MyPuzzle::GaveUp);
        let known = author || !open;
        PuzzlePage {
            group_id: 1,
            group_name: "Bridge Club".into(),
            me: if author { 1 } else { 2 },
            summary: PuzzleSummary {
                id: 5,
                group_id: 1,
                title: (!author).then(|| "Tiny".into()),
                author: Some(PuzzleAuthor {
                    user_id: 1,
                    name: "aiko".into(),
                    guest: false,
                    colour: 0,
                }),
                width: 3,
                height: 3,
                islands: 3,
                created_at: 0,
                closes_at: if open { i64::MAX / 2 } else { 1 },
                open,
                pips: vec![
                    Pip {
                        colour: 2,
                        solved: false,
                    },
                    Pip {
                        colour: 3,
                        solved: true,
                    },
                ],
                mine,
                board: (!open).then(|| FinalBoard {
                    puzzle: puzzle(),
                    solution: vec![2, 1],
                }),
                winner: (!open).then(|| ("chen".into(), 50_000)),
            },
            puzzle: (started || known).then(puzzle),
            solution: known.then(|| vec![2, 1]),
            attempt: started.then(|| AttemptInfo {
                attempt_id: String::new(),
                started_at: 1_000,
                finished_at: finished.then_some(61_000),
                hints: 1,
                checks: 0,
                revealed: mine == MyPuzzle::GaveUp,
                time_ms: finished.then_some(90_000),
                bridges: finished.then(|| vec![2, 1]),
                flag: None,
            }),
            board: vec![
                entry(
                    3,
                    "chen",
                    TodayState::Ranked {
                        rank: 1,
                        time_ms: 50_000,
                        hints: 0,
                        checks: 0,
                    },
                ),
                entry(2, "bram", TodayState::Waiting),
            ],
            board_open: known || finished,
            stats: known.then(|| PuzzleStats {
                players: 2,
                solved: 1,
                fastest_ms: Some(50_000),
                median_ms: Some(50_000),
                hints: 2,
                hardest: Some(Hardest {
                    edge: 1,
                    hints: 2,
                    wrong: 0,
                }),
                ..PuzzleStats::default()
            }),
            server_now: 2,
        }
    }

    /// The page of a group puzzle as the server renders it, in English.
    fn render(page: PuzzlePage) -> String {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        let html = owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::en);
            provide_context(leptos_router::location::RequestUrl::new(
                "/groups/1/puzzles/5",
            ));
            provide_context(crate::state::Rivals::new());
            provide_context(Game::new("test.practice"));
            let play = Play::new(Game::new("test.kadai"));
            play.show(page);
            view! {
                <leptos_router::components::Router>
                    <PuzzleView play />
                </leptos_router::components::Router>
            }
            .to_html()
        });
        drop(owner);
        html
    }

    #[tokio::test]
    async fn every_view_of_a_puzzle_renders() {
        let cover = render(page(MyPuzzle::Fresh, true));
        for text in [
            "kadai-cover",
            "Tiny",
            "by aiko",
            "3×3",
            "1 of 2 done",
            "Start",
        ] {
            assert!(cover.contains(text), "{text}: {cover}");
        }
        assert!(!cover.contains("kadai-solution"));

        let playing = render(page(MyPuzzle::Playing, true));
        for text in ["Give up", "+30s", "Hint", "Check", "Islands"] {
            assert!(playing.contains(text), "{text}");
        }

        let solved = render(page(
            MyPuzzle::Solved {
                time_ms: 90_000,
                rank: Some(2),
            },
            true,
        ));
        for text in [
            "kadai-result",
            "Place 2 of 1 in Bridge Club",
            "Watch everyone",
            "Play it again",
        ] {
            assert!(solved.contains(text), "{text}");
        }

        let gave_up = render(page(MyPuzzle::GaveUp, true));
        assert!(gave_up.contains("Solution revealed"));

        let author = render(page(MyPuzzle::Author, true));
        for text in [
            "kadai-author",
            "s puzzle",
            "1 / 2",
            "kadai-hardest",
            "Withdraw the puzzle",
            "kadai-solution",
        ] {
            assert!(author.contains(text), "{text}: {author}");
        }

        let closed = render(page(MyPuzzle::Fresh, false));
        for text in [
            "kadai-solution",
            "kadai-summary",
            "has closed",
            "t finish",
            "Closed",
        ] {
            assert!(closed.contains(text), "{text}: {closed}");
        }
        let applause = render(page(MyPuzzle::Author, false));
        assert!(applause.contains("Applause from 1 solvers!"), "{applause}");
    }

    #[test]
    fn titles_bylines_and_time_left() {
        let mut summary = page(MyPuzzle::Fresh, true).summary;
        assert_eq!(puzzle_title(Locale::en, &summary), "Tiny");
        summary.title = None;
        assert_eq!(puzzle_title(Locale::en, &summary), "aiko's puzzle");
        assert_eq!(byline(Locale::en, &summary), "by aiko");
        // A blocked author's (the server left the title out)
        if let Some(author) = &mut summary.author {
            author.name = crate::model::BLOCKED_NAME.into();
        }
        assert_eq!(puzzle_title(Locale::en, &summary), "A member's puzzle");
        assert_eq!(byline(Locale::en, &summary), "by a blocked player");
        summary.author = None;
        assert_eq!(puzzle_title(Locale::en, &summary), "A member's puzzle");
        assert_eq!(byline(Locale::en, &summary), "by a former member");
        let day = crate::model::MS_PER_DAY;
        assert_eq!(left_text(Locale::en, 0, 3 * day + 5), "3d left");
        assert_eq!(left_text(Locale::en, 0, day - 1), "23h left");
        assert_eq!(left_text(Locale::en, 0, 90_000), "1 min left");
        assert_eq!(left_text(Locale::en, 5, 5), "Closed");
        assert_eq!(puzzle_path(1, 5), "/groups/1/puzzles/5");
    }
}
