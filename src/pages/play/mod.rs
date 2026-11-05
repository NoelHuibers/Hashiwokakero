//! Play: endless generated puzzles with the solver at hand.

pub mod actions;
pub mod legend;
pub mod new_game;
pub mod pause;
pub mod solutions;
pub mod solver_tools;
pub mod win;

use common::Difficulty;
use leptos::prelude::*;

use crate::components::board::{BoardMode, BoardView};
use crate::components::solver_panel::{PanelButton, SolverPanel};
use crate::components::sprites::{BOAT, PixelSprite};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::progress::Progress;
use crate::i18n::{t, t_string, use_i18n};
use crate::seo::PageMeta;
use crate::state::{self, Game, Settings, Source, Status, format_time};

use actions::GameActions;
use legend::LegendCard;
use new_game::{DifficultyChips, NewPuzzleButton, new_random};
use pause::{PauseButton, PausedCover};
use solutions::SolutionsBadge;
use solver_tools::SolverTools;
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
        <div class="flex min-w-0 flex-1 flex-col items-center justify-center rounded-lg border bg-card px-1.5 py-0.5 shadow-xs sm:h-13 sm:min-w-[4.5rem] sm:flex-none sm:px-3 sm:py-1.5">
            <span class="font-pixel text-lg tabular-nums leading-tight sm:text-xl">{move || value.get()}</span>
            <span class="max-w-full truncate text-[10px] font-bold uppercase tracking-[0.06em] text-muted-foreground max-sm:font-sans sm:tracking-[0.16em]">
                {move || label.get()} <span class="hidden sm:inline">" · " {jp}</span>
            </span>
        </div>
    }
}

#[component]
pub fn PlayPage() -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();

    // First visit: restore the saved game or generate one. The difficulty kept for the next
    // puzzles is the one of the generated puzzle restored
    Effect::new(move |first: Option<()>| {
        if first.is_some() {
            return;
        }
        if !game.ensure_restored() {
            new_random(&game, settings.difficulty.get_untracked());
        } else if let Source::Generated(params) = game.source.get_untracked() {
            settings.difficulty.set(params.difficulty);
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
            // N and P once per press, not repeated while held
            let plain = !ctrl && !ev.alt_key() && !ev.repeat();
            match (ctrl, ev.shift_key(), ev.key().to_lowercase().as_str()) {
                (true, false, "z") => game.undo(),
                (true, true, "z") | (true, _, "y") => game.redo(),
                (false, _, "h") => state::hint(game, settings),
                (false, _, "n") if plain => new_random(&game, settings.difficulty.get_untracked()),
                (false, _, "p") if plain => game.toggle_pause(),
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
    // While paused nothing that touches the board or shows the solution
    let busy =
        Signal::derive(move || game.solving.get() || game.loading.get() || game.paused.get());
    let not_playing = Signal::derive(move || game.status.get() != Status::Playing || busy.get());

    view! {
        <PageMeta
            title=Signal::derive(move || t_string!(i18n, play.meta.title).to_string())
            description=Signal::derive(move || t_string!(i18n, play.meta.description).to_string())
            path="/play"
        />
        <section class="mx-auto max-w-7xl px-4 pt-4 pb-6 sm:px-6 sm:pt-6">
            <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]">
                <div class="flex min-w-0 flex-col gap-3 sm:gap-4">
                    // One grid with the controls. Phones: the heading, the title with pause and a
                    // new puzzle next to it, the clock, then the difficulty chips, a row each. From
                    // `sm` on the title next to the clock, and the chips with the two buttons under
                    // it, right above the board
                    <div class="grid grid-cols-[minmax(0,1fr)_auto] items-end gap-x-3 sm:grid-cols-[auto_minmax(0,1fr)_auto] sm:gap-x-4 sm:gap-y-3">
                        <span class="vertical-jp hidden font-pixel text-sm leading-none text-primary sm:col-start-1 sm:row-span-2 sm:row-start-1 sm:block sm:self-center">
                            {move || {
                                game.source.with(Source::difficulty).map_or("自作", Difficulty::kanji)
                            }}
                        </span>
                        <h1 class="text-xs font-bold uppercase tracking-[0.2em] text-muted-foreground max-sm:col-span-2 sm:col-start-2 sm:row-start-1 sm:self-end">
                            "Hashiwokakero · " {t!(i18n, play.heading)}
                        </h1>
                        // Phones: the badges next to the title while they fit, and without the
                        // island count, which the progress under the board shows too
                        <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 sm:col-start-2 sm:row-start-2 sm:block sm:self-start">
                            <p class="text-2xl font-black max-[359px]:text-xl sm:text-3xl">{move || game.source.get().title(i18n.get_locale())}</p>
                            <div class="flex gap-1.5 sm:mt-1">
                                <SolutionsBadge unique=game.unique />
                                <Badge variant=BadgeVariant::Muted class="hidden sm:inline-flex">
                                    {move || t_string!(i18n, common.game.islands, count = island_count.get())}
                                </Badge>
                            </div>
                        </div>
                        <div class="flex gap-1.5 max-sm:col-start-2 max-sm:row-start-2 sm:col-start-3 sm:row-start-3 sm:gap-2 sm:self-stretch sm:justify-self-end">
                            <PauseButton />
                            <NewPuzzleButton />
                        </div>
                        <div class="flex gap-1.5 max-sm:col-span-2 max-sm:mt-2 sm:col-start-3 sm:row-span-2 sm:row-start-1 sm:gap-2 sm:justify-self-end">
                            <Stat label=Signal::derive(move || t_string!(i18n, common.game.time).to_string()) jp="時間" value=Signal::derive(move || format_time(game.elapsed.get())) />
                            <Stat label=Signal::derive(move || t_string!(i18n, common.game.moves).to_string()) jp="手" value=Signal::derive(move || game.moves.get().to_string()) />
                            <Stat label=Signal::derive(move || t_string!(i18n, common.game.hints).to_string()) jp="助" value=Signal::derive(move || game.hints.get().to_string()) />
                        </div>
                        <DifficultyChips class="max-sm:col-span-2 max-sm:mt-3 sm:col-span-2 sm:col-start-1 sm:row-start-3" />
                    </div>

                    <div class="relative">
                        // Hidden while paused, not just covered: no peeking, no focus, nothing read out
                        <div class:invisible=move || game.paused.get()>
                        <BoardView
                            board=game.board
                            bridges=game.bridges
                            mode=BoardMode::Play
                            on_cycle=Callback::new(move |(edge, delta)| game.cycle(edge, delta))
                            wrong=game.wrong
                            hint=game.hint
                            framed=true
                        />
                        </div>
                        <PausedCover />
                        <Show when=move || game.loading.get() || game.status.get() == Status::Empty>
                            <div class="absolute inset-0 z-10 grid place-items-center rounded-2xl bg-background/55 backdrop-blur-sm">
                                <div class="flex flex-col items-center gap-3 text-center">
                                    <PixelSprite art=&BOAT class="bob w-20" />
                                    <p class="font-pixel text-lg">"生成中 · " {t!(i18n, play.generating)}</p>
                                    <p class="max-w-xs text-sm text-muted-foreground">
                                        {t!(i18n, play.generating_text)}
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

                    <GameActions busy=busy not_playing=not_playing />
                </div>

                <aside class="flex flex-col gap-4">
                    <LegendCard />
                    <SolverTools>
                    <SolverPanel report=game.report busy=game.solving lab="/lab">
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
                    </SolverTools>
                </aside>
            </div>
        </section>
    }
}
