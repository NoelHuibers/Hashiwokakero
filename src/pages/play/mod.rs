//! The practice page: endless generated puzzles with the solver at hand.

pub mod legend;
pub mod new_game;
pub mod seo;
pub mod win;

pub use seo::SeoIntro;

use common::Difficulty;
use leptos::prelude::*;

use crate::components::board::{BoardMode, BoardView};
use crate::components::solver_panel::{PanelButton, SolverPanel};
use crate::components::sprites::{BOAT, PixelSprite};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::progress::Progress;
use crate::i18n::{t, t_string, use_i18n};
use crate::seo::PageMeta;
use crate::state::{self, Game, Settings, Source, Status, format_time};

use legend::LegendCard;
use new_game::{NewGameCard, new_random};
use win::WinOverlay;

/// Share of the islands that are done, for the progress bar.
#[expect(
    clippy::cast_precision_loss,
    reason = "island counts are tiny, far below f64's 52-bit mantissa"
)]
pub fn islands_percent(done: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        done as f64 / total as f64 * 100.0
    }
}

#[component]
fn Stat(
    #[prop(into)] label: Signal<String>,
    jp: &'static str,
    #[prop(into)] value: Signal<String>,
) -> impl IntoView {
    view! {
        <div class="flex min-w-[4.5rem] flex-col items-center rounded-lg border bg-card px-3 py-1.5 shadow-xs">
            <span class="font-pixel text-xl tabular-nums leading-tight">{move || value.get()}</span>
            <span class="text-[10px] font-bold uppercase tracking-[0.16em] text-muted-foreground">
                {move || label.get()} " · " {jp}
            </span>
        </div>
    }
}

#[component]
pub fn PlayPage() -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();

    // First visit: restore the saved game or generate one
    Effect::new(move |first: Option<()>| {
        if first.is_none() && !game.ensure_restored() {
            new_random(&game, settings.difficulty.get_untracked());
        }
    });

    // Keyboard shortcuts
    Effect::new(move |_| {
        let handle = window_event_listener(leptos::ev::keydown, move |ev| {
            let target_is_input = ev
                .target()
                .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok())
                .is_some_and(|el| {
                    matches!(el.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA")
                });
            if target_is_input {
                return;
            }
            let ctrl = ev.ctrl_key() || ev.meta_key();
            match (ctrl, ev.shift_key(), ev.key().to_lowercase().as_str()) {
                (true, false, "z") => game.undo(),
                (true, true, "z") | (true, _, "y") => game.redo(),
                (false, _, "h") => state::hint(game, settings),
                _ => return,
            }
            ev.prevent_default();
        });
        on_cleanup(move || handle.remove());
    });

    let check = Memo::new(move |_| game.board.with(|b| game.bridges.with(|br| b.check(br))));
    let islands_done = Memo::new(move |_| {
        game.board.with(|b| {
            check.with(|c| {
                b.puzzle
                    .islands
                    .iter()
                    .zip(&c.degrees)
                    .filter(|(i, d)| i.required == **d)
                    .count()
            })
        })
    });
    let island_count = Memo::new(move |_| game.board.with(|b| b.puzzle.islands.len()));
    let busy = Signal::derive(move || game.solving.get() || game.loading.get());
    let not_playing = Signal::derive(move || game.status.get() != Status::Playing || busy.get());

    view! {
        <PageMeta
            title=Signal::derive(move || t_string!(i18n, practice.meta.title).to_string())
            description=Signal::derive(move || t_string!(i18n, practice.meta.description).to_string())
            path="/practice"
        />
        <section class="mx-auto max-w-7xl px-4 py-6 sm:px-6">
            <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]">
                <div class="flex min-w-0 flex-col gap-4">
                    <div class="flex flex-wrap items-end justify-between gap-3">
                        <div class="flex items-center gap-3">
                            <span class="vertical-jp hidden font-pixel text-sm leading-none text-primary sm:block">
                                {move || {
                                    game.source.with(Source::difficulty).map_or("自作", Difficulty::kanji)
                                }}
                            </span>
                            <div>
                                <h1 class="text-xs font-bold uppercase tracking-[0.2em] text-muted-foreground">
                                    "Hashiwokakero · " {t!(i18n, practice.heading)}
                                </h1>
                                <p class="text-2xl font-black sm:text-3xl">{move || game.source.get().title(i18n.get_locale())}</p>
                                <div class="mt-1 flex gap-1.5">
                                    {move || match game.unique.get() {
                                        Some(true) => {
                                            view! {
                                                <Badge variant=BadgeVariant::Success>{t!(i18n, common.game.unique_solution)}</Badge>
                                            }
                                                .into_any()
                                        }
                                        Some(false) => {
                                            view! {
                                                <Badge variant=BadgeVariant::Warning>{t!(i18n, common.game.several_solutions)}</Badge>
                                            }
                                                .into_any()
                                        }
                                        None => ().into_any(),
                                    }}
                                    <Badge variant=BadgeVariant::Muted>
                                        {move || t_string!(i18n, common.game.islands, count = island_count.get())}
                                    </Badge>
                                </div>
                            </div>
                        </div>
                        <div class="flex gap-2">
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
                        <Show when=move || game.loading.get() || game.status.get() == Status::Empty>
                            <div class="absolute inset-0 z-10 grid place-items-center rounded-2xl bg-background/55 backdrop-blur-sm">
                                <div class="flex flex-col items-center gap-3 text-center">
                                    <PixelSprite art=&BOAT class="bob w-20" />
                                    <p class="font-pixel text-lg">"生成中 · " {t!(i18n, practice.generating)}</p>
                                    <p class="max-w-xs text-sm text-muted-foreground">
                                        {t!(i18n, practice.generating_text)}
                                    </p>
                                </div>
                            </div>
                        </Show>
                        <WinOverlay />
                    </div>

                    <div class="flex flex-col gap-2">
                        <div class="flex items-center justify-between text-xs text-muted-foreground">
                            <span>
                                <span class="font-pixel text-foreground">
                                    {move || format!("{} / {}", islands_done.get(), island_count.get())}
                                </span>
                                " " {t!(i18n, common.game.islands_complete)}
                            </span>
                            <span>
                                {move || {
                                    let c = check.get();
                                    if c.components > 1 {
                                        t_string!(i18n, common.game.groups, count = c.components)
                                    } else {
                                        t_string!(i18n, common.game.all_connected).to_string()
                                    }
                                }}
                            </span>
                        </div>
                        <Progress
                            value=Signal::derive(move || {
                                islands_percent(islands_done.get(), island_count.get())
                            })
                            class="h-2.5"
                        />
                    </div>

                    <div class="grid grid-cols-3 gap-2 sm:flex sm:flex-wrap">
                        <Button
                            variant=ButtonVariant::Outline
                            attr:disabled=move || not_playing.get() || game.undo_stack.with(Vec::is_empty)
                            on:click=move |_| game.undo()
                        >
                            "↶ " {t!(i18n, common.game.undo)}
                        </Button>
                        <Button
                            variant=ButtonVariant::Outline
                            attr:disabled=move || not_playing.get() || game.redo_stack.with(Vec::is_empty)
                            on:click=move |_| game.redo()
                        >
                            {t!(i18n, common.game.redo)} " ↷"
                        </Button>
                        <Button
                            variant=ButtonVariant::Ghost
                            attr:disabled=move || busy.get() || game.status.get() == Status::Empty
                            on:click=move |_| game.restart()
                        >
                            {t!(i18n, common.game.restart)}
                        </Button>
                        <div class="contents sm:ms-auto sm:flex sm:gap-2">
                            <Button
                                variant=ButtonVariant::Secondary
                                attr:disabled=move || not_playing.get()
                                on:click=move |_| state::check(game, settings)
                            >
                                {t!(i18n, common.game.check)} <span class="hidden sm:inline">" 確認"</span>
                            </Button>
                            <Button
                                variant=ButtonVariant::Secondary
                                attr:disabled=move || not_playing.get()
                                on:click=move |_| state::hint(game, settings)
                            >
                                {t!(i18n, common.game.hint)} <span class="hidden sm:inline">" 助言"</span>
                            </Button>
                        </div>
                    </div>
                </div>

                <aside class="flex flex-col gap-4">
                    <NewGameCard />
                    <SolverPanel report=game.report busy=game.solving>
                        <PanelButton
                            primary=true
                            disabled=busy
                            on_click=Callback::new(move |()| state::reveal(game, settings))
                        >
                            {t!(i18n, common.game.solve)} " 解く"
                        </PanelButton>
                        <PanelButton
                            disabled=busy
                            on_click=Callback::new(move |()| state::check_unique(game, settings))
                        >
                            {t!(i18n, common.game.unique)}
                        </PanelButton>
                        <PanelButton
                            disabled=busy
                            on_click=Callback::new(move |()| crate::pages::download_cnf(game.puzzle()))
                        >
                            "CNF ↓"
                        </PanelButton>
                    </SolverPanel>
                    <LegendCard />
                </aside>
            </div>
        </section>
    }
}
