//! A player's public profile, linked from every leaderboard: their daily record and stamp book.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::load_player;
use crate::components::layout::use_localized;
use crate::components::record::Record;
use crate::components::stamps::{StampBadge, StampBook};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::date_short;
use crate::model::{MS_PER_DAY, PlayerProfile};
use crate::state::{Auth, error_toast};

#[component]
fn Profile(p: PlayerProfile) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let account = use_localized("/account");
    let PlayerProfile {
        id,
        name,
        guest,
        created_at,
        stats,
    } = p;
    let shown = RwSignal::new(stats.shown);
    let initial = name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let mine = move || auth.user.with(|u| u.as_ref().is_some_and(|u| u.id == id));
    let since = move || {
        td_string!(
            i18n.get_locale(),
            ranks.player.since,
            date = date_short(i18n.get_locale(), created_at.div_euclid(MS_PER_DAY))
        )
    };
    let solved = stats.solved > 0;
    let (book, players) = (stats.stamps.clone(), stats.players);
    view! {
        <div class="mb-6 flex items-center gap-4">
            <span class=format!(
                "grid size-16 shrink-0 rotate-[-6deg] place-items-center rounded-lg font-pixel text-3xl {}",
                if guest { "bg-secondary text-secondary-foreground" } else { "hanko" },
            )>{initial}</span>
            <div class="min-w-0">
                <p class="font-pixel text-sm text-primary">"選手 · senshu"</p>
                <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
                    <h1 class="min-w-0 break-words text-3xl font-black">{name}</h1>
                    {shown.get_untracked().map(|stamp| view! { <StampBadge stamp=stamp /> })}
                    {guest.then(|| view! { <Badge variant=BadgeVariant::Secondary>{t!(i18n, common.ranking.guest)}</Badge> })}
                </div>
                <p class="text-sm text-muted-foreground">
                    {since}
                    {move || {
                        mine()
                            .then(|| {
                                view! {
                                    " "
                                    <A href=account attr:class="text-primary underline-offset-4 hover:underline">
                                        {t!(i18n, ranks.player.you)} " →"
                                    </A>
                                }
                            })
                    }}
                </p>
            </div>
        </div>

        <div class="flex flex-col gap-6">
            <Card class="gap-4">
                <CardHeader>
                    <CardTitle class="flex items-baseline gap-2 text-xl">
                        {t!(i18n, ranks.player.record)} <span class="font-pixel text-sm text-primary">"記録"</span>
                    </CardTitle>
                </CardHeader>
                <CardContent class="grid grid-cols-1 gap-4 md:grid-cols-[minmax(0,1fr)_minmax(0,18rem)] md:gap-6">
                    {if solved {
                        view! { <Record stats=stats /> }.into_any()
                    } else {
                        view! {
                            <div class="seigaiha grid h-40 place-items-center rounded-lg border border-dashed text-sm text-muted-foreground md:col-span-2">
                                <p class="rounded-md bg-card px-3 py-1">{t!(i18n, account.no_dailies)} " 待っています"</p>
                            </div>
                        }
                            .into_any()
                    }}
                </CardContent>
            </Card>

            {(!book.is_empty())
                .then(|| {
                    view! {
                        <Card class="gap-4">
                            <CardHeader>
                                <CardTitle class="flex items-baseline gap-2 text-xl">
                                    {t!(i18n, stamps.title)} <span class="font-pixel text-sm text-primary">"判子帳"</span>
                                </CardTitle>
                            </CardHeader>
                            <CardContent>
                                <StampBook book players shown />
                            </CardContent>
                        </Card>
                    }
                })}
        </div>
    }
}

#[component]
pub fn PlayerPage() -> impl IntoView {
    let i18n = use_i18n();
    let params = use_params_map();
    let back = use_localized("/ranks");
    let id = move || params.with(|p| p.get("id").and_then(|id| id.parse::<i64>().ok()));
    // `Some(None)`: there is no such player
    let profile = RwSignal::new(None::<Option<PlayerProfile>>);

    // Fetch whenever the player changes (browser only)
    Effect::new(move |_| {
        let Some(id) = id() else {
            profile.set(Some(None));
            return;
        };
        profile.set(None);
        spawn_local(async move {
            match load_player(id).await {
                Ok(p) => profile.set(Some(p)),
                Err(e) => error_toast(e),
            }
        });
    });

    let title = move || {
        profile.with(|p| match p {
            Some(Some(p)) => format!("{} · Hashi", p.name),
            _ => format!("{} · Hashi", t_string!(i18n, ranks.player.title)),
        })
    };
    view! {
        <Title text=title />
        // Players didn't choose to be found by search engines
        <Meta name="robots" content="noindex" />
        <section class="mx-auto flex max-w-5xl flex-col gap-5 px-4 py-6 sm:px-6">
            <A href=back attr:class="w-fit text-sm text-primary hover:underline">
                "← " {t!(i18n, ranks.player.back)}
            </A>
            <div>
                {move || match profile.get() {
                    None => view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                    Some(None) => view! { <p class="text-muted-foreground">{t!(i18n, ranks.player.not_found)}</p> }.into_any(),
                    Some(Some(p)) => view! { <Profile p /> }.into_any(),
                }}
            </div>
        </section>
    }
}
