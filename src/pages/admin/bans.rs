//! Banning players, and lifting bans.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{admin_ban_player, admin_banned_players, admin_unban_player};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::model::{BanOutcome, BannedPlayer, MS_PER_DAY, date_string};
use crate::state::{ToastKind, toast};

use super::{SECTION, error};

/// "17" or "#17", as the profile's address or the daily review show it.
fn player_number(input: &str) -> Option<i64> {
    input
        .trim()
        .trim_start_matches('#')
        .parse()
        .ok()
        .filter(|&id| id > 0)
}

fn account(b: &BannedPlayer) -> String {
    let id = b.id;
    b.username
        .as_ref()
        .map_or_else(|| format!("guest #{id}"), |u| format!("@{u} · #{id}"))
}

#[component]
pub(super) fn BansAdmin(on_change: Callback<()>) -> impl IntoView {
    let list = RwSignal::new(None::<Vec<BannedPlayer>>);
    let load = move || {
        spawn_local(async move {
            match admin_banned_players().await {
                Ok(l) => list.set(Some(l)),
                Err(e) => error(&e),
            }
        });
    };
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            load();
        }
    });
    let done = move || {
        load();
        on_change.run(());
    };
    let player = RwSignal::new(String::new());
    let reason = RwSignal::new(String::new());
    let ban = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let Some(id) = player_number(&player.get_untracked()) else {
            toast(ToastKind::Error, |_| "That's no player number.".to_string());
            return;
        };
        if !window()
            .confirm_with_message(&format!(
                "Ban player #{id}? They are signed out and can't sign in, and their results leave every board until the ban is lifted."
            ))
            .unwrap_or(false)
        {
            return;
        }
        let why = reason.get_untracked();
        spawn_local(async move {
            match admin_ban_player(id, why).await {
                Ok(BanOutcome::Banned(name)) => {
                    toast(ToastKind::Success, move |_| {
                        format!("{name} (#{id}) is banned.")
                    });
                    player.set(String::new());
                    reason.set(String::new());
                    done();
                }
                Ok(BanOutcome::NoSuchPlayer) => toast(ToastKind::Error, move |_| {
                    format!("There is no player #{id}.")
                }),
                Ok(BanOutcome::Admin) => toast(ToastKind::Error, move |_| {
                    format!("Player #{id} is an admin, admins can't be banned.")
                }),
                Err(e) => error(&e),
            }
        });
    };
    let unban = move |id: i64| {
        if !window()
            .confirm_with_message(&format!("Lift the ban of player #{id}?"))
            .unwrap_or(false)
        {
            return;
        }
        spawn_local(async move {
            match admin_unban_player(id).await {
                Ok(_) => done(),
                Err(e) => error(&e),
            }
        });
    };
    // Inputs at 16px on phones, or iOS zooms in on focus
    let input = "h-10 min-w-0 rounded-md border bg-transparent px-3 text-base sm:h-9 sm:text-sm";

    view! {
        <Card class=SECTION>
            <CardHeader class="max-sm:px-0">
                <CardTitle class="text-xl">"Bans"</CardTitle>
                <CardDescription>
                    "A banned player is signed out everywhere and can't sign in again; their username stays taken. All their daily results leave the boards and the hall of fame, their solvers the Dojo's board, and their profile is gone. Nothing is deleted: lifting the ban puts it all back. Banned guests are deleted with the other guests nobody can sign in to, a day later."
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-4 max-sm:px-0">
                <form class="flex flex-wrap items-center gap-2" on:submit=ban>
                    <label for="ban-player" class="text-sm">"Player #"</label>
                    <input
                        id="ban-player"
                        class=format!("{input} w-24")
                        inputmode="numeric"
                        placeholder="17"
                        prop:value=move || player.get()
                        on:input=move |ev| player.set(event_target_value(&ev))
                    />
                    <input
                        id="ban-reason"
                        aria-label="Reason"
                        class=format!("{input} w-full sm:w-72")
                        maxlength="500"
                        placeholder="Reason (only admins see it)"
                        prop:value=move || reason.get()
                        on:input=move |ev| reason.set(event_target_value(&ev))
                    />
                    <Button variant=ButtonVariant::Destructive size=ButtonSize::Sm attr:r#type="submit">"Ban"</Button>
                    <span class="text-xs text-muted-foreground">"The number is in the address of their profile, /player/17, and in the daily review."</span>
                </form>
                {move || match list.get() {
                    None => view! { <p class="text-sm text-muted-foreground">"…"</p> }.into_any(),
                    Some(l) if l.is_empty() => view! { <p class="text-sm text-muted-foreground">"Nobody is banned."</p> }.into_any(),
                    Some(l) => l
                        .into_iter()
                        .map(|b| {
                            let id = b.id;
                            view! {
                                <article class="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border bg-card p-3">
                                    <span class="font-medium">{b.name.clone()}</span>
                                    <span class="font-mono text-xs text-muted-foreground">{account(&b)}</span>
                                    <span class="text-sm text-muted-foreground tabular-nums">{date_string(b.banned_at.div_euclid(MS_PER_DAY))}</span>
                                    <span class="flex-1"></span>
                                    <Button variant=ButtonVariant::Outline size=ButtonSize::Sm on:click=move |_| unban(id)>"Lift ban"</Button>
                                    {(!b.reason.is_empty()).then(|| view! {
                                        <p class="w-full whitespace-pre-wrap break-words text-sm">{b.reason}</p>
                                    })}
                                </article>
                            }
                        })
                        .collect_view()
                        .into_any(),
                }}
            </CardContent>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_numbers() {
        assert_eq!(player_number("17"), Some(17));
        assert_eq!(player_number(" #17 "), Some(17));
        assert_eq!(player_number(""), None);
        assert_eq!(player_number("noel"), None);
        assert_eq!(player_number("0"), None);
        assert_eq!(player_number("-3"), None);
    }

    #[test]
    fn accounts() {
        let banned = BannedPlayer {
            id: 7,
            name: "Sleepy Koi 12".into(),
            username: None,
            banned_at: 0,
            reason: String::new(),
        };
        assert_eq!(account(&banned), "guest #7");
        let named = BannedPlayer {
            username: Some("noel".into()),
            ..banned
        };
        assert_eq!(account(&named), "@noel · #7");
    }
}
