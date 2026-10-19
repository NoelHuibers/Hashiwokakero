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
    let load = move || {
        spawn_local(async move {
            match admin_messages().await {
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
                    load();
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
            </CardContent>
        </Card>
    }
}
