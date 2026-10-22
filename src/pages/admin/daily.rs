//! Reviewing a day's daily results: every attempt, its move log, and taking results off the board.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{admin_attempts, admin_flag, admin_log};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::model::{AdminAttempt, Flag, MoveEvent, date_string, format_duration};

use super::{SECTION, error, phone_stat};

/// Clicks on different bridges closer together than this are worth a look.
const SUSPICIOUS_GAP_MS: u32 = 120;

fn flag_name(flag: Flag) -> &'static str {
    match flag {
        Flag::Log => "no valid log",
        Flag::TooFast => "too fast",
        Flag::Pace => "machine pace",
        Flag::SecondAccount => "second account",
        Flag::Review => "taken off",
        Flag::Banned => "banned",
        Flag::Unknown => "unknown",
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
    let summary = move || {
        attempts.with(|l| {
            l.as_ref().filter(|l| !l.is_empty()).map(|l| {
                let ranked = l.iter().filter(|a| a.flag.is_none() && finished(a)).count();
                let check = l.iter().filter(|a| suspicious(a)).count();
                format!(
                    "{} {} · {ranked} ranked · {check} to check",
                    l.len(),
                    if l.len() == 1 { "attempt" } else { "attempts" },
                )
            })
        })
    };
    let rows = move || attempts.get().unwrap_or_default();
    view! {
        <Card class=SECTION>
            <CardHeader class="max-sm:px-0">
                <CardTitle class="text-xl">"Daily review"</CardTitle>
                <CardDescription>
                    "Fastest first. Yellow rows are flagged or have clicks under " {SUSPICIOUS_GAP_MS}
                    " ms apart: open the log and judge. Taking a result off the board hides it from all rankings and streaks; putting it back undoes that."
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3 max-sm:px-0">
                <div class="flex items-center gap-2">
                    <div class="flex h-10 items-center rounded-lg border bg-card max-sm:flex-1">
                        <button type="button" aria-label="Previous day" class="h-full rounded-s-lg px-3 hover:bg-accent" on:click=move |_| step(-1)>"←"</button>
                        <span class="flex-1 px-3 text-center font-pixel text-base tabular-nums">
                            {move || shown.get().map(date_string).unwrap_or_default()}
                        </span>
                        <button type="button" aria-label="Next day" class="h-full rounded-e-lg px-3 hover:bg-accent" on:click=move |_| step(1)>"→"</button>
                    </div>
                    <Button
                        variant=ButtonVariant::Outline
                        class="h-10"
                        attr:disabled=move || day.get().is_none()
                        on:click=move |_| day.set(None)
                    >
                        "Today"
                    </Button>
                </div>
                {move || summary().map(|s| view! { <p class="text-sm text-muted-foreground">{s}</p> })}
                {move || match state.get() {
                    None => view! { <p class="text-muted-foreground">"Loading…"</p> }.into_any(),
                    Some(true) => view! { <p class="text-muted-foreground">"Nobody played this day."</p> }.into_any(),
                    Some(false) => view! {
                        <div class="overflow-x-auto max-md:hidden">
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
                                    <For each=rows key=|a| (a.user_id, a.flag, a.revealed, a.finished_at) let:a>
                                        <AttemptRow a open_log on_toggle=Callback::new(toggle) on_log=Callback::new(show_log) />
                                    </For>
                                </tbody>
                            </table>
                        </div>
                        <ul class="flex flex-col gap-2 md:hidden">
                            <For each=rows key=|a| (a.user_id, a.flag, a.revealed, a.finished_at) let:a>
                                <AttemptCard a open_log on_toggle=Callback::new(toggle) on_log=Callback::new(show_log) />
                            </For>
                        </ul>
                    }
                    .into_any(),
                }}
            </CardContent>
        </Card>
    }
}

/// Finished without giving up: a result that counts, unless taken off.
fn finished(a: &AdminAttempt) -> bool {
    a.finished_at.is_some() && !a.revealed
}

fn suspicious(a: &AdminAttempt) -> bool {
    a.flag.is_some() || a.log.fastest_gap_ms.is_some_and(|g| g < SUSPICIOUS_GAP_MS)
}

fn status_badge(a: &AdminAttempt) -> AnyView {
    match (a.flag, a.revealed, a.finished_at) {
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
    }
}

/// Taking a result off the board, or putting it back. A banned player's results only come back
/// with the lifted ban (the Bans tab).
fn board_action(a: &AdminAttempt) -> Option<(&'static str, ButtonVariant)> {
    if a.flag == Some(Flag::Banned) {
        None
    } else if a.flag.is_some() {
        Some(("Put back", ButtonVariant::Outline))
    } else if finished(a) {
        Some(("Take off", ButtonVariant::Destructive))
    } else {
        None
    }
}

fn gap(ms: Option<u32>) -> String {
    ms.map_or_else(|| "–".into(), |g| format!("{g} ms"))
}

fn account(a: &AdminAttempt) -> String {
    let user = a.user_id;
    a.username
        .as_ref()
        .map_or_else(|| format!("guest #{user}"), |u| format!("@{u} · #{user}"))
}

fn log_open(
    open_log: RwSignal<Option<(i64, Vec<MoveEvent>)>>,
    user: i64,
) -> impl Fn() -> bool + Copy {
    move || open_log.with(|l| l.as_ref().is_some_and(|(u, _)| *u == user))
}

fn open_log_view(
    open_log: RwSignal<Option<(i64, Vec<MoveEvent>)>>,
    user: i64,
) -> impl Fn() -> Option<AnyView> {
    move || {
        open_log.with(|l| {
            l.as_ref()
                .filter(|(u, _)| *u == user)
                .map(|(_, log)| view! { <LogView log=log.clone() /> }.into_any())
        })
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
    let is_open = log_open(open_log, user);
    view! {
        <tr class="border-b last:border-0" class:bg-warning-light=suspicious(&a)>
            <td class="py-2 pe-3">
                <span class="font-medium">{if a.name.is_empty() { "(deleted)".to_string() } else { a.name.clone() }}</span>
                <span class="block text-xs text-muted-foreground">{account(&a)}</span>
            </td>
            <td class="py-2 pe-3 text-end font-pixel tabular-nums">{a.time_ms.map(format_duration).unwrap_or_default()}</td>
            <td class="py-2 pe-3 text-end tabular-nums">{format!("{} · {}", a.hints, a.checks)}</td>
            <td class="py-2 pe-3 text-end tabular-nums" data-tooltip=format!("{} changes in the log, over {}", a.log.events, seconds(a.log.span_ms))>
                {a.log.taps}
            </td>
            <td class="py-2 pe-3 text-end tabular-nums">
                {format!("{} · {}", gap(a.log.fastest_gap_ms), gap(a.log.median_gap_ms))}
            </td>
            <td class="py-2 pe-3">{status_badge(&a)}</td>
            <td class="py-2">
                <div class="flex justify-end gap-1">
                    <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on:click=move |_| on_log.run(user)>
                        {move || if is_open() { "Hide log" } else { "Log" }}
                    </Button>
                    {board_action(&a)
                        .map(|(label, variant)| {
                            view! {
                                <Button variant size=ButtonSize::Sm on:click=move |_| on_toggle.run(user)>{label}</Button>
                            }
                        })}
                </div>
            </td>
        </tr>
        {move || {
            open_log_view(open_log, user)()
                .map(|log| {
                    view! {
                        <tr>
                            <td colspan="7" class="pb-3">{log}</td>
                        </tr>
                    }
                })
        }}
    }
}

/// An attempt on a phone: who and how it stands, the numbers, the click gaps that make it
/// suspicious or not, then the buttons, with the log opening inside the card.
#[component]
fn AttemptCard(
    a: AdminAttempt,
    open_log: RwSignal<Option<(i64, Vec<MoveEvent>)>>,
    on_toggle: Callback<i64>,
    on_log: Callback<i64>,
) -> impl IntoView {
    let user = a.user_id;
    let is_open = log_open(open_log, user);
    let suspicious = suspicious(&a);
    let fastest = if a.log.fastest_gap_ms.is_some_and(|g| g < SUSPICIOUS_GAP_MS) {
        "font-semibold text-warning-dark"
    } else {
        "font-semibold text-foreground"
    };
    let gaps = if a.log.fastest_gap_ms.is_some() {
        view! {
            "Click gaps: fastest "
            <span class=fastest>{gap(a.log.fastest_gap_ms)}</span>
            " · median " {gap(a.log.median_gap_ms)}
        }
        .into_any()
    } else {
        view! { "No click gaps logged" }.into_any()
    };
    view! {
        <li class="rounded-lg border bg-card p-3" class:border-warning=suspicious class:bg-warning-light=suspicious>
            <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                    <p class="truncate font-medium">{if a.name.is_empty() { "(deleted)".to_string() } else { a.name.clone() }}</p>
                    <p class="truncate text-xs text-muted-foreground">{account(&a)}</p>
                </div>
                <div class="shrink-0">{status_badge(&a)}</div>
            </div>
            <dl class="mt-3 grid grid-cols-4 gap-2 border-t pt-3">
                {phone_stat("Time", a.time_ms.map_or_else(|| "–".into(), format_duration), "font-pixel")}
                {phone_stat("Hints", a.hints.to_string(), "")}
                {phone_stat("Checks", a.checks.to_string(), "")}
                {phone_stat("Clicks", a.log.taps.to_string(), "")}
            </dl>
            <p class="mt-2 text-xs text-muted-foreground">{gaps}</p>
            <div class="mt-3 flex gap-2">
                <Button variant=ButtonVariant::Outline class="flex-1" on:click=move |_| on_log.run(user)>
                    {move || if is_open() { "Hide log" } else { "Show log" }}
                </Button>
                {board_action(&a)
                    .map(|(label, variant)| {
                        view! { <Button variant class="flex-1" on:click=move |_| on_toggle.run(user)>{label}</Button> }
                    })}
            </div>
            {move || open_log_view(open_log, user)().map(|log| view! { <div class="mt-3">{log}</div> })}
        </li>
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
        <div dir="ltr" class="terminal max-h-80 overflow-auto whitespace-pre rounded-md px-3 py-2 font-pixel text-[12px] leading-relaxed">
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

#[cfg(test)]
mod tests {
    use crate::model::LogSummary;

    use super::*;

    /// An attempt by guest #7, still running.
    fn attempt() -> AdminAttempt {
        AdminAttempt {
            user_id: 7,
            name: "Guest".into(),
            username: None,
            started_at: 1_000,
            finished_at: None,
            time_ms: None,
            hints: 0,
            checks: 0,
            moves: None,
            revealed: false,
            flag: None,
            log: LogSummary::default(),
        }
    }

    fn action(a: &AdminAttempt) -> Option<&'static str> {
        board_action(a).map(|(label, _)| label)
    }

    /// Only ranked results can be taken off, and only taken off ones put back.
    #[test]
    fn results_on_the_board_can_be_taken_off_and_put_back() {
        let running = attempt();
        assert!(!finished(&running));
        assert_eq!(action(&running), None);

        let ranked = AdminAttempt {
            finished_at: Some(61_000),
            ..attempt()
        };
        assert!(finished(&ranked));
        assert_eq!(action(&ranked), Some("Take off"));

        let given_up = AdminAttempt {
            revealed: true,
            ..ranked.clone()
        };
        assert!(!finished(&given_up));
        assert_eq!(action(&given_up), None);

        let taken_off = AdminAttempt {
            flag: Some(Flag::Review),
            ..ranked.clone()
        };
        assert_eq!(action(&taken_off), Some("Put back"));

        // Only lifting the ban puts a banned player's results back
        let banned = AdminAttempt {
            flag: Some(Flag::Banned),
            ..ranked
        };
        assert_eq!(action(&banned), None);
    }

    #[test]
    fn flagged_and_machine_fast_results_are_suspicious() {
        let with_gap = |gap| AdminAttempt {
            log: LogSummary {
                fastest_gap_ms: gap,
                ..LogSummary::default()
            },
            ..attempt()
        };
        assert!(!suspicious(&with_gap(None)));
        assert!(!suspicious(&with_gap(Some(SUSPICIOUS_GAP_MS))));
        assert!(suspicious(&with_gap(Some(SUSPICIOUS_GAP_MS - 1))));
        let flagged = AdminAttempt {
            flag: Some(Flag::TooFast),
            ..attempt()
        };
        assert!(suspicious(&flagged));
    }

    #[test]
    fn accounts_and_times_read_well() {
        assert_eq!(account(&attempt()), "guest #7");
        let named = AdminAttempt {
            username: Some("noel".into()),
            ..attempt()
        };
        assert_eq!(account(&named), "@noel · #7");
        assert_eq!(gap(None), "–");
        assert_eq!(gap(Some(85)), "85 ms");
        assert_eq!(seconds(1_340), "1.3 s");
        assert_eq!(seconds(61_000), "61.0 s");
    }

    #[test]
    fn every_flag_has_its_own_name() {
        let flags = Flag::ALL;
        let mut names: Vec<&str> = flags.into_iter().map(flag_name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), flags.len());
    }
}
