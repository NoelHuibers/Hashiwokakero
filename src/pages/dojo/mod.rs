//! The SAT Dojo: a leaderboard for SAT solvers submitted as WebAssembly.
//!
//! Its pages are tabs of the Lab (`pages::lab`): the leaderboard, how to submit, the community
//! benchmarks, the season archive, and a page per solver.

pub mod benchmarks;
pub mod budgets;
pub mod cactus;
pub mod guide;
pub mod leaderboard;
pub mod names;
pub mod seasons;
pub mod submission;
pub mod submit;
pub mod tracks;
pub mod views;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::{load_dojo_board, load_dojo_me};
use crate::components::layout::use_localized;
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{Locale, t, td_string, use_i18n};
use crate::lang::localized;
use crate::model::{DojoBoard, DojoMe};
use crate::seo::{REPO_URL, server_link, server_link_target};

pub use benchmarks::BenchmarksTab;
use guide::{FairPlay, Families, HowItWorks, Metrics};
use leaderboard::Leaderboard;
pub use names::family_name;
pub use seasons::SeasonsTab;
pub use submission::SolverDetail;
use submit::{MySubmissions, SubmitCard};
use tracks::{IndustrialTrack, SprintLine};

const STARTER_URL: &str = "https://github.com/NoelHuibers/Hashiwokakero/tree/main/dojo/starter";
/// The dev set as `dojo export public/dojo-dev-2.tar.gz` writes it, committed in `public/`.
///
/// Regenerate it (and bump the name, so no cache keeps the old one) whenever
/// `dojo::GENERATOR_VERSION` or `dojo::DEV_SEED` changes.
pub const DEV_SET_URL: &str = "/dojo-dev-2.tar.gz";

/// The page of the submitted solver `id` in `locale` (a page of the Lab since the Dojo's pages
/// are its tabs; `/dojo/<id>` redirects there, `seo::MOVED_PAGES`).
pub fn solver_href(locale: Locale, id: i64) -> String {
    localized(locale, &format!("/lab/solver/{id}"))
}

/// The leaderboard and the visitor's own submissions, loaded once the page is in the browser.
fn load_board_and_me() -> (RwSignal<Option<DojoBoard>>, RwSignal<Option<DojoMe>>) {
    let board = RwSignal::new(None::<DojoBoard>);
    let me = RwSignal::new(None::<DojoMe>);
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            spawn_local(async move {
                if let Ok(b) = load_dojo_board().await {
                    board.set(Some(b));
                }
            });
            spawn_local(async move {
                if let Ok(m) = load_dojo_me().await {
                    me.set(Some(m));
                }
            });
        }
    });
    (board, me)
}

/// The starter kit and the dev set, to begin with.
#[component]
fn StartLinks() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <a href=STARTER_URL target="_blank" rel="noopener" class="inline-flex h-10 items-center rounded-md border bg-card px-4 text-sm font-medium">
            {t!(i18n, dojo.cta.starter)}
        </a>
        <a href=server_link(DEV_SET_URL) target=server_link_target() download class="inline-flex h-10 items-center rounded-md border bg-card px-4 text-sm font-medium">
            {t!(i18n, dojo.cta.dev_set)}
        </a>
    }
}

/// The Leaderboard tab: the season, the leaderboard, the visitor's solvers, the tracks and how
/// the board scores.
#[component]
pub fn LeaderboardTab() -> impl IntoView {
    let i18n = use_i18n();
    let archive = use_localized("/lab/seasons");
    let submit = use_localized("/lab/submit");
    let (board, me) = load_board_and_me();
    let stat = |text: Signal<String>| {
        view! { <span class="rounded-full border bg-card px-3 py-1 text-xs font-semibold">{move || text.get()}</span> }
    };
    let board_stat = move |f: fn(&DojoBoard, Locale) -> String| {
        Signal::derive(move || {
            board.with(|b| b.as_ref().map_or_default(|b| f(b, i18n.get_locale())))
        })
    };
    // Shown once there is something of the visitor's to show
    let mine = move || {
        me.get()
            .filter(|m| m.registered && !m.submissions.is_empty())
            .map(|m| {
                view! {
                    <Card class="gap-2" attr:id="mine">
                        <CardContent>
                            <MySubmissions me=m class="" />
                        </CardContent>
                    </Card>
                }
            })
    };

    view! {
        <div class="flex flex-col gap-3">
            <h2 class="flex flex-wrap items-baseline gap-x-3 text-2xl font-black">
                {t!(i18n, dojo.title)} <span class="font-pixel text-sm text-primary">"道場 · dojo"</span>
            </h2>
            <SprintLine />
            <p class="max-w-3xl text-muted-foreground">{t!(i18n, dojo.intro)}</p>
            <div class="flex flex-wrap gap-2">
                {stat(board_stat(|b, l| td_string!(l, dojo.stats.season, season = b.season)))}
                {stat(board_stat(|b, l| td_string!(l, dojo.stats.formulas, count = b.formulas)))}
                {stat(board_stat(|b, l| td_string!(l, dojo.stats.budget, budget = b.budget / 1_000_000_000)))}
                {stat(board_stat(|b, l| td_string!(l, dojo.stats.queued, count = b.queued)))}
                // The commitment to the hidden set, its seed is revealed in the archive
                <A href=archive attr:class="rounded-full border bg-card px-3 py-1 text-xs font-semibold hover:border-primary">
                    <span
                        dir="ltr"
                        data-tooltip=move || board.with(|b| b.as_ref().map_or_default(|b| format!("SHA-256 {}", b.commitment)))
                    >
                        {move || {
                            board
                                .with(|b| {
                                    b.as_ref()
                                        .map_or_default(|b| {
                                            td_string!(
                                                i18n.get_locale(),
                                                dojo.stats.commitment,
                                                hash = b.commitment.chars().take(12).collect::<String>()
                                            )
                                        })
                                })
                        }}
                    </span>
                </A>
            </div>
            <div class="flex flex-wrap gap-2 pt-1">
                <A href=submit attr:class="inline-flex h-10 items-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground shadow-sm">
                    {t!(i18n, dojo.cta.submit)}
                </A>
                <StartLinks />
            </div>
        </div>
        <Leaderboard board />
        {mine}
        <IndustrialTrack />
        <Metrics />
        <Card class="gap-2">
            <CardHeader>
                <CardTitle class="text-xl">{t!(i18n, dojo.science.title)}</CardTitle>
            </CardHeader>
            <CardContent class="flex flex-col gap-2 text-sm text-muted-foreground">
                <p>{t!(i18n, dojo.science.text)}</p>
                <p class="flex flex-wrap gap-x-4 gap-y-1">
                    <a class="text-primary underline underline-offset-4" href=server_link("/dojo/board.json") target=server_link_target()>"/dojo/board.json"</a>
                    // Breaks within the address on a phone rather than past the screen
                    <a class="text-primary underline underline-offset-4 [overflow-wrap:anywhere]" href=format!("{REPO_URL}/tree/main/dojo") target="_blank" rel="noopener">
                        "github.com/NoelHuibers/Hashiwokakero/dojo"
                    </a>
                </p>
            </CardContent>
        </Card>
    }
}

/// The Submit tab: how to build, train and submit a solver, the formulas it meets, the form and
/// the fair play rules.
#[component]
pub fn SubmitTab() -> impl IntoView {
    let i18n = use_i18n();
    let (board, me) = load_board_and_me();
    view! {
        <div class="flex flex-wrap gap-2">
            <a href="#submit" class="inline-flex h-10 items-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground shadow-sm">
                {t!(i18n, dojo.cta.submit)}
            </a>
            <StartLinks />
        </div>
        <HowItWorks />
        <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
            <SubmitCard me />
            <Families board />
        </div>
        <FairPlay />
    }
}
