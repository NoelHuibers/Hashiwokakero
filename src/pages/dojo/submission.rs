//! One Dojo submission: its state while it waits or runs, then its score or why it was rejected.

use std::time::Duration;

use dojo::metrics::{Breakdown, Contribution, EntryBreakdown, Split};
use dojo::{Rejection, Score};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::load_dojo_submission;
use crate::components::layout::use_localized;
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::card::Card;
use crate::components::ui::progress::Progress;
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::{date_short, prefix};
use crate::model::{DojoState, DojoSubmission, day_of};
use crate::seo::title_text;

use super::cactus::Cactus;
use super::names::{StateBadge, family_kanji, family_name, par2};

/// A file size in KiB, for display.
#[expect(
    clippy::cast_precision_loss,
    reason = "uploads are a few MiB at most, far below f64's 52-bit mantissa"
)]
fn kib(bytes: i64) -> f64 {
    bytes as f64 / 1024.0
}

fn rejection_text(locale: Locale, rejection: &Rejection) -> String {
    match rejection {
        Rejection::TooLarge => td_string!(locale, dojo.rejection.too_large).to_string(),
        Rejection::Invalid(detail) => {
            td_string!(locale, dojo.rejection.invalid, detail = detail.clone())
        }
        Rejection::Import(detail) => {
            td_string!(locale, dojo.rejection.import, detail = detail.clone())
        }
        Rejection::NoStart => td_string!(locale, dojo.rejection.no_start).to_string(),
    }
}

/// Solved and PAR-2 by answer: the whole set, then every family.
#[component]
fn AnswersTable(breakdown: EntryBreakdown) -> impl IntoView {
    let i18n = use_i18n();
    let cells = |split: Option<Split>| {
        // No formulas with that answer: nothing to show, not a PAR-2 of 0
        let split = split.filter(|s| s.total > 0);
        let solved = split.map_or_else(|| "—".to_string(), |s| format!("{}/{}", s.solved, s.total));
        let par2 = split.map_or_else(|| "—".to_string(), |s| format!("{:.3}", s.par2));
        view! {
            <td class="py-2 ps-3 text-end font-pixel tabular-nums">{solved}</td>
            <td class="py-2 ps-2 text-end font-pixel tabular-nums text-muted-foreground">{par2}</td>
        }
    };
    let row = move |label: AnyView, b: &Breakdown, strong: bool| {
        view! {
            <tr class=if strong { "border-b font-semibold" } else { "border-b last:border-0" }>
                <td class="py-2 pe-3">{label}</td>
                {cells(b.sat)}
                {cells(b.unsat)}
                {cells(Some(b.all))}
            </tr>
        }
    };
    let held_out = breakdown.held_out;
    view! {
        <div class="flex flex-col gap-2" data-testid="answers">
            <h2 class="font-bold">{t!(i18n, dojo.detail.answers)}</h2>
            <div class="overflow-x-auto">
                <table class="w-full text-sm">
                    <thead>
                        <tr class="border-b text-xs uppercase tracking-wider text-muted-foreground">
                            <th class="py-2 pe-3 text-start font-semibold">{t!(i18n, dojo.detail.family)}</th>
                            <th colspan="2" class="py-2 ps-3 text-end font-semibold">"SAT"</th>
                            <th colspan="2" class="py-2 ps-3 text-end font-semibold">"UNSAT"</th>
                            <th colspan="2" class="py-2 ps-3 text-end font-semibold">{t!(i18n, dojo.board.all)}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {row(t!(i18n, dojo.detail.total).into_any(), &breakdown.total, true)}
                        {breakdown
                            .families
                            .iter()
                            .map(|f| {
                                let family = f.family;
                                let label = view! {
                                    <span class="font-pixel text-primary">{family_kanji(family)}</span>
                                    " "
                                    {move || family_name(i18n.get_locale(), family)}
                                }
                                    .into_any();
                                row(label, &f.breakdown, false)
                            })
                            .collect_view()}
                    </tbody>
                </table>
            </div>
            <p class="text-xs text-muted-foreground">{t!(i18n, dojo.detail.par2_note)}</p>
            {(held_out > 0)
                .then(|| view! { <p class="text-xs text-muted-foreground">{t!(i18n, dojo.detail.held_out, count = held_out)}</p> })}
        </div>
    }
}

/// What the solver adds to the ranked ones, the four measures side by side.
#[component]
fn ContributionTiles(contribution: Contribution) -> impl IntoView {
    let i18n = use_i18n();
    let tile = |label: AnyView, value: String, tip: String| {
        view! {
            <div class="rounded-lg border bg-card px-3 py-2" data-tooltip=tip>
                <p class="text-xs font-bold uppercase tracking-wider text-muted-foreground">{label}</p>
                <p class="font-pixel text-xl">{value}</p>
            </div>
        }
    };
    let c = contribution;
    view! {
        <div class="flex flex-col gap-2" data-testid="contribution">
            <h2 class="font-bold">{t!(i18n, dojo.detail.contribution)}</h2>
            <div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
                {tile(t!(i18n, dojo.board.marginal).into_any(), format!("{:.3}", c.marginal), t_string!(i18n, dojo.board.marginal_tip).to_string())}
                {tile(t!(i18n, dojo.board.relative).into_any(), format!("{:.1}%", c.relative * 100.0), t_string!(i18n, dojo.board.relative_tip).to_string())}
                {tile(t!(i18n, dojo.board.sota).into_any(), format!("{:.2}", c.sota), t_string!(i18n, dojo.board.sota_tip).to_string())}
                {tile(t!(i18n, dojo.board.shapley).into_any(), format!("{:.3}", c.shapley), t_string!(i18n, dojo.board.shapley_tip).to_string())}
            </div>
        </div>
    }
}

#[component]
fn ScoreView(s: DojoSubmission, score: Score) -> impl IntoView {
    let i18n = use_i18n();
    let wrong = score.wrong;
    let unverified = score.unverified;
    view! {
        {(wrong > 0)
            .then(|| {
                view! {
                    <p class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                        {t!(i18n, dojo.detail.wrong, count = wrong)}
                    </p>
                }
            })}
        {(unverified > 0)
            .then(|| {
                view! {
                    <p class="rounded-md border border-warning/40 bg-warning-light px-3 py-2 text-sm text-warning-dark">
                        {t!(i18n, dojo.detail.unverified, count = unverified)}
                    </p>
                }
            })}
        <div class="grid grid-cols-3 gap-3">
            <div class="rounded-lg border bg-card px-3 py-2">
                <p class="text-xs font-bold uppercase tracking-wider text-muted-foreground">{t!(i18n, dojo.board.par2)}</p>
                <p class="font-pixel text-2xl tabular-nums" data-tooltip=move || format!("{}: {:.1} G", t_string!(i18n, dojo.detail.fuel), score.fuel.max(0.0))>
                    {par2(&score)}
                </p>
            </div>
            <div class="rounded-lg border bg-card px-3 py-2">
                <p class="text-xs font-bold uppercase tracking-wider text-muted-foreground">{t!(i18n, dojo.detail.solved)}</p>
                <p class="font-pixel text-2xl tabular-nums">{format!("{}/{}", score.solved, score.total)}</p>
            </div>
            <div class="rounded-lg border bg-card px-3 py-2">
                <p class="text-xs font-bold uppercase tracking-wider text-muted-foreground">{t!(i18n, dojo.detail.rank_label)}</p>
                <p class="font-pixel text-2xl tabular-nums">{s.rank.map_or_else(|| "—".to_string(), |r| format!("#{r}"))}</p>
            </div>
        </div>
        <div class="overflow-x-auto">
            <table class="w-full text-sm">
                <thead>
                    <tr class="border-b text-xs uppercase tracking-wider text-muted-foreground">
                        <th class="py-2 pe-3 text-start font-semibold">{t!(i18n, dojo.detail.family)}</th>
                        <th class="w-2/5 py-2 pe-3 text-start font-semibold">{t!(i18n, dojo.board.solved)}</th>
                        {(unverified > 0)
                            .then(|| view! { <th class="py-2 pe-3 text-end font-semibold">{t!(i18n, dojo.detail.unverified_col)}</th> })}
                        <th class="py-2 text-end font-semibold">{t!(i18n, dojo.board.par2)}</th>
                    </tr>
                </thead>
                <tbody>
                    {score
                        .families
                        .iter()
                        .map(|f| {
                            let family = f.family;
                            let share = if f.total == 0 { 0.0 } else { f64::from(f.solved) * 100.0 / f64::from(f.total) };
                            view! {
                                <tr class="border-b last:border-0">
                                    <td class="py-2.5 pe-3">
                                        <span class="font-pixel text-primary">{family_kanji(family)}</span>
                                        " "
                                        {move || family_name(i18n.get_locale(), family)}
                                    </td>
                                    <td class="py-2.5 pe-3">
                                        <div class="flex items-center gap-2">
                                            <Progress value=Signal::stored(share) class="h-2 flex-1" />
                                            <span class="w-12 text-end font-pixel tabular-nums">{format!("{}/{}", f.solved, f.total)}</span>
                                        </div>
                                    </td>
                                    {(unverified > 0)
                                        .then(|| {
                                            view! { <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{f.unverified}</td> }
                                        })}
                                    <td class="py-2.5 text-end font-pixel tabular-nums">{format!("{:.3}", f.par2)}</td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
        {s.breakdown.clone().map(|b| view! { <AnswersTable breakdown=b /> })}
        {match s.contribution {
            Some(c) => view! { <ContributionTiles contribution=c /> }.into_any(),
            None => view! { <p class="text-sm text-muted-foreground">{t!(i18n, dojo.detail.contribution_unranked)}</p> }.into_any(),
        }}
        {s.portfolio.clone().map(|p| view! { <Cactus cactus=p.cactus highlight=Some(s.id) /> })}
        {match s.similar_to {
            Some(original) => {
                let href = move || format!("{}/dojo/{original}", prefix(i18n.get_locale()));
                view! {
                    <p class="rounded-md border border-warning/40 bg-warning-light px-3 py-2 text-sm text-warning-dark">
                        {t!(
                            i18n,
                            dojo.detail.similar,
                            id = original,
                            <a> = move |children: ChildrenFn| view! {
                                <A href=href attr:class="font-semibold underline underline-offset-4">{children()}</A>
                            }
                        )}
                    </p>
                }
                    .into_any()
            }
            None if s.rank.is_none() && wrong == 0 => {
                view! { <p class="text-sm text-muted-foreground">{t!(i18n, dojo.detail.unranked)}</p> }.into_any()
            }
            None => ().into_any(),
        }}
    }
}

#[component]
pub fn DojoSubmissionPage() -> impl IntoView {
    let i18n = use_i18n();
    let params = use_params_map();
    let back = use_localized("/dojo");
    let id = move || params.with(|p| p.get("id").and_then(|id| id.parse::<i64>().ok()));
    let submission = RwSignal::new(None::<Option<DojoSubmission>>);
    let fetch = move || {
        let Some(id) = id() else {
            submission.set(Some(None));
            return;
        };
        spawn_local(async move {
            if let Ok(s) = load_dojo_submission(id).await {
                submission.set(Some(s));
            }
        });
    };
    Effect::new(move |_| {
        id();
        fetch();
    });
    // Poll while it waits or runs
    Effect::new(move |_| {
        let busy = submission.with(|s| {
            s.as_ref()
                .and_then(|s| s.as_ref())
                .is_some_and(|s| s.state.busy())
        });
        if busy && let Ok(handle) = set_interval_with_handle(fetch, Duration::from_secs(3)) {
            on_cleanup(move || handle.clear());
        }
    });

    let title = Signal::derive(move || {
        submission.with(|s| match s {
            Some(Some(s)) => td_string!(
                i18n.get_locale(),
                dojo.detail.meta_title,
                name = s.name.clone()
            ),
            _ => t_string!(i18n, dojo.meta.title).to_string(),
        })
    });
    view! {
        // One entry's page, not the Dojo's: no canonical link to the Dojo with its translations,
        // as if it were the same page, and kept out of search engines like players' pages
        <Title text=title_text(move || title.get()) />
        <Meta name="description" content=move || t_string!(i18n, dojo.meta.description) />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto flex max-w-3xl flex-col gap-5 px-4 py-6 sm:px-6">
            <A href=back attr:class="text-sm text-primary hover:underline">
                "← " {t!(i18n, dojo.detail.back)}
            </A>
            {move || match submission.get() {
                None => view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                Some(None) => view! { <p class="text-muted-foreground">{t!(i18n, dojo.detail.not_found)}</p> }.into_any(),
                Some(Some(s)) => {
                    let disqualified = s.score.as_ref().is_some_and(|sc| sc.wrong > 0);
                    let date = date_short(i18n.get_locale_untracked(), day_of(s.created_at));
                    let (done, total) = (s.progress, s.total);
                    view! {
                        <div class="flex flex-col gap-2">
                            <p class="font-pixel text-sm text-primary">{format!("提出 · teishutsu #{}", s.id)}</p>
                            <div class="flex flex-wrap items-center gap-3">
                                <h1 class="min-w-0 break-words text-3xl font-black">{s.name.clone()}</h1>
                                <StateBadge state=s.state disqualified />
                                {s.house.then(|| view! { <Badge variant=BadgeVariant::Secondary>{t!(i18n, dojo.board.house)}</Badge> })}
                            </div>
                            <p class="flex flex-wrap gap-x-3 gap-y-1 text-sm text-muted-foreground">
                                <span>{t!(i18n, dojo.detail.by, author = s.author.clone())}</span>
                                <span>{t!(i18n, dojo.detail.submitted, date)}</span>
                                <span>{t!(i18n, dojo.detail.season, season = s.season)}</span>
                                <span class="font-pixel" data-tooltip=s.sha256.clone()>
                                    {format!("{:.0} KiB · {}", kib(s.size), &s.sha256[..s.sha256.len().min(12)])}
                                </span>
                            </p>
                            {s.source_url
                                .clone()
                                .map(|url| {
                                    view! {
                                        <a href=url target="_blank" rel="noopener nofollow ugc" class="w-fit text-sm text-primary underline underline-offset-4">
                                            {t!(i18n, dojo.detail.source)} " ↗"
                                        </a>
                                    }
                                })}
                            {(!s.description.is_empty())
                                .then(|| view! { <p class="whitespace-pre-line text-muted-foreground">{s.description.clone()}</p> })}
                        </div>
                        <Card class="gap-4 px-5">
                            {match (s.state, s.score.clone(), s.rejection.clone()) {
                                (DojoState::Queued, _, _) => {
                                    view! {
                                        <p class="animate-pulse text-muted-foreground">{t!(i18n, dojo.detail.queued)} " 待機中"</p>
                                    }
                                        .into_any()
                                }
                                (DojoState::Running, _, _) => {
                                    view! {
                                        <p class="text-sm text-muted-foreground">{t!(i18n, dojo.detail.running, done, total)}</p>
                                        <Progress value=Signal::stored(f64::from(done)) max=f64::from(total.max(1)) class="h-3" />
                                    }
                                        .into_any()
                                }
                                (DojoState::Done, Some(score), _) => view! { <ScoreView s=s.clone() score /> }.into_any(),
                                (DojoState::Rejected, _, rejection) => {
                                    view! {
                                        <p class="font-bold">{t!(i18n, dojo.rejection.title)}</p>
                                        <p class="break-words text-sm text-muted-foreground">
                                            {move || rejection.as_ref().map(|r| rejection_text(i18n.get_locale(), r))}
                                        </p>
                                    }
                                        .into_any()
                                }
                                // A newer server's state: just the badge
                                (DojoState::Unknown, _, _) => ().into_any(),
                                _ => view! { <p class="text-muted-foreground">{t!(i18n, dojo.detail.failed)}</p> }.into_any(),
                            }}
                        </Card>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use dojo::Family;
    use dojo::metrics::FamilyBreakdown;

    use super::*;
    use crate::i18n::Locale;

    /// The split by answer and the contribution tiles, rendered on the server.
    #[tokio::test]
    async fn renders_answers_and_contributions() {
        _ = any_spawner::Executor::init_tokio();
        let split = |solved, total, par2| Split {
            solved,
            total,
            par2,
        };
        let breakdown = EntryBreakdown {
            total: Breakdown {
                all: split(3, 5, 4.25),
                sat: Some(split(2, 2, 1.5)),
                unsat: Some(split(0, 0, 0.0)),
                unverified: 0,
            },
            held_out: 3,
            families: vec![FamilyBreakdown {
                family: Family::Mystery,
                breakdown: Breakdown {
                    all: split(1, 3, 9.0),
                    sat: None,
                    unsat: None,
                    unverified: 0,
                },
            }],
        };
        let contribution = Contribution {
            marginal: 0.25,
            log_ratio: 0.1,
            relative: 0.375,
            sota: 2.5,
            shapley: 1.125,
        };
        let owner = Owner::new_root(None);
        let html = owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::en);
            view! {
                <AnswersTable breakdown />
                <ContributionTiles contribution />
            }
            .to_html()
        });
        for text in [
            "All families",
            "Mystery family",
            "2/2",
            "1.500",
            "3/5",
            "4.250",
            "37.5%",
            "2.50",
            "1.125",
            "0.250",
        ] {
            assert!(html.contains(text), "{text} missing in {html}");
        }
        // No UNSAT formulas: no PAR-2 of 0
        assert!(!html.contains("0.000"));
        assert!(html.contains("formulas of the mystery family"));
    }
}
