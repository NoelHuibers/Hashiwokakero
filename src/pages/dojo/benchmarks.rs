//! Community benchmarks ("bring your own benchmark"): the rules, the form, the player's own
//! benchmarks and the pool with its authors.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::api::load_dojo_benchmarks;
use crate::components::layout::use_localized;
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::Button;
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::prefix;
use crate::model::{BenchmarkState, CommunityCredit, DojoBenchmark, DojoBenchmarks};
use crate::seo::PageMeta;

const FIELD: &str = "flex w-full min-w-0 rounded-md border border-input bg-transparent px-3 py-2 text-base shadow-xs outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 md:text-sm";
const LICENSE_URL: &str = dojo::community::LICENSE_URL;
const GBD_URL: &str = "https://benchmark-database.de";

fn error_text(locale: Locale, code: &str) -> String {
    match code {
        "sign_in" => td_string!(locale, dojo.community.errors.sign_in),
        "account_age" => td_string!(locale, dojo.community.errors.account_age),
        "limit" => td_string!(locale, dojo.community.errors.limit),
        "network_limit" => td_string!(locale, dojo.community.errors.network_limit),
        "busy" => td_string!(locale, dojo.community.errors.busy),
        "source" => td_string!(locale, dojo.community.errors.source),
        "gbd" => td_string!(locale, dojo.community.errors.gbd),
        "invalid" => td_string!(locale, dojo.community.errors.invalid),
        "too_large" => td_string!(locale, dojo.community.errors.too_large),
        "duplicate" => td_string!(locale, dojo.community.errors.duplicate),
        "family" => td_string!(locale, dojo.community.errors.family),
        "description" => td_string!(locale, dojo.community.errors.description),
        "license" => td_string!(locale, dojo.community.errors.license),
        _ => td_string!(locale, dojo.community.errors.server),
    }
    .to_string()
}

fn state_text(locale: Locale, state: BenchmarkState) -> String {
    match state {
        BenchmarkState::Queued => td_string!(locale, dojo.community.state.queued),
        BenchmarkState::Checking => td_string!(locale, dojo.community.state.checking),
        BenchmarkState::Verified => td_string!(locale, dojo.community.state.verified),
        BenchmarkState::Refused => td_string!(locale, dojo.community.state.refused),
        BenchmarkState::Accepted => td_string!(locale, dojo.community.state.accepted),
        BenchmarkState::Rejected => td_string!(locale, dojo.community.state.rejected),
    }
    .to_string()
}

pub(super) fn reason_text(locale: Locale, reason: &str) -> String {
    match reason {
        "invalid" => td_string!(locale, dojo.community.reason.invalid),
        "too_large" => td_string!(locale, dojo.community.reason.too_large),
        "too_easy" => td_string!(locale, dojo.community.reason.too_easy),
        "no_answer" => td_string!(locale, dojo.community.reason.no_answer),
        "wrong_claim" => td_string!(locale, dojo.community.reason.wrong_claim),
        "moderation" => td_string!(locale, dojo.community.reason.moderation),
        _ => td_string!(locale, dojo.community.reason.failed),
    }
    .to_string()
}

fn state_variant(state: BenchmarkState) -> BadgeVariant {
    match state {
        BenchmarkState::Queued => BadgeVariant::Muted,
        BenchmarkState::Checking | BenchmarkState::Verified => BadgeVariant::Warning,
        BenchmarkState::Accepted => BadgeVariant::Success,
        BenchmarkState::Refused | BenchmarkState::Rejected => BadgeVariant::Destructive,
    }
}

/// "12,345 variables · 67,890 clauses"
fn size(locale: Locale, vars: u32, clauses: u32) -> String {
    td_string!(
        locale,
        dojo.community.size,
        vars = vars.to_string(),
        clauses = clauses.to_string()
    )
}

/// The author of a pool benchmark, or that the account is gone.
pub(super) fn author_text(locale: Locale, author: &str) -> String {
    if author.is_empty() {
        td_string!(locale, dojo.community.deleted).to_string()
    } else {
        td_string!(locale, dojo.community.by, author = author.to_string())
    }
}

#[component]
fn Rules(per_set: u32) -> impl IntoView {
    let i18n = use_i18n();
    let item = |kanji: &'static str, text: AnyView| {
        view! {
            <li class="flex gap-3">
                <span class="hanko grid size-8 shrink-0 rotate-[-4deg] place-items-center rounded-md font-pixel">{kanji}</span>
                <span>{text}</span>
            </li>
        }
    };
    view! {
        <Card class="gap-3">
            <CardHeader>
                <CardTitle class="text-xl">{t!(i18n, dojo.community.rules_title)}</CardTitle>
            </CardHeader>
            <CardContent>
                <ul class="flex flex-col gap-3 text-sm text-muted-foreground">
                    {item("証", t!(i18n, dojo.community.rule_answer).into_any())}
                    {item("難", t!(i18n, dojo.community.rule_easy).into_any())}
                    {item("量", t!(i18n, dojo.community.rule_size).into_any())}
                    {item("籤", t!(i18n, dojo.community.rule_draw, count = per_set).into_any())}
                    {item(
                        "許",
                        t!(
                            i18n,
                            dojo.community.rule_license,
                            <a> = |children: ChildrenFn| view! {
                                <a href=LICENSE_URL target="_blank" rel="noopener" class="text-primary underline underline-offset-4">
                                    {children()}
                                </a>
                            }
                        )
                            .into_any(),
                    )}
                </ul>
            </CardContent>
        </Card>
    }
}

#[component]
fn SubmitForm(data: DojoBenchmarks) -> impl IntoView {
    let i18n = use_i18n();
    let account = use_localized("/account");
    if !data.registered {
        return view! {
            <p class="text-sm text-muted-foreground">{t!(i18n, dojo.community.sign_in)}</p>
            <A href=account attr:class="inline-flex h-10 w-fit items-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground">
                {t!(i18n, dojo.submit.create_account)}
            </A>
        }
            .into_any();
    }
    let left = data.left_today;
    view! {
        <form action="/dojo/benchmarks/submit" method="post" enctype="multipart/form-data" class="flex flex-col gap-3">
            <input type="hidden" name="prefix" value=move || prefix(i18n.get_locale()) />
            <label class="flex flex-col gap-1.5 text-sm font-medium">
                {t!(i18n, dojo.community.file)}
                <input
                    class=format!("{FIELD} file:me-3 file:rounded file:border-0 file:bg-secondary file:px-2 file:text-sm")
                    name="cnf"
                    type="file"
                    accept=".cnf,.xz,.dimacs,text/plain"
                />
            </label>
            <div class="flex flex-col gap-1.5">
                <label class="flex flex-col gap-1.5 text-sm font-medium">
                    {t!(i18n, dojo.community.gbd)}
                    <input class=format!("{FIELD} font-mono") name="gbd" maxlength="32" pattern="[0-9a-fA-F]{32}" autocomplete="off" dir="ltr" />
                </label>
                <span class="text-xs text-muted-foreground">
                    {t!(
                        i18n,
                        dojo.community.gbd_hint,
                        <a> = |children: ChildrenFn| view! {
                            <a href=GBD_URL target="_blank" rel="noopener" class="text-primary underline underline-offset-4">{children()}</a>
                        }
                    )}
                </span>
            </div>
            <div class="flex flex-col gap-1.5">
                <label class="flex flex-col gap-1.5 text-sm font-medium">
                    {t!(i18n, dojo.community.family)}
                    <input class=FIELD name="family" required maxlength="40" pattern="[a-z0-9][a-z0-9\\-]*" autocomplete="off" dir="ltr" />
                </label>
                <span class="text-xs text-muted-foreground">{t!(i18n, dojo.community.family_hint)}</span>
            </div>
            <label class="flex flex-col gap-1.5 text-sm font-medium">
                {t!(i18n, dojo.community.description)}
                <textarea
                    class=format!("{FIELD} min-h-20")
                    name="description"
                    maxlength="600"
                    placeholder=move || t_string!(i18n, dojo.community.description_hint)
                ></textarea>
            </label>
            <label class="flex flex-col gap-1.5 text-sm font-medium">
                {t!(i18n, dojo.community.claimed)}
                <select class=FIELD name="claimed">
                    <option value="">{t!(i18n, dojo.community.claimed_none)}</option>
                    <option value="SAT">"SAT"</option>
                    <option value="UNSAT">"UNSAT"</option>
                </select>
            </label>
            <label class="flex items-start gap-2 text-sm">
                <input type="checkbox" name="license" value="1" required class="mt-1" />
                <span>
                    {t!(
                        i18n,
                        dojo.community.license,
                        <a> = |children: ChildrenFn| view! {
                            <a href=LICENSE_URL target="_blank" rel="noopener" class="text-primary underline underline-offset-4">{children()}</a>
                        }
                    )}
                </span>
            </label>
            <div class="flex flex-wrap items-center gap-3">
                <Button attr:r#type="submit" attr:disabled=left == 0>
                    {t!(i18n, dojo.community.submit)} " 持参"
                </Button>
                <span class="text-sm text-muted-foreground">{move || t_string!(i18n, dojo.community.left, count = left)}</span>
            </div>
        </form>
    }
        .into_any()
}

#[component]
fn Mine(mine: Vec<DojoBenchmark>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="flex flex-col gap-2 border-t pt-4" id="mine">
            <h3 class="text-sm font-bold uppercase tracking-wider text-muted-foreground">{t!(i18n, dojo.community.mine_title)}</h3>
            {if mine.is_empty() {
                view! { <p class="text-sm text-muted-foreground">{t!(i18n, dojo.community.mine_empty)}</p> }.into_any()
            } else {
                mine.into_iter()
                    .map(|b| {
                        let reason = b.reason.clone();
                        view! {
                            <div class="flex flex-col gap-1 rounded-md border px-3 py-2 text-sm" data-testid="my-benchmark">
                                <div class="flex items-center justify-between gap-3">
                                    <span class="min-w-0 truncate font-medium">{b.family.clone()}</span>
                                    <span class="flex shrink-0 items-center gap-2">
                                        {b.answer.map(|a| view! { <span class="font-pixel text-xs">{a.id()}</span> })}
                                        <Badge variant=state_variant(b.state)>{move || state_text(i18n.get_locale(), b.state)}</Badge>
                                    </span>
                                </div>
                                <span class="text-xs text-muted-foreground">{move || size(i18n.get_locale(), b.vars, b.clauses)}</span>
                                {reason.map(|r| view! { <span class="text-xs text-destructive">{move || reason_text(i18n.get_locale(), &r)}</span> })}
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </div>
    }
}

/// The accepted benchmarks and their authors.
#[component]
pub(super) fn PoolList(pool: Vec<CommunityCredit>) -> impl IntoView {
    let i18n = use_i18n();
    if pool.is_empty() {
        return view! { <p class="text-sm text-muted-foreground">{t!(i18n, dojo.community.pool_empty)}</p> }.into_any();
    }
    view! {
        <ul class="flex flex-col divide-y text-sm">
            {pool
                .into_iter()
                .map(|c| {
                    let gbd = format!("{GBD_URL}/file/{}", c.hash);
                    view! {
                        <li class="flex flex-wrap items-baseline justify-between gap-x-3 py-2">
                            <span>
                                <span class="font-medium">{c.family.clone()}</span>
                                " "
                                <span class="text-muted-foreground">{move || author_text(i18n.get_locale(), &c.author)}</span>
                            </span>
                            <span class="flex items-baseline gap-2 text-xs text-muted-foreground">
                                <span class="font-pixel">{c.answer.id()}</span>
                                <span>{move || size(i18n.get_locale(), c.vars, c.clauses)}</span>
                                <a href=gbd target="_blank" rel="noopener" class="font-mono text-primary hover:underline" dir="ltr">
                                    {c.hash.chars().take(8).collect::<String>()}
                                </a>
                            </span>
                        </li>
                    }
                })
                .collect_view()}
        </ul>
    }
        .into_any()
}

#[component]
pub fn DojoBenchmarksPage() -> impl IntoView {
    let i18n = use_i18n();
    let back = use_localized("/dojo");
    let query = use_query_map();
    let data = RwSignal::new(None::<DojoBenchmarks>);
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            spawn_local(async move {
                if let Ok(d) = load_dojo_benchmarks().await {
                    data.set(Some(d));
                }
            });
        }
    });
    let error = move || query.with(|q| q.get("error"));
    let submitted = move || query.with(|q| q.get("submitted").is_some());
    view! {
        <PageMeta
            title=Signal::derive(move || t_string!(i18n, dojo.community.meta_title).to_string())
            description=Signal::derive(move || t_string!(i18n, dojo.community.meta_description).to_string())
            path="/dojo/benchmarks"
        />
        <section class="mx-auto flex max-w-4xl flex-col gap-6 px-4 py-6 sm:px-6">
            <A href=back attr:class="text-sm text-primary hover:underline">
                "← " {t!(i18n, dojo.seasons.back)}
            </A>
            <div class="flex flex-col gap-3">
                <p class="font-pixel text-sm text-primary">"持参 · jisan"</p>
                <h1 class="text-4xl font-black">{t!(i18n, dojo.community.title)}</h1>
                <p class="max-w-3xl text-muted-foreground">{t!(i18n, dojo.community.intro)}</p>
            </div>
            {move || data.get().map(|d| view! { <Rules per_set=d.per_set /> })}
            <Card class="gap-4" attr:id="submit">
                <CardHeader>
                    <CardTitle class="text-xl">{t!(i18n, dojo.community.form_title)}</CardTitle>
                </CardHeader>
                <CardContent class="flex flex-col gap-4">
                    {move || {
                        error()
                            .map(|code| {
                                view! {
                                    <p class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                                        {error_text(i18n.get_locale(), &code)}
                                    </p>
                                }
                            })
                    }}
                    {move || {
                        submitted()
                            .then(|| {
                                view! {
                                    <p class="rounded-md border border-primary/40 bg-primary/10 px-3 py-2 text-sm">
                                        {t!(i18n, dojo.community.submitted)}
                                    </p>
                                }
                            })
                    }}
                    {move || match data.get() {
                        None => view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                        Some(d) => {
                            let registered = d.registered;
                            let mine = d.mine.clone();
                            view! {
                                <SubmitForm data=d />
                                {registered.then(|| view! { <Mine mine /> })}
                            }
                                .into_any()
                        }
                    }}
                </CardContent>
            </Card>
            <Card class="gap-3" attr:id="pool">
                <CardHeader>
                    <CardTitle class="text-xl">{t!(i18n, dojo.community.pool_title)}</CardTitle>
                </CardHeader>
                <CardContent class="flex flex-col gap-2">
                    {move || {
                        data.get()
                            .map(|d| {
                                let count = d.pool.len();
                                view! {
                                    {(count > 0).then(|| view! { <p class="text-sm text-muted-foreground">{t!(i18n, dojo.community.pool_count, count = count)}</p> })}
                                    <PoolList pool=d.pool />
                                }
                            })
                    }}
                </CardContent>
            </Card>
        </section>
    }
}
