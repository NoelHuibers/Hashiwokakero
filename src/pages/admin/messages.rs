//! The feedback inbox: messages of the feedback form, the new ones first in view. Answer by
//! email, then mark them answered, or close them (praise, spam); delete only what shouldn't be
//! kept at all.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{admin_message_status, admin_messages, delete_admin_message};
use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::model::{
    ContactMessage, ContextField, FeedbackContext, FeedbackKind, FeedbackStatus, MS_PER_DAY,
    date_string,
};
use crate::pages::contact::encode_component;

use super::{SECTION, error};

/// Characters of a message quoted in a reply at most: mail programs take long links badly.
const QUOTE: usize = 1500;

fn kind_name(kind: FeedbackKind) -> &'static str {
    match kind {
        FeedbackKind::Bug => "Bug",
        FeedbackKind::Idea => "Idea",
        FeedbackKind::Question => "Question",
        FeedbackKind::Other => "Other",
    }
}

fn status_name(status: FeedbackStatus) -> &'static str {
    match status {
        FeedbackStatus::New => "New",
        FeedbackStatus::Issue => "Issue",
        FeedbackStatus::Answered => "Answered",
        FeedbackStatus::Closed => "Closed",
    }
}

/// The context in one line, `·` between its parts, the version as `app 1.1.0` or `website`; the
/// errors on lines of their own after it.
fn context_summary(context: &FeedbackContext) -> (String, Vec<String>) {
    let mut parts = Vec::new();
    let mut errors = Vec::new();
    for (field, value) in context.rows() {
        match field {
            ContextField::App if value.is_empty() => parts.push("website".to_string()),
            ContextField::App => parts.push(format!("app {value}")),
            ContextField::Page => parts.push(format!("from {value}")),
            ContextField::Daily => parts.push(format!("daily {value}")),
            ContextField::Player => parts.push(format!("player {value}")),
            ContextField::Errors => errors.push(value),
            _ => parts.push(value),
        }
    }
    (parts.join(" · "), errors)
}

/// A `mailto:` link that answers the message: `Re:` with its number and first words, the
/// message quoted under the cursor. `None` without an address.
fn reply_href(m: &ContactMessage) -> Option<String> {
    if m.email.is_empty() {
        return None;
    }
    let first: String = m
        .message
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(60)
        .collect();
    let subject = format!("Re: Hashi (#{}): {}", m.id, first.trim());
    let mut quoted: String = m.message.chars().take(QUOTE).collect();
    if m.message.chars().count() > QUOTE {
        quoted.push('…');
    }
    let quoted: Vec<String> = quoted.lines().map(|l| format!("> {l}")).collect();
    let body = format!("\n\n{}\n", quoted.join("\n"));
    Some(format!(
        "mailto:{}?subject={}&body={}",
        m.email,
        encode_component(&subject),
        encode_component(&body)
    ))
}

#[component]
pub(super) fn MessagesAdmin(on_change: Callback<()>) -> impl IntoView {
    let list = RwSignal::new(None::<Vec<ContactMessage>>);
    let more = RwSignal::new(false);
    let busy = RwSignal::new(false);
    // The inbox: new ones of any type
    let status = RwSignal::new(Some(FeedbackStatus::New));
    let kind = RwSignal::new(None::<FeedbackKind>);
    let load = move |before: Option<i64>| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let (status, kind) = (status.get_untracked(), kind.get_untracked());
        spawn_local(async move {
            match admin_messages(before, status, kind).await {
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
    let set_status = move |id: i64, to: FeedbackStatus| {
        spawn_local(async move {
            match admin_message_status(id, to).await {
                Ok(_) => {
                    load(None);
                    on_change.run(());
                }
                Err(e) => error(&e),
            }
        });
    };
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
    let select = "h-9 rounded-md border bg-background px-2 text-sm";

    view! {
        <Card class=SECTION>
            <CardHeader class="max-sm:px-0">
                <CardTitle>"Feedback"</CardTitle>
                <CardDescription>
                    "Bugs, ideas, questions and the rest, mailed to CONTACT_TO when SMTP is set up. Answer, then mark them answered or close them; they go by themselves after 180 days."
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3 max-sm:px-0">
                <div class="flex flex-wrap items-center gap-2 text-sm">
                    <label class="flex items-center gap-2">
                        "Status"
                        <select
                            class=select
                            aria-label="Status"
                            on:change=move |ev| {
                                status.set(FeedbackStatus::from_id(&event_target_value(&ev)));
                                load(None);
                            }
                        >
                            {FeedbackStatus::ALL
                                .into_iter()
                                .map(|s| view! { <option value=s.id() selected=move || status.get() == Some(s)>{status_name(s)}</option> })
                                .collect_view()}
                            <option value="" selected=move || status.get().is_none()>"All"</option>
                        </select>
                    </label>
                    <label class="flex items-center gap-2">
                        "Type"
                        <select
                            class=select
                            aria-label="Type"
                            on:change=move |ev| {
                                kind.set(FeedbackKind::from_id(&event_target_value(&ev)));
                                load(None);
                            }
                        >
                            <option value="" selected=move || kind.get().is_none()>"All"</option>
                            {FeedbackKind::ALL
                                .into_iter()
                                .map(|k| view! { <option value=k.id() selected=move || kind.get() == Some(k)>{kind_name(k)}</option> })
                                .collect_view()}
                        </select>
                    </label>
                </div>
                {move || match list.get() {
                    None => view! { <p class="text-sm text-muted-foreground">"…"</p> }.into_any(),
                    Some(l) if l.is_empty() => view! { <p class="text-sm text-muted-foreground">"No messages."</p> }.into_any(),
                    Some(l) => l
                        .into_iter()
                        .map(|m| {
                            let id = m.id;
                            let reply = reply_href(&m);
                            let (context, errors) = m.context.as_ref().map(context_summary).unwrap_or_default();
                            let issue = m.github_issue.map(|n| (n, format!("{}/issues/{n}", crate::seo::REPO_URL)));
                            let new = m.status == FeedbackStatus::New;
                            view! {
                                <article class="flex flex-col gap-2 rounded-lg border bg-card p-3 sm:p-4" data-status=m.status.id()>
                                    <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
                                        <span class="font-mono text-xs text-muted-foreground">{format!("#{id}")}</span>
                                        <Badge variant=if m.kind == FeedbackKind::Bug { BadgeVariant::Destructive } else { BadgeVariant::Secondary } size=BadgeSize::Sm>
                                            {kind_name(m.kind)}
                                        </Badge>
                                        {(!new).then(|| view! { <Badge variant=BadgeVariant::Outline size=BadgeSize::Sm>{status_name(m.status)}</Badge> })}
                                        <span class="font-medium">{if m.name.is_empty() { "(no name)".to_string() } else { m.name.clone() }}</span>
                                        <span class="text-muted-foreground">{if m.email.is_empty() { "(no email)".to_string() } else { m.email.clone() }}</span>
                                        {m.locale.clone().map(|l| view! { <span class="font-mono text-xs text-muted-foreground">{l}</span> })}
                                        <span class="text-muted-foreground tabular-nums">{date_string(m.created_at.div_euclid(MS_PER_DAY))}</span>
                                        {(!m.mailed).then(|| view! { <Badge variant=BadgeVariant::Outline size=BadgeSize::Sm>"not mailed"</Badge> })}
                                    </div>
                                    <p class="whitespace-pre-wrap break-words text-sm">{m.message.clone()}</p>
                                    {(!context.is_empty()).then(|| view! {
                                        <div class="flex flex-col gap-0.5 font-mono text-xs text-muted-foreground" data-testid="message-context">
                                            <p class="break-words">{context}</p>
                                            {errors.into_iter().map(|e| view! { <p class="break-words text-destructive">{e}</p> }).collect_view()}
                                        </div>
                                    })}
                                    {issue.map(|(n, href)| view! {
                                        <a href=href target="_blank" rel="noopener" class="w-fit text-sm text-primary underline-offset-4 hover:underline">
                                            {format!("Issue #{n} →")}
                                        </a>
                                    })}
                                    <div class="flex flex-wrap gap-2">
                                        {reply.map(|href| view! {
                                            <a href=href class="inline-flex h-8 items-center rounded-md border px-3 text-sm font-medium hover:bg-accent">"Reply"</a>
                                        })}
                                        {(m.status != FeedbackStatus::Answered).then(|| view! {
                                            <Button variant=ButtonVariant::Outline size=ButtonSize::Sm on:click=move |_| set_status(id, FeedbackStatus::Answered)>"Answered"</Button>
                                        })}
                                        {(m.status != FeedbackStatus::Closed).then(|| view! {
                                            <Button variant=ButtonVariant::Outline size=ButtonSize::Sm on:click=move |_| set_status(id, FeedbackStatus::Closed)>"Close"</Button>
                                        })}
                                        {(!new).then(|| view! {
                                            <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on:click=move |_| set_status(id, FeedbackStatus::New)>"Reopen"</Button>
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

#[cfg(test)]
mod tests {
    use super::*;

    fn message(email: &str, text: &str) -> ContactMessage {
        ContactMessage {
            id: 12,
            name: "Kim".into(),
            email: email.into(),
            message: text.into(),
            created_at: 0,
            mailed: true,
            kind: FeedbackKind::Bug,
            context: None,
            locale: Some("de".into()),
            status: FeedbackStatus::New,
            github_issue: None,
            handled_at: None,
        }
    }

    /// A reply says which message it answers and quotes it; without an address there is none.
    #[test]
    fn replies_quote_the_message() {
        assert_eq!(reply_href(&message("", "No address given")), None);
        let href = reply_href(&message(
            "kim@example.com",
            "The board froze.\nAfter a hint & a check",
        ))
        .unwrap();
        assert_eq!(
            href,
            "mailto:kim@example.com?subject=Re%3A%20Hashi%20%28%2312%29%3A%20The%20board%20froze.\
             &body=%0A%0A%3E%20The%20board%20froze.%0A%3E%20After%20a%20hint%20%26%20a%20check%0A"
        );
        // A long one is cut
        let href = reply_href(&message("kim@example.com", &"x".repeat(5000))).unwrap();
        assert!(href.len() < 2 * QUOTE, "{}", href.len());
        assert!(href.contains(&encode_component("…")));
    }

    #[test]
    fn contexts_fit_a_line() {
        let context = FeedbackContext {
            app: Some("1.1.0".into()),
            platform: "ios".into(),
            os: "iOS 18.1".into(),
            language: "de".into(),
            page: Some("/practice".into()),
            daily: Some(14),
            player: Some(7),
            errors: vec!["/practice E:database".into()],
            ..FeedbackContext::default()
        };
        assert_eq!(
            context_summary(&context),
            (
                "app 1.1.0 · iOS · iOS 18.1 · de · from /practice · daily #14 · player #7".into(),
                vec!["/practice E:database".into()]
            )
        );
        assert_eq!(context_summary(&FeedbackContext::default()).0, "website");
    }
}
