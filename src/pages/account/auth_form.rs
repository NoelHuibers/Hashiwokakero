//! The sign in and register form.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{login, my_stats, register};
use crate::components::ui::button::Button;
use crate::components::ui::input::{Input, InputType};
use crate::components::ui::label::Label;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::error_text;
use crate::model::UserStats;
use crate::state::{Auth, ToastKind, toast};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    SignIn,
    Register,
}

#[component]
pub(super) fn AuthForm(mode: Mode, stats: RwSignal<Option<UserStats>>) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let id = |field: &str| {
        format!(
            "{}-{field}",
            if mode == Mode::SignIn {
                "login"
            } else {
                "register"
            }
        )
    };

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let (u, p) = (username.get_untracked(), password.get_untracked());
        spawn_local(async move {
            let result = match mode {
                Mode::SignIn => login(u, p).await,
                Mode::Register => register(u, p).await,
            };
            match result {
                Ok(user) => {
                    let name = user.name.clone();
                    toast(ToastKind::Success, move |l| {
                        format!(
                            "{} ようこそ",
                            td_string!(l, account.welcome, name = name.clone())
                        )
                    });
                    auth.user.set(Some(user));
                    password.set(String::new());
                    if let Ok(s) = my_stats().await {
                        stats.set(s);
                    }
                }
                Err(e) => error.set(Some(error_text(i18n.get_locale_untracked(), &e))),
            }
            busy.set(false);
        });
    };

    view! {
        <form class="flex flex-col gap-4" on:submit=submit>
            <div class="flex flex-col gap-2">
                <Label html_for=id("username")>{t!(i18n, account.username)}</Label>
                <Input
                    id=id("username")
                    bind_value=username
                    autocomplete="username"
                    placeholder="bridge_builder"
                    required=true
                    minlength=3
                />
            </div>
            <div class="flex flex-col gap-2">
                <Label html_for=id("password")>{t!(i18n, account.password)}</Label>
                <Input
                    id=id("password")
                    r#type=InputType::Password
                    bind_value=password
                    autocomplete=if mode == Mode::SignIn { "current-password" } else { "new-password" }
                    required=true
                    minlength=if mode == Mode::SignIn { 1 } else { 8 }
                />
                {(mode == Mode::Register)
                    .then(|| view! { <p class="text-xs text-muted-foreground">{t!(i18n, account.password_hint)}</p> })}
            </div>
            {move || error.get().map(|e| view! { <p class="text-sm text-destructive">{e}</p> })}
            <Button attr:r#type="submit" attr:disabled=move || busy.get() class="w-full">
                {move || match (mode, busy.get()) {
                    (_, true) => t_string!(i18n, account.busy).to_string(),
                    (Mode::SignIn, false) => format!("{} 入る", t_string!(i18n, common.sign_in)),
                    (Mode::Register, false) => format!("{} 登録", t_string!(i18n, account.create)),
                }}
            </Button>
        </form>
    }
}
