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
    self, Game, Settings, Source, Status, ToastKind, daily_params, format_time, random_seed, today,
    toast,
};

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

fn new_daily(game: Game) {
    let date = today();
    let params = daily_params(&date);
    state::new_puzzle(game, params, Source::Daily { date, params });
}

#[component]
fn Stat(label: &'static str, jp: &'static str, #[prop(into)] value: Signal<String>) -> impl IntoView {
    view! {
        <div class="flex min-w-[4.5rem] flex-col items-center rounded-lg border bg-card px-3 py-1.5 shadow-xs">
            <span class="font-pixel text-xl tabular-nums leading-tight">{move || value.get()}</span>
            <span class="text-[10px] font-bold uppercase tracking-[0.16em] text-muted-foreground">
                {label} " · " {jp}
            </span>
        </div>
    }
}

#[component]
fn DifficultyPicker() -> impl IntoView {
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
                            title=format!("{} · {w}×{h}", d.label())
                            on:click=move |_| settings.difficulty.set(d)
                        >
                            <span class="font-pixel text-lg leading-none">{d.kanji()}</span>
                            <span class="mt-1 text-[10px] font-bold uppercase tracking-wider">{d.label()}</span>
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
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();
    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    "New puzzle" <span class="font-pixel text-sm text-primary">"新しい橋"</span>
                </CardTitle>
                <CardDescription>
                    "Freshly generated, and proven by the SAT solver to have exactly one solution."
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
                        {move || if game.loading.get() { "Generating…" } else { "Generate 生成" }}
                    </Button>
                    <Button
                        variant=ButtonVariant::Secondary
                        class="flex-1"
                        attr:disabled=move || game.loading.get()
                        on:click=move |_| new_daily(game)
                    >
                        "Daily 今日の橋"
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
    let (single, single_bridges) = legend_board(1, 1);
    let (double, double_bridges) = legend_board(2, 2);
    view! {
        <Card class="gap-3">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-base">
                    "How to build" <span class="font-pixel text-sm text-primary">"操作"</span>
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
                        <span class="text-xs text-muted-foreground">"1 · wooden bridge"</span>
                    </div>
                    <div class="flex flex-col items-center gap-1">
                        <BoardView
                            board=Signal::stored(double)
                            bridges=Signal::stored(double_bridges)
                            mode=BoardMode::View
                            max_height="2.6rem"
                        />
                        <span class="text-xs text-muted-foreground">"2 · vermilion bridge"</span>
                    </div>
                </div>
                <ul class="flex flex-col gap-1.5 text-muted-foreground">
                    <li>"Click the water between two islands, or drag from an island."</li>
                    <li>"Click again for a second bridge, a third click removes them."</li>
                    <li>
                        "Right click or " <Kbd>"Shift"</Kbd> " + click removes a bridge."
                    </li>
                    <li>
                        <Kbd>"Ctrl Z"</Kbd> " undo · " <Kbd>"Ctrl Y"</Kbd> " redo · " <Kbd>"H"</Kbd>
                        " hint"
                    </li>
                </ul>
            </CardContent>
        </Card>
    }
}

#[component]
fn WinOverlay() -> impl IntoView {
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();
    let dismissed = RwSignal::new(false);
    Effect::new(move |_| {
        game.epoch.track();
        game.status.track();
        dismissed.set(false);
    });

    let share = move |_| {
        let text = format!(
            "🌉 Hashi · {} · solved in {} with {} moves{} · 橋をかけろ",
            game.source.get_untracked().title(),
            format_time(game.elapsed.get_untracked()),
            game.moves.get_untracked(),
            match game.hints.get_untracked() {
                0 => String::new(),
                1 => " and 1 hint".to_string(),
                n => format!(" and {n} hints"),
            }
        );
        let _ = window().navigator().clipboard().write_text(&text);
        toast(ToastKind::Success, "Result copied to the clipboard.");
    };

    view! {
        <Show when=move || game.status.get() == Status::Won && !dismissed.get()>
            <Petals />
            <div class="absolute inset-0 z-10 grid place-items-center p-4">
                <Card class="w-full max-w-sm items-center gap-3 px-6 text-center shadow-2xl animate-in zoom-in-95 fade-in">
                    <Stamp kanji="完" class="size-20 text-5xl" />
                    <h2 class="text-2xl font-black">"見事！ Well done!"</h2>
                    <p class="text-muted-foreground">
                        "Every island is connected, in "
                        <span class="font-pixel text-foreground">{move || format_time(game.elapsed.get())}</span>
                        " with " {move || game.moves.get()} " moves"
                        {move || match game.hints.get() {
                            0 => " and no hints.".to_string(),
                            1 => " and one hint.".to_string(),
                            n => format!(" and {n} hints."),
                        }}
                    </p>
                    <div class="flex w-full flex-wrap justify-center gap-2 pt-1">
                        <Button on:click=move |_| new_random(game, settings.difficulty.get_untracked())>
                            "Next puzzle 次へ"
                        </Button>
                        <Button variant=ButtonVariant::Outline on:click=share>
                            "Share"
                        </Button>
                        <Button variant=ButtonVariant::Ghost on:click=move |_| dismissed.set(true)>
                            "Admire"
                        </Button>
                    </div>
                </Card>
            </div>
        </Show>
        <Show when=move || game.status.get() == Status::Revealed && !dismissed.get()>
            <div class="absolute inset-x-0 bottom-3 z-10 flex justify-center px-4">
                <div class="flex flex-wrap items-center gap-3 rounded-xl border bg-card/95 px-4 py-2 text-sm shadow-xl backdrop-blur animate-in slide-in-from-bottom-4 fade-in">
                    <span class="hanko grid size-9 shrink-0 rotate-[-6deg] place-items-center rounded-md font-pixel text-xl">
                        "解"
                    </span>
                    <span>"Solved by the SAT solver."</span>
                    <Button size=ButtonSize::Sm on:click=move |_| game.restart()>
                        "Play it myself"
                    </Button>
                    <Button
                        size=ButtonSize::Sm
                        variant=ButtonVariant::Outline
                        on:click=move |_| new_random(game, settings.difficulty.get_untracked())
                    >
                        "New puzzle"
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
        <section class="mx-auto max-w-7xl px-4 py-6 sm:px-6">
            <div class="grid gap-6 lg:grid-cols-[minmax(0,1fr)_380px]">
                <div class="flex min-w-0 flex-col gap-4">
                    <div class="flex flex-wrap items-end justify-between gap-3">
                        <div class="flex items-center gap-3">
                            <span class="vertical-jp hidden font-pixel text-sm leading-none text-primary sm:block">
                                {move || {
                                    game.source.with(|s| s.difficulty()).map(|d| d.kanji()).unwrap_or("自作")
                                }}
                            </span>
                            <div>
                                <p class="text-xs font-bold uppercase tracking-[0.2em] text-muted-foreground">
                                    "Hashiwokakero"
                                </p>
                                <h1 class="text-2xl font-black sm:text-3xl">{move || game.source.get().title()}</h1>
                                <div class="mt-1 flex gap-1.5">
                                    {move || match game.unique.get() {
                                        Some(true) => {
                                            view! {
                                                <Badge variant=BadgeVariant::Success>"unique solution ✓"</Badge>
                                            }
                                                .into_any()
                                        }
                                        Some(false) => {
                                            view! {
                                                <Badge variant=BadgeVariant::Warning>"several solutions"</Badge>
                                            }
                                                .into_any()
                                        }
                                        None => ().into_any(),
                                    }}
                                    <Badge variant=BadgeVariant::Muted>
                                        {move || format!("{} islands", island_count.get())}
                                    </Badge>
                                </div>
                            </div>
                        </div>
                        <div class="flex gap-2">
                            <Stat label="Time" jp="時間" value=Signal::derive(move || format_time(game.elapsed.get())) />
                            <Stat label="Moves" jp="手" value=Signal::derive(move || game.moves.get().to_string()) />
                            <Stat label="Hints" jp="助" value=Signal::derive(move || game.hints.get().to_string()) />
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
                                    <p class="font-pixel text-lg">"生成中 · generating"</p>
                                    <p class="max-w-xs text-sm text-muted-foreground">
                                        "Growing islands, building bridges, and asking the SAT solver whether the solution is unique…"
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
                                " islands complete"
                            </span>
                            <span>
                                {move || {
                                    let c = check.get();
                                    if c.components > 1 {
                                        format!("{} separate groups", c.components)
                                    } else {
                                        "all connected".to_string()
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

                    <div class="flex flex-wrap gap-2">
                        <Button
                            variant=ButtonVariant::Outline
                            attr:disabled=move || not_playing.get() || game.undo_stack.with(Vec::is_empty)
                            on:click=move |_| game.undo()
                        >
                            "↶ Undo"
                        </Button>
                        <Button
                            variant=ButtonVariant::Outline
                            attr:disabled=move || not_playing.get() || game.redo_stack.with(Vec::is_empty)
                            on:click=move |_| game.redo()
                        >
                            "Redo ↷"
                        </Button>
                        <Button
                            variant=ButtonVariant::Ghost
                            attr:disabled=move || busy.get() || game.status.get() == Status::Empty
                            on:click=move |_| game.restart()
                        >
                            "Restart"
                        </Button>
                        <div class="ml-auto flex gap-2">
                            <Button
                                variant=ButtonVariant::Secondary
                                attr:disabled=move || not_playing.get()
                                on:click=move |_| state::check(game, settings)
                            >
                                "Check 確認"
                            </Button>
                            <Button
                                variant=ButtonVariant::Secondary
                                attr:disabled=move || not_playing.get()
                                on:click=move |_| state::hint(game, settings)
                            >
                                "Hint 助言"
                            </Button>
                        </div>
                    </div>
                </div>

                <aside class="flex flex-col gap-4">
                    <NewGameCard />
                    <SolverPanel report=game.report busy=game.solving>
                        <PanelButton
                            label="Solve 解く"
                            primary=true
                            disabled=busy
                            on_click=Callback::new(move |_| state::reveal(game, settings))
                        />
                        <PanelButton
                            label="Unique?"
                            disabled=busy
                            on_click=Callback::new(move |_| state::check_unique(game, settings))
                        />
                        <PanelButton
                            label="CNF ↓"
                            disabled=busy
                            on_click=Callback::new(move |_| crate::pages::download_cnf(game.puzzle()))
                        />
                    </SolverPanel>
                    <LegendCard />
                </aside>
            </div>
        </section>
    }
}
