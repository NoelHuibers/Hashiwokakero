use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{login, logout, my_stats, register};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::input::{Input, InputType};
use crate::components::ui::label::Label;
use crate::components::ui::tabs::{Tabs, TabsContent, TabsList, TabsTrigger};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{date_short, error_text};
use crate::model::{UserStats, date_string, format_duration};
use crate::state::{Auth, ToastKind, toast};
use leptos_meta::{Meta, Title};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    SignIn,
    Register,
}

#[component]
fn AuthForm(mode: Mode, stats: RwSignal<Option<UserStats>>) -> impl IntoView {
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

#[component]
fn StatTile(label: String, jp: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex flex-col items-center rounded-lg border bg-background px-2 py-3">
            <span class="font-pixel text-2xl tabular-nums leading-tight">{value}</span>
            <span class="text-center text-[10px] font-bold uppercase leading-tight tracking-wider text-muted-foreground">
                {label}
            </span>
            <span class="font-pixel text-[10px] leading-tight text-primary/80">{jp}</span>
        </div>
    }
}

/// The last weeks as a grid of pixel squares, gold for top 3 finishes.
#[component]
fn Calendar(stats: UserStats) -> impl IntoView {
    let i18n = use_i18n();
    let locale = i18n.get_locale_untracked();
    view! {
        <div class="grid grid-cols-7 gap-1.5">
            {stats
                .recent
                .into_iter()
                .map(|d| {
                    let (class, title) = match (d.time_ms, d.rank) {
                        (Some(t), r) => {
                            let rank = r
                                .map(|rank| format!(" · {}", td_string!(locale, account.calendar_rank, rank)))
                                .unwrap_or_default();
                            (
                                if r.is_some_and(|r| r <= 3) { "bg-[var(--px-gold)]" } else { "bg-[var(--px-grass)]" },
                                format!("{} · {}{rank}", date_short(locale, d.day), format_duration(t)),
                            )
                        }
                        _ => (
                            "bg-muted",
                            format!("{} · {}", date_short(locale, d.day), td_string!(locale, account.not_solved)),
                        ),
                    };
                    view! {
                        <div
                            class=format!("aspect-square rounded-[3px] border border-black/5 {class}")
                            title=title
                            data-date=date_string(d.day)
                        ></div>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[component]
pub fn AccountPage() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let stats = RwSignal::new(None::<UserStats>);

    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            spawn_local(async move {
                if let Ok(s) = my_stats().await {
                    stats.set(s);
                }
            });
        }
    });

    let sign_out = move |_| {
        spawn_local(async move {
            match logout().await {
                Ok(()) => {
                    auth.user.set(None);
                    stats.set(None);
                    toast(ToastKind::Info, |l| {
                        format!("{} またね!", td_string!(l, account.signed_out))
                    });
                }
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
        });
    };

    let registered = move || auth.user.with(|u| u.as_ref().is_some_and(|u| !u.guest));

    view! {
        <Title text=move || format!("{} · Hashi", t_string!(i18n, account.title)) />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto max-w-5xl px-4 py-8 sm:px-6">
            <div class="mb-6">
                <p class="font-pixel text-sm text-primary">"自分 · jibun"</p>
                <h1 class="text-3xl font-black">
                    {move || match auth.user.get() {
                        Some(u) if !u.guest => t_string!(i18n, account.hello, name = u.name),
                        _ => t_string!(i18n, account.title).to_string(),
                    }}
                </h1>
            </div>

            <div class="grid grid-cols-1 gap-6 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
                <Card class="gap-4 self-start">
                    <CardHeader>
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            {t!(i18n, account.record)} <span class="font-pixel text-sm text-primary">"記録"</span>
                        </CardTitle>
                        <CardDescription>
                            {move || match auth.user.get() {
                                Some(u) if u.guest => t_string!(i18n, account.guest_here, name = u.name),
                                Some(u) => t_string!(
                                    i18n,
                                    account.member_since,
                                    date = date_short(i18n.get_locale(), u.created_at.div_euclid(crate::model::MS_PER_DAY))
                                )
                                ,
                                None => t_string!(i18n, account.no_record).to_string(),
                            }}
                        </CardDescription>
                    </CardHeader>
                    <CardContent class="flex flex-col gap-4">
                        {move || match stats.get() {
                            Some(s) => {
                                view! {
                                    <div class="grid grid-cols-3 gap-2">
                                        <StatTile label=t_string!(i18n, account.stats.solved).to_string() jp="解" value=s.solved.to_string() />
                                        <StatTile label=t_string!(i18n, account.stats.streak).to_string() jp="連続" value=s.current_streak.to_string() />
                                        <StatTile label=t_string!(i18n, account.stats.best).to_string() jp="最長" value=s.best_streak.to_string() />
                                        <StatTile
                                            label=t_string!(i18n, account.stats.fastest).to_string()
                                            jp="最速"
                                            value=s.best_time_ms.map_or_else(|| "–".into(), format_duration)
                                        />
                                        <StatTile
                                            label=t_string!(i18n, account.stats.average).to_string()
                                            jp="平均"
                                            value=s.average_time_ms.map_or_else(|| "–".into(), format_duration)
                                        />
                                        <StatTile
                                            label=t_string!(i18n, account.stats.week).to_string()
                                            jp="週"
                                            value=s.recent.iter().rev().take(7).filter(|d| d.time_ms.is_some()).count().to_string()
                                        />
                                    </div>
                                    <div class="flex flex-col gap-2">
                                        <p class="text-xs font-bold uppercase tracking-[0.16em] text-muted-foreground">
                                            {t!(i18n, account.last_weeks)}
                                        </p>
                                        <Calendar stats=s />
                                    </div>
                                }
                                    .into_any()
                            }
                            None => {
                                view! {
                                    <div class="seigaiha grid h-40 place-items-center rounded-lg border border-dashed text-sm text-muted-foreground">
                                        <p class="rounded-md bg-card px-3 py-1">{t!(i18n, account.no_dailies)} " 待っています"</p>
                                    </div>
                                }
                                    .into_any()
                            }
                        }}
                    </CardContent>
                </Card>

                <Card class="gap-4 self-start">
                    <Show
                        when=registered
                        fallback=move || {
                            view! {
                                <CardHeader>
                                    <CardTitle class="flex items-baseline gap-2 text-xl">
                                        {t!(i18n, common.sign_in)} <span class="font-pixel text-sm text-primary">"入口"</span>
                                    </CardTitle>
                                    <CardDescription>
                                        {move || {
                                            if auth.user.with(|u| u.as_ref().is_some_and(|u| u.guest)) {
                                                t_string!(i18n, account.guest_cta)
                                            } else {
                                                t_string!(i18n, account.sign_in_cta)
                                            }
                                        }}
                                    </CardDescription>
                                </CardHeader>
                                <CardContent>
                                    <Tabs default_value="register" class="w-full">
                                        <TabsList class="w-full group-data-[orientation=Horizontal]/tabs:h-10 md:group-data-[orientation=Horizontal]/tabs:h-8">
                                            <TabsTrigger value="register">{t!(i18n, account.create)}</TabsTrigger>
                                            <TabsTrigger value="signin">{t!(i18n, common.sign_in)}</TabsTrigger>
                                        </TabsList>
                                        <TabsContent value="register" class="pt-2">
                                            <AuthForm mode=Mode::Register stats=stats />
                                        </TabsContent>
                                        <TabsContent value="signin" class="pt-2">
                                            <AuthForm mode=Mode::SignIn stats=stats />
                                        </TabsContent>
                                    </Tabs>
                                </CardContent>
                            }
                        }
                    >
                        <CardHeader>
                            <div class="flex items-center gap-4">
                                <span class="hanko grid size-16 shrink-0 rotate-[-6deg] place-items-center rounded-lg font-pixel text-3xl">
                                    {move || {
                                        auth.user
                                            .with(|u| u.as_ref().and_then(|u| u.name.chars().next()))
                                            .unwrap_or('?')
                                            .to_uppercase()
                                            .to_string()
                                    }}
                                </span>
                                <div>
                                    <CardTitle class="text-xl">{move || auth.user.with(|u| u.as_ref().map(|u| u.name.clone()))}</CardTitle>
                                    <CardDescription>{t!(i18n, account.name_hint)}</CardDescription>
                                </div>
                            </div>
                        </CardHeader>
                        <CardContent class="flex flex-col gap-2">
                            {move || {
                                auth.user
                                    .with(|u| u.as_ref().is_some_and(|u| u.admin))
                                    .then(|| {
                                        view! {
                                            <a
                                                href="/admin"
                                                class="inline-flex h-10 w-full items-center justify-center rounded-md bg-primary text-sm font-medium text-primary-foreground"
                                            >
                                                "管理 · Admin dashboard"
                                            </a>
                                        }
                                    })
                            }}
                            <Button variant=ButtonVariant::Outline class="w-full" on:click=sign_out>
                                {t!(i18n, account.sign_out)}
                            </Button>
                        </CardContent>
                    </Show>
                </Card>
            </div>
        </section>
    }
}
