//! Deleting the account (or the guest) for good, behind a second step.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::delete_account;
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::input::{Input, InputType};
use crate::components::ui::label::Label;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::error_text;
use crate::model::UserStats;
use crate::state::{Auth, ToastKind, toast};

#[component]
pub(super) fn DeleteAccount(stats: RwSignal<Option<UserStats>>) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let guest = move || auth.user.with(|u| u.as_ref().is_some_and(|u| u.guest));
    let confirming = RwSignal::new(false);
    let password = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let cancel = move |_| {
        confirming.set(false);
        password.set(String::new());
        error.set(None);
    };
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let password = (!guest()).then(|| password.get_untracked());
        spawn_local(async move {
            match delete_account(password).await {
                Ok(()) => {
                    auth.user.set(None);
                    stats.set(None);
                    toast(ToastKind::Info, |l| {
                        format!("{} さようなら", td_string!(l, account.delete.done))
                    });
                }
                Err(e) => error.set(Some(error_text(i18n.get_locale_untracked(), &e))),
            }
            busy.set(false);
        });
    };

    view! {
        <Card class="gap-4 md:order-last md:col-span-2">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, account.delete.title)} <span class="font-pixel text-sm text-destructive">"削除"</span>
                </CardTitle>
                <CardDescription>
                    {move || if guest() { t_string!(i18n, account.delete.intro_guest) } else { t_string!(i18n, account.delete.intro) }}
                </CardDescription>
            </CardHeader>
            <CardContent>
                <Show
                    when=move || confirming.get()
                    fallback=move || {
                        view! {
                            <Button variant=ButtonVariant::Outline class="w-full sm:w-auto" on:click=move |_| confirming.set(true)>
                                {move || if guest() { t_string!(i18n, account.delete.start_guest) } else { t_string!(i18n, account.delete.start) }}
                            </Button>
                        }
                    }
                >
                    <form class="flex max-w-md flex-col gap-3" on:submit=submit>
                        <p class="text-sm font-medium">
                            {move || if guest() { t_string!(i18n, account.delete.confirm_guest) } else { t_string!(i18n, account.delete.confirm) }}
                        </p>
                        <Show when=move || !guest()>
                            <div class="flex flex-col gap-2">
                                <Label html_for="delete-password">{t!(i18n, account.password)}</Label>
                                <Input
                                    id="delete-password"
                                    r#type=InputType::Password
                                    bind_value=password
                                    autocomplete="current-password"
                                    required=true
                                />
                            </div>
                        </Show>
                        {move || error.get().map(|e| view! { <p class="text-sm text-destructive">{e}</p> })}
                        <div class="flex flex-wrap gap-2">
                            <Button variant=ButtonVariant::Destructive attr:r#type="submit" attr:disabled=move || busy.get()>
                                {move || if busy.get() { t_string!(i18n, account.busy) } else { t_string!(i18n, account.delete.button) }}
                            </Button>
                            <Button variant=ButtonVariant::Ghost attr:r#type="button" on:click=cancel>
                                {t!(i18n, account.delete.cancel)}
                            </Button>
                        </div>
                    </form>
                </Show>
            </CardContent>
        </Card>
    }
}
