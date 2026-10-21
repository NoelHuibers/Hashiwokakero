//! Messages from the contact form: answer by email, then delete.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{admin_messages, delete_admin_message};
use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::model::{ContactMessage, MS_PER_DAY, date_string};

use super::{SECTION, error};

#[component]
pub(super) fn MessagesAdmin(on_change: Callback<()>) -> impl IntoView {
    let list = RwSignal::new(None::<Vec<ContactMessage>>);
    let more = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let load = move |before: Option<i64>| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match admin_messages(before).await {
                Ok(l) => {
                    more.set(l.len() == 100);
                    if before.is_some() {
                        list.update(|all| all.get_or_insert_with(Vec::new).extend(l));
                    } else {
                        list.set(Some(l));
                    }
                }
                Err(e) => error(&e),
            }
            busy.set(false);
        });
    };
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            load(None);
        }
    });
    let delete = move |id: i64| {
        if !window()
            .confirm_with_message(&format!("Delete message #{id}?"))
            .unwrap_or(false)
        {
            return;
        }
        spawn_local(async move {
            match delete_admin_message(id).await {
                Ok(()) => {
                    load(None);
                    on_change.run(());
                }
                Err(e) => error(&e),
            }
        });
    };

    view! {
        <Card class=SECTION>
            <CardHeader class="max-sm:px-0">
                <CardTitle>"Contact messages"</CardTitle>
                <CardDescription>
                    "Mailed to CONTACT_TO when SMTP is set up. Delete them once answered; they go by themselves after 180 days."
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3 max-sm:px-0">
                {move || match list.get() {
                    None => view! { <p class="text-sm text-muted-foreground">"…"</p> }.into_any(),
                    Some(l) if l.is_empty() => view! { <p class="text-sm text-muted-foreground">"No messages."</p> }.into_any(),
                    Some(l) => l
                        .into_iter()
                        .map(|m| {
                            let id = m.id;
                            let reply = (!m.email.is_empty())
                                .then(|| format!("mailto:{}?subject=Re: Hashi (%23{id})", m.email));
                            view! {
                                <article class="flex flex-col gap-2 rounded-lg border bg-card p-3 sm:p-4">
                                    <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
                                        <span class="font-mono text-xs text-muted-foreground">{format!("#{id}")}</span>
                                        <span class="font-medium">{if m.name.is_empty() { "(no name)".to_string() } else { m.name.clone() }}</span>
                                        <span class="text-muted-foreground">{if m.email.is_empty() { "(no email)".to_string() } else { m.email.clone() }}</span>
                                        <span class="text-muted-foreground tabular-nums">{date_string(m.created_at.div_euclid(MS_PER_DAY))}</span>
                                        {(!m.mailed).then(|| view! { <Badge variant=BadgeVariant::Outline size=BadgeSize::Sm>"not mailed"</Badge> })}
                                    </div>
                                    <p class="whitespace-pre-wrap break-words text-sm">{m.message}</p>
                                    <div class="flex gap-2">
                                        {reply.map(|href| view! {
                                            <a href=href class="inline-flex h-8 items-center rounded-md border px-3 text-sm font-medium hover:bg-accent">"Reply"</a>
                                        })}
                                        <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on:click=move |_| delete(id)>"Delete"</Button>
                                    </div>
                                </article>
                            }
                        })
                        .collect_view()
                        .into_any(),
                }}
                <Show when=move || more.get()>
                    <Button variant=ButtonVariant::Outline attr:disabled=move || busy.get()
                        on:click=move |_| {
                            let before = list.with_untracked(|rows| rows.as_ref().and_then(|rows| rows.last()).map(|m| m.id));
                            load(before);
                        }>"Load older messages"</Button>
                </Show>
            </CardContent>
        </Card>
    }
}
