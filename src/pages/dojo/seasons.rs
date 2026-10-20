//! The season archive: the current season's commitment, and for every finished season its
//! revealed seed, the hidden set, every result per formula and the final leaderboard.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::load_dojo_seasons;
use crate::components::layout::use_localized;
use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{date_short, prefix};
use crate::model::{DojoEntry, DojoSeason, DojoSeasons, day_of};
use crate::seo::PageMeta;

use super::guide::Code;
use super::names::{family_kanji, family_name, family_text, par2};

/// A file size in MB, for display.
#[expect(
    clippy::cast_precision_loss,
    reason = "sets are a few MB, far below f64's 52-bit mantissa"
)]
fn megabytes(bytes: i64) -> f64 {
    bytes as f64 / 1_000_000.0
}

/// A hash or seed, broken anywhere so it fits on a phone.
#[component]
fn Hash(label: AnyView, value: String) -> impl IntoView {
    view! {
        <div class="flex flex-col gap-0.5">
            <span class="text-xs font-bold uppercase tracking-wider text-muted-foreground">{label}</span>
            <code dir="ltr" class="break-all font-pixel text-xs">{value}</code>
        </div>
    }
}

#[component]
fn FrozenBoard(board: Vec<DojoEntry>) -> impl IntoView {
    let i18n = use_i18n();
    if board.is_empty() {
        return view! { <p class="text-sm text-muted-foreground">{t!(i18n, dojo.seasons.empty_board)}</p> }.into_any();
    }
    view! {
        <div class="overflow-x-auto">
            <table class="w-full text-sm">
                <thead>
                    <tr class="border-b text-start text-xs uppercase tracking-wider text-muted-foreground">
                        <th class="py-2 pe-2 text-start font-semibold">"#"</th>
                        <th class="py-2 pe-3 text-start font-semibold">{t!(i18n, dojo.board.solver)}</th>
                        <th class="py-2 pe-3 text-end font-semibold">{t!(i18n, dojo.board.solved)}</th>
                        <th class="py-2 text-end font-semibold">{t!(i18n, dojo.board.par2)}</th>
                    </tr>
                </thead>
                <tbody>
                    {board
                        .into_iter()
                        .map(|e| {
                            // An empty name: the author deleted their account
                            let deleted = e.name.is_empty();
                            let href = move || format!("{}/dojo/{}", prefix(i18n.get_locale()), e.id);
                            view! {
                                <tr class="border-b last:border-0">
                                    <td class="py-2 pe-2 font-pixel tabular-nums">{e.rank}</td>
                                    <td class="py-2 pe-3">
                                        {if deleted {
                                            view! { <span class="text-muted-foreground">"—"</span> }.into_any()
                                        } else {
                                            view! {
                                                <A href=href attr:class="font-medium hover:text-primary hover:underline">
                                                    {e.name.clone()}
                                                </A>
                                            }
                                                .into_any()
                                        }}
                                        {e
                                            .house
                                            .then(|| {
                                                view! {
                                                    <Badge variant=BadgeVariant::Secondary size=BadgeSize::Sm class="ms-2">
                                                        {t!(i18n, dojo.board.house)}
                                                    </Badge>
                                                }
                                            })}
                                        <span class="block text-xs text-muted-foreground">
                                            {if e.author.is_empty() {
                                                t_string!(i18n, dojo.seasons.deleted).to_string()
                                            } else {
                                                e.author.clone()
                                            }}
                                        </span>
                                    </td>
                                    <td class="py-2 pe-3 text-end font-pixel tabular-nums">
                                        {format!("{}/{}", e.score.solved, e.score.total)}
                                    </td>
                                    <td class="py-2 text-end font-pixel tabular-nums">{par2(&e.score)}</td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
        .into_any()
}

/// A finished season: when, its seed, the downloads and the final board.
#[component]
fn PastSeason(season: DojoSeason) -> impl IntoView {
    let i18n = use_i18n();
    let n = season.season;
    let link = "inline-flex h-9 items-center rounded-md border bg-card px-3 text-sm font-medium hover:bg-accent";
    let dates = move || {
        let locale = i18n.get_locale();
        let day = |ms: Option<i64>| {
            ms.map(|ms| date_short(locale, day_of(ms)))
                .unwrap_or_default()
        };
        td_string!(
            locale,
            dojo.seasons.dates,
            start = day(season.started_at),
            end = day(season.ended_at)
        )
    };
    let set = season.set_size.map(|size| {
        let sha = season.set_sha256.clone().unwrap_or_default();
        view! {
            <a href=format!("/dojo/seasons/{n}/set.tar.gz") download class=link data-tooltip=format!("SHA-256 {sha}")>
                {t!(i18n, dojo.seasons.set, size = format!("{:.1}", megabytes(size)))}
            </a>
        }
    });
    let no_set = season.set_size.is_none();
    // The held-out family, a secret until now
    let mystery = season.mystery.map(|f| {
        view! {
            <div class="flex gap-3" data-testid="mystery-family">
                <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-secondary font-pixel text-lg">
                    {family_kanji(f)}
                </span>
                <div class="text-sm">
                    <p class="font-bold">
                        {move || {
                            let locale = i18n.get_locale();
                            td_string!(locale, dojo.seasons.mystery, family = family_name(locale, f))
                        }}
                    </p>
                    <p class="text-muted-foreground">{move || family_text(i18n.get_locale(), f)}</p>
                </div>
            </div>
        }
    });
    view! {
        <Card class="gap-4" attr:id=format!("season-{n}")>
            <CardHeader>
                <CardTitle class="flex flex-wrap items-baseline gap-x-3 text-xl">
                    {t!(i18n, dojo.seasons.past, season = n)}
                    <span class="text-sm font-normal text-muted-foreground">{dates}</span>
                </CardTitle>
                <CardDescription>
                    {t!(i18n, dojo.seasons.entries, count = season.entries)} " · "
                    {t!(i18n, dojo.seasons.versions, generator = season.generator_version, rules = season.rules_version)}
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
                    <Hash label=t!(i18n, dojo.seasons.commitment).into_any() value=season.commitment.clone() />
                    <Hash label=t!(i18n, dojo.seasons.seed).into_any() value=season.seed.clone().unwrap_or_default() />
                </div>
                {mystery}
                <div class="flex flex-wrap gap-2">
                    {set}
                    <a href=format!("/dojo/seasons/{n}/results.csv") download class=link>
                        {t!(i18n, dojo.seasons.csv)}
                    </a>
                    <a href=format!("/dojo/seasons/{n}/results.json") download class=link>
                        {t!(i18n, dojo.seasons.json)}
                    </a>
                </div>
                {no_set.then(|| view! { <p class="text-sm text-muted-foreground">{t!(i18n, dojo.seasons.no_set)}</p> })}
                <h3 class="font-bold">{t!(i18n, dojo.seasons.board)}</h3>
                <FrozenBoard board=season.board />
            </CardContent>
        </Card>
    }
}

#[component]
pub fn DojoSeasonsPage() -> impl IntoView {
    let i18n = use_i18n();
    let back = use_localized("/dojo");
    let seasons = RwSignal::new(None::<DojoSeasons>);
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            spawn_local(async move {
                if let Ok(s) = load_dojo_seasons().await {
                    seasons.set(Some(s));
                }
            });
        }
    });
    view! {
        <PageMeta
            title=Signal::derive(move || t_string!(i18n, dojo.seasons.meta_title).to_string())
            description=Signal::derive(move || t_string!(i18n, dojo.seasons.meta_description).to_string())
            path="/dojo/seasons"
        />
        <section class="mx-auto flex max-w-4xl flex-col gap-6 px-4 py-6 sm:px-6">
            <A href=back attr:class="text-sm text-primary hover:underline">
                "← " {t!(i18n, dojo.seasons.back)}
            </A>
            <div class="flex flex-col gap-3">
                <p class="font-pixel text-sm text-primary">"季 · kisetsu"</p>
                <h1 class="text-4xl font-black">{t!(i18n, dojo.seasons.title)}</h1>
                <p class="max-w-3xl text-muted-foreground">{t!(i18n, dojo.seasons.intro)}</p>
            </div>
            {move || match seasons.get() {
                None => view! { <p class="py-8 text-center text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                Some(s) => {
                    let current = s.current;
                    let n = current.season;
                    let started = current.started_at;
                    view! {
                        <Card class="gap-3" attr:id="current-season">
                            <CardHeader>
                                <CardTitle class="flex flex-wrap items-baseline gap-x-3 text-xl">
                                    {t!(i18n, dojo.seasons.current, season = n)}
                                    {started
                                        .map(|ms| {
                                            view! {
                                                <span class="text-sm font-normal text-muted-foreground">
                                                    {move || td_string!(i18n.get_locale(), dojo.seasons.started, date = date_short(i18n.get_locale(), day_of(ms)))}
                                                </span>
                                            }
                                        })}
                                </CardTitle>
                                <CardDescription>
                                    {t!(i18n, dojo.seasons.versions, generator = current.generator_version, rules = current.rules_version)}
                                </CardDescription>
                            </CardHeader>
                            <CardContent class="flex flex-col gap-2">
                                <Hash label=t!(i18n, dojo.seasons.commitment).into_any() value=current.commitment.clone() />
                                <p class="text-sm text-muted-foreground">{t!(i18n, dojo.seasons.sealed)}</p>
                            </CardContent>
                        </Card>
                        {if s.past.is_empty() {
                            view! {
                                <div class="seigaiha grid h-32 place-items-center rounded-lg border border-dashed text-muted-foreground">
                                    <p class="rounded-md bg-card px-3 py-1">{t!(i18n, dojo.seasons.none, season = n)}</p>
                                </div>
                            }
                                .into_any()
                        } else {
                            s.past.into_iter().map(|season| view! { <PastSeason season /> }).collect_view().into_any()
                        }}
                    }
                        .into_any()
                }
            }}
            <Card class="gap-3" attr:id="verify">
                <CardHeader>
                    <CardTitle class="flex items-baseline gap-2 text-xl">
                        {t!(i18n, dojo.seasons.verify_title)} " " <span class="font-pixel text-sm text-primary">"検 · ken"</span>
                    </CardTitle>
                </CardHeader>
                <CardContent class="flex flex-col gap-3 text-sm text-muted-foreground">
                    <p>{t!(i18n, dojo.seasons.verify)}</p>
                    <Code>
                        "$ printf %s <seed> | sha256sum        # = commitment\n$ dojo export regenerated/ --seed <seed> --hidden\n$ dojo run my-solver.wasm --set dojo-season-<n>.tar.gz"
                    </Code>
                    <p class="flex flex-wrap gap-x-4 gap-y-1">
                        <a class="text-primary underline underline-offset-4" href="/dojo/seasons.json">"/dojo/seasons.json"</a>
                    </p>
                </CardContent>
            </Card>
        </section>
    }
}
