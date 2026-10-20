//! Moderating community benchmarks: accept verified ones into the pool, reject, verify again,
//! delete.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{admin_benchmarks, update_admin_benchmark};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::model::{AdminBenchmark, BenchmarkAction, BenchmarkState, date_string};

use super::{SECTION, error};

fn badge(state: BenchmarkState) -> impl IntoView {
    let variant = match state {
        BenchmarkState::Queued => BadgeVariant::Muted,
        BenchmarkState::Checking => BadgeVariant::Warning,
        BenchmarkState::Verified => BadgeVariant::Secondary,
        BenchmarkState::Accepted => BadgeVariant::Success,
        BenchmarkState::Refused | BenchmarkState::Rejected => BadgeVariant::Destructive,
    };
    view! { <Badge variant>{state.id()}</Badge> }
}

#[component]
fn Row(b: AdminBenchmark, on_action: Callback<(i64, BenchmarkAction)>) -> impl IntoView {
    let id = b.id;
    let gbd = format!("https://benchmark-database.de/file/{}", b.hash);
    let answer = match (b.claimed, b.answer) {
        (_, Some(a)) => a.id().to_string(),
        (Some(c), None) => format!("{}?", c.id()),
        (None, None) => "–".into(),
    };
    let button = move |label: &'static str, action: BenchmarkAction, variant: ButtonVariant| {
        view! {
            <Button variant size=ButtonSize::Sm on:click=move |_| on_action.run((id, action))>{label}</Button>
        }
    };
    let acceptable = matches!(b.state, BenchmarkState::Verified | BenchmarkState::Rejected)
        && b.answer.is_some();
    view! {
        <li class="flex flex-col gap-2 rounded-lg border bg-card p-3 text-sm">
            <div class="flex flex-wrap items-start justify-between gap-2">
                <div class="min-w-0">
                    <p>
                        <span class="me-1.5 font-pixel text-muted-foreground">"#"{id}</span>
                        <span class="font-medium">{b.family.clone()}</span>
                        <span class="text-muted-foreground">" by "{if b.author.is_empty() { "(deleted)".to_string() } else { b.author.clone() }}</span>
                    </p>
                    <p class="text-xs text-muted-foreground">
                        {format!("{} vars · {} clauses · {} · ", b.vars, b.clauses, answer)}
                        {b.solve_ms.map(|ms| format!("splr {ms} ms · "))}
                        {date_string(crate::model::day_of(b.created_at))}
                        {b.from_gbd.then_some(" · from GBD")}
                        " · "
                        <a href=gbd target="_blank" rel="noopener" class="font-mono text-primary hover:underline">{b.hash.chars().take(12).collect::<String>()}</a>
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    {badge(b.state)}
                    {b.reason.clone().map(|r| view! { <span class="text-xs text-muted-foreground">{r}</span> })}
                </div>
            </div>
            {(!b.description.is_empty()).then(|| view! { <p class="whitespace-pre-wrap text-muted-foreground">{b.description.clone()}</p> })}
            <div class="flex flex-wrap gap-1">
                {acceptable.then(|| button("Accept", BenchmarkAction::Accept, ButtonVariant::Default))}
                {(b.state != BenchmarkState::Rejected).then(|| button("Reject", BenchmarkAction::Reject, ButtonVariant::Outline))}
                {button("Verify again", BenchmarkAction::Requeue, ButtonVariant::Outline)}
                {button("Delete", BenchmarkAction::Delete, ButtonVariant::Destructive)}
            </div>
        </li>
    }
}

#[component]
pub(super) fn BenchmarksAdmin(on_change: Callback<()>) -> impl IntoView {
    let list = RwSignal::new(None::<Vec<AdminBenchmark>>);
    let load = move || {
        spawn_local(async move {
            match admin_benchmarks().await {
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
    let on_action = Callback::new(move |(id, action): (i64, BenchmarkAction)| {
        if action == BenchmarkAction::Delete
            && !window()
                .confirm_with_message(&format!("Delete benchmark #{id} for good?"))
                .unwrap_or(false)
        {
            return;
        }
        spawn_local(async move {
            match update_admin_benchmark(id, action).await {
                Ok(()) => {
                    load();
                    on_change.run(());
                }
                Err(e) => error(&e),
            }
        });
    });
    view! {
        <Card class=SECTION>
            <CardHeader class="max-sm:px-0">
                <CardTitle class="text-xl">"Community benchmarks"</CardTitle>
                <CardDescription>
                    "Verified benchmarks have a certain answer and aren't too easy. Accepted ones join the pool and can be drawn by seasons that start later; a benchmark a season drew stays published with it even when deleted."
                </CardDescription>
            </CardHeader>
            <CardContent class="max-sm:px-0">
                {move || match list.get() {
                    None => view! { <p class="text-muted-foreground">"Loading…"</p> }.into_any(),
                    Some(l) if l.is_empty() => view! { <p class="text-muted-foreground">"No benchmarks yet."</p> }.into_any(),
                    Some(l) => {
                        view! {
                            <ul class="flex flex-col gap-2">
                                <For each=move || l.clone() key=|b| (b.id, b.state, b.reason.clone()) let:b>
                                    <Row b on_action />
                                </For>
                            </ul>
                        }
                            .into_any()
                    }
                }}
            </CardContent>
        </Card>
    }
}
