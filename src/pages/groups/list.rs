//! The player's groups, each a card to open with how many are done today.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::prefix;
use crate::model::{GroupSummary, colour_var};

use super::hanko::{UnreadBadge, refresh_unread};

/// One group of the list: its name, a row of its members' colours, today's progress.
#[component]
fn GroupItem(group: GroupSummary) -> impl IntoView {
    let i18n = use_i18n();
    let GroupSummary {
        id,
        name,
        members,
        done_today,
        owner,
        colours,
    } = group;
    let href = move || format!("{}/groups/{id}", prefix(i18n.get_locale()));
    let percent = done_today * 100 / members.max(1);
    let all_done = members > 0 && done_today == members;
    view! {
        <li>
            <A
                href=href
                attr:class="group flex flex-col gap-3 rounded-xl border bg-background p-4 transition-transform hover:-translate-y-0.5 hover:border-primary/60 focus-visible:outline-2 focus-visible:outline-ring"
                attr:data-group=id
            >
                <div class="flex items-start justify-between gap-3">
                    <div class="min-w-0">
                        <p class="truncate text-lg font-bold group-hover:text-primary">{name}</p>
                        <p class="text-xs text-muted-foreground">
                            {move || td_string!(i18n.get_locale(), groups.list.members, count = members)}
                        </p>
                    </div>
                    <div class="flex shrink-0 items-center gap-1.5">
                        <UnreadBadge group=id />
                        {owner
                            .then(|| {
                                view! {
                                    <Badge variant=BadgeVariant::Secondary size=BadgeSize::Sm class="shrink-0">
                                        "主 " {t!(i18n, groups.members.owner)}
                                    </Badge>
                                }
                            })}
                    </div>
                </div>
                <div class="flex flex-wrap gap-1" aria-hidden="true">
                    {colours
                        .into_iter()
                        .map(|c| {
                            view! { <span class="size-3 rounded-[3px] shadow-xs" style=format!("background: {}", colour_var(c))></span> }
                        })
                        .collect_view()}
                </div>
                <div class="flex flex-col gap-1">
                    <div class="flex items-center justify-between text-xs">
                        <span class="text-muted-foreground">{t!(i18n, groups.list.today)}</span>
                        <span class="font-pixel tabular-nums" class:text-success-dark=all_done>
                            {format!("{done_today}/{members}")}
                            {all_done.then_some(" 済")}
                        </span>
                    </div>
                    <span class="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                        <span class="block h-full rounded-full bg-primary" style=format!("width: {percent}%")></span>
                    </span>
                </div>
            </A>
        </li>
    }
}

/// The list of the player's groups (`None` while it loads).
#[component]
pub fn GroupList(groups: RwSignal<Option<Vec<GroupSummary>>>) -> impl IntoView {
    let i18n = use_i18n();
    // Hanko got since the app loaded show up on the list
    refresh_unread();
    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, groups.list.title)} <span class="font-pixel text-sm text-primary">"仲間"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, groups.list.text)}</CardDescription>
            </CardHeader>
            <CardContent>
                {move || match groups.get() {
                    None => view! { <p class="text-sm text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                    Some(list) if list.is_empty() => {
                        view! {
                            <div class="seigaiha grid h-40 place-items-center rounded-lg border border-dashed text-center text-sm text-muted-foreground">
                                <p class="max-w-xs rounded-md bg-card px-3 py-1">{t!(i18n, groups.list.none)} " 一人旅"</p>
                            </div>
                        }
                            .into_any()
                    }
                    Some(list) => {
                        view! {
                            <ul class="grid grid-cols-1 gap-3 sm:grid-cols-2" aria-label=move || t_string!(i18n, groups.list.title)>
                                {list.into_iter().map(|group| view! { <GroupItem group /> }).collect_view()}
                            </ul>
                        }
                            .into_any()
                    }
                }}
            </CardContent>
        </Card>
    }
}
