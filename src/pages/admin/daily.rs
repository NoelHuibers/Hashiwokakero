//! Reviewing a day's daily results: every attempt, its move log, and taking results off the board.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{admin_attempts, admin_flag, admin_log};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::model::{AdminAttempt, Flag, MoveEvent, date_string, format_duration};

use super::error;

/// Clicks on different bridges closer together than this are worth a look.
const SUSPICIOUS_GAP_MS: u32 = 120;

fn flag_name(flag: Flag) -> &'static str {
    match flag {
        Flag::Log => "no valid log",
        Flag::TooFast => "too fast",
        Flag::Pace => "machine pace",
        Flag::SecondAccount => "second account",
        Flag::Review => "taken off",
    }
}

fn seconds(ms: u32) -> String {
    format!("{:.1} s", f64::from(ms) / 1000.0)
}

#[component]
pub(super) fn DailyAdmin(on_change: Callback<()>) -> impl IntoView {
    let day = RwSignal::new(None::<i64>);
    let shown = RwSignal::new(None::<i64>);
    let attempts = RwSignal::new(None::<Vec<AdminAttempt>>);
    let open_log = RwSignal::new(None::<(i64, Vec<MoveEvent>)>);
    let load = move || {
        let wanted = day.get_untracked();
        spawn_local(async move {
            match admin_attempts(wanted).await {
                Ok((d, list)) => {
                    shown.set(Some(d));
                    attempts.set(Some(list));
                }
                Err(e) => error(&e),
            }
        });
    };
    Effect::new(move |_| {
        day.track();
        load();
    });
    let step = move |delta: i64| {
        if let Some(d) = shown.get_untracked() {
            day.set(Some(d + delta));
            open_log.set(None);
        }
    };
    let set_flag = move |user: i64, flag: Option<Flag>| {
        let Some(d) = shown.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match admin_flag(user, d, flag).await {
                Ok(()) => {
                    load();
                    on_change.run(());
                }
                Err(e) => error(&e),
            }
        });
    };
    // Decides at click time from the current data, never from what a button was built with
    let toggle = move |user: i64| {
        let flagged = attempts.with_untracked(|l| {
            l.as_ref()
                .and_then(|l| l.iter().find(|a| a.user_id == user))
                .is_some_and(|a| a.flag.is_some())
        });
        set_flag(user, if flagged { None } else { Some(Flag::Review) });
    };
    // Loading, empty or a table: the table itself stays, its rows are keyed (see the For)
    let state = Memo::new(move |_| attempts.with(|a| a.as_ref().map(Vec::is_empty)));
    let show_log = move |user: i64| {
        if open_log.with_untracked(|l| l.as_ref().is_some_and(|(u, _)| *u == user)) {
            open_log.set(None);
            return;
        }
        let Some(d) = shown.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match admin_log(user, d).await {
                Ok(log) => open_log.set(Some((user, log))),
                Err(e) => error(&e),
            }
        });
    };
    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="flex flex-wrap items-center gap-3 text-xl">
                    "Daily review"
                    <span class="flex items-center gap-1">
                        <Button variant=ButtonVariant::Outline size=ButtonSize::Sm on:click=move |_| step(-1)>"←"</Button>
                        <span class="font-pixel text-base">{move || shown.get().map(date_string).unwrap_or_default()}</span>
                        <Button variant=ButtonVariant::Outline size=ButtonSize::Sm on:click=move |_| step(1)>"→"</Button>
                        <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on:click=move |_| day.set(None)>"Today"</Button>
                    </span>
                </CardTitle>
                <CardDescription>
                    "Every attempt of the day, fastest first. Rows in yellow are flagged or have clicks under "
                    {SUSPICIOUS_GAP_MS} " ms apart: open the log and judge. Taking a result off the board hides it from all rankings and streaks; putting it back undoes that."
                </CardDescription>
            </CardHeader>
            <CardContent class="overflow-x-auto">
                {move || match state.get() {
                    None => view! { <p class="text-muted-foreground">"Loading…"</p> }.into_any(),
                    Some(true) => view! { <p class="text-muted-foreground">"Nobody played this day."</p> }.into_any(),
                    Some(false) => view! {
                        <table class="w-full text-sm">
                            <thead>
                                <tr class="border-b text-start text-xs uppercase tracking-wider text-muted-foreground">
                                    <th class="py-2 pe-3 text-start">"Player"</th>
                                    <th class="py-2 pe-3 text-end">"Time"</th>
                                    <th class="py-2 pe-3 text-end">"Hints · checks"</th>
                                    <th class="py-2 pe-3 text-end">"Clicks"</th>
                                    <th class="py-2 pe-3 text-end">"Fastest · median gap"</th>
                                    <th class="py-2 pe-3 text-start">"Status"</th>
                                    <th class="py-2 text-end">"Actions"</th>
                                </tr>
                            </thead>
                            <tbody>
                                // Keyed by the row's state too: a changed row is built anew, so
                                // its buttons never keep handlers from another state or row
                                <For
                                    each=move || attempts.get().unwrap_or_default()
                                    key=|a| (a.user_id, a.flag, a.revealed, a.finished_at)
                                    let:a
                                >
                                    <AttemptRow
                                        a
                                        open_log
                                        on_toggle=Callback::new(toggle)
                                        on_log=Callback::new(show_log)
                                    />
                                </For>
                            </tbody>
                        </table>
                    }
                    .into_any(),
                }}
            </CardContent>
        </Card>
    }
}

#[component]
fn AttemptRow(
    a: AdminAttempt,
    open_log: RwSignal<Option<(i64, Vec<MoveEvent>)>>,
    on_toggle: Callback<i64>,
    on_log: Callback<i64>,
) -> impl IntoView {
    let user = a.user_id;
    let finished = a.finished_at.is_some() && !a.revealed;
    let suspicious =
        a.flag.is_some() || a.log.fastest_gap_ms.is_some_and(|g| g < SUSPICIOUS_GAP_MS);
    let status = match (a.flag, a.revealed, a.finished_at) {
        (Some(f), _, _) => {
            view! { <Badge variant=BadgeVariant::Warning>{flag_name(f)}</Badge> }.into_any()
        }
        (None, true, _) => {
            view! { <Badge variant=BadgeVariant::Muted>"gave up"</Badge> }.into_any()
        }
        (None, false, Some(_)) => {
            view! { <Badge variant=BadgeVariant::Success>"ranked"</Badge> }.into_any()
        }
        (None, false, None) => {
            view! { <Badge variant=BadgeVariant::Secondary>"playing"</Badge> }.into_any()
        }
    };
    let action = if a.flag.is_some() {
        Some(("Put back", ButtonVariant::Outline))
    } else if finished {
        Some(("Take off", ButtonVariant::Destructive))
    } else {
        None
    };
    view! {
        <tr class="border-b last:border-0" class:bg-warning-light=suspicious>
            <td class="py-2 pe-3">
                <span class="font-medium">{a.name.clone()}</span>
                <span class="block text-xs text-muted-foreground">
                    {a.username.clone().map_or_else(|| format!("guest #{user}"), |u| format!("@{u} · #{user}"))}
                </span>
            </td>
            <td class="py-2 pe-3 text-end font-pixel tabular-nums">{a.time_ms.map(format_duration).unwrap_or_default()}</td>
            <td class="py-2 pe-3 text-end tabular-nums">{format!("{} · {}", a.hints, a.checks)}</td>
            <td class="py-2 pe-3 text-end tabular-nums" title=format!("{} changes in the log, over {}", a.log.events, seconds(a.log.span_ms))>
                {a.log.taps}
            </td>
            <td class="py-2 pe-3 text-end tabular-nums">
                {format!(
                    "{} · {}",
                    a.log.fastest_gap_ms.map_or_else(|| "–".into(), |g| format!("{g} ms")),
                    a.log.median_gap_ms.map_or_else(|| "–".into(), |g| format!("{g} ms")),
                )}
            </td>
            <td class="py-2 pe-3">{status}</td>
            <td class="py-2">
                <div class="flex justify-end gap-1">
                    <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on:click=move |_| on_log.run(user)>"Log"</Button>
                    {action
                        .map(|(label, variant)| {
                            view! {
                                <Button variant size=ButtonSize::Sm on:click=move |_| on_toggle.run(user)>{label}</Button>
                            }
                        })}
                </div>
            </td>
        </tr>
        {move || {
            open_log.with(|l| {
                l.as_ref().filter(|(u, _)| *u == user).map(|(_, log)| {
                    view! {
                        <tr>
                            <td colspan="7" class="pb-3">
                                <LogView log=log.clone() />
                            </td>
                        </tr>
                    }
                })
            })
        }}
    }
}

/// The move log: one row per change, time since the start, the gap since the previous click.
#[component]
fn LogView(log: Vec<MoveEvent>) -> impl IntoView {
    if log.is_empty() {
        return view! { <p class="rounded-md bg-muted px-3 py-2 text-sm">"No move log (played before logs existed, or not sent)."</p> }.into_any();
    }
    let mut last_tap: Option<u32> = None;
    let rows: Vec<_> = log
        .iter()
        .map(|e| {
            let gap = e
                .tap
                .then(|| last_tap.map(|l| e.t.saturating_sub(l)))
                .flatten();
            if e.tap {
                last_tap = Some(e.t);
            }
            (*e, gap)
        })
        .collect();
    view! {
        <div dir="ltr" class="terminal max-h-80 overflow-y-auto rounded-md px-3 py-2 font-pixel text-[12px] leading-relaxed">
            {rows
                .into_iter()
                .map(|(e, gap)| {
                    let fast = gap.is_some_and(|g| g < SUSPICIOUS_GAP_MS);
                    view! {
                        <div class:text-red-400=fast>
                            {format!(
                                "{:>8}  bridge {:>4} → {}  {}{}",
                                seconds(e.t),
                                e.edge,
                                e.count,
                                if e.tap { "click" } else { "hint/undo" },
                                gap.map_or_else(String::new, |g| format!("  +{g} ms")),
                            )}
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
    .into_any()
}
