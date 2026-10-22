//! Groups (仲間 nakama): `/groups` lists the player's groups and makes new ones, `/groups/:id` is
//! a group's page, `/g/:code` the invite link's join page.
//!
//! The group page is a stack of cards, each in a file of its own, that get the page's data from
//! a [`GroupContext`]. A new card is a component taking `group: GroupContext`, added to the
//! stack in [`GroupView`]: the main column for what happens in the group, the side column for
//! the group itself.

pub mod archipelago;
pub mod awards;
pub mod banzuke;
pub mod create;
pub mod hanko;
pub mod invite;
pub mod join;
pub mod list;
pub mod members;
pub mod replay;
pub mod river;
pub mod today;
pub mod week;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::{load_group, my_groups};
use crate::components::celebration::Stamp;
use crate::components::layout::use_localized;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::date_short;
use crate::model::{GroupPage, GroupSummary, MAX_MEMBERS, group_day};
use crate::seo::title_text;
use crate::state::{Auth, error_toast};

use archipelago::Archipelago;
use banzuke::BanzukeCard;
use create::{CreateCard, ReadyCard};
use hanko::HankoCard;
use list::GroupList;
use members::MembersCard;
use replay::ReplayCard;
use river::RiverCard;
use today::TodayCard;
use week::WeekCard;

pub use join::JoinGroupPage;

/// The data of the group page, for its cards.
#[derive(Clone, Copy, Debug)]
pub struct GroupContext {
    /// The group's page, `None` until it's loaded.
    pub page: RwSignal<Option<GroupPage>>,
    /// Counts the reloads asked for, see [`Self::reload`].
    reloads: RwSignal<u32>,
}

impl GroupContext {
    /// Loads the page again, after a change (a member removed, a new link).
    pub fn reload(self) {
        self.reloads.update(|n| *n += 1);
    }

    /// The group's page, without tracking.
    pub fn get(self) -> Option<GroupPage> {
        self.page.get_untracked()
    }
}

/// The list of the player's groups and the form for a new one.
#[component]
pub fn GroupsPage() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    // `None` while loading
    let groups = RwSignal::new(None::<Vec<GroupSummary>>);
    // The group just made, for its invite step
    let created = RwSignal::new(None::<GroupPage>);

    // Again when the visitor changes: signed in, or a guest made by creating a group
    Effect::new(move |_| {
        let _ = auth.user.with(|u| u.as_ref().map(|u| u.id));
        if !auth.loaded.get() {
            return;
        }
        spawn_local(async move {
            match my_groups().await {
                Ok(list) => groups.set(Some(list)),
                Err(e) => error_toast(e),
            }
        });
    });

    view! {
        <Title text=title_text(move || format!("{} · Hashi", t_string!(i18n, groups.title))) />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto max-w-7xl px-4 py-6 sm:px-6">
            <div class="mb-5">
                <p class="font-pixel text-sm text-primary">"仲間 · nakama"</p>
                <h1 class="text-3xl font-black">{t!(i18n, groups.title)}</h1>
                <p class="mt-1 max-w-2xl text-muted-foreground">{t!(i18n, groups.intro)}</p>
            </div>
            <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]">
                <div class="flex min-w-0 flex-col gap-6">
                    {move || created.get().map(|page| view! { <ReadyCard page /> })}
                    <GroupList groups />
                </div>
                <aside class="flex flex-col gap-6">
                    <CreateCard created />
                </aside>
            </div>
        </section>
    }
}

/// A group's page, for its members. Others get the same "no such group" a group that doesn't
/// exist gets.
#[component]
pub fn GroupDetailPage() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let params = use_params_map();
    let back = use_localized("/groups");
    let id = Memo::new(move |_| params.with(|p| p.get("id").and_then(|id| id.parse::<i64>().ok())));
    let group = GroupContext {
        page: RwSignal::new(None),
        reloads: RwSignal::new(0),
    };
    provide_context(group);
    // `Some(false)`: no such group, or not a member
    let found = RwSignal::new(None::<bool>);

    Effect::new(move |previous: Option<Option<i64>>| {
        let id = id.get();
        group.reloads.track();
        let _ = auth.user.with(|u| u.as_ref().map(|u| u.id));
        if !auth.loaded.get() {
            return id;
        }
        // A reload of the same group keeps showing it meanwhile
        if previous.flatten() != id {
            group.page.set(None);
            found.set(None);
        }
        let Some(id) = id else {
            found.set(Some(false));
            return None;
        };
        spawn_local(async move {
            match load_group(id).await {
                Ok(page) => {
                    found.set(Some(page.is_some()));
                    group.page.set(page);
                }
                Err(e) => error_toast(e),
            }
        });
        Some(id)
    });

    let title = move || {
        group.page.with(|p| match p {
            Some(p) => format!("{} · Hashi", p.name),
            None => format!("{} · Hashi", t_string!(i18n, groups.page.title)),
        })
    };
    view! {
        <Title text=title_text(title) />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto flex max-w-7xl flex-col gap-5 px-4 py-6 sm:px-6">
            <A href=back attr:class="w-fit text-sm text-primary hover:underline">
                "← " {t!(i18n, groups.page.back)}
            </A>
            {move || match found.get() {
                None => view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                Some(false) => view! { <NotMember /> }.into_any(),
                Some(true) => view! { <GroupView group /> }.into_any(),
            }}
        </section>
    }
}

/// A group that doesn't exist, or that the visitor isn't in.
#[component]
fn NotMember() -> impl IntoView {
    let i18n = use_i18n();
    let groups = use_localized("/groups");
    view! {
        <div class="mx-auto flex max-w-xl flex-col items-center gap-4 py-16 text-center" data-testid="group-not-found">
            <Stamp kanji="迷" class="size-20 text-5xl" />
            <h1 class="text-2xl font-black">{t!(i18n, groups.page.not_found)}</h1>
            <p class="text-muted-foreground">{t!(i18n, groups.page.not_found_hint)}</p>
            <A href=groups attr:class="rounded-md bg-primary px-4 py-2 font-medium text-primary-foreground">
                {t!(i18n, groups.page.back)}
            </A>
        </div>
    }
}

/// The group: its name, the archipelago, and the stack of cards.
#[component]
fn GroupView(group: GroupContext) -> impl IntoView {
    let i18n = use_i18n();
    // Last week's yokozuna, crowned on the archipelago once the banzuke is loaded
    let crowns = RwSignal::new(Vec::<i64>::new());
    let header = move || {
        group.page.get().map(|p| {
            let members = p.members.len();
            let since = group_day(p.created_at);
            view! {
                <div class="flex items-center gap-3">
                    <span class="hanko grid size-14 shrink-0 rotate-[-4deg] place-items-center rounded-lg font-pixel text-3xl">
                        "仲"
                    </span>
                    <div class="min-w-0">
                        <p class="font-pixel text-sm text-primary">"仲間 · nakama"</p>
                        <h1 class="break-words text-3xl font-black">{p.name.clone()}</h1>
                        <p class="text-sm text-muted-foreground">
                            {move || {
                                let l = i18n.get_locale();
                                format!(
                                    "{} · {}",
                                    td_string!(l, groups.members.count, count = members, max = MAX_MEMBERS),
                                    td_string!(l, groups.page.since, date = date_short(l, since)),
                                )
                            }}
                        </p>
                    </div>
                </div>
                <div class="flex flex-col gap-2">
                    <Archipelago group=p.id members=p.members shared=p.shared me=p.me crowned=crowns />
                    <p class="text-xs text-muted-foreground">{t!(i18n, groups.archipelago.legend)}</p>
                </div>
            }
        })
    };
    view! {
        {header}
        // What the group does together, across the page
        <RiverCard group />
        // The stack of cards: what happens in the group on the left, the group on the right
        <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]">
            <div class="flex min-w-0 flex-col gap-6">
                <TodayCard group />
                <HankoCard group />
                <ReplayCard group />
                <WeekCard group />
                <BanzukeCard group crowns />
            </div>
            <aside class="flex min-w-0 flex-col gap-6">
                <MembersCard group />
            </aside>
        </div>
    }
}
