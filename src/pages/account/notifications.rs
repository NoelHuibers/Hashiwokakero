//! The account's notification settings: the master switch and one per kind, saved with the
//! account (`api::save_notification_prefs`), with the time zone of this browser or phone for the
//! streak's last call.
//!
//! The settings are the account's, delivery is per device: in a browser that can get them
//! (`state::web_push`), turning them on turns them on here too (the browser asks for permission
//! first; without it the switch goes back), turning them off here as well, and a row says
//! whether this browser gets them, with a button for it.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{load_notification_prefs, save_notification_prefs};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::switch::Switch;
use crate::i18n::{Locale, t, td_string, use_i18n};
use crate::model::{NotificationPrefs, NotifyKind};
use crate::state::notify::{can_deliver, time_zone};
use crate::state::web_push::{self, Outcome, Support};
use crate::state::{ToastKind, error_toast, toast};

/// The name of a kind's switch.
fn kind_name(locale: Locale, kind: NotifyKind) -> String {
    match kind {
        NotifyKind::LastCall => td_string!(locale, account.notifications.last_call),
        NotifyKind::Race => td_string!(locale, account.notifications.race),
        NotifyKind::Rival => td_string!(locale, account.notifications.rival),
        NotifyKind::Shared => td_string!(locale, account.notifications.shared),
        NotifyKind::Group => td_string!(locale, account.notifications.group),
    }
    .to_string()
}

/// What a kind's switch is for.
fn kind_hint(locale: Locale, kind: NotifyKind) -> String {
    match kind {
        NotifyKind::LastCall => td_string!(locale, account.notifications.last_call_hint),
        NotifyKind::Race => td_string!(locale, account.notifications.race_hint),
        NotifyKind::Rival => td_string!(locale, account.notifications.rival_hint),
        NotifyKind::Shared => td_string!(locale, account.notifications.shared_hint),
        NotifyKind::Group => td_string!(locale, account.notifications.group_hint),
    }
    .to_string()
}

/// Sets a switch to what was saved, only if that changes it: a switch that changed means a tap.
fn show(switch: RwSignal<bool>, on: bool) {
    if switch.get_untracked() != on {
        switch.set(on);
    }
}

#[component]
pub(super) fn NotificationSettings() -> impl IntoView {
    let i18n = use_i18n();
    let prefs = RwSignal::new(None::<NotificationPrefs>);
    // What the server has: a failed save goes back to it
    let confirmed = StoredValue::new(None::<NotificationPrefs>);
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            spawn_local(async move {
                match load_notification_prefs().await {
                    Ok(loaded) => {
                        confirmed.set_value(loaded);
                        prefs.set(loaded);
                    }
                    Err(e) => {
                        leptos::logging::warn!("could not load the notification settings: {e}");
                    }
                }
            });
        }
    });
    // Shown at once, saved in the background, one save at a time: taps during a save are sent
    // after it, the latest only, so the server ends up with the last tap whatever the order of
    // the answers would have been
    let saving = StoredValue::new(false);
    let next_save = StoredValue::new(None::<NotificationPrefs>);
    let persist = move |next: NotificationPrefs| {
        prefs.set(Some(next));
        if saving.get_value() {
            next_save.set_value(Some(next));
            return;
        }
        saving.set_value(true);
        spawn_local(async move {
            let mut sending = next;
            loop {
                match save_notification_prefs(sending, time_zone()).await {
                    Ok(stored) => {
                        confirmed.set_value(Some(stored));
                        if let Some(later) = next_save.get_value() {
                            next_save.set_value(None);
                            sending = later;
                            continue;
                        }
                        prefs.set(Some(stored));
                    }
                    Err(e) => {
                        next_save.set_value(None);
                        prefs.set(confirmed.get_value());
                        error_toast(e);
                    }
                }
                break;
            }
            saving.set_value(false);
        });
    };

    // This browser, known once the page runs in it: whether it can get notifications, whether
    // it does, whether the player blocked them for the site, and a subscription under way
    let support = RwSignal::new(None::<Support>);
    let here = RwSignal::new(false);
    let blocked = RwSignal::new(false);
    let pending = RwSignal::new(false);
    let deliverable = RwSignal::new(None::<bool>);
    Effect::new(move || {
        support.set(Some(web_push::here()));
        here.set(web_push::is_on());
        blocked.set(web_push::blocked());
        deliverable.set(Some(can_deliver()));
    });
    let browser = move || support.get() == Some(Support::Ready);
    // Turns this browser on; `then` with whether it worked
    let turn_on_here = move |then: Box<dyn FnOnce(bool)>| {
        pending.set(true);
        let locale = i18n.get_locale_untracked();
        spawn_local(async move {
            let outcome = web_push::subscribe(locale).await;
            pending.set(false);
            here.set(outcome == Outcome::On);
            blocked.set(outcome == Outcome::Denied || web_push::blocked());
            if let Outcome::Failed(e) = &outcome {
                leptos::logging::warn!("could not turn notifications on here: {e}");
                toast(ToastKind::Error, |l| {
                    td_string!(l, account.notifications.push_failed).to_string()
                });
            }
            then(outcome == Outcome::On);
        });
    };
    let turn_off_here = move || {
        here.set(false);
        spawn_local(web_push::unsubscribe());
    };
    // Turning the notifications on in a browser that can get them turns them on here first: if
    // the browser may not show them, the switch goes back. Off turns this browser off.
    let save = move |next: NotificationPrefs| {
        let was_on = prefs.get_untracked().is_some_and(|p| p.enabled);
        let browser = support.get_untracked() == Some(Support::Ready);
        if next.enabled && !was_on && browser && !here.get_untracked() {
            prefs.set(Some(next));
            turn_on_here(Box::new(move |on| {
                if on {
                    persist(next);
                } else {
                    prefs.set(confirmed.get_value());
                }
            }));
            return;
        }
        if !next.enabled && was_on && here.get_untracked() {
            turn_off_here();
        }
        persist(next);
    };

    let master = RwSignal::new(false);
    let switches = NotifyKind::ALL.map(|kind| (kind, RwSignal::new(false)));
    // The saved settings on the switches
    Effect::new(move || {
        if let Some(p) = prefs.get() {
            show(master, p.enabled);
            for (kind, switch) in switches {
                show(switch, p.kind(kind));
            }
        }
    });
    // A tap on a switch
    Effect::new(move || {
        let on = master.get();
        if let Some(p) = prefs.get_untracked()
            && p.enabled != on
        {
            save(p.with_master(on));
        }
    });
    for (kind, switch) in switches {
        Effect::new(move || {
            let on = switch.get();
            if let Some(p) = prefs.get_untracked()
                && p.kind(kind) != on
            {
                save(p.with_kind(kind, on));
            }
        });
    }
    let loading = Signal::derive(move || prefs.with(Option::is_none) || pending.get());

    let rows = switches
        .into_iter()
        .map(|(kind, switch)| {
            let name = move || kind_name(i18n.get_locale(), kind);
            view! {
                <label class="flex min-h-10 cursor-pointer items-center justify-between gap-3 py-1 text-sm font-medium" data-notify-kind=kind.id()>
                    <span>
                        {name}
                        <span class="block text-xs font-normal text-muted-foreground">{move || kind_hint(i18n.get_locale(), kind)}</span>
                    </span>
                    <Switch checked=switch disabled=loading aria_label=Signal::derive(name) />
                </label>
            }
        })
        .collect_view();

    view! {
        <Card class="gap-4 self-start" attr:id="notifications" attr:data-testid="notification-settings">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, account.notifications.title)} <span class="font-pixel text-sm text-primary">"通知"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, account.notifications.text)}</CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-2">
                // Known after hydration: the server can't tell what the browser can do
                <Show when=move || support.get() == Some(Support::HomeScreen)>
                    <p class="rounded-md border border-dashed bg-muted/50 p-2 text-xs text-muted-foreground" data-testid="notifications-home-screen">
                        {t!(i18n, account.notifications.home_screen)}
                    </p>
                </Show>
                <Show when=move || deliverable.get() == Some(false) && support.get() != Some(Support::HomeScreen)>
                    <p class="rounded-md border border-dashed bg-muted/50 p-2 text-xs text-muted-foreground" data-testid="notifications-unavailable">
                        {t!(i18n, account.notifications.unavailable)}
                    </p>
                </Show>
                <label class="flex min-h-10 cursor-pointer items-center justify-between gap-3 text-sm font-semibold" data-testid="notify-master">
                    <span>
                        {t!(i18n, account.notifications.master)}
                        <span class="block text-xs font-normal text-muted-foreground">{t!(i18n, account.notifications.master_hint)}</span>
                    </span>
                    <Switch
                        checked=master
                        disabled=loading
                        aria_label=Signal::derive(move || td_string!(i18n.get_locale(), account.notifications.master).to_string())
                    />
                </label>
                <Show when=move || browser() && blocked.get()>
                    <p class="rounded-md border border-destructive/40 bg-destructive/10 p-2 text-xs" role="status" data-testid="notifications-blocked">
                        {t!(i18n, account.notifications.blocked)}
                    </p>
                </Show>
                <Show when=move || browser() && (master.get() || here.get())>
                    <div class="flex min-h-10 items-center justify-between gap-3 rounded-md bg-muted/50 px-2 py-1 text-sm" data-testid="notify-this-browser">
                        <span>
                            {t!(i18n, account.notifications.this_browser)}
                            <span class="block text-xs text-muted-foreground" data-testid="notify-this-browser-state">
                                {move || {
                                    if here.get() {
                                        td_string!(i18n.get_locale(), account.notifications.browser_on)
                                    } else {
                                        td_string!(i18n.get_locale(), account.notifications.browser_off)
                                    }
                                }}
                            </span>
                        </span>
                        <Button
                            size=ButtonSize::Sm
                            variant=ButtonVariant::Outline
                            attr:disabled=move || pending.get()
                            on:click=move |_| {
                                if here.get_untracked() {
                                    turn_off_here();
                                } else {
                                    turn_on_here(Box::new(|_| {}));
                                }
                            }
                        >
                            {move || {
                                if here.get() {
                                    td_string!(i18n.get_locale(), account.notifications.turn_off_here)
                                } else {
                                    td_string!(i18n.get_locale(), account.notifications.turn_on_here)
                                }
                            }}
                        </Button>
                    </div>
                </Show>
                <div class="flex flex-col border-s-2 ps-3" class:opacity-60=move || !master.get()>{rows}</div>
            </CardContent>
        </Card>
    }
}
