use std::fmt::Write as _;

use common::{Heuristic, Restarts, SolveOutcome, SolveReport, SolverConfig, SolverKind};
use leptos::prelude::*;

use super::ui::badge::{Badge, BadgeVariant};
use super::ui::button::{Button, ButtonSize, ButtonVariant};
use super::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use super::ui::label::Label;
use super::ui::select::SelectNative;
use super::ui::switch::Switch;
use super::ui::toggle_group::{ToggleGroup, ToggleGroupItem, ToggleGroupVariant};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{config_name, heuristic_name, restarts_name, solver_name};
use crate::state::Settings;

const TIMEOUTS: [(u32, &str); 4] = [
    (2_000, "2 s"),
    (5_000, "5 s"),
    (10_000, "10 s"),
    (20_000, "20 s"),
];

/// Solver, heuristic, restart policy and preprocessing options.
#[component]
pub fn SolverOptions() -> impl IntoView {
    let i18n = use_i18n();
    let settings = expect_context::<Settings>();
    let config = settings.solver;
    let kind = Memo::new(move |_| config.get().kind);
    let set = move |f: &dyn Fn(&mut SolverConfig)| {
        config.update(|c| {
            f(c);
            *c = c.normalized();
        });
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
                <Label>{t!(i18n, common.solver.solver)}</Label>
                <ToggleGroup variant=ToggleGroupVariant::Outline spacing=0 class="w-full">
                    {SolverKind::ALL
                        .into_iter()
                        .map(|k| {
                            view! {
                                <ToggleGroupItem
                                    title=Signal::derive(move || solver_name(i18n.get_locale(), k))
                                    pressed=Signal::derive(move || kind.get() == k)
                                    class="font-semibold data-[state=on]:bg-primary data-[state=on]:text-primary-foreground"
                                    on:click=move |_| set(&|c| c.kind = k)
                                >
                                    {move || solver_name(i18n.get_locale(), k)}
                                </ToggleGroupItem>
                            }
                        })
                        .collect_view()}
                </ToggleGroup>
            </div>

            <div class="grid grid-cols-2 gap-3">
                <div class="flex flex-col gap-2">
                    <Label html_for="heuristic">{t!(i18n, common.solver.heuristic)}</Label>
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
                                view! { <option value="vsids">{t!(i18n, common.solver.splr_heuristic)}</option> }.into_any()
                            } else {
                                hs.iter()
                                    .map(|&h| {
                                        view! {
                                            <option value=h.id()>{move || heuristic_name(i18n.get_locale(), h)}</option>
                                        }
                                    })
                                    .collect_view()
                                    .into_any()
                            }
                        }}
                    </SelectNative>
                </div>
                <div class="flex flex-col gap-2">
                    <Label html_for="restarts">{t!(i18n, common.solver.restarts)}</Label>
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
                                let restarts = Restarts::from_id(id).unwrap_or_default();
                                view! { <option value=id>{move || restarts_name(i18n.get_locale(), restarts)}</option> }
                            })
                            .collect_view()}
                    </SelectNative>
                </div>
            </div>

            <div class="grid grid-cols-2 gap-3">
                <div class="flex flex-col gap-2">
                    <Label html_for="subsumption">{t!(i18n, common.solver.subsumption)}</Label>
                    <div class="flex h-9 items-center gap-2">
                        <Switch
                            id="subsumption"
                            checked=subsumption
                            aria_label=Signal::derive(move || t_string!(i18n, common.solver.subsumption_hint).to_string())
                            disabled=Signal::derive(move || !kind.get().supports_subsumption())
                        />
                        <span class="text-xs text-muted-foreground">{t!(i18n, common.solver.preprocess)}</span>
                    </div>
                </div>
                <div class="flex flex-col gap-2">
                    <Label html_for="timeout">{t!(i18n, common.solver.timeout)}</Label>
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

fn fmt_int(n: impl std::fmt::Display) -> String {
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
        let _ = write!(cmd, " -H {}", config.heuristic.id());
    }
    if config.kind.supports_restarts() {
        let _ = write!(cmd, " --restarts {}", config.restarts.id());
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
    let i18n = use_i18n();
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
            dir="ltr"
            class="terminal font-pixel rounded-lg px-4 py-3 text-[13px] leading-relaxed"
            class:busy=move || busy.get()
            aria-live="polite"
        >
            <div class="mb-1 break-words text-[#f2b830]">"$ " {move || settings.solver.with(command_line)}</div>
            {move || {
                if busy.get() {
                    return view! {
                        <div class="text-[#8fd0ea]">
                            "c " {t!(i18n, common.solver.log.working)} <span class="animate-pulse">"▌"</span>
                        </div>
                    }
                        .into_any();
                }
                let Some(r) = report.get() else {
                    return view! {
                        <div class="text-[#a89f8c]">"c " {t!(i18n, common.solver.log.waiting)}</div>
                    }
                        .into_any();
                };
                let s = r.stats;
                let l = i18n.get_locale();
                // The status words are the solver's DIMACS output (`s SATISFIABLE`), the notes ours
                let (status, color) = match &r.outcome {
                    SolveOutcome::Solved { unique: Some(true), .. } => {
                        (format!("SATISFIABLE · {} ✓", td_string!(l, common.solver.log.unique)), "text-[#a3d672]")
                    }
                    SolveOutcome::Solved { unique: Some(false), .. } => {
                        (format!("SATISFIABLE · {}", td_string!(l, common.solver.log.not_unique)), "text-[#f2b830]")
                    }
                    SolveOutcome::Solved { .. } => ("SATISFIABLE".to_string(), "text-[#a3d672]"),
                    SolveOutcome::Unsatisfiable => ("UNSATISFIABLE".to_string(), "text-[#f2715a]"),
                    SolveOutcome::Timeout => {
                        (format!("UNKNOWN · {}", td_string!(l, common.solver.log.timeout)), "text-[#f2715a]")
                    }
                };
                view! {
                    <div>
                        {row(td_string!(l, common.solver.log.config), config_name(l, &s.config))}
                        {row(td_string!(l, common.solver.log.variables), fmt_int(s.variables))}
                        {row(td_string!(l, common.solver.log.clauses), fmt_int(s.clauses))}
                        {row(td_string!(l, common.solver.log.encode), fmt_ms(s.encode_ms))}
                        {row(td_string!(l, common.solver.log.solve), fmt_ms(s.solve_ms))}
                        {s.decisions.map(|d| row(td_string!(l, common.solver.log.decisions), fmt_int(d)))}
                        {s.restarts.map(|d| row(td_string!(l, common.solver.log.restarts), fmt_int(d)))}
                        {row(td_string!(l, common.solver.log.sat_calls), s.sat_calls.to_string())}
                        {row(td_string!(l, common.solver.log.cuts), s.connectivity_cuts.to_string())}
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
    let i18n = use_i18n();
    let settings = expect_context::<Settings>();
    view! {
        <Card class="gap-4 overflow-hidden">
            <CardHeader>
                <div class="flex w-full items-start justify-between gap-2">
                    <div class="flex flex-col gap-1.5">
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            {t!(i18n, common.solver.title)} <span class="font-pixel text-sm text-primary">"充足可能性"</span>
                        </CardTitle>
                        <CardDescription>
                            {t!(i18n, common.solver.description)}
                        </CardDescription>
                    </div>
                    <Badge variant=BadgeVariant::Outline class="shrink-0 font-pixel">
                        {move || solver_name(i18n.get_locale(), settings.solver.get().kind)}
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
    children: Children,
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
            {children()}
        </Button>
    }
}

#[cfg(test)]
mod tests {
    use common::{Heuristic, Restarts, SolverKind};

    use super::*;

    #[test]
    fn times_get_a_unit_that_fits() {
        assert_eq!(fmt_ms(0.25), "250 µs");
        assert_eq!(fmt_ms(1.0), "1.0 ms");
        assert_eq!(fmt_ms(742.06), "742.1 ms");
        assert_eq!(fmt_ms(1000.0), "1.00 s");
        assert_eq!(fmt_ms(12_346.0), "12.35 s");
    }

    #[test]
    fn large_counts_are_grouped_by_thousands() {
        assert_eq!(fmt_int(0_u64), "0");
        assert_eq!(fmt_int(999_usize), "999");
        assert_eq!(fmt_int(1000_usize), "1,000");
        assert_eq!(fmt_int(123_456_u64), "123,456");
        assert_eq!(fmt_int(1_234_567_u64), "1,234,567");
    }

    /// The terminal shows the flags of the options the chosen solver has, no others.
    #[test]
    fn the_command_line_names_only_the_options_the_solver_has() {
        let cdcl = SolverConfig {
            kind: SolverKind::Cdcl,
            heuristic: Heuristic::Vsids,
            restarts: Restarts::Luby { unit: 32 },
            subsumption: true,
            timeout_ms: 1000,
        };
        assert_eq!(
            command_line(&cdcl),
            "hashi solve --solver cdcl -H vsids --restarts luby --subsumption"
        );
        let dpll = SolverConfig {
            kind: SolverKind::Dpll,
            heuristic: Heuristic::Mom,
            ..cdcl
        }
        .normalized();
        assert_eq!(command_line(&dpll), "hashi solve --solver dpll -H mom");
        assert_eq!(
            command_line(&SolverConfig::splr()),
            "hashi solve --solver splr"
        );
    }
}
