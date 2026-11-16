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
    use std::fmt::Write as _;

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

    /// The folder of a language's resources in the Android app: English is the default; Android
    /// knows Indonesian by its old code `in`, and traditional Chinese by its script.
    fn android_values(locale: Locale) -> String {
        match locale.as_str() {
            "en" => "values".into(),
            "id" => "values-in".into(),
            "zh-Hant" => "values-b+zh+Hant".into(),
            tag => format!("values-{tag}"),
        }
    }

    /// A text as an Android string resource: XML, and Android's own escapes on top.
    fn android_text(text: &str) -> String {
        let mut out = String::new();
        for (i, c) in text.chars().enumerate() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '\\' | '"' | '\'' => {
                    out.push('\\');
                    out.push(c);
                }
                '@' | '?' if i == 0 => {
                    out.push('\\');
                    out.push(c);
                }
                c => out.push(c),
            }
        }
        out
    }

    /// The names of the Android app's notification channels in a language
    /// (`Notifications.kt` of `app/src-tauri/gen/android`), one per kind: those of the account
    /// page's switches, with their hints as the channels' descriptions.
    fn android_strings(locale: Locale) -> String {
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <!-- The notification channels' names (Notifications.kt): the account page's, written by\n     \
             the test android_channels_are_named_like_the_switches (src/server/notify/render.rs),\n     \
             GOLDEN_UPDATE=1 rewrites them. Not by hand -->\n<resources>\n",
        );
        let mut string = |name: &str, text: &str| {
            let _ = writeln!(
                xml,
                "    <string name=\"notify_{name}\">{}</string>",
                android_text(text)
            );
        };
        for (kind, name, hint) in [
            (
                NotifyKind::LastCall,
                td_string!(locale, account.notifications.last_call),
                td_string!(locale, account.notifications.last_call_hint),
            ),
            (
                NotifyKind::Race,
                td_string!(locale, account.notifications.race),
                td_string!(locale, account.notifications.race_hint),
            ),
            (
                NotifyKind::Rival,
                td_string!(locale, account.notifications.rival),
                td_string!(locale, account.notifications.rival_hint),
            ),
            (
                NotifyKind::Shared,
                td_string!(locale, account.notifications.shared),
                td_string!(locale, account.notifications.shared_hint),
            ),
            (
                NotifyKind::Group,
                td_string!(locale, account.notifications.group),
                td_string!(locale, account.notifications.group_hint),
            ),
        ] {
            string(kind.id(), name);
            string(&format!("{}_hint", kind.id()), hint);
        }
        // For kinds a newer server has and this app doesn't know yet
        string("other", td_string!(locale, account.notifications.master));
        xml.push_str("</resources>");
        xml
    }

    #[test]
    fn android_channels_are_named_like_the_switches() {
        assert_eq!(
            android_text("Today's \"daily\" & <more> \\ @"),
            "Today\\'s \\\"daily\\\" &amp; &lt;more&gt; \\\\ @"
        );
        assert_eq!(android_text("@home ?x"), "\\@home ?x");
        let res = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("app/src-tauri/gen/android/app/src/main/res");
        for &locale in Locale::get_all() {
            let file = res.join(android_values(locale)).join("notifications.xml");
            let xml = android_strings(locale);
            if std::env::var_os("GOLDEN_UPDATE").is_some() {
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                std::fs::write(&file, format!("{xml}\n")).unwrap();
                continue;
            }
            let stored = std::fs::read_to_string(&file).unwrap_or_default();
            assert!(
                stored.trim_end() == xml,
                "{}: the channels' names differ from the translations, run this test with \
                 GOLDEN_UPDATE=1 to write them\n\n{xml}",
                file.display()
            );
        }
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
