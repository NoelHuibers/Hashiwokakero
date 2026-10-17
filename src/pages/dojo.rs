//! The SAT Dojo: a leaderboard for SAT solvers submitted as WebAssembly.

use std::time::Duration;

use dojo::{Family, Rejection, Score};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::api::{load_dojo_board, load_dojo_me, load_dojo_submission};
use crate::components::layout::use_localized;
use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::button::Button;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::progress::Progress;
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::{date_short, prefix};
use crate::model::{DojoBoard, DojoMe, DojoState, DojoSubmission, day_of};
use crate::seo::{PageMeta, REPO_URL};

const STARTER_URL: &str = "https://github.com/NoelHuibers/Hashiwokakero/tree/main/dojo/starter";
/// Generated when the site is built (`dojo export`), see the Dockerfile.
pub const DEV_SET_URL: &str = "/dojo-dev-1.tar.gz";

const FIELD: &str = "flex w-full min-w-0 rounded-md border border-input bg-transparent px-3 py-2 text-base shadow-xs outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 md:text-sm";

pub fn family_name(locale: Locale, family: Family) -> String {
    match family {
        Family::Hashi => td_string!(locale, dojo.family.hashi),
        Family::HashiParity => td_string!(locale, dojo.family.hashi_parity),
        Family::Random3Sat => td_string!(locale, dojo.family.random_3sat),
        Family::Pigeonhole => td_string!(locale, dojo.family.pigeonhole),
        Family::Queens => td_string!(locale, dojo.family.queens),
    }
    .to_string()
}

fn family_text(locale: Locale, family: Family) -> String {
    match family {
        Family::Hashi => td_string!(locale, dojo.family.hashi_text),
        Family::HashiParity => td_string!(locale, dojo.family.hashi_parity_text),
        Family::Random3Sat => td_string!(locale, dojo.family.random_3sat_text),
        Family::Pigeonhole => td_string!(locale, dojo.family.pigeonhole_text),
        Family::Queens => td_string!(locale, dojo.family.queens_text),
    }
    .to_string()
}

fn family_kanji(family: Family) -> &'static str {
    match family {
        Family::Hashi => "橋",
        Family::HashiParity => "偶",
        Family::Random3Sat => "乱",
        Family::Pigeonhole => "鳩",
        Family::Queens => "女",
    }
}

fn state_name(locale: Locale, state: DojoState) -> String {
    match state {
        DojoState::Queued => td_string!(locale, dojo.state.queued),
        DojoState::Running => td_string!(locale, dojo.state.running),
        DojoState::Done => td_string!(locale, dojo.state.done),
        DojoState::Rejected => td_string!(locale, dojo.state.rejected),
        DojoState::Failed => td_string!(locale, dojo.state.failed),
    }
    .to_string()
}

#[component]
fn StateBadge(state: DojoState, #[prop(optional)] disqualified: bool) -> impl IntoView {
    let i18n = use_i18n();
    let variant = match state {
        _ if disqualified => BadgeVariant::Destructive,
        DojoState::Queued => BadgeVariant::Muted,
        DojoState::Running => BadgeVariant::Warning,
        DojoState::Done => BadgeVariant::Success,
        DojoState::Rejected | DojoState::Failed => BadgeVariant::Destructive,
    };
    view! {
        <Badge variant class=if state == DojoState::Running { "animate-pulse" } else { "" }>
            {move || {
                if disqualified {
                    t_string!(i18n, dojo.state.disqualified).to_string()
                } else {
                    state_name(i18n.get_locale(), state)
                }
            }}
        </Badge>
    }
}

fn par2(score: &Score) -> String {
    format!("{:.3}", score.par2)
}

/// A file size in KiB, for display.
#[expect(
    clippy::cast_precision_loss,
    reason = "uploads are a few MiB at most, far below f64's 52-bit mantissa"
)]
fn kib(bytes: i64) -> f64 {
    bytes as f64 / 1024.0
}

/// One small square per family, filled by the share of solved formulas.
#[component]
fn FamilyChips(score: Score) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="flex gap-1">
            {score
                .families
                .into_iter()
                .map(|f| {
                    let share = if f.total == 0 { 0.0 } else { f64::from(f.solved) / f64::from(f.total) };
                    let title = move || format!("{}: {}/{}", family_name(i18n.get_locale(), f.family), f.solved, f.total);
                    view! {
                        <span
                            class="relative grid size-7 place-items-center overflow-hidden rounded-sm border bg-muted font-pixel text-xs"
                            title=title
                        >
                            <span
                                class="absolute inset-x-0 bottom-0 bg-primary/70"
                                style=format!("height: {:.0}%", share * 100.0)
                            ></span>
                            <span class="relative">{family_kanji(f.family)}</span>
                        </span>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[component]
fn Leaderboard(board: RwSignal<Option<DojoBoard>>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, dojo.board.title)} " " <span class="font-pixel text-sm text-primary">"番付 · banzuke"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, dojo.board.par2_hint)}</CardDescription>
            </CardHeader>
            <CardContent>
                {move || match board.get() {
                    None => view! { <p class="py-8 text-center text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                    Some(b) if b.entries.is_empty() => {
                        view! {
                            <div class="seigaiha grid h-40 place-items-center rounded-lg border border-dashed text-muted-foreground">
                                <p class="rounded-md bg-card px-3 py-1">{t!(i18n, dojo.board.empty)}</p>
                            </div>
                        }
                            .into_any()
                    }
                    Some(b) => {
                        view! {
                            <div class="overflow-x-auto">
                                <table class="w-full text-sm">
                                    <thead>
                                        <tr class="border-b text-start text-xs uppercase tracking-wider text-muted-foreground">
                                            <th class="py-2 pe-2 text-start font-semibold">"#"</th>
                                            <th class="py-2 pe-3 text-start font-semibold">{t!(i18n, dojo.board.solver)}</th>
                                            <th class="py-2 pe-3 text-end font-semibold">{t!(i18n, dojo.board.solved)}</th>
                                            <th class="py-2 pe-3 text-end font-semibold">{t!(i18n, dojo.board.par2)}</th>
                                            <th class="hidden py-2 text-start font-semibold sm:table-cell">{t!(i18n, dojo.board.families)}</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {b
                                            .entries
                                            .into_iter()
                                            .map(|e| {
                                                let href = move || format!("{}/dojo/{}", prefix(i18n.get_locale()), e.id);
                                                let medal = match e.rank {
                                                    1 => "金",
                                                    2 => "銀",
                                                    3 => "銅",
                                                    _ => "",
                                                };
                                                view! {
                                                    <tr class="border-b last:border-0">
                                                        <td class="py-2.5 pe-2 font-pixel tabular-nums">
                                                            {if medal.is_empty() { e.rank.to_string() } else { medal.to_string() }}
                                                        </td>
                                                        <td class="py-2.5 pe-3">
                                                            <A href=href attr:class="font-medium hover:text-primary hover:underline">
                                                                {e.name.clone()}
                                                            </A>
                                                            {e
                                                                .house
                                                                .then(|| {
                                                                    view! {
                                                                        <Badge variant=BadgeVariant::Secondary size=BadgeSize::Sm class="ms-2">
                                                                            {t!(i18n, dojo.board.house)}
                                                                        </Badge>
                                                                    }
                                                                })}
                                                            <span class="block text-xs text-muted-foreground">{e.author.clone()}</span>
                                                        </td>
                                                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">
                                                            {format!("{}/{}", e.score.solved, e.score.total)}
                                                        </td>
                                                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{par2(&e.score)}</td>
                                                        <td class="hidden py-2.5 sm:table-cell">
                                                            <FamilyChips score=e.score />
                                                        </td>
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
                }}
            </CardContent>
        </Card>
    }
}

#[component]
fn Code(children: Children) -> impl IntoView {
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
fn HowItWorks() -> impl IntoView {
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
fn Families() -> impl IntoView {
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
fn FairPlay() -> impl IntoView {
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
                {item("季", t!(i18n, dojo.fair.seasons_title).into_any(), t!(i18n, dojo.fair.seasons).into_any())}
            </CardContent>
        </Card>
    }
}

fn submit_error(locale: Locale, code: &str) -> String {
    match code {
        "sign_in" => td_string!(locale, dojo.submit.errors.sign_in),
        "name" => td_string!(locale, dojo.submit.errors.name),
        "description" => td_string!(locale, dojo.submit.errors.description),
        "url" => td_string!(locale, dojo.submit.errors.url),
        "size" => td_string!(locale, dojo.submit.errors.size),
        "not_wasm" => td_string!(locale, dojo.submit.errors.not_wasm),
        "duplicate" => td_string!(locale, dojo.submit.errors.duplicate),
        "limit" => td_string!(locale, dojo.submit.errors.limit),
        _ => td_string!(locale, dojo.submit.errors.server),
    }
    .to_string()
}

#[component]
fn SubmitCard(me: RwSignal<Option<DojoMe>>) -> impl IntoView {
    let i18n = use_i18n();
    let query = use_query_map();
    let account = use_localized("/account");
    let error = move || query.with(|q| q.get("error"));
    view! {
        <Card class="gap-4" attr:id="submit">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, dojo.submit.title)} " " <span class="font-pixel text-sm text-primary">"入門 · nyūmon"</span>
                </CardTitle>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                {move || {
                    error()
                        .map(|code| {
                            view! {
                                <p class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                                    {submit_error(i18n.get_locale(), &code)}
                                </p>
                            }
                        })
                }}
                {move || match me.get() {
                    None => view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                    Some(m) if !m.registered => {
                        view! {
                            <p class="text-sm text-muted-foreground">{t!(i18n, dojo.submit.sign_in)}</p>
                            <A
                                href=account
                                attr:class="inline-flex h-10 w-fit items-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground"
                            >
                                {t!(i18n, dojo.submit.create_account)}
                            </A>
                        }
                            .into_any()
                    }
                    Some(m) => {
                        let left = m.left_today;
                        view! {
                            <form
                                action="/dojo/submit"
                                method="post"
                                enctype="multipart/form-data"
                                class="flex flex-col gap-3"
                            >
                                <input type="hidden" name="prefix" value=move || prefix(i18n.get_locale()) />
                                <label class="flex flex-col gap-1.5 text-sm font-medium">
                                    {t!(i18n, dojo.submit.name)}
                                    <input class=FIELD name="name" required maxlength="40" autocomplete="off" />
                                </label>
                                <label class="flex flex-col gap-1.5 text-sm font-medium">
                                    {t!(i18n, dojo.submit.description)}
                                    <textarea
                                        class=format!("{FIELD} min-h-20")
                                        name="description"
                                        maxlength="600"
                                        placeholder=move || t_string!(i18n, dojo.submit.description_hint)
                                    ></textarea>
                                </label>
                                <label class="flex flex-col gap-1.5 text-sm font-medium">
                                    {t!(i18n, dojo.submit.source)}
                                    <input class=FIELD name="source_url" type="url" placeholder="https://github.com/..." maxlength="200" />
                                </label>
                                <label class="flex flex-col gap-1.5 text-sm font-medium">
                                    {t!(i18n, dojo.submit.file)}
                                    <input
                                        class=format!("{FIELD} file:me-3 file:rounded file:border-0 file:bg-secondary file:px-2 file:text-sm")
                                        name="wasm"
                                        type="file"
                                        accept=".wasm,application/wasm"
                                        required
                                    />
                                </label>
                                <div class="flex flex-wrap items-center gap-3">
                                    <Button attr:r#type="submit" attr:disabled=left == 0>
                                        {t!(i18n, dojo.submit.button)} " 入門"
                                    </Button>
                                    <span class="text-sm text-muted-foreground">
                                        {move || {
                                            if left == 0 {
                                                t_string!(i18n, dojo.submit.none_left).to_string()
                                            } else {
                                                t_string!(i18n, dojo.submit.left, count = left)
                                            }
                                        }}
                                    </span>
                                </div>
                            </form>
                            <MySubmissions me=m />
                        }
                            .into_any()
                    }
                }}
            </CardContent>
        </Card>
    }
}

#[component]
fn MySubmissions(me: DojoMe) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="flex flex-col gap-2 border-t pt-4">
            <h3 class="text-sm font-bold uppercase tracking-wider text-muted-foreground">{t!(i18n, dojo.mine.title)}</h3>
            {if me.submissions.is_empty() {
                view! { <p class="text-sm text-muted-foreground">{t!(i18n, dojo.mine.empty)}</p> }.into_any()
            } else {
                me.submissions
                    .into_iter()
                    .map(|s| {
                        let href = move || format!("{}/dojo/{}", prefix(i18n.get_locale()), s.id);
                        let disqualified = s.score.as_ref().is_some_and(|sc| sc.wrong > 0);
                        let summary = s.score.as_ref().map(|sc| format!("{}/{} · {}", sc.solved, sc.total, par2(sc)));
                        view! {
                            <A href=href attr:class="flex items-center justify-between gap-3 rounded-md border px-3 py-2 text-sm hover:bg-secondary">
                                <span class="min-w-0 truncate font-medium">{s.name.clone()}</span>
                                <span class="flex shrink-0 items-center gap-2">
                                    <span class="font-pixel tabular-nums text-muted-foreground">{summary}</span>
                                    <StateBadge state=s.state disqualified />
                                </span>
                            </A>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </div>
    }
}

#[component]
pub fn DojoPage() -> impl IntoView {
    let i18n = use_i18n();
    let lab = use_localized("/lab");
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
                <p class="max-w-3xl text-muted-foreground">{t!(i18n, dojo.intro)}</p>
                <div class="flex flex-wrap gap-2">
                    {stat(board_stat(|b, l| td_string!(l, dojo.stats.season, season = b.season)))}
                    {stat(board_stat(|b, l| td_string!(l, dojo.stats.formulas, count = b.formulas)))}
                    {stat(board_stat(|b, l| td_string!(l, dojo.stats.budget, budget = b.budget / 1_000_000_000)))}
                    {stat(board_stat(|b, l| td_string!(l, dojo.stats.queued, count = b.queued)))}
                </div>
                <div class="flex flex-wrap gap-2 pt-1">
                    <a href="#submit" class="inline-flex h-10 items-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground shadow-sm">
                        {t!(i18n, dojo.cta.submit)}
                    </a>
                    <a href=STARTER_URL target="_blank" rel="noopener" class="inline-flex h-10 items-center rounded-md border bg-card px-4 text-sm font-medium">
                        {t!(i18n, dojo.cta.starter)}
                    </a>
                    <a href=DEV_SET_URL download class="inline-flex h-10 items-center rounded-md border bg-card px-4 text-sm font-medium">
                        {t!(i18n, dojo.cta.dev_set)}
                    </a>
                    <A href=lab attr:class="inline-flex h-10 items-center rounded-md px-4 text-sm font-medium text-muted-foreground hover:text-foreground">
                        "研究室 · " {t!(i18n, common.nav.lab)}
                    </A>
                </div>
            </div>
            <Leaderboard board />
            <HowItWorks />
            <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
                <Families />
                <SubmitCard me />
            </div>
            <FairPlay />
            <Card class="gap-2">
                <CardHeader>
                    <CardTitle class="text-xl">{t!(i18n, dojo.science.title)}</CardTitle>
                </CardHeader>
                <CardContent class="flex flex-col gap-2 text-sm text-muted-foreground">
                    <p>{t!(i18n, dojo.science.text)}</p>
                    <p class="flex flex-wrap gap-x-4 gap-y-1">
                        <a class="text-primary underline underline-offset-4" href="/dojo/board.json">"/dojo/board.json"</a>
                        <a class="text-primary underline underline-offset-4" href=format!("{REPO_URL}/tree/main/dojo") target="_blank" rel="noopener">
                            "github.com/NoelHuibers/Hashiwokakero/dojo"
                        </a>
                    </p>
                </CardContent>
            </Card>
        </section>
    }
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

#[component]
fn ScoreView(s: DojoSubmission, score: Score) -> impl IntoView {
    let i18n = use_i18n();
    let wrong = score.wrong;
    view! {
        {(wrong > 0)
            .then(|| {
                view! {
                    <p class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                        {t!(i18n, dojo.detail.wrong, count = wrong)}
                    </p>
                }
            })}
        <div class="grid grid-cols-3 gap-3">
            <div class="rounded-lg border bg-card px-3 py-2">
                <p class="text-xs font-bold uppercase tracking-wider text-muted-foreground">{t!(i18n, dojo.board.par2)}</p>
                <p class="font-pixel text-2xl tabular-nums" title=move || format!("{}: {:.1} G", t_string!(i18n, dojo.detail.fuel), score.fuel.max(0.0))>
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
                                    <td class="py-2.5 text-end font-pixel tabular-nums">{format!("{:.3}", f.par2)}</td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
        {(s.rank.is_none() && wrong == 0)
            .then(|| view! { <p class="text-sm text-muted-foreground">{t!(i18n, dojo.detail.unranked)}</p> })}
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
        <PageMeta title description=Signal::derive(move || t_string!(i18n, dojo.meta.description).to_string()) path="/dojo" />
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
                                <span class="font-pixel" title=s.sha256.clone()>
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
