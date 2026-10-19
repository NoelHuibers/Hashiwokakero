//! Names on the boards: the ones the list of blocked names flags, and renaming reported players.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{admin_names, admin_rename_player, update_admin_submission};
use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::model::{FlaggedName, NameKind, SubmissionAction};
use crate::state::{ToastKind, toast};

use super::{SECTION, error};

#[component]
pub(super) fn NamesAdmin(on_change: Callback<()>) -> impl IntoView {
    let list = RwSignal::new(None::<Vec<FlaggedName>>);
    let load = move || {
        spawn_local(async move {
            match admin_names().await {
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
    let done = move || {
        load();
        on_change.run(());
    };
    let rename = move |id: i64| {
        if !window()
            .confirm_with_message(&format!("Rename player #{id} to \"Player {id}\"?"))
            .unwrap_or(false)
        {
            return;
        }
        spawn_local(async move {
            match admin_rename_player(id).await {
                Ok(Some(name)) => {
                    toast(ToastKind::Success, move |_| {
                        format!("Player #{id} is now {name}.")
                    });
                    done();
                }
                Ok(None) => toast(ToastKind::Error, move |_| {
                    format!("There is no player #{id}.")
                }),
                Err(e) => error(&e),
            }
        });
    };
    let delete = move |id: i64| {
        if !window()
            .confirm_with_message(&format!("Delete submission #{id} for good?"))
            .unwrap_or(false)
        {
            return;
        }
        spawn_local(async move {
            match update_admin_submission(id, SubmissionAction::Delete).await {
                Ok(()) => done(),
                Err(e) => error(&e),
            }
        });
    };
    let reported = RwSignal::new(String::new());
    let rename_reported = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        match reported
            .get_untracked()
            .trim()
            .trim_start_matches('#')
            .parse::<i64>()
        {
            Ok(id) => {
                rename(id);
                reported.set(String::new());
            }
            Err(_) => toast(ToastKind::Error, |_| "That's no player number.".to_string()),
        }
    };
    // Inputs at 16px on phones, or iOS zooms in on focus
    let input =
        "h-10 w-32 min-w-0 rounded-md border bg-transparent px-3 text-base sm:h-9 sm:text-sm";

    view! {
        <Card class=SECTION>
            <CardHeader class="max-sm:px-0">
                <CardTitle class="text-xl">"Names"</CardTitle>
                <CardDescription>
                    "Names on the boards that src/server/blocked_names.txt flags: chosen before the word was on the list, or official looking. New ones are refused when they are chosen. Renaming gives a player \"Player <number>\" on every board; they keep their username to sign in and can choose a new name right away."
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-4 max-sm:px-0">
                <form class="flex flex-wrap items-center gap-2" on:submit=rename_reported>
                    <label for="rename-player" class="text-sm">"Reported player #"</label>
                    <input
                        id="rename-player"
                        class=input
                        inputmode="numeric"
                        placeholder="17"
                        prop:value=move || reported.get()
                        on:input=move |ev| reported.set(event_target_value(&ev))
                    />
                    <Button size=ButtonSize::Sm attr:r#type="submit">"Rename"</Button>
                    <span class="text-xs text-muted-foreground">"The number is in the address of their profile, /player/17."</span>
                </form>
                {move || match list.get() {
                    None => view! { <p class="text-sm text-muted-foreground">"…"</p> }.into_any(),
                    Some(l) if l.is_empty() => view! { <p class="text-sm text-muted-foreground">"No flagged names."</p> }.into_any(),
                    Some(l) => l
                        .into_iter()
                        .map(|n| {
                            let id = n.id;
                            let (kind, href) = match n.kind {
                                NameKind::Player => ("player", format!("/player/{id}")),
                                NameKind::Solver => ("solver", format!("/dojo/{id}")),
                            };
                            view! {
                                <article class="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border bg-card p-3">
                                    <span class="font-mono text-xs text-muted-foreground">{format!("{kind} #{id}")}</span>
                                    <a href=href class="font-medium underline-offset-4 hover:underline">{n.name}</a>
                                    <Badge variant=BadgeVariant::Outline size=BadgeSize::Sm>{n.word}</Badge>
                                    <span class="flex-1"></span>
                                    {match n.kind {
                                        NameKind::Player => view! {
                                            <Button variant=ButtonVariant::Outline size=ButtonSize::Sm on:click=move |_| rename(id)>"Rename"</Button>
                                        }.into_any(),
                                        NameKind::Solver => view! {
                                            <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on:click=move |_| delete(id)>"Delete"</Button>
                                        }.into_any(),
                                    }}
                                </article>
                            }
                        })
                        .collect_view()
                        .into_any(),
                }}
            </CardContent>
        </Card>
    }
}
