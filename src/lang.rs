//! Languages: native names, fonts, and locale aware links.

use leptos::prelude::ServerFnError;
use leptos_i18n::Locale as _;

use crate::i18n::{Locale, td_string};
use crate::model::ApiError;

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

/// Google Fonts family for scripts Zen Maru Gothic doesn't cover. It goes first in the font
/// stack (see `--app-font` in tailwind.css), Zen Maru Gothic covers Latin, Cyrillic,
/// Vietnamese and Japanese.
pub fn script_font(locale: Locale) -> Option<&'static str> {
    match locale {
        Locale::ko => Some("Noto+Sans+KR:wght@400;500;700;900"),
        Locale::zh => Some("Noto+Sans+SC:wght@400;500;700;900"),
        Locale::zh_Hant => Some("Noto+Sans+TC:wght@400;500;700;900"),
        Locale::ar => Some("Baloo+Bhaijaan+2:wght@400;500;700;800"),
        Locale::hi => Some("Baloo+2:wght@400;500;700;800"),
        Locale::th => Some("Mali:wght@400;500;700"),
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

fn weekday_name(locale: Locale, weekday: usize) -> &'static str {
    match weekday {
        0 => td_string!(locale, common.weekday.mon),
        1 => td_string!(locale, common.weekday.tue),
        2 => td_string!(locale, common.weekday.wed),
        3 => td_string!(locale, common.weekday.thu),
        4 => td_string!(locale, common.weekday.fri),
        5 => td_string!(locale, common.weekday.sat),
        _ => td_string!(locale, common.weekday.sun),
    }
}

fn month_name(locale: Locale, month: u32) -> &'static str {
    match month {
        1 => td_string!(locale, common.month.jan),
        2 => td_string!(locale, common.month.feb),
        3 => td_string!(locale, common.month.mar),
        4 => td_string!(locale, common.month.apr),
        5 => td_string!(locale, common.month.may),
        6 => td_string!(locale, common.month.jun),
        7 => td_string!(locale, common.month.jul),
        8 => td_string!(locale, common.month.aug),
        9 => td_string!(locale, common.month.sep),
        10 => td_string!(locale, common.month.oct),
        11 => td_string!(locale, common.month.nov),
        _ => td_string!(locale, common.month.dec),
    }
}

/// "Saturday, 17 October 2026" in the word order of the language.
pub fn date_long(locale: Locale, day: i64) -> String {
    let (year, month, d) = crate::model::civil(day);
    td_string!(
        locale,
        common.date_long,
        weekday = weekday_name(locale, crate::model::weekday(day)),
        day = d,
        month = month_name(locale, month),
        year
    )
}

/// "17 October 2026" in the word order of the language.
pub fn date_short(locale: Locale, day: i64) -> String {
    let (year, month, d) = crate::model::civil(day);
    td_string!(
        locale,
        common.date_short,
        day = d,
        month = month_name(locale, month),
        year
    )
}

/// Name of a difficulty, e.g. "Hard" / "Schwer".
pub fn difficulty_name(locale: Locale, difficulty: common::Difficulty) -> String {
    use common::Difficulty as D;
    match difficulty {
        D::Easy => td_string!(locale, common.difficulty.easy),
        D::Medium => td_string!(locale, common.difficulty.medium),
        D::Hard => td_string!(locale, common.difficulty.hard),
        D::Expert => td_string!(locale, common.difficulty.expert),
        D::Oni => td_string!(locale, common.difficulty.oni),
    }
    .to_string()
}

pub fn solver_name(locale: Locale, kind: common::SolverKind) -> String {
    use common::SolverKind as K;
    match kind {
        K::Splr => "splr".to_string(),
        K::Cdcl => td_string!(locale, common.solver.own_cdcl).to_string(),
        K::Dpll => td_string!(locale, common.solver.own_dpll).to_string(),
    }
}

pub fn restarts_name(locale: Locale, restarts: common::Restarts) -> String {
    use common::Restarts as R;
    match restarts {
        R::None => td_string!(locale, common.solver.restarts_none),
        R::Fixed { .. } => td_string!(locale, common.solver.restarts_fixed),
        R::Geometric { .. } => td_string!(locale, common.solver.restarts_geometric),
        R::Luby { .. } => td_string!(locale, common.solver.restarts_luby),
    }
    .to_string()
}

/// "Own CDCL · VSIDS · Luby sequence", like `SolverConfig::describe` but translated.
pub fn config_name(locale: Locale, config: &common::SolverConfig) -> String {
    let mut parts = vec![solver_name(locale, config.kind)];
    if !config.kind.heuristics().is_empty() {
        parts.push(config.heuristic.label().to_string());
    }
    if config.kind.supports_restarts() {
        parts.push(restarts_name(locale, config.restarts));
    }
    if config.subsumption {
        parts.push(td_string!(locale, common.solver.subsumption_short).to_string());
    }
    parts.join(" · ")
}

/// A server error for players, in their language.
pub fn error_text(locale: Locale, error: &ServerFnError) -> String {
    let message = error.to_string();
    let Some(code) = ApiError::from_message(&message) else {
        let detail = message
            .rsplit_once(": ")
            .map_or(message.as_str(), |(_, d)| d);
        return td_string!(locale, common.errors.unknown, detail);
    };
    match code {
        ApiError::Database => td_string!(locale, common.errors.database),
        ApiError::Internal => td_string!(locale, common.errors.internal),
        ApiError::ArchiveOnlyPast => td_string!(locale, common.errors.archive_only_past),
        ApiError::PasswordTooShort => td_string!(locale, common.errors.password_too_short),
        ApiError::PasswordTooLong => td_string!(locale, common.errors.password_too_long),
        ApiError::UsernameInvalid => td_string!(locale, common.errors.username_invalid),
        ApiError::UsernameTaken => td_string!(locale, common.errors.username_taken),
        ApiError::WrongLogin => td_string!(locale, common.errors.wrong_login),
        ApiError::TooManyAttempts => td_string!(locale, common.errors.too_many_attempts),
        ApiError::SessionExpired => td_string!(locale, common.errors.session_expired),
        ApiError::NotStarted => td_string!(locale, common.errors.not_started),
        ApiError::NotSolved => td_string!(locale, common.errors.not_solved),
        ApiError::WrongPuzzle => td_string!(locale, common.errors.wrong_puzzle),
        ApiError::AlreadyFinished => td_string!(locale, common.errors.already_finished),
        ApiError::DailyLocked => td_string!(locale, common.errors.daily_locked),
    }
    .to_string()
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
