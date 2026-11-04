//! Changing the password, behind the current one. The other devices are signed out, this one
//! stays signed in (`auth::change_password`).

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::change_password;
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::input::{Input, InputType};
use crate::components::ui::label::Label;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::error_text;
use crate::state::{ToastKind, toast};

#[component]
pub(super) fn ChangePasswordForm() -> impl IntoView {
    let i18n = use_i18n();
    let editing = RwSignal::new(false);
    let current = RwSignal::new(String::new());
    let new = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let close = move || {
        editing.set(false);
        current.set(String::new());
        new.set(String::new());
        error.set(None);
    };
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let (c, n) = (current.get_untracked(), new.get_untracked());
        spawn_local(async move {
            match change_password(c, n).await {
                Ok(()) => {
                    toast(ToastKind::Success, |l| {
                        format!("{} 合言葉", td_string!(l, account.password_change.done))
                    });
                    close();
                }
                Err(e) => error.set(Some(error_text(i18n.get_locale_untracked(), &e))),
            }
            busy.set(false);
        });
    };

    view! {
        {move || {
            if editing.get() {
                view! {
                    <form class="flex flex-col gap-3 rounded-lg border p-3" on:submit=submit>
                        <div class="flex flex-col gap-2">
                            <Label html_for="password-current">{t!(i18n, account.password_change.current)}</Label>
                            <Input
                                id="password-current"
                                r#type=InputType::Password
                                bind_value=current
                                autocomplete="current-password"
                                required=true
                            />
                        </div>
                        <div class="flex flex-col gap-2">
                            <Label html_for="password-new">{t!(i18n, account.password_change.new)}</Label>
                            <Input
                                id="password-new"
                                r#type=InputType::Password
                                bind_value=new
                                autocomplete="new-password"
                                required=true
                                minlength=8
                            />
                            <p class="text-xs text-muted-foreground">{t!(i18n, account.password_change.hint)}</p>
                        </div>
                        {move || error.get().map(|e| view! { <p class="text-sm text-destructive">{e}</p> })}
                        <div class="flex flex-wrap gap-2">
                            <Button attr:r#type="submit" attr:disabled=move || busy.get()>
                                {move || {
                                    if busy.get() {
                                        t_string!(i18n, account.busy)
                                    } else {
                                        t_string!(i18n, account.password_change.button)
                                    }
                                }}
                            </Button>
                            <Button variant=ButtonVariant::Ghost attr:r#type="button" on:click=move |_| close()>
                                {t!(i18n, account.delete.cancel)}
                            </Button>
                        </div>
                    </form>
                }
                    .into_any()
            } else {
                view! {
                    <Button variant=ButtonVariant::Outline class="w-full" on:click=move |_| editing.set(true)>
                        {t!(i18n, account.password_change.start)}
                    </Button>
                }
                    .into_any()
            }
        }}
    }
}
