//! The account page: the daily record, signing in and out.

pub mod auth_form;
pub mod stats;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};

use crate::api::{logout, my_stats};
use crate::components::stamps::StampBook;
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::tabs::{Tabs, TabsContent, TabsList, TabsTrigger};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{date_short, error_text};
use crate::model::{UserStats, format_duration};
use crate::state::{Auth, ToastKind, toast};

use auth_form::{AuthForm, Mode};
use stats::{Calendar, StatTile};

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
                                        <StatTile
                                            label=t_string!(i18n, account.stats.month).to_string()
                                            jp="月"
                                            value=s.month_rank.map_or_else(|| "–".into(), |r| format!("#{r}"))
                                        />
                                        <StatTile label=t_string!(i18n, account.stats.points).to_string() jp="点" value=s.month_points.to_string() />
                                        <StatTile
                                            label=t_string!(i18n, account.stats.medals).to_string()
                                            jp="金銀銅"
                                            value=format!("{}·{}·{}", s.gold, s.silver, s.bronze)
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

                {move || {
                    stats
                        .get()
                        .filter(|s| !s.stamps.is_empty())
                        .map(|s| {
                            view! {
                                <Card class="gap-4 md:order-last md:col-span-2">
                                    <CardHeader>
                                        <CardTitle class="flex items-baseline gap-2 text-xl">
                                            {t!(i18n, stamps.title)} <span class="font-pixel text-sm text-primary">"判子帳"</span>
                                        </CardTitle>
                                        <CardDescription>{t!(i18n, stamps.intro)}</CardDescription>
                                    </CardHeader>
                                    <CardContent>
                                        <StampBook book=s.stamps />
                                    </CardContent>
                                </Card>
                            }
                        })
                }}

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
