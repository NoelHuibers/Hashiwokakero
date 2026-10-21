//! The text about Hashiwokakero below the daily, for visitors from a search engine.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::layout::use_localized;
use crate::components::rules::Rules;
use crate::components::tips::TipCards;
use crate::i18n::{t, use_i18n};
use crate::model::{CHECK_PENALTY_MS, HINT_PENALTY_MS};

/// How many of the tips the home page shows, the rules page has them all.
const TIPS: usize = 3;

/// `+10 s`, kept left to right in right-to-left text (isolated, U+2066 … U+2069), where it
/// would read `s +10`.
pub fn penalty(ms: i64) -> String {
    format!("\u{2066}+{} s\u{2069}", ms / 1000)
}

/// A heading of the section with its kanji.
#[component]
fn Heading(jp: &'static str, children: Children) -> impl IntoView {
    view! {
        <h3 class="flex items-baseline gap-2 text-xl font-black sm:text-2xl">
            {children()}
            <span class="font-pixel text-sm font-normal text-primary" aria-hidden="true">
                {jp}
            </span>
        </h3>
    }
}

/// One of the facts about the daily.
#[component]
fn Fact(jp: &'static str, title: AnyView, children: Children) -> impl IntoView {
    view! {
        <div class="flex flex-col gap-1.5 rounded-xl border bg-card p-4 text-card-foreground shadow-sm">
            <h4 class="flex items-baseline justify-between gap-2 font-bold">
                {title}
                <span class="font-pixel text-sm font-normal text-primary" aria-hidden="true">
                    {jp}
                </span>
            </h4>
            <p class="text-sm leading-relaxed text-muted-foreground">{children()}</p>
        </div>
    }
}

/// A question that opens to its answer.
#[component]
fn Question(question: AnyView, children: Children) -> impl IntoView {
    view! {
        <details class="group border-t py-3 last:border-b">
            <summary class="flex cursor-pointer list-none items-center justify-between gap-4 font-bold [&::-webkit-details-marker]:hidden">
                {question}
                <span
                    class="font-pixel text-lg leading-none text-primary transition-transform group-open:rotate-45"
                    aria-hidden="true"
                >
                    "+"
                </span>
            </summary>
            <p class="mt-2 max-w-3xl text-sm leading-relaxed text-muted-foreground">{children()}</p>
        </details>
    }
}

/// What Hashiwokakero is, its rules, how the daily works and the first solving tips, for people
/// who arrive from a search engine (and for the search engine itself, the board is just an SVG
/// to it). The rules page goes deeper.
#[component]
pub(super) fn About() -> impl IntoView {
    const LINK: &str = "font-medium text-primary underline decoration-primary/40 underline-offset-4 hover:decoration-primary";
    let i18n = use_i18n();
    let rules = use_localized("/rules");
    let tips = move || format!("{}#tips", rules());
    let practice = use_localized("/practice");
    let create = use_localized("/create");
    view! {
        <article class="mx-auto flex max-w-7xl flex-col gap-12 px-4 pb-4 pt-10 sm:px-6">
            <div class="flex items-end gap-3">
                <span class="vertical-jp hidden font-pixel text-sm leading-none text-primary sm:block">
                    "橋をかけろ"
                </span>
                <div>
                    <p class="font-pixel text-sm text-primary">{t!(i18n, daily.about.kicker)}</p>
                    <h2 class="text-2xl font-black sm:text-3xl">{t!(i18n, daily.about.title)}</h2>
                    <p class="mt-2 max-w-3xl leading-relaxed text-muted-foreground">
                        {t!(i18n, daily.about.intro, <b> = <b class="font-semibold text-foreground" />)}
                    </p>
                </div>
            </div>

            <section class="flex flex-col gap-5">
                <Heading jp="遊び方">{t!(i18n, daily.about.how_title)}</Heading>
                <Rules nested=true class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-4" />
            </section>

            <section class="flex flex-col gap-5">
                <Heading jp="毎日">{t!(i18n, daily.about.daily_title)}</Heading>
                <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
                    <Fact jp="日" title=t!(i18n, daily.about.day_title).into_any()>
                        {t!(i18n, daily.about.day)}
                    </Fact>
                    <Fact jp="時間" title=t!(i18n, daily.about.clock_title).into_any()>
                        {t!(i18n, daily.about.clock)}
                    </Fact>
                    <Fact jp="罰" title=t!(i18n, daily.about.penalty_title).into_any()>
                        {t!(
                            i18n,
                            daily.about.penalty,
                            check = penalty(CHECK_PENALTY_MS),
                            hint = penalty(HINT_PENALTY_MS)
                        )}
                    </Fact>
                    <Fact jp="降参" title=t!(i18n, daily.about.give_up_title).into_any()>
                        {t!(i18n, daily.about.give_up)}
                    </Fact>
                </div>
            </section>

            <section class="flex flex-col gap-5">
                <div>
                    <Heading jp="コツ">{t!(i18n, daily.about.tips_title)}</Heading>
                    <p class="mt-1 max-w-3xl text-muted-foreground">
                        {t!(i18n, daily.about.tips_intro)}
                    </p>
                </div>
                <TipCards
                    count=TIPS
                    nested=true
                    class="grid grid-cols-1 gap-4 md:grid-cols-3"
                />
            </section>

            <section class="flex flex-col gap-5">
                <Heading jp="質問">{t!(i18n, daily.about.faq_title)}</Heading>
                <div>
                    <Question question=t!(i18n, rules.faq.unique_q).into_any()>
                        {t!(i18n, rules.faq.unique_a)}
                    </Question>
                    <Question question=t!(i18n, daily.about.account_q).into_any()>
                        {t!(i18n, daily.about.account_a)}
                    </Question>
                    <Question question=t!(i18n, daily.about.next_q).into_any()>
                        {t!(i18n, daily.about.next_a)}
                    </Question>
                    <Question question=t!(i18n, daily.about.more_q).into_any()>
                        {t!(
                            i18n,
                            daily.about.more_a,
                            <practice> = move |children: ChildrenFn| view! { <A href=practice attr:class=LINK>{children()}</A> },
                            <workshop> = move |children: ChildrenFn| view! { <A href=create attr:class=LINK>{children()}</A> }
                        )}
                    </Question>
                </div>
                <div class="flex flex-wrap gap-3">
                    <A
                        href=tips
                        attr:class="inline-flex min-h-10 items-center rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground shadow-xs hover:bg-primary/90"
                    >
                        {t!(i18n, daily.about.to_rules)}
                    </A>
                    <A
                        href=practice
                        attr:class="inline-flex min-h-10 items-center rounded-md border bg-background px-4 py-2 text-sm font-medium shadow-xs hover:bg-accent hover:text-accent-foreground dark:border-input dark:bg-input/30"
                    >
                        {t!(i18n, daily.about.to_practice)}
                    </A>
                </div>
            </section>
        </article>
    }
}
