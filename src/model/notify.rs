//! Notifications: what a player wants to be told about (per account, see `server::notify`), the
//! one-time offer, and what a device gets.

use serde::{Deserialize, Serialize};

/// What a notification is about. Each has its own switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NotifyKind {
    /// The streak's last call: today's daily isn't solved yet and the next one comes soon.
    LastCall,
    /// Someone in one of the player's groups opened a race.
    Race,
    /// A rival beat the player's time on today's daily.
    Rival,
    /// Players solved a puzzle the player shared.
    Shared,
    /// A new puzzle (kadai) was posted in one of the player's groups.
    Group,
}

impl NotifyKind {
    pub const ALL: [Self; 5] = [
        Self::LastCall,
        Self::Race,
        Self::Rival,
        Self::Shared,
        Self::Group,
    ];

    /// The id stored in the outbox and sent to devices. Never changes.
    pub const fn id(self) -> &'static str {
        match self {
            Self::LastCall => "last_call",
            Self::Race => "race",
            Self::Rival => "rival",
            Self::Shared => "shared",
            Self::Group => "group",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }
}

/// A player's notification settings, kept with their account: the master switch, a switch per
/// kind, and whether the one-time offer was shown. Everything off for a new player.
///
/// The master switch keeps the kinds' switches: off and on again brings back the same choice.
/// The kinds a player picks only count while it is on ([`Self::wants`]). A kind that comes later
/// is off for a page or app that doesn't send it yet (`serde(default)`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "switches, each on or off on its own"
)]
pub struct NotificationPrefs {
    pub enabled: bool,
    pub last_call: bool,
    pub race: bool,
    pub rival: bool,
    pub shared: bool,
    pub group: bool,
    /// The offer after a finished daily was shown (accepted or not): it never comes again.
    pub offer_shown: bool,
}

impl NotificationPrefs {
    /// The switch of one kind, whatever the master switch says.
    pub const fn kind(&self, kind: NotifyKind) -> bool {
        match kind {
            NotifyKind::LastCall => self.last_call,
            NotifyKind::Race => self.race,
            NotifyKind::Rival => self.rival,
            NotifyKind::Shared => self.shared,
            NotifyKind::Group => self.group,
        }
    }

    const fn set(&mut self, kind: NotifyKind, on: bool) {
        match kind {
            NotifyKind::LastCall => self.last_call = on,
            NotifyKind::Race => self.race = on,
            NotifyKind::Rival => self.rival = on,
            NotifyKind::Shared => self.shared = on,
            NotifyKind::Group => self.group = on,
        }
    }

    pub fn any_kind(&self) -> bool {
        NotifyKind::ALL.into_iter().any(|k| self.kind(k))
    }

    /// Whether the player gets notifications of `kind`.
    pub const fn wants(&self, kind: NotifyKind) -> bool {
        self.enabled && self.kind(kind)
    }

    /// The master switch turned on or off. On with no kind chosen yet turns every kind on.
    #[must_use]
    pub fn with_master(mut self, on: bool) -> Self {
        self.enabled = on;
        self.normalized()
    }

    /// One kind's switch turned on or off. Turning one on while the master switch is off turns
    /// that on too (with the kinds chosen before); turning off the last one turns it off.
    #[must_use]
    pub fn with_kind(mut self, kind: NotifyKind, on: bool) -> Self {
        self.set(kind, on);
        if on {
            self.enabled = true;
        } else if !self.any_kind() {
            self.enabled = false;
        }
        self
    }

    /// As stored: on with no kind chosen means all of them.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        if self.enabled && !self.any_kind() {
            for kind in NotifyKind::ALL {
                self.set(kind, true);
            }
        }
        self
    }

    /// The answer to the one-time offer: yes turns everything on, either way it was shown.
    #[must_use]
    pub fn answered_offer(mut self, accept: bool) -> Self {
        if accept {
            self.enabled = true;
            for kind in NotifyKind::ALL {
                self.set(kind, true);
            }
        }
        self.offer_shown = true;
        self
    }
}

/// Whether the page offers notifications after a finished daily: once per account, only to a
/// player who hasn't turned them on yet, and only where this device can get them (`can_deliver`
/// of `state::notify`).
pub fn should_offer(prefs: Option<&NotificationPrefs>, can_deliver: bool) -> bool {
    can_deliver && prefs.is_some_and(|p| !p.offer_shown && !p.enabled)
}

/// How a device gets its notifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceKind {
    /// A browser, through Web Push (the browser vendor's push service).
    Web,
    /// The iPhone and iPad app, through Apple's push service.
    Apns,
    /// The Android app: it asks the server itself now and then (no push service).
    Android,
}

impl DeviceKind {
    pub const ALL: [Self; 3] = [Self::Web, Self::Apns, Self::Android];

    /// The id stored in the database. Never changes.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Apns => "apns",
            Self::Android => "android",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }
}

/// Which of Apple's push services an iOS app's device token belongs to.
///
/// The app's `aps-environment` entitlement: `development` for a build signed for development
/// (the simulator, Xcode's runs), `production` for the App Store and `TestFlight`.
///
/// The app registers it with its token as the device's keys (`{"env":"production"}`, see
/// [`Self::keys`]): a token of one only works with that service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApnsEnvironment {
    Development,
    Production,
}

impl ApnsEnvironment {
    pub const ALL: [Self; 2] = [Self::Development, Self::Production];

    /// The entitlement's value. Never changes.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.id() == id)
    }

    /// The device's keys as the app registers them: `{"env":"<id>"}`.
    pub fn keys(self) -> String {
        format!(r#"{{"env":"{}"}}"#, self.id())
    }

    /// The environment of a device's keys, `None` if they don't name one.
    pub fn of_keys(keys: &str) -> Option<Self> {
        #[derive(Deserialize)]
        struct Keys {
            env: ApnsEnvironment,
        }
        serde_json::from_str::<Keys>(keys).ok().map(|k| k.env)
    }
}

/// A device registered for notifications: its id and its secret.
///
/// Only the device knows the secret (the server keeps a hash). It identifies the device without
/// a session: to ask for its notifications (Android) and to unregister after the player signed
/// out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRegistration {
    pub id: i64,
    pub secret: String,
}

/// A notification as a device shows it, in the device's language.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushMessage {
    /// Increasing: a device that got this one has every earlier one.
    pub id: i64,
    /// [`NotifyKind::id`]: a string, so an older app can take a kind it doesn't know.
    pub kind: String,
    pub title: String,
    pub body: String,
    /// Where a tap goes, a full URL of the site.
    pub url: String,
    /// Notifications with the same tag replace each other on the device.
    pub tag: String,
    /// Not to be shown after this moment (ms since the epoch).
    pub expires_at: i64,
}

/// What the Android app gets when it asks for its notifications (`poll_notifications`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPoll {
    /// The new notifications, oldest first.
    pub messages: Vec<PushMessage>,
    /// When the app should ask again for the streak's last call (ms since the epoch): the moment
    /// today's would be due, else tomorrow's. `None` while the player doesn't want it. The app
    /// asks around then on top of its regular polls, which the phone may hold back for hours.
    #[serde(default)]
    pub next_last_call: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_is_off_by_default() {
        let prefs = NotificationPrefs::default();
        assert!(!prefs.enabled && !prefs.any_kind() && !prefs.offer_shown);
        for kind in NotifyKind::ALL {
            assert!(!prefs.wants(kind));
        }
    }

    #[test]
    fn the_master_switch_turns_everything_on_when_nothing_is_chosen() {
        let on = NotificationPrefs::default().with_master(true);
        for kind in NotifyKind::ALL {
            assert!(on.wants(kind), "{kind:?}");
        }
        // Off keeps the choice, on again brings it back
        let some = on.with_kind(NotifyKind::Race, false);
        let off = some.with_master(false);
        assert!(NotifyKind::ALL.into_iter().all(|k| !off.wants(k)));
        assert!(off.kind(NotifyKind::LastCall) && !off.kind(NotifyKind::Race));
        assert_eq!(off.with_master(true), some);
    }

    #[test]
    fn kinds_gate_only_with_the_master_switch_on() {
        let prefs = NotificationPrefs {
            last_call: true,
            ..NotificationPrefs::default()
        };
        assert!(!prefs.wants(NotifyKind::LastCall));
        let prefs = NotificationPrefs {
            enabled: true,
            ..prefs
        };
        assert!(prefs.wants(NotifyKind::LastCall));
        assert!(!prefs.wants(NotifyKind::Rival));
    }

    #[test]
    fn a_kind_turned_on_turns_the_master_on_and_the_last_one_off_turns_it_off() {
        let prefs = NotificationPrefs::default().with_kind(NotifyKind::Shared, true);
        assert!(prefs.enabled && prefs.wants(NotifyKind::Shared));
        assert!(!prefs.wants(NotifyKind::LastCall), "only the chosen one");
        let prefs = prefs.with_kind(NotifyKind::Shared, false);
        assert!(!prefs.enabled && !prefs.any_kind());
        // So the master switch turns them all on again
        assert!(
            NotifyKind::ALL
                .into_iter()
                .all(|k| prefs.with_master(true).wants(k))
        );
    }

    #[test]
    fn the_offer_comes_once_where_a_device_can_get_notifications() {
        let fresh = NotificationPrefs::default();
        assert!(should_offer(Some(&fresh), true));
        assert!(!should_offer(Some(&fresh), false), "no delivery here");
        assert!(!should_offer(None, true), "no account");
        let declined = fresh.answered_offer(false);
        assert!(!declined.enabled && declined.offer_shown);
        assert!(!should_offer(Some(&declined), true));
        let accepted = fresh.answered_offer(true);
        assert!(NotifyKind::ALL.into_iter().all(|k| accepted.wants(k)));
        assert!(!should_offer(Some(&accepted), true));
        // Turned on in the settings before the offer came: no offer
        assert!(!should_offer(Some(&fresh.with_master(true)), true));
    }

    #[test]
    fn ids_round_trip() {
        for kind in NotifyKind::ALL {
            assert_eq!(NotifyKind::from_id(kind.id()), Some(kind));
        }
        for kind in DeviceKind::ALL {
            assert_eq!(DeviceKind::from_id(kind.id()), Some(kind));
        }
        assert_eq!(NotifyKind::from_id("nope"), None);
        for env in ApnsEnvironment::ALL {
            assert_eq!(ApnsEnvironment::from_id(env.id()), Some(env));
            assert_eq!(ApnsEnvironment::of_keys(&env.keys()), Some(env));
        }
    }

    #[test]
    fn an_ios_device_names_its_push_environment() {
        assert_eq!(
            ApnsEnvironment::Production.keys(),
            r#"{"env":"production"}"#
        );
        assert_eq!(
            ApnsEnvironment::of_keys(r#"{ "env": "development" }"#),
            Some(ApnsEnvironment::Development)
        );
        for broken in ["", "{}", "null", r#"{"env":"staging"}"#, r#"{"env":1}"#] {
            assert_eq!(ApnsEnvironment::of_keys(broken), None, "{broken}");
        }
    }
}
