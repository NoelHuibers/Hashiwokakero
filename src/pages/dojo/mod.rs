//! The SAT Dojo: a leaderboard for SAT solvers submitted as WebAssembly.

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
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::model::{DojoBoard, DojoMe};
use crate::seo::{PageMeta, REPO_URL, server_link, server_link_target};

pub use benchmarks::DojoBenchmarksPage;
use guide::{FairPlay, Families, HowItWorks, Metrics};
use leaderboard::Leaderboard;
pub use names::family_name;
pub use seasons::DojoSeasonsPage;
pub use submission::DojoSubmissionPage;
use submit::SubmitCard;
use tracks::{IndustrialTrack, SprintLine};

const STARTER_URL: &str = "https://github.com/NoelHuibers/Hashiwokakero/tree/main/dojo/starter";
/// The dev set as `dojo export public/dojo-dev-2.tar.gz` writes it, committed in `public/`.
///
/// Regenerate it (and bump the name, so no cache keeps the old one) whenever
/// `dojo::GENERATOR_VERSION` or `dojo::DEV_SEED` changes.
pub const DEV_SET_URL: &str = "/dojo-dev-2.tar.gz";

#[component]
pub fn DojoPage() -> impl IntoView {
    let i18n = use_i18n();
    let lab = use_localized("/lab");
    let archive = use_localized("/dojo/seasons");
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
    let stat = |text: Signal<String>| {
        view! { <span class="rounded-full border bg-card px-3 py-1 text-xs font-semibold">{move || text.get()}</span> }
    };
    let board_stat = move |f: fn(&DojoBoard, Locale) -> String| {
        Signal::derive(move || {
            board.with(|b| {
                b.as_ref()
                    .map(|b| f(b, i18n.get_locale()))
                    .unwrap_or_default()
            })
        })
    };

    view! {
        <PageMeta
            title=Signal::derive(move || t_string!(i18n, dojo.meta.title).to_string())
            description=Signal::derive(move || t_string!(i18n, dojo.meta.description).to_string())
            path="/dojo"
        />
        <section class="mx-auto flex max-w-6xl flex-col gap-8 px-4 py-6 sm:px-6">
            <div class="flex flex-col gap-3">
                <p class="font-pixel text-sm text-primary">"道場 · dojo"</p>
                <h1 class="text-4xl font-black">{t!(i18n, dojo.title)}</h1>
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
                            data-tooltip=move || board.with(|b| b.as_ref().map(|b| format!("SHA-256 {}", b.commitment)).unwrap_or_default())
                        >
                            {move || {
                                board
                                    .with(|b| {
                                        b.as_ref()
                                            .map(|b| {
                                                td_string!(
                                                    i18n.get_locale(),
                                                    dojo.stats.commitment,
                                                    hash = b.commitment.chars().take(12).collect::<String>()
                                                )
                                            })
                                            .unwrap_or_default()
                                    })
                            }}
                        </span>
                    </A>
                </div>
                <div class="flex flex-wrap gap-2 pt-1">
                    <a href="#submit" class="inline-flex h-10 items-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground shadow-sm">
                        {t!(i18n, dojo.cta.submit)}
                    </a>
                    <a href=STARTER_URL target="_blank" rel="noopener" class="inline-flex h-10 items-center rounded-md border bg-card px-4 text-sm font-medium">
                        {t!(i18n, dojo.cta.starter)}
                    </a>
                    <a href=server_link(DEV_SET_URL) target=server_link_target() download class="inline-flex h-10 items-center rounded-md border bg-card px-4 text-sm font-medium">
                        {t!(i18n, dojo.cta.dev_set)}
                    </a>
                    <A href=archive attr:class="inline-flex h-10 items-center rounded-md border bg-card px-4 text-sm font-medium">
                        {t!(i18n, dojo.cta.seasons)}
                    </A>
                    <A href=lab attr:class="inline-flex h-10 items-center rounded-md px-4 text-sm font-medium text-muted-foreground hover:text-foreground">
                        "研究室 · " {t!(i18n, common.nav.lab)}
                    </A>
                </div>
            </div>
            <Leaderboard board />
            <HowItWorks />
            <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
                <Families board />
                <SubmitCard me />
            </div>
            <IndustrialTrack />
            <FairPlay />
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
        </section>
    }
}
