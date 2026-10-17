use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{login, logout, my_stats, register};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::input::{Input, InputType};
use crate::components::ui::label::Label;
use crate::components::ui::tabs::{Tabs, TabsContent, TabsList, TabsTrigger};
use crate::model::{UserStats, date_string, format_duration, pretty_date};
use crate::state::{Auth, ToastKind, toast};
use leptos_meta::{Meta, Title};

/// Server errors arrive as "error running server function: …", keep the message only.
fn message(e: ServerFnError) -> String {
    let text = e.to_string();
    text.rsplit_once(": ").map_or(text.clone(), |(_, m)| m.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    SignIn,
    Register,
}

#[component]
fn AuthForm(mode: Mode, stats: RwSignal<Option<UserStats>>) -> impl IntoView {
    let auth = expect_context::<Auth>();
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let id = |field: &str| format!("{}-{field}", if mode == Mode::SignIn { "login" } else { "register" });

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
                    toast(ToastKind::Success, format!("Welcome, {}! ようこそ", user.name));
                    auth.user.set(Some(user));
                    password.set(String::new());
                    if let Ok(s) = my_stats().await {
                        stats.set(s);
                    }
                }
                Err(e) => error.set(Some(message(e))),
            }
            busy.set(false);
        });
    };

    view! {
        <form class="flex flex-col gap-4" on:submit=submit>
            <div class="flex flex-col gap-2">
                <Label html_for=id("username")>"Username"</Label>
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
                <Label html_for=id("password")>"Password"</Label>
                <Input
                    id=id("password")
                    r#type=InputType::Password
                    bind_value=password
                    autocomplete=if mode == Mode::SignIn { "current-password" } else { "new-password" }
                    required=true
                    minlength=if mode == Mode::SignIn { 1 } else { 8 }
                />
                {(mode == Mode::Register)
                    .then(|| view! { <p class="text-xs text-muted-foreground">"At least 8 characters."</p> })}
            </div>
            {move || error.get().map(|e| view! { <p class="text-sm text-destructive">{e}</p> })}
            <Button attr:r#type="submit" attr:disabled=move || busy.get() class="w-full">
                {move || match (mode, busy.get()) {
                    (_, true) => "Just a moment…",
                    (Mode::SignIn, false) => "Sign in 入る",
                    (Mode::Register, false) => "Create account 登録",
                }}
            </Button>
        </form>
    }
}

#[component]
fn StatTile(label: &'static str, jp: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex flex-col items-center rounded-lg border bg-background px-2 py-3">
            <span class="font-pixel text-2xl tabular-nums leading-tight">{value}</span>
            <span class="text-[10px] font-bold uppercase tracking-[0.14em] text-muted-foreground">
                {label} " · " {jp}
            </span>
        </div>
    }
}

/// The last weeks as a grid of pixel squares, gold for top 3 finishes.
#[component]
fn Calendar(stats: UserStats) -> impl IntoView {
    view! {
        <div class="grid grid-cols-7 gap-1.5">
            {stats
                .recent
                .into_iter()
                .map(|d| {
                    let (class, title) = match (d.time_ms, d.rank) {
                        (Some(t), Some(r)) if r <= 3 => {
                            ("bg-[var(--px-gold)]", format!("{} · {} · rank {r}", pretty_date(d.day), format_duration(t)))
                        }
                        (Some(t), r) => {
                            (
                                "bg-[var(--px-grass)]",
                                format!(
                                    "{} · {}{}",
                                    pretty_date(d.day),
                                    format_duration(t),
                                    r.map(|r| format!(" · rank {r}")).unwrap_or_default(),
                                ),
                            )
                        }
                        _ => ("bg-muted", format!("{} · not solved", pretty_date(d.day))),
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
                    toast(ToastKind::Info, "Signed out. またね!");
                }
                Err(e) => toast(ToastKind::Error, message(e)),
            }
        });
    };

    let registered = move || auth.user.with(|u| u.as_ref().is_some_and(|u| !u.guest));

    view! {
        <Title text="Your account · Hashi" />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto max-w-5xl px-4 py-8 sm:px-6">
            <div class="mb-6">
                <p class="font-pixel text-sm text-primary">"自分 · jibun"</p>
                <h1 class="text-3xl font-black">
                    {move || match auth.user.get() {
                        Some(u) if !u.guest => format!("こんにちは, {}", u.name),
                        _ => "Your account".to_string(),
                    }}
                </h1>
            </div>

            <div class="grid gap-6 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
                <Card class="gap-4 self-start">
                    <CardHeader>
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            "Your bridges" <span class="font-pixel text-sm text-primary">"記録"</span>
                        </CardTitle>
                        <CardDescription>
                            {move || match auth.user.get() {
                                Some(u) if u.guest => format!("Playing as {} (guest) in this browser.", u.name),
                                Some(u) => format!("Member since {}.", pretty_date(u.created_at.div_euclid(crate::model::MS_PER_DAY))),
                                None => "Play a daily puzzle to start your record.".to_string(),
                            }}
                        </CardDescription>
                    </CardHeader>
                    <CardContent class="flex flex-col gap-4">
                        {move || match stats.get() {
                            Some(s) => {
                                view! {
                                    <div class="grid grid-cols-3 gap-2">
                                        <StatTile label="Solved" jp="解" value=s.solved.to_string() />
                                        <StatTile label="Streak" jp="連続" value=format!("{}日", s.current_streak) />
                                        <StatTile label="Best" jp="最長" value=format!("{}日", s.best_streak) />
                                        <StatTile
                                            label="Fastest"
                                            jp="最速"
                                            value=s.best_time_ms.map(format_duration).unwrap_or("–".into())
                                        />
                                        <StatTile
                                            label="Average"
                                            jp="平均"
                                            value=s.average_time_ms.map(format_duration).unwrap_or("–".into())
                                        />
                                        <StatTile
                                            label="This week"
                                            jp="週"
                                            value=s.recent.iter().rev().take(7).filter(|d| d.time_ms.is_some()).count().to_string()
                                        />
                                    </div>
                                    <div class="flex flex-col gap-2">
                                        <p class="text-xs font-bold uppercase tracking-[0.16em] text-muted-foreground">
                                            "Last five weeks"
                                        </p>
                                        <Calendar stats=s />
                                    </div>
                                }
                                    .into_any()
                            }
                            None => {
                                view! {
                                    <div class="seigaiha grid h-40 place-items-center rounded-lg border border-dashed text-sm text-muted-foreground">
                                        <p class="rounded-md bg-card px-3 py-1">"No dailies yet. 待っています"</p>
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
                                        "Sign in" <span class="font-pixel text-sm text-primary">"入口"</span>
                                    </CardTitle>
                                    <CardDescription>
                                        {move || {
                                            if auth.user.with(|u| u.as_ref().is_some_and(|u| u.guest)) {
                                                "Create an account to keep this guest's results and streak, and to play on other devices."
                                            } else {
                                                "Sign in to see your streak on any device. You can also just play, as a guest."
                                            }
                                        }}
                                    </CardDescription>
                                </CardHeader>
                                <CardContent>
                                    <Tabs default_value="register" class="w-full">
                                        <TabsList class="w-full">
                                            <TabsTrigger value="register">"Create account"</TabsTrigger>
                                            <TabsTrigger value="signin">"Sign in"</TabsTrigger>
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
                                    <CardDescription>"Your results count on the leaderboard under this name."</CardDescription>
                                </div>
                            </div>
                        </CardHeader>
                        <CardContent>
                            <Button variant=ButtonVariant::Outline class="w-full" on:click=sign_out>
                                "Sign out"
                            </Button>
                        </CardContent>
                    </Show>
                </Card>
            </div>
        </section>
    }
}
