//! The one-time offer of the streak's last call, on the win card of a finished daily.
//!
//! Only where notifications can reach this device (`state::notify::can_deliver`), once per
//! account, and only to a player who hasn't turned them on yet (`model::should_offer`). Yes asks
//! this device for leave first (Android), then registers it (`state::notify::follow`).

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{answer_notification_offer, load_notification_prefs};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::i18n::{t, use_i18n};
use crate::model::{NotificationPrefs, should_offer};
use crate::state::error_toast;
use crate::state::notify::{allowed_here, can_deliver, follow, time_zone};

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
    // Refused by the device: the offer stays, with where to allow them
    let refused = RwSignal::new(false);
    let accept = move |_| {
        spawn_local(async move {
            if !allowed_here().await {
                refused.set(true);
                return;
            }
            open.set(false);
            match answer_notification_offer(true, time_zone()).await {
                Ok(saved) => {
                    if let Err(e) = follow(saved.enabled, i18n.get_locale_untracked()).await {
                        error_toast(e);
                    }
                }
                Err(e) => error_toast(e),
            }
        });
    };
    view! {
        <Show when=move || open.get()>
            <div class="flex w-full flex-col items-center gap-2 rounded-lg border bg-muted/50 p-3 text-sm" data-testid="notification-offer">
                <p class="font-bold">{t!(i18n, account.notifications.offer_title)}</p>
                <p class="text-muted-foreground">{t!(i18n, account.notifications.offer_text)}</p>
                <Show when=move || refused.get()>
                    <p class="text-xs text-muted-foreground" data-testid="notifications-refused">
                        {t!(i18n, account.notifications.refused)}
                    </p>
                </Show>
                <div class="flex flex-wrap justify-center gap-2">
                    <Button size=ButtonSize::Sm on:click=accept>
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
