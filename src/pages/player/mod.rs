//! A player's public profile, linked from every leaderboard: their daily record and stamp book,
//! the head-to-head record against them if they are a rival, and reporting or blocking them.

pub mod head_to_head;
pub mod moderation;

use leptos::prelude::*;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::load_player;
use crate::components::layout::use_localized;
use crate::components::record::Record;
use crate::components::rivals::RivalButton;
use crate::components::stamps::{StampBadge, StampBook};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::{date_short, localized, og_locale};
use crate::model::{MS_PER_DAY, PlayerProfile};
use crate::pages::not_found_status;
use crate::seo::{SITE_NAME, SITE_URL, og_image, title_text};
use crate::state::{Auth, Blocks, error_toast};

use head_to_head::HeadToHeadCard;
use moderation::ProfileActions;

#[component]
fn Profile(p: PlayerProfile) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let blocks = expect_context::<Blocks>();
    let account = use_localized("/account");
    let PlayerProfile {
        id,
        name,
        guest,
        created_at,
        stats,
    } = p;
    let shown = RwSignal::new(stats.shown);
    let name_for_button = name.clone();
    let name_for_actions = name.clone();
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
        <div class="mb-6 flex flex-wrap items-center gap-x-4 gap-y-2">
            <span class=format!(
                "grid size-16 shrink-0 rotate-[-6deg] place-items-center rounded-lg font-pixel text-3xl {}",
                if guest { "bg-secondary text-secondary-foreground" } else { "hanko" },
            )>{initial}</span>
            <div class="min-w-0 flex-1">
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
                // Their name is hidden from the viewer everywhere else
                <Show when=move || blocks.is(id)>
                    <p class="text-sm text-muted-foreground" data-blocked-note="">{t!(i18n, ranks.player.block.blocked)}</p>
                </Show>
            </div>
            // Under the name on phones, beside it from sm on
            <div class="flex w-full flex-wrap items-center justify-end gap-1 sm:ms-auto sm:w-auto sm:shrink-0 sm:flex-col sm:items-end sm:self-center">
                // A blocked player can't be a rival
                <Show when=move || !blocks.is(id)>
                    <RivalButton user_id=id name=name_for_button.clone() />
                </Show>
                <ProfileActions player=id name=name_for_actions />
            </div>
        </div>

        <div class="flex flex-col gap-6">
            <HeadToHeadCard player=id />
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

/// What a link to a player's page shows where it's shared: who, and their record in short.
fn preview(locale: Locale, p: &PlayerProfile) -> String {
    let who = td_string!(locale, ranks.player.preview, name = p.name.clone());
    let s = &p.stats;
    let record = if s.solved > 0 {
        td_string!(
            locale,
            ranks.player.preview_stats,
            solved = s.solved,
            best = s.best_streak,
            medals = s.gold + s.silver + s.bronze
        )
    } else {
        td_string!(locale, account.no_dailies).to_string()
    };
    format!("{who} {record}")
}

/// The title, description and social tags of a player's page, for links to it. Unlike
/// [`crate::seo::PageMeta`] without a canonical URL or other languages: the page isn't indexed.
#[component]
fn PlayerMeta(p: PlayerProfile) -> impl IntoView {
    let i18n = use_i18n();
    let locale = i18n.get_locale_untracked();
    let title = format!("{} · {SITE_NAME}", p.name);
    let description = preview(locale, &p);
    let url = format!(
        "{SITE_URL}{}",
        localized(locale, &format!("/player/{}", p.id))
    );
    let image = og_image(locale);
    let image_alt = td_string!(locale, common.og_image_alt);
    let tab = title.clone();
    view! {
        <Title text=title_text(move || tab.clone()) />
        <Meta name="description" content=description.clone() />
        <Meta property="og:type" content="profile" />
        <Meta property="profile:username" content=p.name />
        <Meta property="og:site_name" content=SITE_NAME />
        <Meta property="og:locale" content=og_locale(locale) />
        <Meta property="og:title" content=title.clone() />
        <Meta property="og:description" content=description.clone() />
        <Meta property="og:url" content=url />
        <Meta property="og:image" content=image.clone() />
        <Meta property="og:image:type" content="image/png" />
        <Meta property="og:image:width" content="1200" />
        <Meta property="og:image:height" content="630" />
        <Meta property="og:image:alt" content=image_alt />
        <Meta name="twitter:card" content="summary_large_image" />
        <Meta name="twitter:title" content=title />
        <Meta name="twitter:description" content=description />
        <Meta name="twitter:image" content=image />
        <Meta name="twitter:image:alt" content=image_alt />
    }
}

#[component]
pub fn PlayerPage() -> impl IntoView {
    let i18n = use_i18n();
    let params = use_params_map();
    let back = use_localized("/ranks");
    let id = move || params.with(|p| p.get("id").and_then(|id| id.parse::<i64>().ok()));
    // Loaded on the server as well, which renders the page once it's there (`SsrMode::Async` in
    // app.rs), so a link to it shows the player's name and record. `Ok(None)`: there is no such
    // player. The browser loads another one when the URL changes.
    let profile = Resource::new(id, |id| async move {
        match id {
            Some(id) => load_player(id).await,
            None => Ok(None),
        }
    });
    Effect::new(move |_| {
        if let Some(Err(e)) = profile.get() {
            error_toast(e);
        }
    });

    let loading =
        move || view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> };
    view! {
        // Until the player is loaded, and when they can't be
        <Title text=title_text(move || format!("{} · Hashi", t_string!(i18n, ranks.player.title))) />
        // Players didn't choose to be found by search engines
        <Meta name="robots" content="noindex" />
        <section class="mx-auto flex max-w-5xl flex-col gap-5 px-4 py-6 sm:px-6">
            <A href=back attr:class="w-fit text-sm text-primary hover:underline">
                "← " {t!(i18n, ranks.player.back)}
            </A>
            <div>
                <Suspense fallback=loading>
                    {move || Suspend::new(async move {
                        match profile.await {
                            Ok(Some(p)) => view! { <PlayerMeta p=p.clone() /> <Profile p /> }.into_any(),
                            Ok(None) => {
                                not_found_status();
                                view! {
                                    <Title text=title_text(move || format!("{} · Hashi", t_string!(i18n, common.not_found.title))) />
                                    <p class="text-muted-foreground">{t!(i18n, ranks.player.not_found)}</p>
                                }
                                    .into_any()
                            }
                            // Shown as a toast
                            Err(_) => loading().into_any(),
                        }
                    })}
                </Suspense>
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::UserStats;

    fn player(stats: UserStats) -> PlayerProfile {
        PlayerProfile {
            id: 7,
            name: "aiko".into(),
            guest: false,
            created_at: 0,
            stats,
        }
    }

    #[test]
    fn previews_sum_up_the_record() {
        let stats = UserStats {
            solved: 12,
            current_streak: 2,
            best_streak: 5,
            gold: 1,
            silver: 2,
            bronze: 0,
            ..UserStats::default()
        };
        assert_eq!(
            preview(Locale::en, &player(stats)),
            "aiko on Hashi, the daily Hashiwokakero puzzle: record and stamp book. \
             Dailies solved: 12 · Best streak: 5 · Medals: 3"
        );
        assert_eq!(
            preview(Locale::de, &player(UserStats::default())),
            "aiko auf Hashi, dem täglichen Hashiwokakero-Rätsel: Bilanz und Stempelheft. \
             Noch keine Tagesrätsel."
        );
    }
}
