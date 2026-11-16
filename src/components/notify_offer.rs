//! The one-time offer of the streak's last call, on the win card of a finished daily.
//!
//! Only where notifications can reach this device (`state::notify::can_deliver`), once per
//! account, and only to a player who hasn't turned them on yet (`model::should_offer`). In a
//! browser, yes turns them on in this browser first (`state::web_push`): if the player blocks
//! them there, the offer says how to allow them instead.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{answer_notification_offer, load_notification_prefs};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::i18n::{t, td_string, use_i18n};
use crate::model::{NotificationPrefs, should_offer};
use crate::state::notify::{can_deliver, time_zone};
use crate::state::web_push::{self, Outcome, Support};
use crate::state::{ToastKind, error_toast, toast};

/// The offer, or nothing. Seen counts as answered: it doesn't come again, yes or not.
#[component]
pub fn NotificationOffer() -> impl IntoView {
    let i18n = use_i18n();
    let prefs = RwSignal::new(None::<NotificationPrefs>);
    let open = RwSignal::new(false);
    if can_deliver() {
        spawn_local(async move {
            if let Ok(Some(loaded)) = load_notification_prefs().await
                && should_offer(Some(&loaded), true)
            {
                prefs.set(Some(loaded));
                open.set(true);
                // Shown once: noted at once, whatever the player does
                if let Err(e) = answer_notification_offer(false, time_zone()).await {
                    leptos::logging::warn!("could not note the notification offer: {e}");
                }
            }
        });
    }
    let blocked = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let accept = move |_| {
        let browser = web_push::here() == Support::Ready;
        let locale = i18n.get_locale_untracked();
        busy.set(true);
        spawn_local(async move {
            if browser {
                match web_push::subscribe(locale).await {
                    Outcome::On => {}
                    Outcome::Denied => {
                        busy.set(false);
                        blocked.set(true);
                        return;
                    }
                    Outcome::Dismissed => {
                        open.set(false);
                        return;
                    }
                    Outcome::Failed(e) => {
                        leptos::logging::warn!("could not turn notifications on here: {e}");
                        open.set(false);
                        toast(ToastKind::Error, |l| {
                            td_string!(l, account.notifications.push_failed).to_string()
                        });
                        return;
                    }
                }
            }
            open.set(false);
            if let Err(e) = answer_notification_offer(true, time_zone()).await {
                error_toast(e);
            }
        });
    };
    view! {
        <Show when=move || open.get()>
            <div class="flex w-full flex-col items-center gap-2 rounded-lg border bg-muted/50 p-3 text-sm" data-testid="notification-offer">
                <p class="font-bold">{t!(i18n, account.notifications.offer_title)}</p>
                // Blocked: how to allow them, and yes tries again
                <Show
                    when=move || blocked.get()
                    fallback=move || view! { <p class="text-muted-foreground">{t!(i18n, account.notifications.offer_text)}</p> }
                >
                    <p class="text-muted-foreground" role="status" data-testid="notifications-blocked">
                        {t!(i18n, account.notifications.blocked)}
                    </p>
                </Show>
                <div class="flex flex-wrap justify-center gap-2">
                    <Button size=ButtonSize::Sm attr:disabled=move || busy.get() on:click=accept>
                        {t!(i18n, account.notifications.offer_yes)}
                    </Button>
                    <Button size=ButtonSize::Sm variant=ButtonVariant::Ghost on:click=move |_| open.set(false)>
                        {t!(i18n, account.notifications.offer_no)}
                    </Button>
                </div>
            </div>
        </Show>
    }
}
