//! The members and the group itself: the invite link, the members (the owner can remove them),
//! a new link for the owner, leaving for everyone.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;

use crate::api::{leave_group, remove_member, rotate_invite};
use crate::components::leaderboard::PlayerName;
use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::dialog::ConfirmDialog;
use crate::components::ui::separator::Separator;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{date_short, localized, player_name};
use crate::model::{GroupMember, GroupPage, MAX_MEMBERS, colour_var, group_day};
use crate::state::{ToastKind, error_toast, toast};

use super::GroupContext;
use super::archipelago::initial;
use super::invite::InviteLink;

#[component]
fn MemberRow(member: GroupMember, group: GroupContext, me: i64, owner: bool) -> impl IntoView {
    let i18n = use_i18n();
    let GroupMember {
        user_id,
        name,
        guest,
        colour,
        joined_at,
        owner: is_owner,
        stamp,
        ..
    } = member;
    let shown = player_name(i18n.get_locale_untracked(), &name);
    let removable = owner && user_id != me;
    let removing = RwSignal::new(false);
    let remove = {
        let name = shown.clone();
        move || {
            let Some(id) = group.get().map(|p| p.id) else {
                return;
            };
            let name = name.clone();
            spawn_local(async move {
                match remove_member(id, user_id).await {
                    Ok(()) => {
                        toast(ToastKind::Info, move |l| {
                            td_string!(l, groups.members.removed, name = name.clone())
                        });
                        group.reload();
                    }
                    Err(e) => error_toast(e),
                }
            });
        }
    };
    let joined = group_day(joined_at);
    let label_name = shown.clone();
    let question_name = shown.clone();
    let yes_name = shown;
    view! {
        <li class="flex items-center gap-3 py-2" data-member=user_id>
            <span
                class="nakama-stamp grid size-8 shrink-0 rotate-[-6deg] place-items-center rounded-md font-pixel text-sm"
                style=format!("--member: {}", colour_var(colour))
                aria-hidden="true"
            >
                {initial(&name)}
            </span>
            <div class="flex min-w-0 flex-1 flex-col">
                <div class="flex min-w-0 items-center gap-1.5 [&>div]:min-w-0">
                    <PlayerName user_id name guest stamp me=user_id == me />
                    {is_owner
                        .then(|| {
                            view! {
                                <Badge variant=BadgeVariant::Secondary size=BadgeSize::Sm class="shrink-0">
                                    "主 " {t!(i18n, groups.members.owner)}
                                </Badge>
                            }
                        })}
                </div>
                <span class="text-xs text-muted-foreground">
                    {move || td_string!(i18n.get_locale(), groups.members.joined, date = date_short(i18n.get_locale(), joined))}
                </span>
            </div>
            {removable
                .then(|| {
                    let label = move || td_string!(i18n.get_locale(), groups.members.remove_label, name = label_name.clone());
                    view! {
                        <Button
                            variant=ButtonVariant::Ghost
                            size=ButtonSize::IconSm
                            class="shrink-0 text-muted-foreground hover:text-destructive"
                            attr:aria-label=label.clone()
                            attr:data-tooltip=label
                            attr:aria-haspopup="dialog"
                            on:click=move |_| removing.set(true)
                        >
                            "✕"
                        </Button>
                        <ConfirmDialog
                            open=removing
                            question=Signal::derive(move || {
                                td_string!(i18n.get_locale(), groups.members.remove_confirm, name = question_name.clone())
                            })
                            yes=Signal::derive(move || {
                                td_string!(i18n.get_locale(), groups.members.remove_label, name = yes_name.clone())
                            })
                            on_yes=remove
                            destructive=true
                        />
                    }
                })}
        </li>
    }
}

/// The group page's members card.
#[component]
pub fn MembersCard(group: GroupContext) -> impl IntoView {
    let i18n = use_i18n();
    let navigate = use_navigate();
    let page = move || group.page.get();
    let is_owner = move || {
        group
            .page
            .with(|p| p.as_ref().is_some_and(GroupPage::is_owner))
    };

    let rotating = RwSignal::new(false);
    let rotate = move || {
        let Some(id) = group.get().map(|p| p.id) else {
            return;
        };
        spawn_local(async move {
            match rotate_invite(id).await {
                Ok(code) => {
                    group.page.update(|p| {
                        if let Some(p) = p {
                            p.invite = code;
                        }
                    });
                    toast(ToastKind::Success, |l| {
                        td_string!(l, groups.invite.rotated).to_string()
                    });
                }
                Err(e) => error_toast(e),
            }
        });
    };

    // The last one to leave takes the group along (`server::groups::take_out`)
    let last_one = move || {
        group
            .page
            .with(|p| p.as_ref().is_some_and(GroupPage::last_one))
    };
    let leaving = RwSignal::new(false);
    let leave = move || {
        let Some(p) = group.get() else { return };
        let l = i18n.get_locale_untracked();
        let last = p.last_one();
        let navigate = navigate.clone();
        spawn_local(async move {
            match leave_group(p.id).await {
                Ok(()) => {
                    let name = p.name.clone();
                    toast(ToastKind::Info, move |l| {
                        if last {
                            td_string!(l, groups.members.deleted, name = name.clone())
                        } else {
                            td_string!(l, groups.members.left, name = name.clone())
                        }
                    });
                    navigate(&localized(l, "/groups"), NavigateOptions::default());
                }
                Err(e) => error_toast(e),
            }
        });
    };

    let leave_label = move || {
        if last_one() {
            t_string!(i18n, groups.members.leave_delete).to_string()
        } else {
            t_string!(i18n, groups.members.leave).to_string()
        }
    };
    // The last one is told the group goes with them
    let leave_question = Signal::derive(move || {
        let l = i18n.get_locale();
        group.page.with(|p| match p {
            Some(p) if p.last_one() => {
                td_string!(l, groups.members.delete_confirm, name = p.name.clone())
            }
            Some(p) => td_string!(l, groups.members.leave_confirm, name = p.name.clone()),
            None => String::new(),
        })
    });
    let name = Signal::derive(move || {
        group
            .page
            .with(|p| p.as_ref().map(|p| p.name.clone()).unwrap_or_default())
    });
    let code = Signal::derive(move || {
        group
            .page
            .with(|p| p.as_ref().map(|p| p.invite.clone()).unwrap_or_default())
    });
    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, groups.members.title)} <span class="font-pixel text-sm text-primary">"仲間"</span>
                </CardTitle>
                <CardDescription>
                    {move || {
                        page()
                            .map(|p| td_string!(i18n.get_locale(), groups.members.count, count = p.members.len(), max = MAX_MEMBERS))
                    }}
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                <InviteLink name code />
                <Show when=is_owner>
                    <Button
                        variant=ButtonVariant::Ghost
                        size=ButtonSize::Sm
                        class="-mt-2 w-fit text-muted-foreground"
                        attr:aria-haspopup="dialog"
                        on:click=move |_| rotating.set(true)
                    >
                        "↻ " {t!(i18n, groups.invite.rotate)}
                    </Button>
                </Show>
                <ConfirmDialog
                    open=rotating
                    question=Signal::derive(move || t_string!(i18n, groups.invite.rotate_confirm).to_string())
                    yes=Signal::derive(move || t_string!(i18n, groups.invite.rotate).to_string())
                    on_yes=rotate
                />
                <Separator />
                <ul class="flex flex-col divide-y" aria-label=move || t_string!(i18n, groups.members.title)>
                    {move || {
                        page()
                            .map(|p| {
                                let (me, owner) = (p.me, p.is_owner());
                                p.members
                                    .into_iter()
                                    .map(|member| view! { <MemberRow member group me owner /> })
                                    .collect_view()
                            })
                    }}
                </ul>
                <Button
                    variant=ButtonVariant::Outline
                    size=ButtonSize::Sm
                    class="w-full text-destructive"
                    attr:aria-haspopup="dialog"
                    on:click=move |_| leaving.set(true)
                >
                    {leave_label}
                </Button>
                <ConfirmDialog open=leaving question=leave_question yes=Signal::derive(leave_label) on_yes=leave destructive=true />
            </CardContent>
        </Card>
    }
}
