use common::{Board, Difficulty, GenerateParams, Pos, Puzzle};
use leptos::prelude::*;

use crate::components::board::{BoardMode, BoardView};
use crate::components::celebration::{Petals, Stamp};
use crate::components::sprites::{BOAT, PixelSprite};
use crate::components::solver_panel::{PanelButton, SolverPanel};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::kbd::Kbd;
use crate::components::ui::progress::Progress;
use crate::state::{
    self, Game, Settings, Source, Status, ToastKind, format_time, random_seed, toast,
};
use crate::components::layout::use_localized;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::difficulty_name;
use crate::seo::PageMeta;

fn new_random(game: Game, difficulty: Difficulty) {
    let (width, height) = difficulty.size();
    let params = GenerateParams {
        width,
        height,
        difficulty,
        seed: random_seed(),
    };
    state::new_puzzle(game, params, Source::Generated(params));
}

#[component]
fn Stat(#[prop(into)] label: Signal<String>, jp: &'static str, #[prop(into)] value: Signal<String>) -> impl IntoView {
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
fn DifficultyPicker() -> impl IntoView {
    let i18n = use_i18n();
    let settings = expect_context::<Settings>();
    view! {
        <div class="grid grid-cols-5 gap-1.5">
            {Difficulty::ALL
                .into_iter()
                .map(|d| {
                    let (w, h) = d.size();
                    view! {
                        <button
                            class=move || {
                                format!(
                                    "flex flex-col items-center rounded-lg border px-1 py-2 transition-all hover:-translate-y-0.5 {}",
                                    if settings.difficulty.get() == d {
                                        "border-primary bg-primary text-primary-foreground shadow-md"
                                    } else {
                                        "bg-background hover:border-primary/50"
                                    },
                                )
                            }
                            title=move || format!("{} · {w}×{h}", difficulty_name(i18n.get_locale(), d))
                            on:click=move |_| settings.difficulty.set(d)
                        >
                            <span class="font-pixel text-lg leading-none">{d.kanji()}</span>
                            <span class="mt-1 max-w-full truncate text-[10px] font-bold uppercase tracking-wider">
                                {move || difficulty_name(i18n.get_locale(), d)}
                            </span>
                            <span class="text-[10px] opacity-70">{format!("{w}×{h}")}</span>
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[component]
fn NewGameCard() -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();
    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, practice.card.title)} <span class="font-pixel text-sm text-primary">"練習"</span>
                </CardTitle>
                <CardDescription>
                    {t!(i18n, practice.card.text)}
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                <DifficultyPicker />
                <div class="flex gap-2">
                    <Button
                        class="flex-1"
                        attr:disabled=move || game.loading.get()
                        on:click=move |_| new_random(game, settings.difficulty.get_untracked())
                    >
                        {move || if game.loading.get() { t_string!(i18n, practice.generating).to_string() } else { format!("{} 生成", t_string!(i18n, practice.generate)) }}
                    </Button>
                </div>
            </CardContent>
        </Card>
    }
}

/// A tiny board showing one kind of bridge, for the legend.
fn legend_board(required: u8, count: u8) -> (Board, Vec<u8>) {
    let mut puzzle = Puzzle::empty(4, 1);
    puzzle.set(Pos::new(0, 0), required);
    puzzle.set(Pos::new(3, 0), required);
    let board = Board::new(puzzle);
    (board, vec![count])
}

#[component]
fn LegendCard() -> impl IntoView {
    let i18n = use_i18n();
    let (single, single_bridges) = legend_board(1, 1);
    let (double, double_bridges) = legend_board(2, 2);
    view! {
        <Card class="gap-3">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-base">
                    {t!(i18n, practice.legend.title)} <span class="font-pixel text-sm text-primary">"操作"</span>
                </CardTitle>
            </CardHeader>
            <CardContent class="flex flex-col gap-3 text-sm">
                <div class="grid grid-cols-2 gap-3">
                    <div class="flex flex-col items-center gap-1">
                        <BoardView
                            board=Signal::stored(single)
                            bridges=Signal::stored(single_bridges)
                            mode=BoardMode::View
                            max_height="2.6rem"
                        />
                        <span class="text-xs text-muted-foreground">"1 · " {t!(i18n, practice.legend.single)}</span>
                    </div>
                    <div class="flex flex-col items-center gap-1">
                        <BoardView
                            board=Signal::stored(double)
                            bridges=Signal::stored(double_bridges)
                            mode=BoardMode::View
                            max_height="2.6rem"
                        />
                        <span class="text-xs text-muted-foreground">"2 · " {t!(i18n, practice.legend.double)}</span>
                    </div>
                </div>
                <ul class="flex flex-col gap-1.5 text-muted-foreground">
                    <li>{t!(i18n, practice.legend.build)}</li>
                    <li>{t!(i18n, practice.legend.again)}</li>
                    <li>{t!(i18n, practice.legend.remove, <kbd> = <Kbd />)}</li>
                    <li>
                        <Kbd>"Ctrl Z"</Kbd> " " {t!(i18n, common.game.undo)} " · " <Kbd>"Ctrl Y"</Kbd> " "
                        {t!(i18n, common.game.redo)} " · " <Kbd>"H"</Kbd> " " {t!(i18n, common.game.hint)}
                    </li>
                </ul>
            </CardContent>
        </Card>
    }
}

#[component]
fn WinOverlay() -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();
    let dismissed = RwSignal::new(false);
    Effect::new(move |_| {
        game.epoch.track();
        game.status.track();
        dismissed.set(false);
    });

    let share = move |_| {
        let locale = i18n.get_locale_untracked();
        let text = format!(
            "🌉 Hashi · {} · {} · 橋をかけろ",
            game.source.get_untracked().title(locale),
            td_string!(
                locale,
                practice.share,
                time = format_time(game.elapsed.get_untracked()),
                moves = game.moves.get_untracked(),
                hints = game.hints.get_untracked()
            ),
        );
        let _ = window().navigator().clipboard().write_text(&text);
        toast(ToastKind::Success, |l| td_string!(l, common.toast.copied).to_string());
    };

    view! {
        <Show when=move || game.status.get() == Status::Won && !dismissed.get()>
            <Petals />
            <div class="absolute inset-0 z-10 grid place-items-center p-4">
                <Card class="w-full max-w-sm items-center gap-3 px-6 text-center shadow-2xl animate-in zoom-in-95 fade-in">
                    <Stamp kanji="完" class="size-20 text-5xl" />
                    <h2 class="text-2xl font-black">"見事！ " {t!(i18n, common.game.well_done)}</h2>
                    <p class="text-muted-foreground">
                        {t!(
                            i18n,
                            practice.won,
                            time = move || format_time(game.elapsed.get()),
                            moves = move || game.moves.get(),
                            hints = move || game.hints.get(),
                            <b> = <span class="font-pixel text-foreground" />
                        )}
                    </p>
                    <div class="flex w-full flex-wrap justify-center gap-2 pt-1">
                        <Button on:click=move |_| new_random(game, settings.difficulty.get_untracked())>
                            {t!(i18n, practice.next)} " 次へ"
                        </Button>
                        <Button variant=ButtonVariant::Outline on:click=share>
                            {t!(i18n, practice.share_button)}
                        </Button>
                        <Button variant=ButtonVariant::Ghost on:click=move |_| dismissed.set(true)>
                            {t!(i18n, practice.admire)}
                        </Button>
                    </div>
                </Card>
            </div>
        </Show>
        <Show when=move || game.status.get() == Status::Revealed && !dismissed.get()>
            // Below the board on phones, where it would cover the bottom rows
            <div class="relative z-10 mt-3 flex justify-center sm:absolute sm:inset-x-0 sm:bottom-3 sm:mt-0 sm:px-4">
                <div class="flex flex-wrap items-center justify-center gap-3 rounded-xl border bg-card/95 px-4 py-2 text-sm shadow-xl backdrop-blur animate-in slide-in-from-bottom-4 fade-in">
                    <span class="hanko grid size-9 shrink-0 rotate-[-6deg] place-items-center rounded-md font-pixel text-xl">
                        "解"
                    </span>
                    <span>{t!(i18n, practice.revealed)}</span>
                    <Button size=ButtonSize::Sm on:click=move |_| game.restart()>
                        {t!(i18n, practice.play_myself)}
                    </Button>
                    <Button
                        size=ButtonSize::Sm
                        variant=ButtonVariant::Outline
                        on:click=move |_| new_random(game, settings.difficulty.get_untracked())
                    >
                        {t!(i18n, practice.new_puzzle)}
                    </Button>
                    <Button size=ButtonSize::Sm variant=ButtonVariant::Ghost on:click=move |_| dismissed.set(true)>
                        "✕"
                    </Button>
                </div>
            </div>
        </Show>
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
            new_random(game, settings.difficulty.get_untracked());
        }
    });

    // Keyboard shortcuts
    Effect::new(move |_| {
        let handle = window_event_listener(leptos::ev::keydown, move |ev| {
            let target_is_input = ev
                .target()
                .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok())
                .is_some_and(|el| matches!(el.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA"));
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
                                    game.source.with(|s| s.difficulty()).map(|d| d.kanji()).unwrap_or("自作")
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
                                        {move || t_string!(i18n, common.game.islands, count = island_count.get()).to_string()}
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
                                        t_string!(i18n, common.game.groups, count = c.components).to_string()
                                    } else {
                                        t_string!(i18n, common.game.all_connected).to_string()
                                    }
                                }}
                            </span>
                        </div>
                        <Progress
                            value=Signal::derive(move || {
                                let n = island_count.get();
                                if n == 0 { 0.0 } else { islands_done.get() as f64 / n as f64 * 100.0 }
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
                            on_click=Callback::new(move |_| state::reveal(game, settings))
                        >
                            {t!(i18n, common.game.solve)} " 解く"
                        </PanelButton>
                        <PanelButton
                            disabled=busy
                            on_click=Callback::new(move |_| state::check_unique(game, settings))
                        >
                            {t!(i18n, common.game.unique)}
                        </PanelButton>
                        <PanelButton
                            disabled=busy
                            on_click=Callback::new(move |_| crate::pages::download_cnf(game.puzzle()))
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

/// Text about the puzzle below the game, for people who arrive from a search engine (and for
/// the search engine itself, the board is just an SVG to it).
#[component]
pub fn SeoIntro() -> impl IntoView {
    let i18n = use_i18n();
    let rules = use_localized("/rules");
    let create = use_localized("/create");
    let lab = use_localized("/lab");
    const LINK: &str = "font-medium text-primary underline decoration-primary/40 underline-offset-4 hover:decoration-primary";
    view! {
        <article class="mx-auto max-w-7xl px-4 pb-4 pt-10 sm:px-6">
            <div class="mb-6 flex items-end gap-3">
                <span class="vertical-jp hidden font-pixel text-sm leading-none text-primary sm:block">"橋をかけろ"</span>
                <div>
                    <p class="font-pixel text-sm text-primary">{t!(i18n, practice.about.kicker)}</p>
                    <h2 class="text-2xl font-black sm:text-3xl">{t!(i18n, practice.about.title)}</h2>
                </div>
            </div>
            <div class="grid grid-cols-1 gap-x-10 gap-y-8 text-[15px] leading-relaxed text-muted-foreground md:grid-cols-2">
                <section class="flex flex-col gap-2">
                    <h3 class="text-lg font-bold text-foreground">{t!(i18n, practice.about.what_title)}</h3>
                    <p>{t!(i18n, practice.about.what, <b> = <b class="font-semibold text-foreground" />)}</p>
                </section>
                <section class="flex flex-col gap-2">
                    <h3 class="text-lg font-bold text-foreground">{t!(i18n, practice.about.how_title)}</h3>
                    <p>
                        {t!(
                            i18n,
                            practice.about.how,
                            <a> = move |children: ChildrenFn| view! { <a class=LINK href=rules>{children()}</a> }
                        )}
                    </p>
                </section>
                <section class="flex flex-col gap-2">
                    <h3 class="text-lg font-bold text-foreground">{t!(i18n, practice.about.techniques_title)}</h3>
                    <ul class="list-disc space-y-1 ps-5">
                        <li>{t!(i18n, practice.about.technique_double)}</li>
                        <li>{t!(i18n, practice.about.technique_isolation)}</li>
                        <li>{t!(i18n, practice.about.technique_count)}</li>
                        <li>{t!(i18n, practice.about.technique_block)}</li>
                        <li>{t!(i18n, practice.about.technique_connect)}</li>
                    </ul>
                </section>
                <section class="flex flex-col gap-2">
                    <h3 class="text-lg font-bold text-foreground">{t!(i18n, practice.about.why_title)}</h3>
                    <ul class="list-disc space-y-1 ps-5">
                        <li>{t!(i18n, practice.about.why_unique, <b> = <b class="font-semibold text-foreground" />)}</li>
                        <li>{t!(i18n, practice.about.why_unlimited)}</li>
                        <li>{t!(i18n, practice.about.why_art)}</li>
                        <li>
                            {t!(
                                i18n,
                                practice.about.why_workshop,
                                <workshop> = move |children: ChildrenFn| view! { <a class=LINK href=create>{children()}</a> },
                                <lab> = move |children: ChildrenFn| view! { <a class=LINK href=lab>{children()}</a> }
                            )}
                        </li>
                    </ul>
                </section>
            </div>
        </article>
    }
}
