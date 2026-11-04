//! The recovery email on the account page: the address, one waiting for its confirmation, adding,
//! changing and removing it behind the password (`server::recovery`).

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_i18n::Locale as _;

use crate::api::{load_recovery_email, remove_recovery_email, set_recovery_email};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::input::{Input, InputType};
use crate::components::ui::label::Label;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::error_text;
use crate::model::{ApiError, RecoveryEmail};
use crate::state::{ToastKind, toast};

/// What the card shows below the address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Editing {
    Nothing,
    Address,
    Removal,
}

#[component]
pub(super) fn RecoveryEmailCard() -> impl IntoView {
    let i18n = use_i18n();
    let info = RwSignal::new(None::<RecoveryEmail>);
    let editing = RwSignal::new(Editing::Nothing);
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            spawn_local(async move {
                match load_recovery_email().await {
                    Ok(loaded) => info.set(Some(loaded)),
                    Err(e) => error.set(Some(error_text(i18n.get_locale_untracked(), &e))),
                }
            });
        }
    });

    let close = move || {
        editing.set(Editing::Nothing);
        email.set(String::new());
        password.set(String::new());
        error.set(None);
    };
    let open = move |what: Editing| {
        close();
        editing.set(what);
    };
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let what = editing.get_untracked();
        let (address, pass) = (email.get_untracked(), password.get_untracked());
        let locale = i18n.get_locale_untracked().as_str().to_string();
        spawn_local(async move {
            let result = if what == Editing::Removal {
                remove_recovery_email(pass, locale).await
            } else {
                set_recovery_email(address, pass, locale).await
            };
            match result {
                Ok(updated) => {
                    let pending = updated.pending.clone();
                    info.set(Some(updated));
                    close();
                    match (what, pending) {
                        (Editing::Removal, _) => toast(ToastKind::Success, |l| {
                            td_string!(l, account.recovery.removed).to_string()
                        }),
                        (_, Some(pending)) => toast(ToastKind::Success, move |l| {
                            format!(
                                "{} 確認",
                                td_string!(l, account.recovery.sent, email = pending.clone())
                            )
                        }),
                        // The address it had already
                        (_, None) => {}
                    }
                }
                Err(e) => error.set(Some(error_text(i18n.get_locale_untracked(), &e))),
            }
            busy.set(false);
        });
    };

    let address = move || info.with(|i| i.as_ref().and_then(|i| i.address.clone()));
    let pending = move || info.with(|i| i.as_ref().and_then(|i| i.pending.clone()));
    let mail = move || info.with(|i| i.as_ref().is_some_and(|i| i.mail));

    view! {
        <Card class="gap-4 self-start" attr:data-recovery-email="">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, account.recovery.title)}
                    <span class="font-pixel text-sm text-primary">"連絡"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, account.recovery.text)}</CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                {move || {
                    info.with(Option::is_some)
                        .then(|| {
                            view! {
                                <p class="text-sm" data-recovery-address="">
                                    {move || match address() {
                                        Some(email) => {
                                            t_string!(i18n, account.recovery.current, email)
                                        }
                                        None => t_string!(i18n, account.recovery.none).to_string(),
                                    }}
                                </p>
                            }
                        })
                }}
                {move || {
                    pending()
                        .map(|email| {
                            view! {
                                <p
                                    class="rounded-md border border-dashed px-3 py-2 text-sm text-muted-foreground"
                                    data-recovery-pending=""
                                >
                                    {move || {
                                        t_string!(
                                            i18n, account.recovery.pending, email = email.clone()
                                        )
                                    }}
                                </p>
                            }
                        })
                }} // Without a mail server there are no links to send
                {move || {
                    (info.with(Option::is_some) && !mail())
                        .then(|| {
                            view! {
                                <p class="text-sm text-muted-foreground">
                                    {move || error_text(
                                        i18n.get_locale(),
                                        &ServerFnError::new(format!("E:{}", ApiError::MailUnavailable.code())),
                                    )}
                                </p>
                            }
                        })
                }}
                {move || match editing.get() {
                    Editing::Nothing => {
                        view! {
                            {move || {
                                mail()
                                    .then(|| {
                                        view! {
                                            <Button
                                                variant=ButtonVariant::Outline
                                                class="w-full"
                                                on:click=move |_| open(Editing::Address)
                                            >
                                                {move || {
                                                    if address().is_some() {
                                                        t_string!(i18n, account.recovery.change)
                                                    } else {
                                                        t_string!(i18n, account.recovery.add)
                                                    }
                                                }}
                                            </Button>
                                        }
                                    })
                            }}
                            {move || {
                                (address().is_some() || pending().is_some())
                                    .then(|| {
                                        view! {
                                            <Button
                                                variant=ButtonVariant::Ghost
                                                class="w-full"
                                                on:click=move |_| open(Editing::Removal)
                                            >
                                                {t!(i18n, account.recovery.remove)}
                                            </Button>
                                        }
                                    })
                            }}
                            {move || {
                                error
                                    .get()
                                    .map(|e| view! { <p class="text-sm text-destructive">{e}</p> })
                            }}
                        }
                            .into_any()
                    }
                    what => {
                        let removal = what == Editing::Removal;
                        view! {
                            <form
                                class="flex flex-col gap-3 rounded-lg border p-3"
                                on:submit=submit
                            >
                                {(!removal)
                                    .then(|| {
                                        view! {
                                            <div class="flex flex-col gap-2">
                                                <Label html_for="recovery-email">
                                                    {t!(i18n, account.recovery.address)}
                                                </Label>
                                                <Input
                                                    id="recovery-email"
                                                    r#type=InputType::Email
                                                    bind_value=email
                                                    autocomplete="email"
                                                    placeholder="you@example.org"
                                                    required=true
                                                />
                                            </div>
                                        }
                                    })}
                                {removal
                                    .then(|| {
                                        view! {
                                            <p class="text-sm">
                                                {t!(i18n, account.recovery.remove_confirm)}
                                            </p>
                                        }
                                    })}
                                <div class="flex flex-col gap-2">
                                    <Label html_for="recovery-password">
                                        {t!(i18n, account.password)}
                                    </Label>
                                    <Input
                                        id="recovery-password"
                                        r#type=InputType::Password
                                        bind_value=password
                                        autocomplete="current-password"
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
                                <div class="flex flex-wrap gap-2">
                                    <Button
                                        attr:r#type="submit"
                                        attr:disabled=move || busy.get()
                                        variant=if removal {
                                            ButtonVariant::Destructive
                                        } else {
                                            ButtonVariant::Default
                                        }
                                    >
                                        {move || {
                                            if busy.get() {
                                                t_string!(i18n, account.busy)
                                            } else if removal {
                                                t_string!(i18n, account.recovery.remove_button)
                                            } else {
                                                t_string!(i18n, account.recovery.button)
                                            }
                                        }}
                                    </Button>
                                    <Button
                                        variant=ButtonVariant::Ghost
                                        attr:r#type="button"
                                        on:click=move |_| close()
                                    >
                                        {t!(i18n, account.delete.cancel)}
                                    </Button>
                                </div>
                            </form>
                        }
                            .into_any()
                    }
                }}
            </CardContent>
        </Card>
    }
}
