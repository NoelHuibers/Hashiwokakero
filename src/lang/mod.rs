//! Languages: native names, fonts, and locale aware links.

mod names;

use leptos_i18n::Locale as _;

use crate::i18n::Locale;

pub use names::*;

/// Name of a language in itself, for the language picker.
pub fn native_name(locale: Locale) -> &'static str {
    match locale {
        Locale::en => "English",
        Locale::de => "Deutsch",
        Locale::fr => "Français",
        Locale::it => "Italiano",
        Locale::es => "Español",
        Locale::pt => "Português",
        Locale::nl => "Nederlands",
        Locale::pl => "Polski",
        Locale::tr => "Türkçe",
        Locale::ru => "Русский",
        Locale::uk => "Українська",
        Locale::ar => "العربية",
        Locale::hi => "हिन्दी",
        Locale::id => "Bahasa Indonesia",
        Locale::vi => "Tiếng Việt",
        Locale::th => "ไทย",
        Locale::ja => "日本語",
        Locale::ko => "한국어",
        Locale::zh => "简体中文",
        Locale::zh_Hant => "繁體中文",
    }
}

/// Open Graph locale (`og:locale`), language plus region.
pub fn og_locale(locale: Locale) -> &'static str {
    match locale {
        Locale::en => "en_US",
        Locale::de => "de_DE",
        Locale::fr => "fr_FR",
        Locale::it => "it_IT",
        Locale::es => "es_ES",
        Locale::pt => "pt_BR",
        Locale::nl => "nl_NL",
        Locale::pl => "pl_PL",
        Locale::tr => "tr_TR",
        Locale::ru => "ru_RU",
        Locale::uk => "uk_UA",
        Locale::ar => "ar_AR",
        Locale::hi => "hi_IN",
        Locale::id => "id_ID",
        Locale::vi => "vi_VN",
        Locale::th => "th_TH",
        Locale::ja => "ja_JP",
        Locale::ko => "ko_KR",
        Locale::zh => "zh_CN",
        Locale::zh_Hant => "zh_TW",
    }
}

/// Stylesheet in /fonts/ (see scripts/fonts.mjs) for a script Murecho doesn't cover.
///
/// Its font goes first in the font stacks (see `--app-font` in tailwind.css). Murecho covers
/// Latin, Cyrillic, Greek and Japanese, but not the Vietnamese diacritics.
pub fn script_font(locale: Locale) -> Option<&'static str> {
    match locale {
        Locale::ko => Some("noto-sans-kr"),
        Locale::zh => Some("noto-sans-sc"),
        Locale::zh_Hant => Some("noto-sans-tc"),
        Locale::ar => Some("noto-sans-arabic"),
        Locale::hi => Some("noto-sans-devanagari"),
        Locale::th => Some("noto-sans-thai"),
        Locale::vi => Some("be-vietnam-pro"),
        _ => None,
    }
}

/// URL prefix of a language, empty for English (the default).
pub fn prefix(locale: Locale) -> String {
    if locale == Locale::default() {
        String::new()
    } else {
        format!("/{}", locale.as_str())
    }
}

/// `path` (e.g. `/rules`) in the given language, e.g. `/de/rules`, `/ja`.
pub fn localized(locale: Locale, path: &str) -> String {
    match (prefix(locale), path) {
        (p, "/") if !p.is_empty() => p,
        (p, path) => format!("{p}{path}"),
    }
}

/// `path` without its language prefix: `/de/rules` -> `/rules`.
pub fn unlocalized(path: &str) -> String {
    let rest = path.trim_start_matches('/');
    let (first, tail) = rest.split_once('/').unwrap_or((rest, ""));
    if Locale::get_all()
        .iter()
        .any(|l| *l != Locale::default() && l.as_str() == first)
    {
        format!("/{tail}")
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(localized(Locale::en, "/"), "/");
        assert_eq!(localized(Locale::en, "/rules"), "/rules");
        assert_eq!(localized(Locale::de, "/"), "/de");
        assert_eq!(localized(Locale::zh_Hant, "/rules"), "/zh-Hant/rules");
        assert_eq!(unlocalized("/de/rules"), "/rules");
        assert_eq!(unlocalized("/de"), "/");
        assert_eq!(unlocalized("/rules"), "/rules");
        assert_eq!(unlocalized("/"), "/");
    }

    #[test]
    fn every_locale_has_a_name() {
        for l in Locale::get_all() {
            assert_ne!(native_name(*l), "");
            assert!(og_locale(*l).contains('_'));
        }
    }

    /// Every string of a namespace, as `key.path -> text`.
    fn strings(
        value: &serde_json::Value,
        prefix: &str,
        out: &mut std::collections::BTreeMap<String, String>,
    ) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    strings(v, &format!("{prefix}{k}."), out);
                }
            }
            serde_json::Value::String(s) => {
                out.insert(prefix.trim_end_matches('.').to_string(), s.clone());
            }
            other => panic!("{prefix}: not a string: {other}"),
        }
    }

    /// The `{{ variables }}` and `<tags>` of a string, sorted.
    fn placeholders(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find("{{") {
            let end = rest[start..]
                .find("}}")
                .map_or(rest.len(), |e| start + e + 2);
            found.push(rest[start..end].replace(' ', ""));
            rest = &rest[end..];
        }
        let mut rest = text;
        while let Some(start) = rest.find('<') {
            let end = rest[start..]
                .find('>')
                .map_or(rest.len(), |e| start + e + 1);
            found.push(rest[start..end].to_string());
            rest = &rest[end..];
        }
        found.sort();
        found
    }

    /// Translations have exactly the English keys, with the same variables and tags.
    #[test]
    fn translations_match_english() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let read = |lang: &str, ns: &str| {
            let path = root.join(lang).join(format!("{ns}.json"));
            let json: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}")),
            )
            .unwrap_or_else(|e| panic!("{path:?}: {e}"));
            let mut out = std::collections::BTreeMap::new();
            strings(&json, "", &mut out);
            out
        };
        let namespaces: Vec<String> = std::fs::read_dir(root.join("en"))
            .expect("locales/en")
            .filter_map(|e| e.ok()?.path().file_stem()?.to_str().map(String::from))
            .collect();
        let mut problems = Vec::new();
        for locale in Locale::get_all().iter().filter(|l| **l != Locale::en) {
            let lang = locale.as_str();
            for ns in &namespaces {
                let english = read("en", ns);
                let translated = read(lang, ns);
                for key in english.keys().filter(|k| !translated.contains_key(*k)) {
                    problems.push(format!("{lang}/{ns}: missing {key}"));
                }
                for key in translated.keys().filter(|k| !english.contains_key(*k)) {
                    problems.push(format!("{lang}/{ns}: extra {key}"));
                }
                for (key, text) in &translated {
                    if text.trim().is_empty() {
                        problems.push(format!("{lang}/{ns}: {key} is empty"));
                    } else if let Some(en) = english.get(key)
                        && placeholders(en) != placeholders(text)
                    {
                        problems.push(format!("{lang}/{ns}: {key} has other variables or tags"));
                    }
                }
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }
}
