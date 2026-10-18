//! Running the SAT Dojo: re-evaluating, clearing copies, deleting and adding house entries.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;

use crate::api::{admin_submissions, update_admin_submission};
use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::model::{AdminSubmission, DojoState, SubmissionAction, date_string};

use super::error;

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
