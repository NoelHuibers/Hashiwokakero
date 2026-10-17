use common::{
    Board, Difficulty, GenerateParams, Heuristic, Puzzle, Restarts, SolveOutcome, SolveReport, SolverConfig,
    SolverKind,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::{generate_puzzle, solve_puzzle};
use crate::components::board::{BoardMode, BoardView};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::tabs::{Tabs, TabsContent, TabsList, TabsTrigger};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{config_name, difficulty_name, error_text};
use crate::seo::PageMeta;
use crate::state::{Editor, Game, ToastKind, random_seed, toast};

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
    let i18n = use_i18n();
    let dojo = crate::components::layout::use_localized("/dojo");
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
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
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
            toast(ToastKind::Info, |l| td_string!(l, lab.toast.pick_puzzle).to_string());
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
            toast(ToastKind::Info, |l| td_string!(l, lab.toast.pick_solver).to_string());
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
                    Err(e) => RunState::Failed(error_text(i18n.get_locale_untracked(), &e)),
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

    let source_button = move |s: PuzzleSource, label: AnyView| {
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
            title=Signal::derive(move || t_string!(i18n, lab.meta.title).to_string())
            description=Signal::derive(move || t_string!(i18n, lab.meta.description).to_string())
            path="/lab"
        />
        <section class="mx-auto max-w-7xl px-4 py-6 sm:px-6">
            <div class="mb-5">
                <p class="font-pixel text-sm text-primary">"研究室 · kenkyūshitsu"</p>
                <h1 class="text-3xl font-black">{t!(i18n, lab.title)}</h1>
                <p class="mt-1 max-w-3xl text-muted-foreground">
                    {t!(i18n, lab.intro)}
                </p>
            </div>

            <A
                href=dojo
                attr:class="mb-5 flex flex-wrap items-center gap-3 rounded-xl border border-primary/40 bg-accent px-4 py-3 transition-colors hover:bg-accent/70"
            >
                <span class="hanko grid size-10 shrink-0 rotate-[-6deg] place-items-center rounded-md font-pixel text-xl">"道"</span>
                <span class="min-w-0 flex-1">
                    <span class="block font-bold">{t!(i18n, dojo.lab_banner.title)}</span>
                    <span class="block text-sm text-muted-foreground">{t!(i18n, dojo.lab_banner.text)}</span>
                </span>
                <span class="text-sm font-semibold text-primary">{t!(i18n, dojo.lab_banner.button)} " →"</span>
            </A>
            <div class="grid grid-cols-1 gap-6 lg:grid-cols-[360px_minmax(0,1fr)]">
                <aside class="flex flex-col gap-4">
                    <Card class="gap-4">
                        <CardHeader>
                            <CardTitle class="text-lg">{t!(i18n, lab.puzzle.title)}</CardTitle>
                            <CardDescription>
                                {move || {
                                    puzzle
                                        .get()
                                        .map(|p| {
                                            t_string!(
                                                i18n, lab.puzzle.size, width = p.width, height = p.height, count = p.islands.len()
                                            )
                                                .to_string()
                                        })
                                        .unwrap_or_else(|| t_string!(i18n, lab.puzzle.none).to_string())
                                }}
                            </CardDescription>
                        </CardHeader>
                        <CardContent class="flex flex-col gap-3">
                            <div class="flex gap-2">
                                {source_button(PuzzleSource::Game, t!(i18n, lab.puzzle.game).into_any())}
                                {source_button(PuzzleSource::Workshop, t!(i18n, lab.puzzle.workshop).into_any())}
                                {source_button(PuzzleSource::Fresh, t!(i18n, lab.puzzle.fresh).into_any())}
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
                                                    attr:title=move || difficulty_name(i18n.get_locale(), d)
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
                            <CardTitle class="text-lg">{t!(i18n, lab.solvers.title)}</CardTitle>
                            <CardDescription>{t!(i18n, lab.solvers.description)}</CardDescription>
                        </CardHeader>
                        <CardContent class="flex flex-col gap-3">
                            <Tabs default_value="cdcl" class="w-full">
                                <TabsList class="w-full group-data-[orientation=Horizontal]/tabs:h-10 md:group-data-[orientation=Horizontal]/tabs:h-8">
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
                                                                <span>{move || config_name(i18n.get_locale(), &c)}</span>
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
                                <span class="text-muted-foreground">{t!(i18n, lab.solvers.timeout)}</span>
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
                                {move || {
                                    if running.get() {
                                        t_string!(i18n, lab.racing).to_string()
                                    } else {
                                        format!("{} 競争", t_string!(i18n, lab.start))
                                    }
                                }}
                            </Button>
                        </CardContent>
                    </Card>
                </aside>

                <Card class="gap-4">
                    <CardHeader>
                        <CardTitle class="flex items-baseline gap-2 text-lg">
                            {t!(i18n, lab.results.title)} " " <span class="font-pixel text-sm text-primary">"結果"</span>
                        </CardTitle>
                        <CardDescription>
                            {t!(i18n, lab.results.description)}
                        </CardDescription>
                    </CardHeader>
                    <CardContent>
                        <Show
                            when=move || runs.with(|r| !r.is_empty())
                            fallback=move || {
                                view! {
                                    <div class="seigaiha grid h-64 place-items-center rounded-lg border border-dashed text-muted-foreground">
                                        <p class="rounded-md bg-card px-3 py-1">{t!(i18n, lab.results.empty)} " 待機中"</p>
                                    </div>
                                }
                            }
                        >
                            <div class="overflow-x-auto">
                                <table class="w-full text-sm">
                                    <thead>
                                        <tr class="border-b text-start text-xs uppercase tracking-wider text-muted-foreground">
                                            <th class="py-2 pe-3 font-semibold">{t!(i18n, lab.table.config)}</th>
                                            <th class="py-2 pe-3 font-semibold">{t!(i18n, lab.table.result)}</th>
                                            <th class="py-2 text-end font-semibold sm:w-[34%] sm:pe-3 sm:text-start">{t!(i18n, common.game.time)}</th>
                                            <th class="hidden py-2 pe-3 text-end font-semibold sm:table-cell">{t!(i18n, lab.table.decisions)}</th>
                                            <th class="hidden py-2 pe-3 text-end font-semibold sm:table-cell">{t!(i18n, lab.table.restarts)}</th>
                                            <th class="hidden py-2 text-end font-semibold sm:table-cell">{t!(i18n, lab.table.calls)}</th>
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
                                                            view! { <Badge variant=BadgeVariant::Muted>{t!(i18n, lab.badge.queued)}</Badge> }
                                                                .into_any()
                                                        }
                                                        RunState::Running => {
                                                            view! {
                                                                <Badge variant=BadgeVariant::Warning class="animate-pulse">
                                                                    {t!(i18n, lab.badge.solving)}
                                                                </Badge>
                                                            }
                                                                .into_any()
                                                        }
                                                        RunState::Failed(e) => {
                                                            view! {
                                                                <Badge variant=BadgeVariant::Destructive attr:title=e.clone()>
                                                                    {t!(i18n, lab.badge.error)}
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
                                                                        <Badge variant=BadgeVariant::Destructive>{t!(i18n, lab.badge.timeout)}</Badge>
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
                                                    let summary = if decisions.is_empty() {
                                                        String::new()
                                                    } else {
                                                        let mut parts = vec![t_string!(i18n, lab.summary.decisions, count = decisions.clone()).to_string()];
                                                        if restarts != "—" {
                                                            parts.push(t_string!(i18n, lab.summary.restarts, count = restarts.clone()).to_string());
                                                        }
                                                        parts.push(t_string!(i18n, lab.summary.calls, count = calls.clone()).to_string());
                                                        parts.join(" · ")
                                                    };
                                                    view! {
                                                        <tr class="border-b last:border-0" class:bg-accent=is_fastest>
                                                            <td class="py-2.5 pe-3">
                                                                <span class="font-medium">{config_name(i18n.get_locale(), &config)}</span>
                                                                {is_fastest
                                                                    .then(|| {
                                                                        view! {
                                                                            <span class="ms-2 font-pixel text-xs text-primary">"最速"</span>
                                                                        }
                                                                    })}
                                                                // Phones: the counters as a line instead of three columns
                                                                {(!summary.is_empty())
                                                                    .then(|| {
                                                                        view! {
                                                                            <span class="mt-0.5 block text-xs text-muted-foreground sm:hidden">
                                                                                {summary}
                                                                            </span>
                                                                        }
                                                                    })}
                                                            </td>
                                                            <td class="py-2.5 pe-3">{result}</td>
                                                            <td class="py-2.5 sm:pe-3">
                                                                <div class="flex items-center justify-end gap-2 sm:justify-start">
                                                                    <div class="hidden h-3 flex-1 overflow-hidden rounded-sm bg-muted sm:block">
                                                                        <div
                                                                            class="h-full bg-primary transition-all duration-500"
                                                                            style=format!(
                                                                                "width: {bar:.1}%; background-image: repeating-linear-gradient(90deg, transparent 0 6px, rgb(0 0 0 / 0.12) 6px 8px);",
                                                                            )
                                                                        ></div>
                                                                    </div>
                                                                    <span class="whitespace-nowrap text-end font-pixel tabular-nums sm:w-16">{time}</span>
                                                                </div>
                                                            </td>
                                                            <td class="hidden py-2.5 pe-3 text-end font-pixel tabular-nums sm:table-cell">{decisions}</td>
                                                            <td class="hidden py-2.5 pe-3 text-end font-pixel tabular-nums sm:table-cell">{restarts}</td>
                                                            <td class="hidden py-2.5 text-end font-pixel tabular-nums sm:table-cell">{calls}</td>
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
