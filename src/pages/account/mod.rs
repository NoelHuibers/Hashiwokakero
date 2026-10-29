//! The account page: the daily record, signing in and out, renaming and deleting the account,
//! the players one blocked, and the way to the feedback form.

pub mod auth_form;
pub mod blocked;
pub mod delete;
pub mod rename;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;

use crate::api::{logout, my_stats};
use crate::components::record::Record;
use crate::components::stamps::{GamesButton, StampBook};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::tabs::{Tabs, TabsContent, TabsList, TabsTrigger};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{date_short, prefix};
use crate::model::{FeedbackKind, UserStats};
use crate::pages::contact::feedback_link;
use crate::seo::title_text;
use crate::state::{Auth, ToastKind, error_toast, toast};

use auth_form::{AuthForm, Mode};
use blocked::BlockedPlayers;
use delete::DeleteAccount;
use rename::ChangeUsername;

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
    // In the app, the stamp book goes to Game Center or Play Games too: the page's own, and
    // the account's after a sign in (`api::native::games_progress`)
    #[cfg(feature = "csr")]
    Effect::new(move || {
        stats.with(|s| {
            if let Some(s) = s {
                crate::api::native::games_progress(s);
            }
        });
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
                Err(e) => error_toast(e),
            }
        });
    };

    let registered = move || auth.user.with(|u| u.as_ref().is_some_and(|u| !u.guest));

    view! {
        <Title text=title_text(move || format!("{} · Hashi", t_string!(i18n, account.title))) />
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
                            Some(s) => view! { <Record stats=s /> }.into_any(),
                            None => {
                                view! {
                                    <div class="seigaiha grid h-40 place-items-center rounded-lg border border-dashed text-sm text-muted-foreground">
                                        <p class="rounded-md bg-card px-3 py-1">{t!(i18n, account.no_dailies)} " 待っています"</p>
                                    </div>
                                }
                                    .into_any()
                            }
                        }}
                        {move || {
                            stats.with(Option::is_some).then(|| auth.user.with(|u| u.as_ref().map(|u| u.id))).flatten().map(|id| {
                                let href = move || format!("{}/player/{id}", prefix(i18n.get_locale()));
                                view! {
                                    <A href=href attr:class="w-fit text-sm text-primary underline-offset-4 hover:underline">
                                        {t!(i18n, account.public_profile)} " →"
                                    </A>
                                }
                            })
                        }}
                    </CardContent>
                </Card>

                {move || {
                    stats
                        .get()
                        .filter(|s| !s.stamps.is_empty())
                        .map(|s| {
                            let shown = RwSignal::new(s.shown);
                            view! {
                                <Card class="gap-4 md:order-last md:col-span-2">
                                    <CardHeader>
                                        <CardTitle class="flex items-baseline gap-2 text-xl">
                                            {t!(i18n, stamps.title)} <span class="font-pixel text-sm text-primary">"判子帳"</span>
                                        </CardTitle>
                                        <CardDescription>{t!(i18n, stamps.intro)}</CardDescription>
                                    </CardHeader>
                                    <CardContent class="flex flex-col gap-4">
                                        <StampBook book=s.stamps players=s.players shown pick=true />
                                        <GamesButton />
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
                            // Some admins keep their name, see `auth::change_username`
                            {move || auth.user.with(|u| u.as_ref().is_some_and(|u| !u.name_locked)).then(|| view! { <ChangeUsername /> })}
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

                <FeedbackCard />
                {move || auth.user.with(Option::is_some).then(|| view! { <BlockedPlayers /> })}
                {move || auth.user.with(Option::is_some).then(|| view! { <DeleteAccount stats /> })}
            </div>
        </section>
    }
}

/// The way to us from the account: a question (about the account, say) or a bug.
#[component]
fn FeedbackCard() -> impl IntoView {
    let i18n = use_i18n();
    let link =
        move |kind: FeedbackKind| move || feedback_link(i18n.get_locale(), Some(kind), "/account");
    let button = "inline-flex h-10 flex-1 items-center justify-center rounded-md border bg-background px-3 text-sm font-medium shadow-xs hover:bg-accent";
    view! {
        <Card class="gap-4 self-start">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, account.feedback.title)} <span class="font-pixel text-sm text-primary">"意見"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, account.feedback.text)}</CardDescription>
            </CardHeader>
            <CardContent class="flex flex-wrap gap-2">
                <A href=link(FeedbackKind::Question) attr:class=button>{t!(i18n, common.help.question)}</A>
                <A href=link(FeedbackKind::Bug) attr:class=button>{t!(i18n, common.help.bug)}</A>
            </CardContent>
        </Card>
    }
}
