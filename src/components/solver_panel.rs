use common::{Heuristic, Restarts, SolveOutcome, SolveReport, SolverConfig, SolverKind};
use leptos::prelude::*;

use super::ui::badge::{Badge, BadgeVariant};
use super::ui::button::{Button, ButtonSize, ButtonVariant};
use super::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use super::ui::label::Label;
use super::ui::select::SelectNative;
use super::ui::switch::Switch;
use super::ui::toggle_group::{ToggleGroup, ToggleGroupItem, ToggleGroupVariant};
use crate::state::Settings;

const TIMEOUTS: [(u32, &str); 4] = [(2_000, "2 s"), (5_000, "5 s"), (10_000, "10 s"), (20_000, "20 s")];

/// Solver, heuristic, restart policy and preprocessing options.
#[component]
pub fn SolverOptions() -> impl IntoView {
    let settings = expect_context::<Settings>();
    let config = settings.solver;
    let kind = Memo::new(move |_| config.get().kind);
    let set = move |f: &dyn Fn(&mut SolverConfig)| {
        config.update(|c| {
            f(c);
            *c = c.normalized();
        })
    };

    let subsumption = RwSignal::new(config.get_untracked().subsumption);
    Effect::new(move |_| {
        let on = subsumption.get();
        if config.with_untracked(|c| c.subsumption != on) {
            config.update(|c| c.subsumption = on);
        }
    });
    Effect::new(move |_| subsumption.set(config.with(|c| c.subsumption)));

    view! {
        <div class="flex flex-col gap-4">
            <div class="flex flex-col gap-2">
                <Label>"Solver"</Label>
                <ToggleGroup variant=ToggleGroupVariant::Outline spacing=0 class="w-full">
                    {SolverKind::ALL
                        .into_iter()
                        .map(|k| {
                            view! {
                                <ToggleGroupItem
                                    title=k.label()
                                    pressed=Signal::derive(move || kind.get() == k)
                                    class="font-semibold data-[state=on]:bg-primary data-[state=on]:text-primary-foreground"
                                    on:click=move |_| set(&|c| c.kind = k)
                                >
                                    {k.label()}
                                </ToggleGroupItem>
                            }
                        })
                        .collect_view()}
                </ToggleGroup>
            </div>

            <div class="grid grid-cols-2 gap-3">
                <div class="flex flex-col gap-2">
                    <Label html_for="heuristic">"Branching heuristic"</Label>
                    <SelectNative
                        id="heuristic"
                        value=Signal::derive(move || config.get().heuristic.id().to_string())
                        disabled=Signal::derive(move || kind.get().heuristics().is_empty())
                        on_change=Callback::new(move |v: String| {
                            if let Some(h) = Heuristic::from_id(&v) {
                                set(&|c| c.heuristic = h);
                            }
                        })
                    >
                        {move || {
                            let hs = kind.get().heuristics();
                            if hs.is_empty() {
                                view! { <option value="vsids">"splr's own (EVSIDS)"</option> }.into_any()
                            } else {
                                hs.iter()
                                    .map(|h| view! { <option value=h.id()>{h.label()}</option> })
                                    .collect_view()
                                    .into_any()
                            }
                        }}
                    </SelectNative>
                </div>
                <div class="flex flex-col gap-2">
                    <Label html_for="restarts">"Restarts"</Label>
                    <SelectNative
                        id="restarts"
                        value=Signal::derive(move || config.get().restarts.id().to_string())
                        disabled=Signal::derive(move || !kind.get().supports_restarts())
                        on_change=Callback::new(move |v: String| {
                            if let Some(r) = Restarts::from_id(&v) {
                                set(&|c| c.restarts = r);
                            }
                        })
                    >
                        {["none", "fixed", "geometric", "luby"]
                            .into_iter()
                            .map(|id| {
                                let label = Restarts::from_id(id).map(|r| r.label()).unwrap_or(id);
                                view! { <option value=id>{label}</option> }
                            })
                            .collect_view()}
                    </SelectNative>
                </div>
            </div>

            <div class="grid grid-cols-2 gap-3">
                <div class="flex flex-col gap-2">
                    <Label html_for="subsumption">"Subsumption"</Label>
                    <div class="flex h-9 items-center gap-2">
                        <Switch
                            id="subsumption"
                            checked=subsumption
                            aria_label="Eliminate subsumed clauses before solving"
                            disabled=Signal::derive(move || !kind.get().supports_subsumption())
                        />
                        <span class="text-xs text-muted-foreground">"preprocess"</span>
                    </div>
                </div>
                <div class="flex flex-col gap-2">
                    <Label html_for="timeout">"Timeout"</Label>
                    <SelectNative
                        id="timeout"
                        value=Signal::derive(move || config.get().timeout_ms.to_string())
                        on_change=Callback::new(move |v: String| {
                            if let Ok(ms) = v.parse::<u32>() {
                                set(&|c| c.timeout_ms = ms);
                            }
                        })
                    >
                        {TIMEOUTS
                            .into_iter()
                            .map(|(ms, label)| view! { <option value=ms.to_string()>{label}</option> })
                            .collect_view()}
                    </SelectNative>
                </div>
            </div>
        </div>
    }
}

fn fmt_ms(ms: f64) -> String {
    if ms < 1.0 {
        format!("{:.0} µs", ms * 1000.0)
    } else if ms < 1000.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}

fn fmt_int(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `solve --solver cdcl ...` line for the terminal, mirroring the CLI flags.
fn command_line(config: &SolverConfig) -> String {
    let mut cmd = format!("hashi solve --solver {}", config.kind.id());
    if !config.kind.heuristics().is_empty() {
        cmd.push_str(&format!(" -H {}", config.heuristic.id()));
    }
    if config.kind.supports_restarts() {
        cmd.push_str(&format!(" --restarts {}", config.restarts.id()));
    }
    if config.subsumption {
        cmd.push_str(" --subsumption");
    }
    cmd
}

/// Solver output styled like a terminal session.
#[component]
pub fn SolverTerminal(
    #[prop(into)] report: Signal<Option<SolveReport>>,
    #[prop(into)] busy: Signal<bool>,
) -> impl IntoView {
    let settings = expect_context::<Settings>();
    let row = |key: &'static str, value: String| {
        view! {
            <div class="flex justify-between gap-4">
                <span class="text-[#a89f8c]">"c " {key}</span>
                <span class="tabular-nums">{value}</span>
            </div>
        }
    };
    view! {
        <div
            class="terminal font-pixel rounded-lg px-4 py-3 text-[13px] leading-relaxed"
            class:busy=move || busy.get()
            aria-live="polite"
        >
            <div class="mb-1 break-words text-[#f2b830]">"$ " {move || settings.solver.with(command_line)}</div>
            {move || {
                if busy.get() {
                    return view! {
                        <div class="text-[#8fd0ea]">
                            "c encoding & solving" <span class="animate-pulse">"▌"</span>
                        </div>
                    }
                        .into_any();
                }
                let Some(r) = report.get() else {
                    return view! {
                        <div class="text-[#a89f8c]">"c waiting for a puzzle to solve…"</div>
                    }
                        .into_any();
                };
                let s = r.stats;
                let (status, color) = match &r.outcome {
                    SolveOutcome::Solved { unique: Some(true), .. } => {
                        ("SATISFIABLE · unique ✓".to_string(), "text-[#a3d672]")
                    }
                    SolveOutcome::Solved { unique: Some(false), .. } => {
                        ("SATISFIABLE · not unique".to_string(), "text-[#f2b830]")
                    }
                    SolveOutcome::Solved { .. } => ("SATISFIABLE".to_string(), "text-[#a3d672]"),
                    SolveOutcome::Unsatisfiable => ("UNSATISFIABLE".to_string(), "text-[#f2715a]"),
                    SolveOutcome::Timeout => ("UNKNOWN · timeout".to_string(), "text-[#f2715a]"),
                };
                view! {
                    <div>
                        {row("config", s.config.clone())}
                        {row("variables", fmt_int(s.variables))}
                        {row("clauses", fmt_int(s.clauses))}
                        {row("encode", fmt_ms(s.encode_ms))}
                        {row("solve", fmt_ms(s.solve_ms))}
                        {s.decisions.map(|d| row("decisions", fmt_int(d as usize)))}
                        {s.restarts.map(|d| row("restarts", fmt_int(d as usize)))}
                        {row(
                            "sat calls",
                            format!(
                                "{} ({} cut{})",
                                s.sat_calls,
                                s.connectivity_cuts,
                                if s.connectivity_cuts == 1 { "" } else { "s" },
                            ),
                        )}
                        <div class=format!("mt-1 {color}")>"s " {status}</div>
                    </div>
                }
                    .into_any()
            }}
        </div>
    }
}

/// The solver card: options, actions and output.
#[component]
pub fn SolverPanel(
    #[prop(into)] report: Signal<Option<SolveReport>>,
    #[prop(into)] busy: Signal<bool>,
    children: Children,
) -> impl IntoView {
    let settings = expect_context::<Settings>();
    view! {
        <Card class="gap-4 overflow-hidden">
            <CardHeader>
                <div class="flex w-full items-start justify-between gap-2">
                    <div class="flex flex-col gap-1.5">
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            "SAT Solver" <span class="font-pixel text-sm text-primary">"充足可能性"</span>
                        </CardTitle>
                        <CardDescription>
                            "The puzzle is encoded as CNF and solved by our own solvers from the SAT solving lab."
                        </CardDescription>
                    </div>
                    <Badge variant=BadgeVariant::Outline class="shrink-0 font-pixel">
                        {move || settings.solver.get().kind.label()}
                    </Badge>
                </div>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                <SolverOptions />
                <div class="grid grid-cols-3 gap-2">{children()}</div>
                <SolverTerminal report=report busy=busy />
            </CardContent>
        </Card>
    }
}

/// A small helper for the action buttons inside the solver panel.
#[component]
pub fn PanelButton(
    #[prop(into)] label: String,
    #[prop(optional)] primary: bool,
    #[prop(into, optional)] disabled: Signal<bool>,
    #[prop(into)] on_click: Callback<()>,
) -> impl IntoView {
    view! {
        <Button
            variant=if primary { ButtonVariant::Default } else { ButtonVariant::Outline }
            size=ButtonSize::Sm
            class="w-full"
            attr:disabled=move || disabled.get()
            on:click=move |_| on_click.run(())
        >
            {label}
        </Button>
    }
}
