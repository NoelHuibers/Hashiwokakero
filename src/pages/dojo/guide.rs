//! The Dojo's explanations: how to take part, the formula families and the fair play rules.

use dojo::Family;
use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::layout::use_localized;
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{t, use_i18n};
use crate::model::DojoBoard;

use super::names::{family_kanji, family_name, family_text};
use super::tracks::SprintBadge;

/// What the Dojo's proof rules follow: an UNSAT answer needs a proof a verified checker accepts.
const SAT_COMPETITION_RULES: &str = "https://satcompetition.github.io/2026/rules.html";

/// The SAT Competition 2026 results talk, where the organisers raised memorisation versus
/// generalisation (an AI-generated solver with family-specific sub-solvers): why hidden sets
/// have a held-out family.
const SAT_COMPETITION_2026_SLIDES: &str =
    "https://satcompetition.github.io/2026/downloads/satcomp26slides.pdf";

/// Fréchette, Kotthoff, Michalak, Rahwan, Hoos, Leyton-Brown: "Using the Shapley Value to
/// Analyze Algorithm Portfolios", AAAI 2016.
const SHAPLEY_PAPER: &str = "https://ojs.aaai.org/index.php/AAAI/article/view/10440";

/// The Sparkle SAT Challenge 2018, whose live leaderboard ranked solvers by their relative
/// marginal contribution to a portfolio.
const SPARKLE: &str = "https://ada.liacs.nl/events/sparkle-sat-18/";

/// CASC, the CADE ATP System Competition, which reports a state-of-the-art contribution.
const CASC: &str = "https://tptp.org/CASC/";

/// The calibration study of fuel against native time (method, data, the cost model).
const CALIBRATION: &str =
    "https://github.com/NoelHuibers/Hashiwokakero/blob/main/dojo/CALIBRATION.md";

#[component]
pub(super) fn Code(children: Children) -> impl IntoView {
    view! {
        <pre dir="ltr" class="terminal overflow-x-auto rounded-md px-3 py-2 font-pixel text-[12px] leading-relaxed">
            {children()}
        </pre>
    }
}

#[component]
fn Step(number: &'static str, title: AnyView, children: Children) -> impl IntoView {
    view! {
        <Card class="gap-3">
            <CardHeader>
                <CardTitle class="flex items-center gap-3 text-lg">
                    <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-primary font-pixel text-lg text-primary-foreground">
                        {number}
                    </span>
                    {title}
                </CardTitle>
            </CardHeader>
            <CardContent class="flex flex-col gap-3 text-sm text-muted-foreground">{children()}</CardContent>
        </Card>
    }
}

#[component]
pub(super) fn HowItWorks() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <section class="flex flex-col gap-4">
            <h2 class="flex items-baseline gap-2 text-2xl font-black">
                {t!(i18n, dojo.how.title)} " " <span class="font-pixel text-sm text-primary">"稽古 · keiko"</span>
                <SprintBadge />
            </h2>
            <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
                <Step number="一" title=t!(i18n, dojo.how.build_title).into_any()>
                    <p>{t!(i18n, dojo.how.build)}</p>
                    <Code>"$ cargo build --release --target wasm32-wasip1\n\nstdin   p cnf 3 2\n        1 -2 0\n        2 3 0\nstdout  s SATISFIABLE\n        v 1 2 -3 0"</Code>
                    <p>{t!(i18n, dojo.how.proof)}</p>
                    <Code>"argv    solver /proof/proof.out\nstdin   p cnf 1 2\n        1 0\n        -1 0\nstdout  s UNSATISFIABLE\nproof   0"</Code>
                </Step>
                <Step number="二" title=t!(i18n, dojo.how.train_title).into_any()>
                    <p>{t!(i18n, dojo.how.train)}</p>
                    <Code>"$ cargo install --git https://github.com/\n    NoelHuibers/Hashiwokakero dojo --features worker\n$ dojo run my-solver.wasm\n\nsolved 33/36  PAR-2 2.658 Gfuel"</Code>
                </Step>
                <Step number="三" title=t!(i18n, dojo.how.submit_title).into_any()>
                    <p>{t!(i18n, dojo.how.submit)}</p>
                </Step>
                <Step number="四" title=t!(i18n, dojo.how.climb_title).into_any()>
                    <p>{t!(i18n, dojo.how.climb)}</p>
                </Step>
            </div>
        </section>
    }
}

#[component]
pub(super) fn Families(board: RwSignal<Option<DojoBoard>>) -> impl IntoView {
    let i18n = use_i18n();
    let benchmarks = use_localized("/dojo/benchmarks");
    // Who contributed to the community pool
    let credits = move || {
        board.with(|b| {
            b.as_ref()
                .filter(|b| !b.community.authors.is_empty())
                .map(|b| b.community.authors.join(", "))
        })
    };
    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="text-xl">{t!(i18n, dojo.family.title)}</CardTitle>
            </CardHeader>
            <CardContent class="grid grid-cols-1 gap-4 sm:grid-cols-2">
                // The dev families, and the held-out one that only hidden sets have
                {Family::DEV
                    .into_iter()
                    .chain([Family::Mystery])
                    .map(|f| {
                        view! {
                            <div class="flex gap-3">
                                <span class="grid size-10 shrink-0 place-items-center rounded-lg bg-secondary font-pixel text-xl">
                                    {family_kanji(f)}
                                </span>
                                <div>
                                    <h3 class="font-bold">{move || family_name(i18n.get_locale(), f)}</h3>
                                    <p class="text-sm text-muted-foreground">{move || family_text(i18n.get_locale(), f)}</p>
                                </div>
                            </div>
                        }
                    })
                    .collect_view()}
                <div class="flex gap-3 sm:col-span-2" data-testid="community-family">
                    <span class="grid size-10 shrink-0 place-items-center rounded-lg bg-secondary font-pixel text-xl">
                        {family_kanji(Family::Community)}
                    </span>
                    <div class="flex flex-col gap-1">
                        <h3 class="font-bold">{move || family_name(i18n.get_locale(), Family::Community)}</h3>
                        <p class="text-sm text-muted-foreground">{move || family_text(i18n.get_locale(), Family::Community)}</p>
                        {move || {
                            credits()
                                .map(|names| {
                                    view! {
                                        <p class="text-sm">
                                            <span class="font-semibold">{t!(i18n, dojo.community.thanks)}</span>
                                            " "
                                            {names}
                                        </p>
                                    }
                                })
                        }}
                        <A href=benchmarks attr:class="w-fit text-sm text-primary underline underline-offset-4">
                            {t!(i18n, dojo.community.cta)}
                        </A>
                    </div>
                </div>
            </CardContent>
        </Card>
    }
}

#[component]
pub(super) fn FairPlay() -> impl IntoView {
    let i18n = use_i18n();
    let item = |kanji: &'static str, title: AnyView, text: AnyView| {
        view! {
            <div class="flex gap-3">
                <span class="hanko grid size-9 shrink-0 rotate-[-4deg] place-items-center rounded-md font-pixel text-lg">
                    {kanji}
                </span>
                <div>
                    <h3 class="font-bold">{title}</h3>
                    <p class="text-sm text-muted-foreground">{text}</p>
                </div>
            </div>
        }
    };
    view! {
        <Card class="gap-4" attr:id="fair-play">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, dojo.fair.title)} " " <span class="font-pixel text-sm text-primary">"礼 · rei"</span>
                </CardTitle>
            </CardHeader>
            <CardContent class="grid grid-cols-1 gap-5 md:grid-cols-2">
                {item("燃", t!(i18n, dojo.fair.fuel_title).into_any(), t!(i18n, dojo.fair.fuel).into_any())}
                {item(
                    "測",
                    t!(i18n, dojo.fair.calibrated_title).into_any(),
                    t!(
                        i18n,
                        dojo.fair.calibrated,
                        <a> = |children: ChildrenFn| view! {
                            <a href=CALIBRATION target="_blank" rel="noopener" class="text-primary underline underline-offset-4">
                                {children()}
                            </a>
                        }
                    )
                        .into_any(),
                )}
                {item("秘", t!(i18n, dojo.fair.hidden_title).into_any(), t!(i18n, dojo.fair.hidden).into_any())}
                {item(
                    "検",
                    t!(i18n, dojo.fair.checked_title).into_any(),
                    t!(
                        i18n,
                        dojo.fair.checked,
                        <a> = |children: ChildrenFn| view! {
                            <a href=SAT_COMPETITION_RULES target="_blank" rel="noopener" class="text-primary underline underline-offset-4">
                                {children()}
                            </a>
                        }
                    )
                        .into_any(),
                )}
                {item(
                    "謎",
                    t!(i18n, dojo.fair.heldout_title).into_any(),
                    t!(
                        i18n,
                        dojo.fair.heldout,
                        <a> = |children: ChildrenFn| view! {
                            <a href=SAT_COMPETITION_2026_SLIDES target="_blank" rel="noopener" class="text-primary underline underline-offset-4">
                                {children()}
                            </a>
                        }
                    )
                        .into_any(),
                )}
                {item("封", t!(i18n, dojo.fair.sandbox_title).into_any(), t!(i18n, dojo.fair.sandbox).into_any())}
                {item("一", t!(i18n, dojo.fair.accounts_title).into_any(), t!(i18n, dojo.fair.accounts).into_any())}
                {item("写", t!(i18n, dojo.fair.copies_title).into_any(), t!(i18n, dojo.fair.copies).into_any())}
                {item("季", t!(i18n, dojo.fair.seasons_title).into_any(), t!(i18n, dojo.fair.seasons).into_any())}
                {item("印", t!(i18n, dojo.fair.commit_title).into_any(), t!(i18n, dojo.fair.commit).into_any())}
            </CardContent>
        </Card>
    }
}

/// A link in a guide text.
pub(super) fn cite(href: &'static str) -> impl Fn(ChildrenFn) -> AnyView + Copy {
    move |children: ChildrenFn| {
        view! {
            <a href=href target="_blank" rel="noopener" class="text-primary underline underline-offset-4">
                {children()}
            </a>
        }
        .into_any()
    }
}

/// The metrics beyond PAR-2: SAT/UNSAT/ALL, cactus plots and the contributions, with sources.
#[component]
pub(super) fn Metrics() -> impl IntoView {
    let i18n = use_i18n();
    let item = |kanji: &'static str, title: AnyView, text: AnyView| {
        view! {
            <div class="flex gap-3">
                <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-secondary font-pixel text-lg">{kanji}</span>
                <div>
                    <h3 class="font-bold">{title}</h3>
                    <p class="text-sm text-muted-foreground">{text}</p>
                </div>
            </div>
        }
    };
    view! {
        <Card class="gap-4" attr:id="metrics">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, dojo.metrics.title)} " " <span class="font-pixel text-sm text-primary">"評 · hyō"</span>
                </CardTitle>
            </CardHeader>
            <CardContent class="flex flex-col gap-5">
                <p class="text-sm text-muted-foreground">{t!(i18n, dojo.metrics.intro)}</p>
                <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
                    {item(
                        "答",
                        t!(i18n, dojo.metrics.answers_title).into_any(),
                        t!(i18n, dojo.metrics.answers, <a> = cite(SAT_COMPETITION_2026_SLIDES)).into_any(),
                    )}
                    {item("仙", t!(i18n, dojo.metrics.cactus_title).into_any(), t!(i18n, dojo.metrics.cactus).into_any())}
                    {item(
                        "差",
                        t!(i18n, dojo.metrics.marginal_title).into_any(),
                        t!(i18n, dojo.metrics.marginal, <a> = cite(SPARKLE)).into_any(),
                    )}
                    {item("先", t!(i18n, dojo.metrics.sota_title).into_any(), t!(i18n, dojo.metrics.sota, <a> = cite(CASC)).into_any())}
                    {item(
                        "分",
                        t!(i18n, dojo.metrics.shapley_title).into_any(),
                        t!(i18n, dojo.metrics.shapley, <a> = cite(SHAPLEY_PAPER)).into_any(),
                    )}
                </div>
            </CardContent>
        </Card>
    }
}
