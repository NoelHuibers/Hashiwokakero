//! A shared puzzle (共有), `/p/:code`: the puzzle of a link a player shared, played like Play
//! (its clock, undo, hints and checks from the solvers), with who shared it, their time to beat
//! and everyone who solved it. Solving it sends the time: a player's first solve is on the board,
//! later ones change nothing. Guests play too, a guest is made for their first time.
//!
//! The server renders the page with the puzzle loaded, for the title and the preview of the link
//! (`SsrMode::Async` in app.rs). In the app it needs the network: without it, it says so.

pub mod board;

use std::time::Duration;

use common::Puzzle;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::{load_shared_puzzle, submit_shared_puzzle};
use crate::components::board::{BoardMode, BoardView};
use crate::components::celebration::{Petals, Stamp};
use crate::components::layout::use_localized;
use crate::components::share_puzzle::{SharePuzzle, share_text};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::Card;
use crate::components::ui::progress::Progress;
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::{localized, og_locale};
use crate::model::{
    SharedEntry, SharedPuzzlePage, SharedSolve, is_blocked_name, share_clock, share_path,
};
use crate::pages::daily::Stat;
use crate::pages::not_found_status;
use crate::pages::play::islands_percent;
use crate::seo::{SITE_NAME, SITE_URL, og_share_image, title_text};
use crate::state::{self, Auth, Game, Settings, Source, Status, error_toast, format_time};

use board::BoardCard;

/// Where the game of a shared puzzle is kept in the browser: one at a time, the last one opened.
pub const SHARED_KEY: &str = "hashi.shared.v1";

/// The name of the sharer as the page shows it, `None` for none (deleted, banned, blocked).
fn sharer_name(page: &SharedPuzzlePage) -> Option<&str> {
    page.sharer
        .as_ref()
        .map(|s| s.name.as_str())
        .filter(|name| !is_blocked_name(name))
}

/// The puzzle's name: whose it is.
pub fn shared_title(locale: Locale, page: &SharedPuzzlePage) -> String {
    match sharer_name(page) {
        Some(name) => td_string!(locale, shared.page.title, name = name.to_string()),
        None => td_string!(locale, shared.page.title_anonymous).to_string(),
    }
}

/// Who shared it.
pub fn byline(locale: Locale, page: &SharedPuzzlePage) -> String {
    match &page.sharer {
        Some(s) if is_blocked_name(&s.name) => {
            td_string!(locale, shared.page.by_blocked).to_string()
        }
        Some(s) => td_string!(locale, shared.page.by, name = s.name.clone()),
        None => td_string!(locale, shared.page.by_former).to_string(),
    }
}

/// `7×7 · 12 islands`.
fn size_text(locale: Locale, puzzle: &Puzzle) -> String {
    format!(
        "{}×{} · {}",
        puzzle.width,
        puzzle.height,
        td_string!(locale, common.game.islands, count = puzzle.islands.len())
    )
}

/// What a link to the page shows where it's shared: who, the size, the time to beat.
pub fn preview(locale: Locale, page: &SharedPuzzlePage) -> (String, String) {
    let size = format!("{}×{}", page.puzzle.width, page.puzzle.height);
    let title = match sharer_name(page) {
        Some(name) => td_string!(locale, shared.meta.title, name = name.to_string()),
        None => td_string!(locale, shared.meta.title_anonymous).to_string(),
    };
    let description = match (sharer_name(page), page.sharer.as_ref().and_then(|s| s.result.as_ref())) {
        (Some(name), Some(result)) => td_string!(
            locale,
            shared.meta.time,
            name = name.to_string(),
            size,
            time = share_clock(result.score_ms)
        ),
        (Some(name), None) => {
            td_string!(locale, shared.meta.untimed, name = name.to_string(), size)
        }
        (None, _) => td_string!(locale, shared.meta.anonymous, size),
    };
    (title, description)
}

/// The title, description and social tags of the page, for links to it. Like a player's page
/// without a canonical URL or other languages: it isn't indexed.
#[component]
fn SharedMeta(page: SharedPuzzlePage) -> impl IntoView {
    let i18n = use_i18n();
    let locale = i18n.get_locale_untracked();
    let (title, description) = preview(locale, &page);
    let url = format!("{SITE_URL}{}", localized(locale, &share_path(&page.code)));
    let image = og_share_image(locale, &page.code);
    let image_alt = description.clone();
    let tab = title.clone();
    view! {
        <Title text=title_text(move || format!("{tab} · {SITE_NAME}")) />
        <Meta name="description" content=description.clone() />
        <Meta property="og:type" content="website" />
        <Meta property="og:site_name" content=SITE_NAME />
        <Meta property="og:locale" content=og_locale(locale) />
        <Meta property="og:title" content=title.clone() />
        <Meta property="og:description" content=description.clone() />
        <Meta property="og:url" content=url />
        <Meta property="og:image" content=image.clone() />
        <Meta property="og:image:type" content="image/png" />
        <Meta property="og:image:width" content="1200" />
        <Meta property="og:image:height" content="630" />
        <Meta property="og:image:alt" content=image_alt.clone() />
        <Meta name="twitter:card" content="summary_large_image" />
        <Meta name="twitter:title" content=title />
        <Meta name="twitter:description" content=description />
        <Meta name="twitter:image" content=image />
        <Meta name="twitter:image:alt" content=image_alt />
    }
}

/// A link that leads nowhere.
#[component]
fn Missing() -> impl IntoView {
    let i18n = use_i18n();
    let play = use_localized("/practice");
    not_found_status();
    view! {
        <Title text=title_text(move || format!("{} · Hashi", t_string!(i18n, common.not_found.title))) />
        <div class="mx-auto flex max-w-xl flex-col items-center gap-4 py-16 text-center" data-testid="shared-missing">
            <Stamp kanji="迷" class="size-20 text-5xl" />
            <h1 class="text-2xl font-black">{t!(i18n, shared.page.missing)}</h1>
            <p class="text-muted-foreground">{t!(i18n, shared.page.missing_hint)}</p>
            <A href=play attr:class="rounded-md bg-primary px-4 py-2 font-medium text-primary-foreground">
                {t!(i18n, shared.page.to_play)}
            </A>
        </div>
    }
}

/// The server couldn't be reached (the app offline) or failed.
#[component]
fn Unreachable(retry: Callback<()>) -> impl IntoView {
    let i18n = use_i18n();
    let play = use_localized("/practice");
    view! {
        <div class="mx-auto flex max-w-xl flex-col items-center gap-4 py-16 text-center" data-testid="shared-offline" role="status">
            <Stamp kanji="圏" class="size-20 text-5xl" />
            <h1 class="text-2xl font-black">{t!(i18n, shared.page.offline)}</h1>
            <p class="text-muted-foreground">{t!(i18n, shared.page.offline_hint)}</p>
            <div class="flex flex-wrap justify-center gap-2">
                <Button on:click=move |_| retry.run(())>{t!(i18n, shared.page.retry)}</Button>
                <A href=play attr:class="inline-flex items-center rounded-md border px-4 py-2 font-medium">
                    {t!(i18n, shared.page.to_play)}
                </A>
            </div>
        </div>
    }
}

/// The page of a shared puzzle.
#[component]
pub fn SharedPuzzle() -> impl IntoView {
    let i18n = use_i18n();
    let params = use_params_map();
    let code = move || params.with(|p| p.get("code").unwrap_or_default());
    // Loaded on the server as well, which renders the page once it's there (`SsrMode::Async` in
    // app.rs). Loaded again by "try again" after a failure (`attempt`).
    let attempt = RwSignal::new(0u32);
    let loaded = Resource::new(
        move || (code(), attempt.get()),
        |(code, _)| async move { load_shared_puzzle(code).await },
    );
    let loading =
        move || view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> };
    view! {
        // Until the puzzle is loaded, and when it can't be
        <Title text=title_text(move || format!("{} · Hashi", t_string!(i18n, shared.page.title_anonymous))) />
        // People didn't choose to be found by search engines through their links
        <Meta name="robots" content="noindex" />
        <section class="mx-auto flex max-w-7xl flex-col gap-5 px-4 pt-4 pb-6 sm:px-6 sm:pt-6">
            <Suspense fallback=loading>
                {move || Suspend::new(async move {
                    match loaded.await {
                        Ok(Some(page)) => view! {
                            <SharedMeta page=page.clone() />
                            <Playing initial=page />
                        }
                            .into_any(),
                        Ok(None) => view! { <Missing /> }.into_any(),
                        Err(_) => view! {
                            <Unreachable retry=Callback::new(move |()| attempt.update(|a| *a += 1)) />
                        }
                            .into_any(),
                    }
                })}
            </Suspense>
        </section>
    }
}

/// What the page knows while it is played.
#[derive(Clone, Copy, Debug)]
pub struct Shared {
    pub game: Game,
    pub page: RwSignal<SharedPuzzlePage>,
    /// Solved here: whether the solve counted (the player's first), `None` until the server said.
    pub counted: RwSignal<Option<bool>>,
    /// The time of the solve here, in ms.
    pub solved_ms: RwSignal<Option<i64>>,
    pub sending: RwSignal<bool>,
    pub failed: RwSignal<bool>,
}

impl Shared {
    fn new(page: SharedPuzzlePage) -> Self {
        let game = Game::new(SHARED_KEY);
        game.set_puzzle(page.puzzle.clone(), None, Some(true), Source::Custom);
        Self {
            game,
            page: RwSignal::new(page),
            counted: RwSignal::new(None),
            solved_ms: RwSignal::new(None),
            sending: RwSignal::new(false),
            failed: RwSignal::new(false),
        }
    }

    /// The solve on the board, as the server takes it.
    fn solve(&self) -> SharedSolve {
        SharedSolve {
            bridges: self.game.bridges.get_untracked(),
            time_ms: i64::from(self.game.elapsed.get_untracked()) * 1000,
            moves: self.game.moves.get_untracked(),
            hints: self.game.hints.get_untracked(),
        }
    }

    /// Sends the solve on the board.
    fn submit(self) {
        let code = self.page.with_untracked(|p| p.code.clone());
        let solve = self.solve();
        self.solved_ms.set(Some(solve.time_ms));
        self.sending.set(true);
        self.failed.set(false);
        let auth = use_context::<Auth>();
        spawn_local(async move {
            let reply = submit_shared_puzzle(code, solve).await;
            if self.sending.try_set(false).is_some() {
                return;
            }
            match reply {
                Ok(sent) => {
                    if let Some(auth) = auth
                        && auth.user.with_untracked(|u| u.as_ref().map(|u| u.id)) != Some(sent.user.id)
                    {
                        auth.user.set(Some(sent.user));
                    }
                    self.counted.set(Some(sent.counted));
                    self.page.set(sent.page);
                }
                Err(e) => {
                    self.failed.set(true);
                    error_toast(e);
                }
            }
        });
    }
}

/// The puzzle loaded: board, controls and the cards beside it.
#[component]
fn Playing(initial: SharedPuzzlePage) -> impl IntoView {
    let i18n = use_i18n();
    let settings = expect_context::<Settings>();
    let shared = Shared::new(initial);
    let game = shared.game;

    // Browser only: the game kept from before, if it is this puzzle's; the clock; saving
    Effect::new(move |first: Option<()>| {
        if first.is_some() {
            return;
        }
        let fresh = shared.page.with_untracked(|p| p.puzzle.clone());
        if !(game.ensure_restored() && game.puzzle() == fresh) {
            game.set_puzzle(fresh, None, Some(true), Source::Custom);
        }
        if let Ok(handle) = set_interval_with_handle(
            move || {
                let visible = document().visibility_state() == web_sys::VisibilityState::Visible;
                if visible && game.status.try_get_untracked() == Some(Status::Playing) {
                    game.elapsed.update(|e| *e += 1);
                }
            },
            Duration::from_secs(1),
        ) {
            on_cleanup(move || handle.clear());
        }
        let keys = window_event_listener(leptos::ev::keydown, move |ev| {
            let ctrl = ev.ctrl_key() || ev.meta_key();
            match (ctrl, ev.shift_key(), ev.key().to_lowercase().as_str()) {
                (true, false, "z") => game.undo(),
                (true, true, "z") | (true, _, "y") => game.redo(),
                _ => return,
            }
            ev.prevent_default();
        });
        on_cleanup(move || keys.remove());
    });
    Effect::new(move |first: Option<()>| {
        // Not the empty game the page starts with on the server
        game.save();
        let _ = first;
    });
    // Solved here: the time goes to the server. The sounds of hints and wrong bridges, like every
    // game
    Effect::new(move |_| {
        let solved = game
            .board
            .with(|b| game.bridges.with(|br| b.check(br).solved));
        if solved && game.status.get_untracked() == Status::Playing {
            game.status.set(Status::Won);
            crate::audio::play(crate::audio::Sfx::Win);
            untrack(|| shared.submit());
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

    let check = Memo::new(move |_| game.board.with(|b| game.bridges.with(|br| b.check(br))));
    let islands = Memo::new(move |_| {
        game.board.with(|b| {
            let done = check.with(|c| {
                b.puzzle
                    .islands
                    .iter()
                    .zip(&c.degrees)
                    .filter(|(i, d)| i.required == **d)
                    .count()
            });
            (done, b.puzzle.islands.len())
        })
    });
    let playing = Signal::derive(move || game.status.get() == Status::Playing && !game.solving.get());
    let won = Signal::derive(move || game.status.get() == Status::Won);

    view! {
        <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]" data-testid="shared-page">
            <div class="flex min-w-0 flex-col gap-3 sm:gap-4">
                <div class="flex flex-wrap items-end justify-between gap-3">
                    <Header shared />
                    <div class="flex w-full gap-1.5 sm:w-auto sm:gap-2">
                        <Stat label=Signal::derive(move || t_string!(i18n, common.game.time).to_string()) jp="時間" value=Signal::derive(move || format_time(game.elapsed.get())) />
                        <Stat label=Signal::derive(move || t_string!(i18n, common.game.moves).to_string()) jp="手" value=Signal::derive(move || game.moves.get().to_string()) />
                        <Stat label=Signal::derive(move || t_string!(i18n, common.game.hints).to_string()) jp="助" value=Signal::derive(move || game.hints.get().to_string()) />
                    </div>
                </div>
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
                    <Show when=move || won.get() && shared.solved_ms.get().is_some()>
                        <Petals />
                    </Show>
                </div>
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
                <Show when=move || shared.failed.get()>
                    <div role="status" class="flex flex-wrap items-center gap-3 rounded-lg border border-warning/40 bg-warning-light px-3 py-2 text-sm text-warning-dark">
                        <span>{t!(i18n, shared.page.submit_failed)}</span>
                        <Button variant=ButtonVariant::Outline attr:disabled=move || shared.sending.get() on:click=move |_| shared.submit()>
                            {t!(i18n, shared.page.submit_retry)}
                        </Button>
                    </div>
                </Show>
                <div class="flex flex-wrap items-center gap-1.5 sm:gap-2">
                    <Button
                        variant=ButtonVariant::Outline
                        class="h-11"
                        attr:disabled=move || !playing.get() || game.undo_stack.with(Vec::is_empty)
                        on:click=move |_| game.undo()
                    >
                        "↶ " {t!(i18n, common.game.undo)}
                    </Button>
                    <Button
                        variant=ButtonVariant::Outline
                        class="h-11"
                        attr:disabled=move || !playing.get() || game.redo_stack.with(Vec::is_empty)
                        on:click=move |_| game.redo()
                    >
                        {t!(i18n, common.game.redo)} " ↷"
                    </Button>
                    <span class="hidden flex-1 sm:block"></span>
                    <Button
                        variant=ButtonVariant::Secondary
                        class="h-11"
                        attr:disabled=move || !playing.get()
                        on:click=move |_| state::check(game, settings)
                    >
                        {t!(i18n, common.game.check)} <span class="hidden sm:inline">" 確認"</span>
                    </Button>
                    <Button
                        variant=ButtonVariant::Secondary
                        class="h-11"
                        attr:disabled=move || !playing.get()
                        on:click=move |_| state::hint(game, settings)
                    >
                        {t!(i18n, common.game.hint)} <span class="hidden sm:inline">" 助言"</span>
                    </Button>
                    <Button
                        variant=ButtonVariant::Ghost
                        class="h-11"
                        attr:disabled=move || game.solving.get()
                        on:click=move |_| game.restart()
                    >
                        {move || if won.get() {
                            t_string!(i18n, shared.result.again_button).to_string()
                        } else {
                            t_string!(i18n, common.game.restart).to_string()
                        }}
                    </Button>
                </div>
            </div>
            <aside class="flex min-w-0 flex-col gap-4">
                <Show when=move || won.get()>
                    <ResultCard shared />
                </Show>
                <BoardCard page=shared.page />
            </aside>
        </div>
    }
}

/// The puzzle's name, who shared it, its size and the time to beat.
#[component]
fn Header(shared: Shared) -> impl IntoView {
    let i18n = use_i18n();
    let page = shared.page;
    let to_beat = Memo::new(move |_| {
        page.with(|p| {
            p.sharer
                .as_ref()
                .map(|s| s.result.as_ref().map(|r| (r.score_ms, r.hints)))
        })
    });
    view! {
        <div class="flex min-w-0 items-center gap-3">
            <span class="hanko grid size-14 shrink-0 rotate-[-4deg] place-items-center rounded-lg font-pixel text-3xl" aria-hidden="true">
                "共"
            </span>
            <div class="min-w-0">
                <p class="font-pixel text-sm text-primary">"共有 · kyōyū"</p>
                <h1 class="break-words text-2xl font-black sm:text-3xl" data-testid="shared-title">
                    {move || page.with(|p| shared_title(i18n.get_locale(), p))}
                </h1>
                <div class="mt-1 flex flex-wrap items-center gap-1.5 text-sm text-muted-foreground">
                    <span data-testid="shared-by">{move || page.with(|p| byline(i18n.get_locale(), p))}</span>
                    <Badge variant=BadgeVariant::Muted>
                        {move || page.with(|p| size_text(i18n.get_locale(), &p.puzzle))}
                    </Badge>
                    {move || match to_beat.get() {
                        Some(Some((score, hints))) => view! {
                            <Badge variant=BadgeVariant::Default attr:data-testid="shared-to-beat">
                                {t_string!(i18n, shared.page.to_beat)} " " <span class="font-pixel">{share_clock(score)}</span>
                                {(hints > 0).then(|| view! {
                                    <span class="opacity-80" data-tooltip=move || t_string!(i18n, common.ranking.penalties)>
                                        {format!(" · {hints}h")}
                                    </span>
                                })}
                            </Badge>
                        }
                            .into_any(),
                        Some(None) => view! { <span>{t!(i18n, shared.page.untimed)}</span> }.into_any(),
                        None => ().into_any(),
                    }}
                </div>
            </div>
        </div>
    }
}

/// The player's result once solved here, and the puzzle to pass on.
#[component]
fn ResultCard(shared: Shared) -> impl IntoView {
    let i18n = use_i18n();
    let page = shared.page;
    let mine = Memo::new(move |_| page.with(|p| p.mine.clone()));
    let line = move || {
        let total = page.with(|p| p.solvers);
        match (mine.get(), shared.counted.get(), shared.solved_ms.get()) {
            // Solved before: that time stays
            (Some(first), Some(false), Some(now)) => {
                let (time, first) = (share_clock(now), share_clock(first.score_ms));
                Some(t!(i18n, shared.result.again, time, first, <b> = <span class="font-pixel text-foreground" />).into_any())
            }
            (Some(first), _, _) => {
                let (time, rank) = (share_clock(first.score_ms), first.rank);
                Some(t!(i18n, shared.result.first, time, rank, total, <b> = <span class="font-pixel text-foreground" />).into_any())
            }
            _ => None,
        }
    };
    // Beat the sharer, and it counted
    let beat = move || {
        let (mine, sharer) = page.with(|p| {
            (
                p.mine.clone(),
                p.sharer
                    .as_ref()
                    .filter(|s| !is_blocked_name(&s.name))
                    .and_then(|s| s.result.clone().map(|r| (s.name.clone(), r))),
            )
        });
        match (mine, sharer, shared.counted.get()) {
            (Some(mine), Some((name, theirs)), Some(true))
                if mine.user_id != theirs.user_id && mine.score_ms < theirs.score_ms =>
            {
                Some(name)
            }
            _ => None,
        }
    };
    let request = move || {
        let game = shared.game;
        (game.puzzle(), Some(shared.solve()))
    };
    let text = move |locale, url: &str| {
        let solve = page.with_untracked(|p| {
            p.mine
                .as_ref()
                .map(|m: &SharedEntry| (m.time_ms, u32::try_from(m.hints).unwrap_or(u32::MAX)))
        });
        let title = page.with_untracked(|p| shared_title(locale, p));
        share_text(locale, &title, solve, url)
    };
    view! {
        <Card class="items-center gap-3 px-6 text-center" attr:data-testid="shared-result">
            <Stamp kanji="完" class="size-16 text-4xl" />
            <h2 class="text-xl font-black">"見事！ " {t!(i18n, shared.result.title)}</h2>
            <p class="text-muted-foreground">
                {move || line().unwrap_or_else(|| t_string!(i18n, common.game.loading).to_string().into_any())}
            </p>
            {move || beat().map(|name| view! {
                <p class="font-medium text-success-dark">{t_string!(i18n, shared.result.beat, name)}</p>
            })}
            <Show when=move || mine.with(Option::is_some)>
                <SharePuzzle request text outline=true />
            </Show>
        </Card>
    }
}

#[cfg(test)]
mod tests;
