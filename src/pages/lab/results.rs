//! The lab's results: one row per solver run, the fastest highlighted.

use common::{SolveOutcome, SolverConfig};
use leptos::prelude::*;

use crate::components::leaderboard::HIGHLIGHT_ROW;
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::config_name;

use super::RunState;

/// On phones the time is the last visible column (the counters are hidden), it ends the pill.
const PHONE_END: &str = "max-sm:[&>td:nth-child(3)]:rounded-e-lg";

fn fmt_ms(ms: f64) -> String {
    if ms < 1000.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}

#[component]
pub(super) fn Results(runs: RwSignal<Vec<(SolverConfig, RunState)>>) -> impl IntoView {
    let i18n = use_i18n();
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

    view! {
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
                                    <th class="py-2 ps-2 pe-3 font-semibold">{t!(i18n, lab.table.config)}</th>
                                    <th class="py-2 pe-3 font-semibold">{t!(i18n, lab.table.result)}</th>
                                    <th class="py-2 pe-2 text-end font-semibold sm:w-[34%] sm:pe-3 sm:text-start">{t!(i18n, common.game.time)}</th>
                                    <th class="hidden py-2 pe-3 text-end font-semibold sm:table-cell">{t!(i18n, lab.table.decisions)}</th>
                                    <th class="hidden py-2 pe-3 text-end font-semibold sm:table-cell">{t!(i18n, lab.table.restarts)}</th>
                                    <th class="hidden py-2 pe-2 text-end font-semibold sm:table-cell">{t!(i18n, lab.table.calls)}</th>
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
                                                        r.stats.decisions.map_or_else(|| "—".into(), |d| d.to_string()),
                                                        r.stats.restarts.map_or_else(|| "—".into(), |d| d.to_string()),
                                                        r.stats.sat_calls.to_string(),
                                                    )
                                                }
                                                _ => (String::new(), 0.0, String::new(), String::new(), String::new()),
                                            };
                                            let summary = if decisions.is_empty() {
                                                String::new()
                                            } else {
                                                let mut parts = vec![t_string!(i18n, lab.summary.decisions, count = decisions.clone())];
                                                if restarts != "—" {
                                                    parts.push(t_string!(i18n, lab.summary.restarts, count = restarts.clone()));
                                                }
                                                parts.push(t_string!(i18n, lab.summary.calls, count = calls.clone()));
                                                parts.join(" · ")
                                            };
                                            view! {
                                                <tr class=format!(
                                                    "border-b last:border-0 {}",
                                                    if is_fastest { format!("{HIGHLIGHT_ROW} {PHONE_END}") } else { String::new() },
                                                )>
                                                    <td class="py-2.5 ps-2 pe-3">
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
                                                    <td class="py-2.5 pe-2 sm:pe-3">
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
                                                    <td class="hidden py-2.5 pe-2 text-end font-pixel tabular-nums sm:table-cell">{calls}</td>
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
    }
}
