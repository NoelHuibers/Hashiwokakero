//! Running the SAT Dojo: re-evaluating, clearing copies, deleting and adding house entries.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;

use crate::api::{admin_submissions, update_admin_submission};
use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::model::{AdminSubmission, DojoState, SubmissionAction, date_string};

use super::{SECTION, error, phone_stat};

#[component]
pub(super) fn DojoAdmin(on_change: Callback<()>) -> impl IntoView {
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
    let rows = move || list.get().unwrap_or_default();
    let on_action = Callback::new(move |(id, action)| act(id, action));
    // Inputs at 16px on phones, or iOS zooms in on focus
    let input =
        "h-10 w-full min-w-0 rounded-md border bg-transparent px-3 text-base sm:h-9 sm:text-sm";
    view! {
        <Card class=SECTION>
            <CardHeader class="max-sm:px-0">
                <CardTitle class="text-xl">"SAT Dojo submissions"</CardTitle>
                <CardDescription>
                    "Re-evaluate runs a submission again on the current hidden set. \"Not a copy\" ranks a submission marked as behaving like an earlier one. Delete removes it for good."
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-4 max-sm:px-0">
                {move || message().map(|m| view! { <p class="rounded-md bg-accent px-3 py-2 text-sm">{m}</p> })}
                {move || if loaded.get() {
                    view! {
                        <div class="overflow-x-auto max-md:hidden">
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
                                    <For each=rows key=|s| (s.id, s.state, s.similar_to, s.solved, s.wrong) let:s>
                                        <SubmissionRow s on_action />
                                    </For>
                                </tbody>
                            </table>
                        </div>
                        <ul class="flex flex-col gap-2 md:hidden">
                            <For each=rows key=|s| (s.id, s.state, s.similar_to, s.solved, s.wrong) let:s>
                                <SubmissionCard s on_action />
                            </For>
                        </ul>
                    }
                    .into_any()
                } else {
                    view! { <p class="text-muted-foreground">"Loading…"</p> }.into_any()
                }}
                <form
                    action="/admin/house"
                    method="post"
                    enctype="multipart/form-data"
                    class="flex flex-col gap-3 border-t pt-4 sm:flex-row sm:flex-wrap sm:items-end"
                >
                    <p class="text-sm font-medium sm:basis-full">"Add a house entry"</p>
                    <input name="name" required maxlength="40" placeholder="Name, e.g. Hashi CDCL" aria-label="Name" class=format!("{input} sm:w-48") />
                    <input name="description" maxlength="600" placeholder="Description" aria-label="Description" class=format!("{input} sm:flex-1") />
                    <input name="wasm" type="file" accept=".wasm" required aria-label="Solver (.wasm)" class="max-w-full text-sm file:me-3 file:rounded-md file:border file:bg-card file:px-3 file:py-1.5 file:text-sm file:font-medium" />
                    <Button attr:r#type="submit" class="max-sm:h-10 max-sm:w-full">"Add"</Button>
                </form>
            </CardContent>
        </Card>
    }
}

fn state_badge(s: &AdminSubmission) -> AnyView {
    match s.state {
        _ if s.wrong > 0 => {
            view! { <Badge variant=BadgeVariant::Destructive>"disqualified"</Badge> }.into_any()
        }
        DojoState::Done => view! { <Badge variant=BadgeVariant::Success>"done"</Badge> }.into_any(),
        DojoState::Queued => {
            view! { <Badge variant=BadgeVariant::Muted>"queued"</Badge> }.into_any()
        }
        DojoState::Unknown => {
            view! { <Badge variant=BadgeVariant::Muted>"unknown"</Badge> }.into_any()
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
    }
}

fn solved(s: &AdminSubmission) -> String {
    s.solved
        .map_or_else(|| "–".into(), |n| format!("{n}/{}", s.total))
}

fn par2(s: &AdminSubmission) -> String {
    s.par2.map_or_else(|| "–".into(), |p| format!("{p:.3}"))
}

/// Re-evaluate, "Not a copy" for one marked as a copy, Delete; `class` sizes them.
fn actions(
    s: &AdminSubmission,
    on_action: Callback<(i64, SubmissionAction)>,
    size: ButtonSize,
    class: &'static str,
) -> impl IntoView + use<> {
    let id = s.id;
    view! {
        <Button variant=ButtonVariant::Outline size class=class on:click=move |_| on_action.run((id, SubmissionAction::Requeue))>"Re-evaluate"</Button>
        {s.similar_to.map(|_| view! { <Button variant=ButtonVariant::Outline size class=class on:click=move |_| on_action.run((id, SubmissionAction::ClearCopy))>"Not a copy"</Button> })}
        <Button variant=ButtonVariant::Destructive size class=class on:click=move |_| on_action.run((id, SubmissionAction::Delete))>"Delete"</Button>
    }
}

#[component]
fn SubmissionRow(
    s: AdminSubmission,
    on_action: Callback<(i64, SubmissionAction)>,
) -> impl IntoView {
    let id = s.id;
    view! {
        <tr class="border-b last:border-0">
            <td class="py-2 pe-3 font-pixel">{id}</td>
            <td class="py-2 pe-3">
                <a href=format!("/dojo/{id}") class="font-medium hover:underline">{s.name.clone()}</a>
                {s.house.then(|| view! { <Badge variant=BadgeVariant::Secondary size=BadgeSize::Sm class="ms-2">"house"</Badge> })}
                <span class="block text-xs text-muted-foreground">{s.author.clone()}</span>
            </td>
            <td class="py-2 pe-3">
                {state_badge(&s)}
                {s.similar_to.map(|c| view! { <span class="ms-2 text-xs text-warning-dark">{format!("copy of #{c}")}</span> })}
            </td>
            <td class="py-2 pe-3 text-end tabular-nums">{s.solved.map(|n| format!("{n}/{}", s.total)).unwrap_or_default()}</td>
            <td class="py-2 pe-3 text-end font-pixel tabular-nums">{s.par2.map(|p| format!("{p:.3}")).unwrap_or_default()}</td>
            <td class="py-2 pe-3 text-xs">{date_string(crate::model::day_of(s.created_at))}" · season "{s.season}</td>
            <td class="py-2">
                <div class="flex justify-end gap-1">{actions(&s, on_action, ButtonSize::Sm, "")}</div>
            </td>
        </tr>
    }
}

/// A submission on a phone: the solver and its state, its numbers, the buttons.
#[component]
fn SubmissionCard(
    s: AdminSubmission,
    on_action: Callback<(i64, SubmissionAction)>,
) -> impl IntoView {
    let id = s.id;
    view! {
        <li class="rounded-lg border bg-card p-3" class:border-warning=s.similar_to.is_some()>
            <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                    <p class="truncate">
                        <span class="me-1.5 font-pixel text-muted-foreground">"#"{id}</span>
                        <a href=format!("/dojo/{id}") class="font-medium hover:underline">{s.name.clone()}</a>
                    </p>
                    <p class="truncate text-xs text-muted-foreground">{s.author.clone()}" · season "{s.season}</p>
                </div>
                <div class="flex shrink-0 flex-col items-end gap-1">
                    {state_badge(&s)}
                    {s.house.then(|| view! { <Badge variant=BadgeVariant::Secondary size=BadgeSize::Sm>"house"</Badge> })}
                </div>
            </div>
            {s.similar_to.map(|c| view! { <p class="mt-2 text-xs font-medium text-warning-dark">{format!("Behaves like #{c}: not ranked")}</p> })}
            <dl class="mt-3 grid grid-cols-3 gap-2 border-t pt-3">
                {phone_stat("Solved", solved(&s), "")}
                {phone_stat("PAR-2", par2(&s), "font-pixel")}
                {phone_stat("Submitted", date_string(crate::model::day_of(s.created_at)), "")}
            </dl>
            <div class="mt-3 flex flex-wrap gap-2">{actions(&s, on_action, ButtonSize::Default, "flex-auto")}</div>
        </li>
    }
}
