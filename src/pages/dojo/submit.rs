//! Submitting a solver to the Dojo and the list of your own submissions.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::components::layout::use_localized;
use crate::components::ui::button::Button;
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::{localized, prefix};
use crate::model::DojoMe;
use crate::seo::SITE_URL;

use super::names::{StateBadge, par2};

const FIELD: &str = "flex w-full min-w-0 rounded-md border border-input bg-transparent px-3 py-2 text-base shadow-xs outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 md:text-sm";

/// The app's stand-in for an upload form: the forms post files the plain way, with the
/// website's cookie, and the server answers with a redirect, so uploads go through the website
/// (its page at `path`).
#[component]
pub(super) fn UploadOnWebsite(path: &'static str) -> impl IntoView {
    let i18n = use_i18n();
    let href = move || format!("{SITE_URL}{}", localized(i18n.get_locale(), path));
    view! {
        <p class="text-sm text-muted-foreground">
            // In the app the phone's browser (`api::native::open_links_outside`)
            {t!(
                i18n,
                dojo.submit.on_website,
                <a> = move |children: ChildrenFn| view! {
                    <a href=href target="_blank" rel="noopener" class="text-primary underline underline-offset-4">{children()}</a>
                }
            )}
        </p>
    }
}

fn submit_error(locale: Locale, code: &str) -> String {
    match code {
        "sign_in" => td_string!(locale, dojo.submit.errors.sign_in),
        "name" => td_string!(locale, dojo.submit.errors.name),
        "name_not_allowed" => td_string!(locale, dojo.submit.errors.name_not_allowed),
        "description" => td_string!(locale, dojo.submit.errors.description),
        "url" => td_string!(locale, dojo.submit.errors.url),
        "size" => td_string!(locale, dojo.submit.errors.size),
        "not_wasm" => td_string!(locale, dojo.submit.errors.not_wasm),
        "duplicate" => td_string!(locale, dojo.submit.errors.duplicate),
        "limit" => td_string!(locale, dojo.submit.errors.limit),
        "account_age" => td_string!(locale, dojo.submit.errors.account_age),
        "network_limit" => td_string!(locale, dojo.submit.errors.network_limit),
        "busy" => td_string!(locale, dojo.submit.errors.busy),
        _ => td_string!(locale, dojo.submit.errors.server),
    }
    .to_string()
}

#[component]
pub(super) fn SubmitCard(me: RwSignal<Option<DojoMe>>) -> impl IntoView {
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
                    Some(m) if cfg!(feature = "csr") => {
                        view! {
                            <UploadOnWebsite path="/dojo" />
                            <MySubmissions me=m />
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

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    /// The app's note in place of an upload form links to the page on the website, in the
    /// player's language, in a window of its own.
    #[tokio::test]
    async fn the_app_uploads_on_the_website() {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        let html = owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::de);
            view! { <UploadOnWebsite path="/dojo/benchmarks" /> }.to_html()
        });
        assert!(
            html.contains("href=\"https://hashi.rs/de/dojo/benchmarks\" target=\"_blank\""),
            "{html}"
        );
        assert!(!html.contains("<form"));
    }
}
