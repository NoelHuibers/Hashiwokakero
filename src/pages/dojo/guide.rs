//! The Dojo's explanations: how to take part, the formula families and the fair play rules.

use dojo::Family;
use leptos::prelude::*;

use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{t, use_i18n};

use super::names::{family_kanji, family_name, family_text};

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
            </h2>
            <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
                <Step number="一" title=t!(i18n, dojo.how.build_title).into_any()>
                    <p>{t!(i18n, dojo.how.build)}</p>
                    <Code>"$ cargo build --release --target wasm32-wasip1\n\nstdin   p cnf 3 2\n        1 -2 0\n        2 3 0\nstdout  s SATISFIABLE\n        v 1 2 -3 0"</Code>
                </Step>
                <Step number="二" title=t!(i18n, dojo.how.train_title).into_any()>
                    <p>{t!(i18n, dojo.how.train)}</p>
                    <Code>"$ cargo install --git https://github.com/\n    NoelHuibers/Hashiwokakero dojo --features worker\n$ dojo run my-solver.wasm\n\nsolved 26/34  PAR-2 4.855 Gfuel"</Code>
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
pub(super) fn Families() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="text-xl">{t!(i18n, dojo.family.title)}</CardTitle>
            </CardHeader>
            <CardContent class="grid grid-cols-1 gap-4 sm:grid-cols-2">
                {Family::ALL
                    .into_iter()
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
                {item("秘", t!(i18n, dojo.fair.hidden_title).into_any(), t!(i18n, dojo.fair.hidden).into_any())}
                {item("検", t!(i18n, dojo.fair.checked_title).into_any(), t!(i18n, dojo.fair.checked).into_any())}
                {item("封", t!(i18n, dojo.fair.sandbox_title).into_any(), t!(i18n, dojo.fair.sandbox).into_any())}
                {item("一", t!(i18n, dojo.fair.accounts_title).into_any(), t!(i18n, dojo.fair.accounts).into_any())}
                {item("写", t!(i18n, dojo.fair.copies_title).into_any(), t!(i18n, dojo.fair.copies).into_any())}
                {item("季", t!(i18n, dojo.fair.seasons_title).into_any(), t!(i18n, dojo.fair.seasons).into_any())}
                {item("印", t!(i18n, dojo.fair.commit_title).into_any(), t!(i18n, dojo.fair.commit).into_any())}
            </CardContent>
        </Card>
    }
}
