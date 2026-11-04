//! The pages of the links in mails, and "Forgot password?" (`server::recovery`): asking for a
//! reset link, choosing a new password with it, confirming a recovery email.
//!
//! The token of a link is after the `#`, which browsers don't send to the server: the pages read
//! it once they run in the browser. Nothing happens by opening a link, only by a click, so a mail
//! program that opens links to check them uses up nothing.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_i18n::Locale as _;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;
use leptos_router::hooks::{use_location, use_navigate};

use crate::api::{
    confirm_recovery_email, forgot_password, password_reset_available, reset_password,
};
use crate::components::layout::use_localized;
use crate::components::ui::button::Button;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader};
use crate::components::ui::input::{Input, InputType};
use crate::components::ui::label::Label;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::error_text;
use crate::model::ApiError;
use crate::seo::title_text;
use crate::state::{Auth, ToastKind, toast};

const LINK: &str = "text-sm text-primary underline-offset-4 hover:underline";

/// The token of the link the page was opened with, once the page runs in the browser: `Some("")`
/// if there is none.
fn link_token() -> RwSignal<Option<String>> {
    let location = use_location();
    let token = RwSignal::new(None::<String>);
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            let hash = location.hash.get_untracked();
            token.set(Some(hash.trim_start_matches('#').trim().to_string()));
        }
    });
    token
}

/// The page's frame: kanji, heading and a card.
#[component]
fn Frame(kanji: &'static str, title: TextProp, children: Children) -> impl IntoView {
    let heading = title.clone();
    view! {
        <Title text=title_text(move || format!("{} · Hashi", title.get())) />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto max-w-lg px-4 py-8 sm:px-6">
            <div class="mb-6">
                <p class="font-pixel text-sm text-primary">{kanji}</p>
                <h1 class="text-3xl font-black">{move || heading.get()}</h1>
            </div>
            <Card class="gap-4">{children()}</Card>
        </section>
    }
}

/// "Forgot password?": a username or recovery email, and a reset link goes to the address if
/// there is such an account. What the page says next is the same either way.
#[component]
pub fn ForgotPasswordPage() -> impl IntoView {
    let i18n = use_i18n();
    let login = RwSignal::new(String::new());
    let available = RwSignal::new(true);
    let sent = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let account = use_localized("/account");

    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            spawn_local(async move {
                if let Ok(can) = password_reset_available().await {
                    available.set(can);
                }
            });
        }
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let locale = i18n.get_locale_untracked().as_str().to_string();
        let login = login.get_untracked();
        spawn_local(async move {
            match forgot_password(login, locale).await {
                Ok(()) => sent.set(true),
                Err(e)
                    if ApiError::from_message(&e.to_string())
                        == Some(ApiError::MailUnavailable) =>
                {
                    available.set(false);
                }
                Err(e) => error.set(Some(error_text(i18n.get_locale_untracked(), &e))),
            }
            busy.set(false);
        });
    };

    view! {
        <Frame
            kanji="合言葉 · aikotoba"
            title=TextProp::from(move || t_string!(i18n, account.forgot.title).to_string())
        >
            <CardHeader>
                <CardDescription>
                    {move || {
                        if available.get() {
                            t_string!(i18n, account.forgot.text)
                        } else {
                            t_string!(i18n, account.forgot.unavailable)
                        }
                    }}
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                {move || {
                    if !available.get() {
                        ().into_any()
                    } else if sent.get() {
                        view! {
                            <p
                                class="rounded-md border border-dashed px-3 py-2 text-sm"
                                role="status"
                            >
                                {t!(i18n, account.forgot.sent)}
                            </p>
                        }
                            .into_any()
                    } else {
                        view! {
                            <form class="flex flex-col gap-4" on:submit=submit>
                                <div class="flex flex-col gap-2">
                                    <Label html_for="forgot-login">
                                        {t!(i18n, account.forgot.login)}
                                    </Label>
                                    <Input
                                        id="forgot-login"
                                        bind_value=login
                                        autocomplete="username"
                                        required=true
                                    />
                                </div>
                                {move || {
                                    error
                                        .get()
                                        .map(|e| {
                                            view! { <p class="text-sm text-destructive">{e}</p> }
                                        })
                                }}
                                <Button
                                    attr:r#type="submit"
                                    attr:disabled=move || busy.get()
                                    class="w-full"
                                >
                                    {move || {
                                        if busy.get() {
                                            t_string!(i18n, account.busy)
                                        } else {
                                            t_string!(i18n, account.forgot.button)
                                        }
                                    }}
                                </Button>
                            </form>
                        }
                            .into_any()
                    }
                }} <p class="text-xs text-muted-foreground">{t!(i18n, account.forgot.no_email)}</p>
                <A href=account attr:class=LINK>
                    "← "
                    {t!(i18n, account.forgot.back)}
                </A>
            </CardContent>
        </Frame>
    }
}

/// The page of a reset link: a new password, and the browser is signed in with it.
#[component]
pub fn ResetPasswordPage() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let token = link_token();
    let password = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let expired = RwSignal::new(false);
    let account = use_localized("/account");
    let forgot = use_localized("/account/forgot");
    let navigate = use_navigate();
    let guest = move || auth.user.with(|u| u.as_ref().is_some_and(|u| u.guest));

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let (token, password) = (
            token.get_untracked().unwrap_or_default(),
            password.get_untracked(),
        );
        let navigate = navigate.clone();
        spawn_local(async move {
            match reset_password(token, password).await {
                Ok(user) => {
                    let name = user.name.clone();
                    toast(ToastKind::Success, move |l| {
                        format!(
                            "{} ようこそ",
                            td_string!(l, account.welcome, name = name.clone())
                        )
                    });
                    auth.user.set(Some(user));
                    navigate(&account(), leptos_router::NavigateOptions::default());
                }
                Err(e) => {
                    expired
                        .set(ApiError::from_message(&e.to_string()) == Some(ApiError::LinkExpired));
                    error.set(Some(error_text(i18n.get_locale_untracked(), &e)));
                }
            }
            busy.set(false);
        });
    };

    view! {
        <Frame
            kanji="合言葉 · aikotoba"
            title=TextProp::from(move || t_string!(i18n, account.reset.title).to_string())
        >
            <CardHeader>
                <CardDescription>{t!(i18n, account.reset.text)}</CardDescription>
            </CardHeader>
            <CardContent>
                <form class="flex flex-col gap-4" on:submit=submit>
                    <div class="flex flex-col gap-2">
                        <Label html_for="reset-password">
                            {t!(i18n, account.password_change.new)}
                        </Label>
                        <Input
                            id="reset-password"
                            r#type=InputType::Password
                            bind_value=password
                            autocomplete="new-password"
                            required=true
                            minlength=8
                        />
                        <p class="text-xs text-muted-foreground">
                            {t!(i18n, account.password_hint)}
                        </p>
                    </div>
                    // Signing in takes the guest's results along, it doesn't happen unsaid
                    {move || {
                        guest()
                            .then(|| {
                                view! {
                                    <p class="text-sm text-muted-foreground">
                                        {t!(i18n, account.reset.guest)}
                                    </p>
                                }
                            })
                    }}
                    {move || {
                        error.get().map(|e| view! { <p class="text-sm text-destructive">{e}</p> })
                    }}
                    {move || {
                        expired
                            .get()
                            .then(|| {
                                view! {
                                    <A href=forgot attr:class=LINK>
                                        {t!(i18n, account.reset.again)}
                                        " →"
                                    </A>
                                }
                            })
                    }}
                    <Button
                        attr:r#type="submit"
                        attr:disabled=move || busy.get() || token.with(Option::is_none)
                        class="w-full"
                    >
                        {move || {
                            if busy.get() {
                                t_string!(i18n, account.busy)
                            } else {
                                t_string!(i18n, account.reset.button)
                            }
                        }}
                    </Button>
                </form>
            </CardContent>
        </Frame>
    }
}

/// The page of a confirmation link: a click, and the address is the account's recovery email.
#[component]
pub fn ConfirmEmailPage() -> impl IntoView {
    let i18n = use_i18n();
    let token = link_token();
    let confirmed = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let account = use_localized("/account");

    let confirm = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let token = token.get_untracked().unwrap_or_default();
        spawn_local(async move {
            match confirm_recovery_email(token).await {
                Ok(email) => confirmed.set(Some(email)),
                Err(e) => error.set(Some(error_text(i18n.get_locale_untracked(), &e))),
            }
            busy.set(false);
        });
    };

    view! {
        <Frame
            kanji="確認 · kakunin"
            title=TextProp::from(move || t_string!(i18n, account.confirm.title).to_string())
        >
            <CardHeader>
                <CardDescription>{t!(i18n, account.confirm.text)}</CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                {move || match confirmed.get() {
                    Some(email) => {
                        view! {
                            <p
                                class="rounded-md border border-dashed px-3 py-2 text-sm"
                                role="status"
                            >
                                {move || {
                                    t_string!(i18n, account.confirm.done, email = email.clone())
                                }}
                            </p>
                        }
                            .into_any()
                    }
                    None => {
                        view! {
                            {move || {
                                error
                                    .get()
                                    .map(|e| view! { <p class="text-sm text-destructive">{e}</p> })
                            }}
                            <Button
                                on:click=confirm
                                attr:disabled=move || busy.get() || token.with(Option::is_none)
                                class="w-full"
                            >
                                {move || {
                                    if busy.get() {
                                        t_string!(i18n, account.busy)
                                    } else {
                                        t_string!(i18n, account.confirm.button)
                                    }
                                }}
                            </Button>
                        }
                            .into_any()
                    }
                }} <A href=account attr:class=LINK>
                    {t!(i18n, account.confirm.to_account)}
                    " →"
                </A>
            </CardContent>
        </Frame>
    }
}
