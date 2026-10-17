use common::{Board, Difficulty, GenerateParams, MAX_SIZE, MIN_SIZE, Pos, Puzzle, SolveOutcome};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

use crate::api::generate_puzzle;
use crate::components::board::{BoardMode, BoardView, CellAction};
use crate::components::solver_panel::{PanelButton, SolverPanel};
use crate::components::ui::alert::{Alert, AlertDescription, AlertTitle};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::kbd::Kbd;
use crate::components::ui::label::Label;
use crate::state::{self, Editor, Game, Settings, Source, ToastKind, download, random_seed, toast};
use crate::seo::PageMeta;

#[component]
fn SizeInput(label: &'static str, #[prop(into)] value: Signal<u8>, on_change: Callback<u8>) -> impl IntoView {
    let id = format!("size-{}", label.to_lowercase());
    view! {
        <div class="flex flex-col gap-2">
            <Label html_for=id.clone()>
                {label} <span class="ml-auto font-pixel tabular-nums">{move || value.get()}</span>
            </Label>
            <input
                id=id
                type="range"
                min=MIN_SIZE
                max=MAX_SIZE
                prop:value=move || value.get()
                class="h-9 w-full cursor-pointer accent-[var(--primary)]"
                on:input=move |ev| {
                    if let Ok(v) = event_target_value(&ev).parse::<u8>() {
                        on_change.run(v.clamp(MIN_SIZE, MAX_SIZE));
                    }
                }
            />
        </div>
    }
}

/// Quick sanity checks that don't need a SAT solver.
fn warnings(puzzle: &Puzzle) -> Vec<String> {
    let mut out = Vec::new();
    if puzzle.islands.is_empty() {
        return out;
    }
    let board = Board::new(puzzle.clone());
    for (i, island) in puzzle.islands.iter().enumerate() {
        let max = board.incident[i].len() as u8 * 2;
        if max < island.required {
            out.push(format!(
                "Island {} at ({}, {}) needs {} bridges but can have at most {max}.",
                island.required,
                island.pos.x + 1,
                island.pos.y + 1,
                island.required
            ));
        }
    }
    let total: u32 = puzzle.islands.iter().map(|i| i.required as u32).sum();
    if total % 2 == 1 {
        out.push(format!("The numbers add up to {total}, an odd sum can never be solved."));
    }
    out.truncate(4);
    out
}

#[component]
pub fn CreatePage() -> impl IntoView {
    let editor = expect_context::<Editor>();
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();
    let navigate = use_navigate();

    let board = Memo::new(move |_| Board::new(editor.puzzle.get()));
    let bridges = Signal::derive(move || {
        let n = board.with(|b| b.edges.len());
        editor.solution.get().filter(|s| s.len() == n).unwrap_or_else(|| vec![0; n])
    });
    let generating = RwSignal::new(false);
    let width = Signal::derive(move || editor.puzzle.with(|p| p.width));
    let height = Signal::derive(move || editor.puzzle.with(|p| p.height));
    let issues = Memo::new(move |_| editor.puzzle.with(warnings));
    let busy = Signal::derive(move || editor.solving.get() || generating.get());
    let empty = Signal::derive(move || editor.puzzle.with(|p| p.islands.is_empty()));

    // None: clicks count up, Some(0): erase, Some(n): place n (tap again to remove). Phones have
    // no right click or keyboard, the palette makes every number one tap away.
    let brush = RwSignal::new(None::<u8>);
    let on_cell = Callback::new(move |(pos, action): (Pos, CellAction)| {
        editor.edit(|p| {
            let current = p.island_at(pos).map_or(0, |i| p.islands[i].required);
            let next = match (action, brush.get_untracked()) {
                (CellAction::Increment, Some(n)) if n == current => 0,
                (CellAction::Increment, Some(n)) => n,
                (CellAction::Increment, None) => (current + 1) % 9,
                (CellAction::Decrement, _) => (current + 8) % 9,
                (CellAction::Set(n), _) => n.min(8),
            };
            p.set(pos, next);
        })
    });

    let solve = move |unique: bool| {
        let puzzle = editor.puzzle.get_untracked();
        let config = settings.solver.get_untracked();
        spawn_local(async move {
            let solution = state::run_solver(puzzle.clone(), config, unique, editor.report, editor.solving).await;
            // Ignore the result if the puzzle was edited meanwhile
            if editor.puzzle.with_untracked(|p| *p == puzzle) {
                editor.solution.set(solution);
                let outcome = editor.report.with_untracked(|r| r.as_ref().map(|r| r.outcome.clone()));
                if unique && let Some(SolveOutcome::Solved { unique: Some(u), .. }) = outcome {
                    if u {
                        toast(ToastKind::Success, "Unique: a fine puzzle!");
                    } else {
                        toast(ToastKind::Info, "More than one solution, add or change islands.");
                    }
                }
            }
        });
    };

    let generate = move |difficulty: Difficulty| {
        let params = GenerateParams {
            width: width.get_untracked(),
            height: height.get_untracked(),
            difficulty,
            seed: random_seed(),
        };
        generating.set(true);
        spawn_local(async move {
            match generate_puzzle(params).await {
                Ok(g) => {
                    editor.edit(|p| *p = g.puzzle);
                    editor.solution.set(None);
                }
                Err(e) => toast(ToastKind::Error, format!("Could not generate: {e}")),
            }
            generating.set(false);
        });
    };

    let file_input = NodeRef::<leptos::html::Input>::new();
    let import = move |_| {
        let Some(input) = file_input.get_untracked() else { return };
        let Some(file) = input.files().and_then(|f| f.get(0)) else { return };
        let name = file.name();
        input.set_value("");
        spawn_local(async move {
            let text = wasm_bindgen_futures::JsFuture::from(file.text()).await;
            match text.ok().and_then(|t| t.as_string()).map(|t| Puzzle::parse_txt(&t)) {
                Some(Ok(puzzle)) => {
                    editor.edit(|p| *p = puzzle);
                    toast(ToastKind::Success, format!("Imported {name}"));
                }
                Some(Err(e)) => toast(ToastKind::Error, format!("{name}: {e}")),
                None => toast(ToastKind::Error, format!("Could not read {name}")),
            }
        });
    };

    let play = move |_| {
        let puzzle = editor.puzzle.get_untracked();
        let solution = editor.solution.get_untracked();
        let unique = editor.report.with_untracked(|r| match r.as_ref().map(|r| &r.outcome) {
            Some(SolveOutcome::Solved { unique, .. }) => *unique,
            _ => None,
        });
        game.set_puzzle(puzzle, solution, unique, Source::Custom);
        navigate("/practice", Default::default());
    };

    view! {
        <PageMeta
            title="Make your own Hashiwokakero puzzle · Hashi"
            description="Design a Bridges puzzle in the workshop: place islands, let a SAT solver check that it has exactly one solution, export it as a text file, and play it."
            path="/create"
        />
        <section class="mx-auto max-w-7xl px-4 py-6 sm:px-6">
            <div class="mb-5 flex flex-wrap items-end justify-between gap-3">
                <div>
                    <p class="font-pixel text-sm text-primary">"作る · tsukuru"</p>
                    <h1 class="text-3xl font-black">"Puzzle workshop"</h1>
                    <p class="mt-1 max-w-2xl text-muted-foreground">
                        "Place islands, let the SAT solver check that your puzzle has exactly one solution, then play it."
                    </p>
                </div>
                <div class="hidden flex-wrap gap-2 text-xs text-muted-foreground md:flex">
                    <span><Kbd>"Click"</Kbd> " +1"</span>
                    <span><Kbd>"Right click"</Kbd> " −1"</span>
                    <span><Kbd>"1"</Kbd> "–" <Kbd>"8"</Kbd> " set · " <Kbd>"⌫"</Kbd> " erase (while hovering)"</span>
                </div>
            </div>

            <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]">
                <div class="flex min-w-0 flex-col gap-4">
                    <div class="grid grid-cols-5 gap-1.5 sm:grid-cols-10 sm:gap-1" role="radiogroup" aria-label="Brush">
                        {std::iter::once(None)
                            .chain((1..=8).map(Some))
                            .chain(std::iter::once(Some(0)))
                            .map(|b| {
                                let (label, title) = match b {
                                    None => ("+1".to_string(), "Each tap counts up".to_string()),
                                    Some(0) => ("消".to_string(), "Erase".to_string()),
                                    Some(n) => (n.to_string(), format!("Place a {n}")),
                                };
                                view! {
                                    <button
                                        type="button"
                                        role="radio"
                                        title=title
                                        aria-checked=move || (brush.get() == b).to_string()
                                        class=move || {
                                            format!(
                                                "grid h-10 place-items-center rounded-md border font-pixel text-base transition-colors {}",
                                                if brush.get() == b {
                                                    "border-primary bg-primary text-primary-foreground shadow-sm"
                                                } else {
                                                    "bg-card hover:border-primary/50"
                                                },
                                            )
                                        }
                                        on:click=move |_| brush.set(b)
                                    >
                                        {label}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                    <BoardView
                        board=board
                        bridges=bridges
                        mode=BoardMode::Edit
                        on_cell=on_cell
                        framed=true
                    />
                    {move || {
                        let list = issues.get();
                        (!list.is_empty())
                            .then(|| {
                                view! {
                                    <Alert class="border-warning/50 bg-warning-light text-warning-dark">
                                        <AlertTitle>"注意 · Heads up"</AlertTitle>
                                        <AlertDescription>
                                            <ul class="list-disc pl-5">
                                                {list.into_iter().map(|w| view! { <li>{w}</li> }).collect_view()}
                                            </ul>
                                        </AlertDescription>
                                    </Alert>
                                }
                            })
                    }}
                </div>

                <aside class="flex flex-col gap-4">
                    <Card class="gap-4">
                        <CardHeader>
                            <CardTitle class="flex items-baseline gap-2 text-xl">
                                "Grid" <span class="font-pixel text-sm text-primary">"盤"</span>
                            </CardTitle>
                            <CardDescription>
                                {move || {
                                    let n = editor.puzzle.with(|p| p.islands.len());
                                    format!("{}×{} cells, {n} islands", width.get(), height.get())
                                }}
                            </CardDescription>
                        </CardHeader>
                        <CardContent class="flex flex-col gap-4">
                            <div class="grid grid-cols-2 gap-4">
                                <SizeInput
                                    label="Width"
                                    value=width
                                    on_change=Callback::new(move |w| {
                                        editor.edit(|p| *p = p.resized(w, p.height))
                                    })
                                />
                                <SizeInput
                                    label="Height"
                                    value=height
                                    on_change=Callback::new(move |h| {
                                        editor.edit(|p| *p = p.resized(p.width, h))
                                    })
                                />
                            </div>
                            <div class="flex flex-col gap-2">
                                <Label>"Fill with a generated puzzle"</Label>
                                <div class="grid grid-cols-5 gap-1.5">
                                    {Difficulty::ALL
                                        .into_iter()
                                        .map(|d| {
                                            view! {
                                                <Button
                                                    variant=ButtonVariant::Outline
                                                    size=ButtonSize::Sm
                                                    class="w-full font-pixel"
                                                    attr:title=d.label()
                                                    attr:disabled=move || busy.get()
                                                    on:click=move |_| generate(d)
                                                >
                                                    {d.kanji()}
                                                </Button>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </div>
                            <div class="grid grid-cols-3 gap-2">
                                <Button
                                    variant=ButtonVariant::Outline
                                    size=ButtonSize::Sm
                                    class="w-full"
                                    on:click=move |_| {
                                        if let Some(input) = file_input.get_untracked() {
                                            input.click();
                                        }
                                    }
                                >
                                    "Import .txt"
                                </Button>
                                <Button
                                    variant=ButtonVariant::Outline
                                    size=ButtonSize::Sm
                                    class="w-full"
                                    attr:disabled=move || empty.get()
                                    on:click=move |_| {
                                        download("hashi.txt", &editor.puzzle.get_untracked().to_txt(), "text/plain")
                                    }
                                >
                                    "Export .txt"
                                </Button>
                                <Button
                                    variant=ButtonVariant::Ghost
                                    size=ButtonSize::Sm
                                    class="w-full"
                                    attr:disabled=move || empty.get()
                                    on:click=move |_| editor.edit(|p| *p = Puzzle::empty(p.width, p.height))
                                >
                                    "Clear"
                                </Button>
                            </div>
                            <input
                                node_ref=file_input
                                type="file"
                                accept=".txt,text/plain"
                                class="hidden"
                                on:change=import
                            />
                            <Button attr:disabled=move || empty.get() on:click=play>
                                "Play this puzzle 遊ぶ →"
                            </Button>
                        </CardContent>
                    </Card>

                    <SolverPanel report=editor.report busy=editor.solving>
                        <PanelButton
                            label="Solve 解く"
                            primary=true
                            disabled=Signal::derive(move || busy.get() || empty.get())
                            on_click=Callback::new(move |_| solve(false))
                        />
                        <PanelButton
                            label="Unique?"
                            disabled=Signal::derive(move || busy.get() || empty.get())
                            on_click=Callback::new(move |_| solve(true))
                        />
                        <PanelButton
                            label="CNF ↓"
                            disabled=Signal::derive(move || busy.get() || empty.get())
                            on_click=Callback::new(move |_| {
                                crate::pages::download_cnf(editor.puzzle.get_untracked())
                            })
                        />
                    </SolverPanel>
                </aside>
            </div>
        </section>
    }
}
