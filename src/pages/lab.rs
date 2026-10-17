use common::{
    Board, Difficulty, GenerateParams, Heuristic, Puzzle, Restarts, SolveOutcome, SolveReport, SolverConfig,
    SolverKind,
};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{generate_puzzle, solve_puzzle};
use crate::components::board::{BoardMode, BoardView};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::tabs::{Tabs, TabsContent, TabsList, TabsTrigger};
use crate::state::{Editor, Game, ToastKind, random_seed, toast};
use crate::seo::PageMeta;

fn cdcl(heuristic: Heuristic, restarts: Restarts, subsumption: bool) -> SolverConfig {
    SolverConfig {
        kind: SolverKind::Cdcl,
        heuristic,
        restarts,
        subsumption,
        ..Default::default()
    }
}

fn dpll(heuristic: Heuristic) -> SolverConfig {
    SolverConfig {
        kind: SolverKind::Dpll,
        heuristic,
        restarts: Restarts::None,
        ..Default::default()
    }
}

fn presets() -> Vec<SolverConfig> {
    let luby = Restarts::Luby { unit: 32 };
    let mut list = vec![
        cdcl(Heuristic::Vsids, luby, false),
        cdcl(Heuristic::Vsids, Restarts::None, false),
        cdcl(Heuristic::Vsids, Restarts::from_id("geometric").unwrap_or_default(), false),
        cdcl(Heuristic::Vsids, Restarts::from_id("fixed").unwrap_or_default(), false),
        cdcl(Heuristic::Vsids, luby, true),
        cdcl(Heuristic::Arbitrary, luby, false),
    ];
    list.extend(SolverKind::Dpll.heuristics().iter().map(|&h| dpll(h)));
    list.push(SolverConfig::splr());
    list
}

#[derive(Clone, Debug, PartialEq)]
enum RunState {
    Queued,
    Running,
    Done(SolveReport),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PuzzleSource {
    Game,
    Workshop,
    Fresh,
}

fn fmt_ms(ms: f64) -> String {
    if ms < 1000.0 { format!("{ms:.1} ms") } else { format!("{:.2} s", ms / 1000.0) }
}

#[component]
pub fn LabPage() -> impl IntoView {
    let game = expect_context::<Game>();
    let editor = expect_context::<Editor>();

    let presets = presets();
    let default_on = [0usize, 1, 2, 4, 6, 9, 11, 14];
    let selected = RwSignal::new(
        (0..presets.len()).map(|i| default_on.contains(&i)).collect::<Vec<bool>>(),
    );
    let source = RwSignal::new(PuzzleSource::Game);
    let fresh = RwSignal::new(None::<Puzzle>);
    let timeout_ms = RwSignal::new(5_000u32);
    let runs = RwSignal::new(Vec::<(SolverConfig, RunState)>::new());
    let running = RwSignal::new(false);
    let presets = StoredValue::new(presets);

    let puzzle = Memo::new(move |_| match source.get() {
        PuzzleSource::Game => Some(game.board.with(|b| b.puzzle.clone())),
        PuzzleSource::Workshop => Some(editor.puzzle.get()),
        PuzzleSource::Fresh => fresh.get(),
    }
    .filter(|p| !p.islands.is_empty()));
    let preview = Signal::derive(move || Board::new(puzzle.get().unwrap_or_else(|| Puzzle::empty(8, 8))));
    let preview_bridges = Signal::derive(move || preview.with(Board::empty_bridges));

    let generate = move |difficulty: Difficulty| {
        let (width, height) = difficulty.size();
        let params = GenerateParams {
            width,
            height,
            difficulty,
            seed: random_seed(),
        };
        running.set(true);
        spawn_local(async move {
            match generate_puzzle(params).await {
                Ok(g) => fresh.set(Some(g.puzzle)),
                Err(e) => toast(ToastKind::Error, format!("Could not generate: {e}")),
            }
            running.set(false);
        });
    };

    // Use the saved game if there is one, otherwise start with a fresh puzzle
    Effect::new(move |first: Option<()>| {
        if first.is_none() && !game.ensure_restored() {
            source.set(PuzzleSource::Fresh);
            generate(Difficulty::Medium);
        }
    });

    let race = move |_| {
        let Some(puzzle) = puzzle.get_untracked() else {
            toast(ToastKind::Info, "Pick a puzzle first.");
            return;
        };
        let configs: Vec<SolverConfig> = presets.with_value(|p| {
            selected.with_untracked(|s| {
                p.iter()
                    .zip(s)
                    .filter(|(_, on)| **on)
                    .map(|(c, _)| SolverConfig {
                        timeout_ms: timeout_ms.get_untracked(),
                        ..*c
                    })
                    .collect()
            })
        });
        if configs.is_empty() {
            toast(ToastKind::Info, "Select at least one solver configuration.");
            return;
        }
        runs.set(configs.iter().map(|c| (*c, RunState::Queued)).collect());
        running.set(true);
        spawn_local(async move {
            // One after another, so they don't compete for the server's CPU
            for (i, config) in configs.into_iter().enumerate() {
                runs.update(|r| r[i].1 = RunState::Running);
                let state = match solve_puzzle(puzzle.clone(), config, false).await {
                    Ok(report) => RunState::Done(report),
                    Err(e) => RunState::Failed(e.to_string()),
                };
                runs.update(|r| {
                    if let Some(slot) = r.get_mut(i) {
                        slot.1 = state;
                    }
                });
            }
            running.set(false);
        });
    };

    let max_ms = Memo::new(move |_| {
        runs.with(|r| {
            r.iter()
                .filter_map(|(_, s)| match s {
                    RunState::Done(rep) => Some(rep.stats.encode_ms + rep.stats.solve_ms),
                    _ => None,
                })
                .fold(0.0f64, f64::max)
        })
    });
    let fastest = Memo::new(move |_| {
        runs.with(|r| {
            r.iter()
                .enumerate()
                .filter_map(|(i, (_, s))| match s {
                    RunState::Done(rep) if matches!(rep.outcome, SolveOutcome::Solved { .. }) => {
                        Some((i, rep.stats.solve_ms))
                    }
                    _ => None,
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        })
    });

    let source_button = move |s: PuzzleSource, label: &'static str| {
        view! {
            <Button
                variant=Signal::derive(move || {
                    if source.get() == s { ButtonVariant::Default } else { ButtonVariant::Outline }
                })
                size=ButtonSize::Sm
                class="flex-1"
                on:click=move |_| source.set(s)
            >
                {label}
            </Button>
        }
    };

    view! {
        <PageMeta
            title="SAT solver lab · CDCL, DPLL and splr on Hashi puzzles"
            description="Race one Hashiwokakero puzzle through our own CDCL and DPLL SAT solvers with different branching heuristics and restart policies, and compare them with splr."
            path="/lab"
        />
        <section class="mx-auto max-w-7xl px-4 py-6 sm:px-6">
            <div class="mb-5">
                <p class="font-pixel text-sm text-primary">"研究室 · kenkyūshitsu"</p>
                <h1 class="text-3xl font-black">"Solver lab"</h1>
                <p class="mt-1 max-w-3xl text-muted-foreground">
                    "Race one puzzle through our own CDCL and DPLL solvers in different modes (branching heuristics, restart policies, subsumption) and compare them to splr."
                </p>
            </div>

            <div class="grid gap-6 lg:grid-cols-[360px_minmax(0,1fr)]">
                <aside class="flex flex-col gap-4">
                    <Card class="gap-4">
                        <CardHeader>
                            <CardTitle class="text-lg">"1 · Puzzle"</CardTitle>
                            <CardDescription>
                                {move || {
                                    puzzle
                                        .get()
                                        .map(|p| format!("{}×{} with {} islands", p.width, p.height, p.islands.len()))
                                        .unwrap_or_else(|| "No puzzle selected".to_string())
                                }}
                            </CardDescription>
                        </CardHeader>
                        <CardContent class="flex flex-col gap-3">
                            <div class="flex gap-2">
                                {source_button(PuzzleSource::Game, "Current game")}
                                {source_button(PuzzleSource::Workshop, "Workshop")}
                                {source_button(PuzzleSource::Fresh, "Fresh")}
                            </div>
                            <Show when=move || source.get() == PuzzleSource::Fresh>
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
                                                    attr:disabled=move || running.get()
                                                    on:click=move |_| generate(d)
                                                >
                                                    {d.kanji()}
                                                </Button>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </Show>
                            <BoardView
                                board=preview
                                bridges=preview_bridges
                                mode=BoardMode::View
                                max_height="16rem"
                                framed=true
                            />
                        </CardContent>
                    </Card>

                    <Card class="gap-4">
                        <CardHeader>
                            <CardTitle class="text-lg">"2 · Solvers"</CardTitle>
                            <CardDescription>"Pick the configurations to compare."</CardDescription>
                        </CardHeader>
                        <CardContent class="flex flex-col gap-3">
                            <Tabs default_value="cdcl" class="w-full">
                                <TabsList class="w-full">
                                    <TabsTrigger value="cdcl">"CDCL"</TabsTrigger>
                                    <TabsTrigger value="dpll">"DPLL"</TabsTrigger>
                                    <TabsTrigger value="splr">"splr"</TabsTrigger>
                                </TabsList>
                                {[SolverKind::Cdcl, SolverKind::Dpll, SolverKind::Splr]
                                    .into_iter()
                                    .map(|kind| {
                                        view! {
                                            <TabsContent value=kind.id() class="flex flex-col gap-1.5">
                                                {presets
                                                    .with_value(|p| p.clone())
                                                    .into_iter()
                                                    .enumerate()
                                                    .filter(|(_, c)| c.kind == kind)
                                                    .map(|(i, c)| {
                                                        let on = move || selected.with(|s| s[i]);
                                                        view! {
                                                            <label class="flex cursor-pointer items-center gap-3 rounded-md border px-3 py-2 text-sm transition-colors hover:bg-secondary has-[:checked]:border-primary/60 has-[:checked]:bg-accent">
                                                                <input
                                                                    type="checkbox"
                                                                    class="size-4 accent-[var(--primary)]"
                                                                    prop:checked=on
                                                                    on:change=move |_| selected.update(|s| s[i] = !s[i])
                                                                />
                                                                <span>{c.describe()}</span>
                                                            </label>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </TabsContent>
                                        }
                                    })
                                    .collect_view()}
                            </Tabs>
                            <div class="flex items-center gap-2 text-sm">
                                <span class="text-muted-foreground">"Timeout per run"</span>
                                {[2_000u32, 5_000, 10_000]
                                    .into_iter()
                                    .map(|ms| {
                                        view! {
                                            <Button
                                                size=ButtonSize::Sm
                                                variant=Signal::derive(move || {
                                                    if timeout_ms.get() == ms {
                                                        ButtonVariant::Secondary
                                                    } else {
                                                        ButtonVariant::Ghost
                                                    }
                                                })
                                                on:click=move |_| timeout_ms.set(ms)
                                            >
                                                {format!("{} s", ms / 1000)}
                                            </Button>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                            <Button
                                attr:disabled=move || running.get() || puzzle.with(Option::is_none)
                                on:click=race
                            >
                                {move || if running.get() { "Racing…" } else { "Start the race 競争" }}
                            </Button>
                        </CardContent>
                    </Card>
                </aside>

                <Card class="gap-4">
                    <CardHeader>
                        <CardTitle class="flex items-baseline gap-2 text-lg">
                            "3 · Results" <span class="font-pixel text-sm text-primary">"結果"</span>
                        </CardTitle>
                        <CardDescription>
                            "Total time is encoding plus solving. Disconnected solutions are refuted lazily, each refutation costs another SAT call."
                        </CardDescription>
                    </CardHeader>
                    <CardContent>
                        <Show
                            when=move || runs.with(|r| !r.is_empty())
                            fallback=|| {
                                view! {
                                    <div class="seigaiha grid h-64 place-items-center rounded-lg border border-dashed text-muted-foreground">
                                        <p class="rounded-md bg-card px-3 py-1">"No race yet. 待機中"</p>
                                    </div>
                                }
                            }
                        >
                            <div class="overflow-x-auto">
                                <table class="w-full text-sm">
                                    <thead>
                                        <tr class="border-b text-left text-xs uppercase tracking-wider text-muted-foreground">
                                            <th class="py-2 pr-3 font-semibold">"Configuration"</th>
                                            <th class="py-2 pr-3 font-semibold">"Result"</th>
                                            <th class="w-[34%] py-2 pr-3 font-semibold">"Time"</th>
                                            <th class="py-2 pr-3 text-right font-semibold">"Decisions"</th>
                                            <th class="py-2 pr-3 text-right font-semibold">"Restarts"</th>
                                            <th class="py-2 text-right font-semibold">"SAT calls"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {move || {
                                            runs.get()
                                                .into_iter()
                                                .enumerate()
                                                .map(|(i, (config, state))| {
                                                    let is_fastest = fastest.get() == Some(i);
                                                    let result = match &state {
                                                        RunState::Queued => {
                                                            view! { <Badge variant=BadgeVariant::Muted>"queued"</Badge> }
                                                                .into_any()
                                                        }
                                                        RunState::Running => {
                                                            view! {
                                                                <Badge variant=BadgeVariant::Warning class="animate-pulse">
                                                                    "solving…"
                                                                </Badge>
                                                            }
                                                                .into_any()
                                                        }
                                                        RunState::Failed(e) => {
                                                            view! {
                                                                <Badge variant=BadgeVariant::Destructive attr:title=e.clone()>
                                                                    "error"
                                                                </Badge>
                                                            }
                                                                .into_any()
                                                        }
                                                        RunState::Done(r) => {
                                                            match r.outcome {
                                                                SolveOutcome::Solved { .. } => {
                                                                    view! { <Badge variant=BadgeVariant::Success>"SAT"</Badge> }
                                                                        .into_any()
                                                                }
                                                                SolveOutcome::Unsatisfiable => {
                                                                    view! { <Badge variant=BadgeVariant::Info>"UNSAT"</Badge> }
                                                                        .into_any()
                                                                }
                                                                SolveOutcome::Timeout => {
                                                                    view! {
                                                                        <Badge variant=BadgeVariant::Destructive>"timeout"</Badge>
                                                                    }
                                                                        .into_any()
                                                                }
                                                            }
                                                        }
                                                    };
                                                    let (time, bar, decisions, restarts, calls) = match &state {
                                                        RunState::Done(r) => {
                                                            let total = r.stats.encode_ms + r.stats.solve_ms;
                                                            let max = max_ms.get().max(0.001);
                                                            (
                                                                fmt_ms(total),
                                                                (total / max * 100.0).clamp(2.0, 100.0),
                                                                r.stats.decisions.map_or("—".into(), |d| d.to_string()),
                                                                r.stats.restarts.map_or("—".into(), |d| d.to_string()),
                                                                r.stats.sat_calls.to_string(),
                                                            )
                                                        }
                                                        _ => (String::new(), 0.0, String::new(), String::new(), String::new()),
                                                    };
                                                    view! {
                                                        <tr class="border-b last:border-0" class:bg-accent=is_fastest>
                                                            <td class="py-2.5 pr-3">
                                                                <span class="font-medium">{config.describe()}</span>
                                                                {is_fastest
                                                                    .then(|| {
                                                                        view! {
                                                                            <span class="ml-2 font-pixel text-xs text-primary">"最速"</span>
                                                                        }
                                                                    })}
                                                            </td>
                                                            <td class="py-2.5 pr-3">{result}</td>
                                                            <td class="py-2.5 pr-3">
                                                                <div class="flex items-center gap-2">
                                                                    <div class="h-3 flex-1 overflow-hidden rounded-sm bg-muted">
                                                                        <div
                                                                            class="h-full bg-primary transition-all duration-500"
                                                                            style=format!(
                                                                                "width: {bar:.1}%; background-image: repeating-linear-gradient(90deg, transparent 0 6px, rgb(0 0 0 / 0.12) 6px 8px);",
                                                                            )
                                                                        ></div>
                                                                    </div>
                                                                    <span class="w-16 text-right font-pixel tabular-nums">{time}</span>
                                                                </div>
                                                            </td>
                                                            <td class="py-2.5 pr-3 text-right font-pixel tabular-nums">{decisions}</td>
                                                            <td class="py-2.5 pr-3 text-right font-pixel tabular-nums">{restarts}</td>
                                                            <td class="py-2.5 text-right font-pixel tabular-nums">{calls}</td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()
                                        }}
                                    </tbody>
                                </table>
                            </div>
                        </Show>
                    </CardContent>
                </Card>
            </div>
        </section>
    }
}
