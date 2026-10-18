//! The admin dashboard: today at a glance, reviewing daily results and running the Dojo. Only for
//! the usernames in `HASHI_ADMINS`; everyone else sees the usual "not found". English only.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Meta;
use leptos_router::hooks::use_query_map;

use crate::api::{
    admin_attempts, admin_flag, admin_log, admin_submissions, load_admin_overview,
    update_admin_submission,
};
use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::lang::error_text;
use crate::model::{
    AdminAttempt, AdminOverview, AdminSubmission, DojoState, Flag, MoveEvent, SubmissionAction,
    date_string, format_duration,
};
use crate::pages::NotFound;
use crate::state::{Auth, ToastKind, toast};

/// Clicks on different bridges closer together than this are worth a look.
const SUSPICIOUS_GAP_MS: u32 = 120;

fn error(e: &ServerFnError) {
    let e = e.clone();
    toast(ToastKind::Error, move |l| error_text(l, &e));
}

#[component]
pub fn AdminPage() -> impl IntoView {
    let auth = expect_context::<Auth>();
    // Only decided in the browser, once the session is known: the server renders a placeholder
    let mounted = RwSignal::new(false);
    Effect::new(move |_| mounted.set(true));
    let admin = move || mounted.get() && auth.user.with(|u| u.as_ref().is_some_and(|u| u.admin));
    view! {
        <Meta name="robots" content="noindex" />
        <Show
            when=admin
            fallback=move || {
                if mounted.get() && auth.loaded.get() {
                    view! { <NotFound /> }.into_any()
                } else {
                    view! { <p class="py-24 text-center text-muted-foreground">"…"</p> }.into_any()
                }
            }
        >
            <Dashboard />
        </Show>
    }
}

#[component]
fn Dashboard() -> impl IntoView {
    let query = use_query_map();
    let tab = RwSignal::new(
        if query.with_untracked(|q| q.get("tab").as_deref() == Some("dojo")) {
            "dojo"
        } else {
            "daily"
        },
    );
    let overview = RwSignal::new(None::<AdminOverview>);
    let load_overview = move || {
        spawn_local(async move {
            match load_admin_overview().await {
                Ok(o) => overview.set(Some(o)),
                Err(e) => error(&e),
            }
        });
    };
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            load_overview();
        }
    });
    let tab_button = move |id: &'static str, label: &'static str| {
        view! {
            <Button
                variant=Signal::derive(move || {
                    if tab.get() == id { ButtonVariant::Default } else { ButtonVariant::Outline }
                })
                size=ButtonSize::Sm
                on:click=move |_| tab.set(id)
            >
                {label}
            </Button>
        }
    };
    view! {
        <section class="mx-auto flex max-w-7xl flex-col gap-6 px-4 py-6 sm:px-6">
            <div>
                <p class="font-pixel text-sm text-primary">"管理 · kanri"</p>
                <h1 class="text-3xl font-black">"Admin"</h1>
            </div>
            <Overview overview />
            <div class="flex gap-2">
                {tab_button("daily", "Daily review")}
                {tab_button("dojo", "SAT Dojo")}
            </div>
            {move || match tab.get() {
                "dojo" => view! { <DojoAdmin on_change=Callback::new(move |()| load_overview()) /> }.into_any(),
                _ => view! { <DailyAdmin on_change=Callback::new(move |()| load_overview()) /> }.into_any(),
            }}
        </section>
    }
}

#[component]
fn Overview(overview: RwSignal<Option<AdminOverview>>) -> impl IntoView {
    let tile = |label: &'static str, value: Signal<String>| {
        view! {
            <div class="rounded-lg border bg-card px-4 py-3">
                <p class="text-xs font-bold uppercase tracking-wider text-muted-foreground">{label}</p>
                <p class="font-pixel text-2xl tabular-nums">{move || value.get()}</p>
            </div>
        }
    };
    let stat = move |f: fn(&AdminOverview) -> String| {
        Signal::derive(move || overview.with(|o| o.as_ref().map_or_else(|| "…".into(), f)))
    };
    view! {
        <div class="grid grid-cols-2 gap-3 md:grid-cols-4">
            {tile("Players", stat(|o| format!("{} ({} registered)", o.users, o.registered)))}
            {tile("Today's daily", stat(|o| format!("{} started · {} ranked", o.started, o.finished)))}
            {tile("Flagged today", stat(|o| o.flagged.to_string()))}
            {tile("Dojo", stat(|o| format!("{} submissions · {} waiting", o.dojo_total, o.dojo_pending)))}
        </div>
    }
}

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
fn DailyAdmin(on_change: Callback<()>) -> impl IntoView {
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

#[component]
fn DojoAdmin(on_change: Callback<()>) -> impl IntoView {
    let query = use_query_map();
    let list = RwSignal::new(None::<Vec<AdminSubmission>>);
    let load = move || {
        spawn_local(async move {
            match admin_submissions().await {
                Ok(l) => list.set(Some(l)),
                Err(e) => error(&e),
            }
        });
    };
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            load();
        }
    });
    let act = move |id: i64, action: SubmissionAction| {
        if action == SubmissionAction::Delete
            && !window()
                .confirm_with_message(&format!("Delete submission #{id} for good?"))
                .unwrap_or(false)
        {
            return;
        }
        spawn_local(async move {
            match update_admin_submission(id, action).await {
                Ok(()) => {
                    load();
                    on_change.run(());
                }
                Err(e) => error(&e),
            }
        });
    };
    let loaded = Memo::new(move |_| list.with(Option::is_some));
    let message = move || {
        query.with(|q| match (q.get("added"), q.get("error")) {
            (Some(id), _) => Some(format!("House entry #{id} added and queued.")),
            (_, Some(code)) => Some(format!("Couldn't add it: {code}.")),
            _ => None,
        })
    };
    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="text-xl">"SAT Dojo submissions"</CardTitle>
                <CardDescription>
                    "Re-evaluate runs a submission again on the current hidden set. \"Not a copy\" ranks a submission marked as behaving like an earlier one. Delete removes it for good."
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                {move || message().map(|m| view! { <p class="rounded-md bg-accent px-3 py-2 text-sm">{m}</p> })}
                <div class="overflow-x-auto">
                    {move || if loaded.get() {
                        view! {
                            <table class="w-full text-sm">
                                <thead>
                                    <tr class="border-b text-xs uppercase tracking-wider text-muted-foreground">
                                        <th class="py-2 pe-3 text-start">"#"</th>
                                        <th class="py-2 pe-3 text-start">"Solver"</th>
                                        <th class="py-2 pe-3 text-start">"State"</th>
                                        <th class="py-2 pe-3 text-end">"Solved"</th>
                                        <th class="py-2 pe-3 text-end">"PAR-2"</th>
                                        <th class="py-2 pe-3 text-start">"Submitted"</th>
                                        <th class="py-2 text-end">"Actions"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    // Keyed: each row's buttons belong to that submission, also after
                                    // rows above it are deleted
                                    <For
                                        each=move || list.get().unwrap_or_default()
                                        key=|s| (s.id, s.state, s.similar_to, s.solved, s.wrong)
                                        let:s
                                    >
                                        <SubmissionRow s on_action=Callback::new(move |(id, action)| act(id, action)) />
                                    </For>
                                </tbody>
                            </table>
                        }
                        .into_any()
                    } else {
                        view! { <p class="text-muted-foreground">"Loading…"</p> }.into_any()
                    }}
                </div>
                <form action="/admin/house" method="post" enctype="multipart/form-data" class="flex flex-wrap items-end gap-3 border-t pt-4">
                    <label class="flex flex-col gap-1 text-sm font-medium">
                        "Add a house entry"
                        <input name="name" required maxlength="40" placeholder="Hashi CDCL" class="h-9 rounded-md border bg-transparent px-3" />
                    </label>
                    <input name="description" maxlength="600" placeholder="Description" class="h-9 flex-1 rounded-md border bg-transparent px-3" />
                    <input name="wasm" type="file" accept=".wasm" required class="text-sm" />
                    <Button attr:r#type="submit" size=ButtonSize::Sm>"Add"</Button>
                </form>
            </CardContent>
        </Card>
    }
}

#[component]
fn SubmissionRow(
    s: AdminSubmission,
    on_action: Callback<(i64, SubmissionAction)>,
) -> impl IntoView {
    let id = s.id;
    let state = match s.state {
        _ if s.wrong > 0 => {
            view! { <Badge variant=BadgeVariant::Destructive>"disqualified"</Badge> }.into_any()
        }
        DojoState::Done => view! { <Badge variant=BadgeVariant::Success>"done"</Badge> }.into_any(),
        DojoState::Queued => {
            view! { <Badge variant=BadgeVariant::Muted>"queued"</Badge> }.into_any()
        }
        DojoState::Running => {
            view! { <Badge variant=BadgeVariant::Warning>"running"</Badge> }.into_any()
        }
        DojoState::Rejected => {
            view! { <Badge variant=BadgeVariant::Destructive>"rejected"</Badge> }.into_any()
        }
        DojoState::Failed => {
            view! { <Badge variant=BadgeVariant::Destructive>"failed"</Badge> }.into_any()
        }
    };
    let copy = s.similar_to;
    view! {
        <tr class="border-b last:border-0">
            <td class="py-2 pe-3 font-pixel">{id}</td>
            <td class="py-2 pe-3">
                <a href=format!("/dojo/{id}") class="font-medium hover:underline">{s.name.clone()}</a>
                {s.house.then(|| view! { <Badge variant=BadgeVariant::Secondary size=BadgeSize::Sm class="ms-2">"house"</Badge> })}
                <span class="block text-xs text-muted-foreground">{s.author.clone()}</span>
            </td>
            <td class="py-2 pe-3">
                {state}
                {copy.map(|c| view! { <span class="ms-2 text-xs text-warning-dark">{format!("copy of #{c}")}</span> })}
            </td>
            <td class="py-2 pe-3 text-end tabular-nums">{s.solved.map(|n| format!("{n}/{}", s.total)).unwrap_or_default()}</td>
            <td class="py-2 pe-3 text-end font-pixel tabular-nums">{s.par2.map(|p| format!("{p:.3}")).unwrap_or_default()}</td>
            <td class="py-2 pe-3 text-xs">{date_string(crate::model::day_of(s.created_at))}" · season "{s.season}</td>
            <td class="py-2">
                <div class="flex justify-end gap-1">
                    <Button variant=ButtonVariant::Outline size=ButtonSize::Sm on:click=move |_| on_action.run((id, SubmissionAction::Requeue))>"Re-evaluate"</Button>
                    {copy.map(|_| view! { <Button variant=ButtonVariant::Outline size=ButtonSize::Sm on:click=move |_| on_action.run((id, SubmissionAction::ClearCopy))>"Not a copy"</Button> })}
                    <Button variant=ButtonVariant::Destructive size=ButtonSize::Sm on:click=move |_| on_action.run((id, SubmissionAction::Delete))>"Delete"</Button>
                </div>
            </td>
        </tr>
    }
}
