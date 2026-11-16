//! A notification's text in a device's language: the outbox keeps what the text needs (`args`,
//! JSON), not the text, so every device of a player gets it in its own language.

use leptos_i18n::Locale as _;
use serde_json::Value;

use crate::i18n::{Locale, td_string};
use crate::lang::localized;
use crate::model::{NotifyKind, format_duration};
use crate::seo::SITE_URL;

/// What the text of a notification needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Args {
    /// The player it's about.
    pub name: String,
    /// The group of a race or a group puzzle.
    pub group: String,
    /// A group puzzle's title, empty without one.
    pub title: String,
    /// How many solved a shared puzzle.
    pub count: i64,
    /// A daily time, ms.
    pub time_ms: i64,
}

impl Args {
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "name": self.name,
            "group": self.group,
            "title": self.title,
            "count": self.count,
            "time_ms": self.time_ms,
        })
        .to_string()
    }

    /// What was stored, missing fields empty.
    pub fn from_json(json: &str) -> Self {
        let value: Value = serde_json::from_str(json).unwrap_or_default();
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
        let number = |key: &str| value[key].as_i64().unwrap_or_default();
        Self {
            name: text("name"),
            group: text("group"),
            title: text("title"),
            count: number("count"),
            time_ms: number("time_ms"),
        }
    }
}

/// The language of a stored locale tag, English for one this build doesn't have.
pub fn locale_of(tag: &str) -> Locale {
    Locale::get_all()
        .iter()
        .copied()
        .find(|l| l.as_str() == tag)
        .unwrap_or_default()
}

/// The title and the text of a notification.
pub(super) fn text(kind: NotifyKind, args: &Args, locale: Locale) -> (String, String) {
    let name = args.name.as_str();
    let group = args.group.as_str();
    match kind {
        NotifyKind::LastCall => (
            td_string!(locale, account.notifications.push.last_call_title).to_string(),
            td_string!(locale, account.notifications.push.last_call_body).to_string(),
        ),
        NotifyKind::Race => (
            td_string!(locale, account.notifications.push.race_title, name),
            td_string!(locale, account.notifications.push.race_body, group),
        ),
        NotifyKind::Rival => (
            td_string!(locale, account.notifications.push.rival_title, name),
            td_string!(
                locale,
                account.notifications.push.rival_body,
                time = format_duration(args.time_ms)
            ),
        ),
        NotifyKind::Shared if args.count <= 1 => (
            td_string!(locale, account.notifications.push.shared_one_title, name),
            td_string!(locale, account.notifications.push.shared_one_body).to_string(),
        ),
        NotifyKind::Shared => (
            td_string!(locale, account.notifications.push.shared_many_title).to_string(),
            td_string!(
                locale,
                account.notifications.push.shared_many_body,
                count = args.count,
                name
            ),
        ),
        NotifyKind::Group => (
            td_string!(locale, account.notifications.push.group_title, group),
            if args.title.is_empty() {
                td_string!(locale, account.notifications.push.group_body_untitled, name)
            } else {
                td_string!(
                    locale,
                    account.notifications.push.group_body,
                    name,
                    title = args.title.as_str()
                )
            },
        ),
    }
}

/// Where a tap goes: the page in the device's language, a full URL (the apps open the site's
/// links themselves, see `apps`).
pub(super) fn url(path: &str, locale: Locale) -> String {
    format!("{SITE_URL}{}", localized(locale, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> Args {
        Args {
            name: "Ada".into(),
            group: "Lunch club".into(),
            title: "Spiral".into(),
            count: 3,
            time_ms: 83_400,
        }
    }

    #[test]
    fn args_round_trip_and_tolerate_missing_fields() {
        assert_eq!(Args::from_json(&args().to_json()), args());
        assert_eq!(Args::from_json("{}"), Args::default());
        assert_eq!(Args::from_json("not json"), Args::default());
    }

    #[test]
    fn texts_name_the_player_in_every_language() {
        for &locale in Locale::get_all() {
            let tag = locale.as_str();
            for kind in [NotifyKind::Race, NotifyKind::Rival, NotifyKind::Group] {
                let (title, body) = text(kind, &args(), locale);
                assert!(
                    title.contains("Ada") || body.contains("Ada"),
                    "{tag} {kind:?}"
                );
                assert!(
                    !title.contains("{{") && !body.contains("{{"),
                    "{tag} {kind:?}"
                );
            }
            let (title, body) = text(NotifyKind::Group, &args(), locale);
            assert!(
                title.contains("Lunch club") && body.contains("Spiral"),
                "{tag}"
            );
            let (_, body) = text(NotifyKind::Rival, &args(), locale);
            assert!(body.contains("1:23.4"), "{tag}: {body}");
            let (_, body) = text(NotifyKind::Shared, &args(), locale);
            assert!(body.contains('3') && body.contains("Ada"), "{tag}: {body}");
            let one = Args { count: 1, ..args() };
            let (title, _) = text(NotifyKind::Shared, &one, locale);
            assert!(title.contains("Ada"), "{tag}: {title}");
            let untitled = Args {
                title: String::new(),
                ..args()
            };
            let (_, body) = text(NotifyKind::Group, &untitled, locale);
            assert!(
                body.contains("Ada") && !body.contains("“”"),
                "{tag}: {body}"
            );
            let (title, body) = text(NotifyKind::LastCall, &Args::default(), locale);
            assert!(!title.is_empty() && !body.is_empty(), "{tag}");
        }
    }

    #[test]
    fn texts_are_in_the_devices_language() {
        let (en, _) = text(NotifyKind::LastCall, &Args::default(), Locale::en);
        let (de, _) = text(NotifyKind::LastCall, &Args::default(), Locale::de);
        let (ja, _) = text(NotifyKind::LastCall, &Args::default(), Locale::ja);
        assert_eq!(en, "Last call for your streak");
        assert_eq!(de, "Letzter Aufruf für deine Serie");
        assert_eq!(ja, "連続記録のラストコール");
        assert_eq!(locale_of("ja"), Locale::ja);
        assert_eq!(locale_of("zh-Hant"), Locale::zh_Hant);
        assert_eq!(locale_of("xx"), Locale::en);
    }

    #[test]
    fn taps_open_the_page_in_the_devices_language() {
        assert_eq!(url("/", Locale::en), "https://hashi.rs/");
        assert_eq!(url("/", Locale::de), "https://hashi.rs/de");
        assert_eq!(
            url("/p/k7m2xq9a", Locale::ja),
            "https://hashi.rs/ja/p/k7m2xq9a"
        );
    }
}
