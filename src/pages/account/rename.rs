//! Changing the username, behind the password and once a month (`RENAME_EVERY_DAYS`).

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::change_username;
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::input::{Input, InputType};
use crate::components::ui::label::Label;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{date_short, error_text};
use crate::model::{MS_PER_DAY, RENAME_EVERY_DAYS};
use crate::state::{Auth, ToastKind, toast};

/// The browser clock, unix time in ms.
#[expect(
    clippy::cast_possible_truncation,
    reason = "`Date.now()` is a whole number of ms, far inside the i64 range"
)]
fn browser_now() -> i64 {
    js_sys::Date::now() as i64
}

#[component]
pub(super) fn ChangeUsername() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let current = move || {
        auth.user
            .with(|u| u.as_ref().map(|u| u.name.clone()).unwrap_or_default())
    };
    // The day it works again, if the last change was too recent
    let wait_until = move || {
        auth.user
            .with(|u| u.as_ref().and_then(|u| u.rename_from(browser_now())))
            .map(|ms| ms.div_euclid(MS_PER_DAY))
    };
    let editing = RwSignal::new(false);
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let unchanged = move || username.with(|u| u.trim() == current());

    let start = move |_| {
        username.set(current());
        editing.set(true);
    };
    let cancel = move |_| {
        editing.set(false);
        password.set(String::new());
        error.set(None);
    };
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() || unchanged() {
            return;
        }
        busy.set(true);
        error.set(None);
        let (u, p) = (username.get_untracked(), password.get_untracked());
        spawn_local(async move {
            match change_username(u, p).await {
                Ok(user) => {
                    let name = user.name.clone();
                    toast(ToastKind::Success, move |l| {
                        format!(
                            "{} 改名",
                            td_string!(l, account.rename.done, name = name.clone())
                        )
                    });
                    auth.user.set(Some(user));
                    editing.set(false);
                    password.set(String::new());
                }
                Err(e) => error.set(Some(error_text(i18n.get_locale_untracked(), &e))),
            }
            busy.set(false);
        });
    };

    view! {
        {move || match wait_until() {
            Some(day) => {
                view! {
                    <p class="text-sm text-muted-foreground">
                        {t_string!(i18n, account.rename.wait, date = date_short(i18n.get_locale(), day))}
                    </p>
                }
                    .into_any()
            }
            None if !editing.get() => {
                view! {
                    <Button variant=ButtonVariant::Outline class="w-full" on:click=start>
                        {t!(i18n, account.rename.start)}
                    </Button>
                }
                    .into_any()
            }
            None => {
                view! {
                    <form class="flex flex-col gap-3 rounded-lg border p-3" on:submit=submit>
                        <div class="flex flex-col gap-2">
                            <Label html_for="rename-username">{t!(i18n, account.rename.new_name)}</Label>
                            <Input id="rename-username" bind_value=username autocomplete="username" required=true minlength=3 />
                            <p class="text-xs text-muted-foreground">
                                {t_string!(i18n, account.rename.hint, days = RENAME_EVERY_DAYS)}
                            </p>
                        </div>
                        <div class="flex flex-col gap-2">
                            <Label html_for="rename-password">{t!(i18n, account.password)}</Label>
                            <Input
                                id="rename-password"
                                r#type=InputType::Password
                                bind_value=password
                                autocomplete="current-password"
                                required=true
                            />
                        </div>
                        {move || error.get().map(|e| view! { <p class="text-sm text-destructive">{e}</p> })}
                        <div class="flex flex-wrap gap-2">
                            <Button attr:r#type="submit" attr:disabled=move || busy.get() || unchanged()>
                                {move || if busy.get() { t_string!(i18n, account.busy) } else { t_string!(i18n, account.rename.button) }}
                            </Button>
                            <Button variant=ButtonVariant::Ghost attr:r#type="button" on:click=cancel>
                                {t!(i18n, account.delete.cancel)}
                            </Button>
                        </div>
                    </form>
                }
                    .into_any()
            }
        }}
    }
}
