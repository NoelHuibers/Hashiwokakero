//! The invite link `/g/<code>`: the group's name and islands, and a big button to join. Opening
//! the link joins nobody, the click does. A visitor without a user joins as a new guest.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};
use leptos_router::NavigateOptions;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::api::{group_invite, join_group};
use crate::components::celebration::Stamp;
use crate::components::layout::use_localized;
use crate::components::ui::button::{Button, ButtonSize};
use crate::components::ui::card::Card;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{error_text, prefix};
use crate::model::InvitePreview;
use crate::seo::title_text;
use crate::state::{Auth, ToastKind, error_toast, toast};

use super::archipelago::Archipelago;

#[component]
pub fn JoinGroupPage() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let params = use_params_map();
    // Stored, so the closures using it stay `Copy`
    let navigate = StoredValue::new(use_navigate());
    let groups = use_localized("/groups");
    let code = Memo::new(move |_| params.with(|p| p.get("code").unwrap_or_default()));
    // `Some(None)`: the link doesn't work
    let preview = RwSignal::new(None::<Option<InvitePreview>>);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let go_to = move |id: i64| {
        let path = format!("{}/groups/{id}", prefix(i18n.get_locale_untracked()));
        navigate.with_value(|navigate| {
            navigate(
                &path,
                NavigateOptions {
                    replace: true,
                    ..NavigateOptions::default()
                },
            );
        });
    };

    Effect::new({
        move |_| {
            let code = code.get();
            if !auth.loaded.get() {
                return;
            }
            spawn_local(async move {
                match group_invite(code).await {
                    // A member already: on to the group
                    Ok(Some(p)) if p.joined.is_some() => {
                        if let Some(id) = p.joined {
                            go_to(id);
                        }
                    }
                    Ok(p) => preview.set(Some(p)),
                    Err(e) => error_toast(e),
                }
            });
        }
    });

    let join = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let name = preview
            .with_untracked(|p| p.as_ref().and_then(|p| p.as_ref()).map(|p| p.name.clone()))
            .unwrap_or_default();
        spawn_local(async move {
            match join_group(code.get_untracked()).await {
                Ok(id) => {
                    // A visitor without a user is a guest now
                    auth.refresh();
                    toast(ToastKind::Success, move |l| {
                        format!(
                            "{} ようこそ",
                            td_string!(l, groups.join.joined, name = name.clone())
                        )
                    });
                    go_to(id);
                }
                Err(e) => {
                    error.set(Some(error_text(i18n.get_locale_untracked(), &e)));
                    busy.set(false);
                }
            }
        });
    };

    let title = move || {
        preview.with(|p| match p {
            Some(Some(p)) => format!(
                "{} · Hashi",
                td_string!(i18n.get_locale(), groups.join.title, name = p.name.clone())
            ),
            _ => format!("{} · Hashi", t_string!(i18n, groups.join.page_title)),
        })
    };
    view! {
        <Title text=title_text(title) />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto flex max-w-2xl flex-col gap-5 px-4 py-8 sm:px-6">
            {move || match preview.get() {
                None => view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                Some(None) => {
                    view! {
                        <div class="flex flex-col items-center gap-4 py-12 text-center" data-testid="invite-invalid">
                            <Stamp kanji="迷" class="size-20 text-5xl" />
                            <h1 class="text-2xl font-black">{t!(i18n, groups.join.invalid_title)}</h1>
                            <p class="text-muted-foreground">{t!(i18n, groups.join.invalid_text)}</p>
                            <A href=groups attr:class="rounded-md bg-primary px-4 py-2 font-medium text-primary-foreground">
                                {t!(i18n, groups.page.back)}
                            </A>
                        </div>
                    }
                        .into_any()
                }
                Some(Some(p)) => {
                    let count = p.members.len();
                    let full = p.full;
                    let guest = move || auth.user.with(Option::is_none);
                    view! {
                        <Card class="gap-5 px-5 sm:px-8">
                            <div class="flex flex-col items-center gap-2 text-center">
                                <p class="font-pixel text-sm text-primary">"招待 · shōtai"</p>
                                <h1 class="break-words text-3xl font-black">
                                    {td_string!(i18n.get_locale_untracked(), groups.join.title, name = p.name.clone())}
                                </h1>
                                <p class="text-muted-foreground">
                                    {move || td_string!(i18n.get_locale(), groups.join.text, count)}
                                </p>
                            </div>
                            <Archipelago group=p.id members=p.members />
                            {if full {
                                view! {
                                    <p class="rounded-md border border-warning/40 bg-warning-light px-3 py-2 text-center text-sm text-warning-dark">
                                        {t!(i18n, groups.join.full)}
                                    </p>
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <div class="flex flex-col items-center gap-2">
                                        <Button
                                            size=ButtonSize::Lg
                                            class="h-12 w-full max-w-sm text-base"
                                            attr:disabled=move || busy.get()
                                            on:click=join
                                        >
                                            {move || if busy.get() { t_string!(i18n, groups.join.busy) } else { t_string!(i18n, groups.join.button) }}
                                            " 参加"
                                        </Button>
                                        {move || error.get().map(|e| view! { <p class="text-sm text-destructive">{e}</p> })}
                                        <Show when=guest>
                                            <p class="max-w-sm text-center text-xs text-muted-foreground">{t!(i18n, groups.join.guest_hint)}</p>
                                        </Show>
                                    </div>
                                }
                                    .into_any()
                            }}
                        </Card>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}
