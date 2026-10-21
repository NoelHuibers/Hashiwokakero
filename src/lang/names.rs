//! Translated names: dates, difficulties, stamps, solver settings, file and server errors.

use leptos::prelude::ServerFnError;

use crate::i18n::{Locale, td_string};
use crate::model::{ApiError, Stamp, month_civil};

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

/// "October 2026", a month of the monthly race (see `model::month_of`).
pub fn month_long(locale: Locale, month: i64) -> String {
    let (year, m) = month_civil(month);
    let name = match m {
        1 => td_string!(locale, ranks.month.names.jan),
        2 => td_string!(locale, ranks.month.names.feb),
        3 => td_string!(locale, ranks.month.names.mar),
        4 => td_string!(locale, ranks.month.names.apr),
        5 => td_string!(locale, ranks.month.names.may),
        6 => td_string!(locale, ranks.month.names.jun),
        7 => td_string!(locale, ranks.month.names.jul),
        8 => td_string!(locale, ranks.month.names.aug),
        9 => td_string!(locale, ranks.month.names.sep),
        10 => td_string!(locale, ranks.month.names.oct),
        11 => td_string!(locale, ranks.month.names.nov),
        _ => td_string!(locale, ranks.month.names.dec),
    };
    td_string!(locale, ranks.month.label, month = name, year)
}

/// Name of a stamp, e.g. "Early bird".
pub fn stamp_name(locale: Locale, stamp: Stamp) -> &'static str {
    match stamp {
        Stamp::First => td_string!(locale, stamps.names.first),
        Stamp::Solved10 => td_string!(locale, stamps.names.solved_10),
        Stamp::Solved50 => td_string!(locale, stamps.names.solved_50),
        Stamp::Solved100 => td_string!(locale, stamps.names.solved_100),
        Stamp::Solved365 => td_string!(locale, stamps.names.solved_365),
        Stamp::Bridges1000 => td_string!(locale, stamps.names.bridges_1000),
        Stamp::Bridges10000 => td_string!(locale, stamps.names.bridges_10000),
        Stamp::Islands1000 => td_string!(locale, stamps.names.islands_1000),
        Stamp::Streak7 => td_string!(locale, stamps.names.streak_7),
        Stamp::Streak30 => td_string!(locale, stamps.names.streak_30),
        Stamp::Streak100 => td_string!(locale, stamps.names.streak_100),
        Stamp::FullWeek => td_string!(locale, stamps.names.full_week),
        Stamp::FullMoon => td_string!(locale, stamps.names.full_moon),
        Stamp::FourSeasons => td_string!(locale, stamps.names.four_seasons),
        Stamp::Oni => td_string!(locale, stamps.names.oni),
        Stamp::Clean => td_string!(locale, stamps.names.clean),
        Stamp::StoneBridge => td_string!(locale, stamps.names.stone_bridge),
        Stamp::Storm => td_string!(locale, stamps.names.storm),
        Stamp::Phoenix => td_string!(locale, stamps.names.phoenix),
        Stamp::Tortoise => td_string!(locale, stamps.names.tortoise),
        Stamp::Metronome => td_string!(locale, stamps.names.metronome),
        Stamp::EarlyBird => td_string!(locale, stamps.names.early_bird),
        Stamp::ZeroHour => td_string!(locale, stamps.names.zero_hour),
        Stamp::OnTheBrink => td_string!(locale, stamps.names.on_the_brink),
        Stamp::NightOwl => td_string!(locale, stamps.names.night_owl),
        Stamp::LunchBreak => td_string!(locale, stamps.names.lunch_break),
        Stamp::Swift => td_string!(locale, stamps.names.swift),
        Stamp::Podium => td_string!(locale, stamps.names.podium),
        Stamp::Rainbow => td_string!(locale, stamps.names.rainbow),
        Stamp::PhotoFinish => td_string!(locale, stamps.names.photo_finish),
        Stamp::Gold => td_string!(locale, stamps.names.gold),
        Stamp::HatTrick => td_string!(locale, stamps.names.hat_trick),
        Stamp::Gold10 => td_string!(locale, stamps.names.gold_10),
        Stamp::Champion => td_string!(locale, stamps.names.champion),
        Stamp::NewYear => td_string!(locale, stamps.names.new_year),
        Stamp::Tanabata => td_string!(locale, stamps.names.tanabata),
        Stamp::Anniversary => td_string!(locale, stamps.names.anniversary),
        Stamp::Dojo => td_string!(locale, stamps.names.dojo),
        Stamp::BlackBelt => td_string!(locale, stamps.names.black_belt),
        Stamp::Sensei => td_string!(locale, stamps.names.sensei),
        Stamp::Collector => td_string!(locale, stamps.names.collector),
        Stamp::SecondThoughts => td_string!(locale, stamps.names.second_thoughts),
        Stamp::Palindrome => td_string!(locale, stamps.names.palindrome),
        Stamp::LanterneRouge => td_string!(locale, stamps.names.lanterne_rouge),
        Stamp::LeapDay => td_string!(locale, stamps.names.leap_day),
        Stamp::Yokozuna => td_string!(locale, stamps.names.yokozuna),
        Stamp::Sansho => td_string!(locale, stamps.names.sansho),
    }
}

/// How to earn a stamp.
pub fn stamp_text(locale: Locale, stamp: Stamp) -> &'static str {
    match stamp {
        Stamp::First => td_string!(locale, stamps.texts.first),
        Stamp::Solved10 => td_string!(locale, stamps.texts.solved_10),
        Stamp::Solved50 => td_string!(locale, stamps.texts.solved_50),
        Stamp::Solved100 => td_string!(locale, stamps.texts.solved_100),
        Stamp::Solved365 => td_string!(locale, stamps.texts.solved_365),
        Stamp::Bridges1000 => td_string!(locale, stamps.texts.bridges_1000),
        Stamp::Bridges10000 => td_string!(locale, stamps.texts.bridges_10000),
        Stamp::Islands1000 => td_string!(locale, stamps.texts.islands_1000),
        Stamp::Streak7 => td_string!(locale, stamps.texts.streak_7),
        Stamp::Streak30 => td_string!(locale, stamps.texts.streak_30),
        Stamp::Streak100 => td_string!(locale, stamps.texts.streak_100),
        Stamp::FullWeek => td_string!(locale, stamps.texts.full_week),
        Stamp::FullMoon => td_string!(locale, stamps.texts.full_moon),
        Stamp::FourSeasons => td_string!(locale, stamps.texts.four_seasons),
        Stamp::Oni => td_string!(locale, stamps.texts.oni),
        Stamp::Clean => td_string!(locale, stamps.texts.clean),
        Stamp::StoneBridge => td_string!(locale, stamps.texts.stone_bridge),
        Stamp::Storm => td_string!(locale, stamps.texts.storm),
        Stamp::Phoenix => td_string!(locale, stamps.texts.phoenix),
        Stamp::Tortoise => td_string!(locale, stamps.texts.tortoise),
        Stamp::Metronome => td_string!(locale, stamps.texts.metronome),
        Stamp::EarlyBird => td_string!(locale, stamps.texts.early_bird),
        Stamp::ZeroHour => td_string!(locale, stamps.texts.zero_hour),
        Stamp::OnTheBrink => td_string!(locale, stamps.texts.on_the_brink),
        Stamp::NightOwl => td_string!(locale, stamps.texts.night_owl),
        Stamp::LunchBreak => td_string!(locale, stamps.texts.lunch_break),
        Stamp::Swift => td_string!(locale, stamps.texts.swift),
        Stamp::Podium => td_string!(locale, stamps.texts.podium),
        Stamp::Rainbow => td_string!(locale, stamps.texts.rainbow),
        Stamp::PhotoFinish => td_string!(locale, stamps.texts.photo_finish),
        Stamp::Gold => td_string!(locale, stamps.texts.gold),
        Stamp::HatTrick => td_string!(locale, stamps.texts.hat_trick),
        Stamp::Gold10 => td_string!(locale, stamps.texts.gold_10),
        Stamp::Champion => td_string!(locale, stamps.texts.champion),
        Stamp::NewYear => td_string!(locale, stamps.texts.new_year),
        Stamp::Tanabata => td_string!(locale, stamps.texts.tanabata),
        Stamp::Anniversary => td_string!(locale, stamps.texts.anniversary),
        Stamp::Dojo => td_string!(locale, stamps.texts.dojo),
        Stamp::BlackBelt => td_string!(locale, stamps.texts.black_belt),
        Stamp::Sensei => td_string!(locale, stamps.texts.sensei),
        Stamp::Collector => td_string!(locale, stamps.texts.collector),
        Stamp::SecondThoughts => td_string!(locale, stamps.texts.second_thoughts),
        Stamp::Palindrome => td_string!(locale, stamps.texts.palindrome),
        Stamp::LanterneRouge => td_string!(locale, stamps.texts.lanterne_rouge),
        Stamp::LeapDay => td_string!(locale, stamps.texts.leap_day),
        Stamp::Yokozuna => td_string!(locale, stamps.texts.yokozuna),
        Stamp::Sansho => td_string!(locale, stamps.texts.sansho),
    }
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

/// Name of a branching heuristic; most are named after their authors and stay as they are.
pub fn heuristic_name(locale: Locale, heuristic: common::Heuristic) -> String {
    use common::Heuristic as H;
    match heuristic {
        H::Arbitrary => td_string!(locale, common.solver.heuristic_arbitrary).to_string(),
        H::Custom => td_string!(locale, common.solver.heuristic_custom).to_string(),
        h => h.label().to_string(),
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
        parts.push(heuristic_name(locale, config.heuristic));
    }
    if config.kind.supports_restarts() {
        parts.push(restarts_name(locale, config.restarts));
    }
    if config.subsumption {
        parts.push(td_string!(locale, common.solver.subsumption_short).to_string());
    }
    parts.join(" · ")
}

/// Why an imported puzzle file couldn't be read, in the player's language.
pub fn parse_error_text(locale: Locale, error: &common::ParseError) -> String {
    use common::ParseError as E;
    match error {
        E::Empty => td_string!(locale, create.parse.empty).to_string(),
        E::MissingSize => td_string!(locale, create.parse.missing_size).to_string(),
        E::InvalidSize(header) => td_string!(locale, create.parse.invalid_size, header),
        E::TooSmall => td_string!(locale, create.parse.too_small).to_string(),
        E::TooManyRows { expected } => {
            td_string!(locale, create.parse.too_many_rows, expected)
        }
        E::InvalidChar { char, row } => {
            td_string!(locale, create.parse.invalid_char, character = char, row)
        }
        E::RowWidth {
            row,
            found,
            expected,
        } => {
            td_string!(locale, create.parse.row_width, row, found, expected)
        }
        E::RowCount { expected, found } => {
            td_string!(locale, create.parse.row_count, expected, found)
        }
    }
}

/// The last part of an error message the server didn't give a code (a network error, say), short
/// enough for a toast. `None` for markup or an empty message: no response body ends up on the page.
fn error_detail(message: &str) -> Option<String> {
    const MAX_CHARS: usize = 100;
    let detail = message.rsplit_once(": ").map_or(message, |(_, d)| d).trim();
    if detail.is_empty() || detail.contains('<') {
        return None;
    }
    if detail.chars().count() <= MAX_CHARS {
        return Some(detail.to_string());
    }
    let short: String = detail.chars().take(MAX_CHARS).collect();
    Some(format!("{}…", short.trim_end()))
}

/// A server error for players, in their language.
pub fn error_text(locale: Locale, error: &ServerFnError) -> String {
    if crate::api::is_outdated(error) {
        return td_string!(locale, common.errors.outdated).to_string();
    }
    let message = error.to_string();
    let Some(code) = ApiError::from_message(&message) else {
        return match error_detail(&message) {
            Some(detail) => td_string!(locale, common.errors.unknown, detail),
            None => td_string!(locale, common.errors.internal).to_string(),
        };
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
        ApiError::Forbidden => td_string!(locale, common.errors.forbidden),
        ApiError::WrongPassword => td_string!(locale, common.errors.wrong_password),
        ApiError::EmailInvalid => td_string!(locale, common.errors.email_invalid),
        ApiError::MessageInvalid => td_string!(locale, common.errors.message_invalid),
        ApiError::TooManyMessages => td_string!(locale, common.errors.too_many_messages),
        ApiError::RenameTooSoon => td_string!(locale, common.errors.rename_too_soon),
        ApiError::UsernameNotAllowed => td_string!(locale, common.errors.username_not_allowed),
        ApiError::Banned => td_string!(locale, common.errors.banned),
        ApiError::GroupNameInvalid => td_string!(locale, common.errors.group_name_invalid),
        ApiError::GroupNameNotAllowed => td_string!(locale, common.errors.group_name_not_allowed),
        ApiError::GroupNotFound => td_string!(locale, common.errors.group_not_found),
        ApiError::GroupFull => td_string!(locale, common.errors.group_full),
        ApiError::TooManyGroups => td_string!(locale, common.errors.too_many_groups),
        ApiError::GroupOwnerOnly => td_string!(locale, common.errors.group_owner_only),
        ApiError::RivalSelf => td_string!(locale, rivals.errors.rival_self),
        ApiError::TooManyRivals => td_string!(locale, rivals.errors.too_many_rivals),
        ApiError::NoSuchPlayer => td_string!(locale, rivals.errors.no_such_player),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use leptos_i18n::Locale as _;

    use super::*;
    use crate::model::{LAUNCH_DAY, month_of};

    /// A filled-in text: not empty and without a `{{ variable }}` left over.
    fn filled(text: &str) -> bool {
        !text.trim().is_empty() && !text.contains("{{") && !text.contains("}}")
    }

    /// Every name and message exists in every language, with its variables filled in.
    #[test]
    fn every_name_is_translated() {
        for &l in Locale::get_all() {
            for stamp in Stamp::ALL {
                assert!(filled(stamp_name(l, stamp)), "{l:?} {stamp:?}");
                assert!(filled(stamp_text(l, stamp)), "{l:?} {stamp:?}");
            }
            for code in ApiError::ALL {
                let text = error_text(l, &ServerFnError::from(code));
                assert!(filled(&text), "{l:?} {code:?}: {text}");
            }
            let unknown = error_text(l, &ServerFnError::new("the database is on fire"));
            assert!(
                unknown.contains("the database is on fire"),
                "{l:?}: {unknown}"
            );
            for d in common::Difficulty::ALL {
                assert!(filled(&difficulty_name(l, d)), "{l:?} {d:?}");
            }
            for kind in common::SolverKind::ALL {
                for h in kind.heuristics() {
                    for restarts in ["none", "fixed", "geometric", "luby"] {
                        let config = common::SolverConfig {
                            kind,
                            heuristic: *h,
                            restarts: common::Restarts::from_id(restarts).unwrap(),
                            subsumption: kind.supports_subsumption(),
                            ..common::SolverConfig::default()
                        };
                        assert!(filled(&config_name(l, &config)), "{l:?} {config:?}");
                    }
                }
            }
        }
    }

    /// The error the client makes of a response body that isn't a server function error (see
    /// `FromServerFnError::de`), e.g. the 404 page.
    fn from_body(body: &str) -> ServerFnError {
        use leptos::server_fn::Bytes;
        use leptos::server_fn::error::FromServerFnError;
        ServerFnError::de(Bytes::from(body.to_string()))
    }

    /// A tab of an older build calls a server function the server no longer has and gets the
    /// 404 page back: players are told to reload, not shown the page's HTML.
    #[test]
    fn web_pages_ask_for_a_reload() {
        let page = r#"<!DOCTYPE html><html lang="en"><head><title>Hashi | Not found</title><style>body { color: red }</style></head><body></body></html>"#;
        for body in [
            page,
            &page.replace('|', "-"),
            "<html><body>Not found</body></html>",
        ] {
            for &l in Locale::get_all() {
                let text = error_text(l, &from_body(body));
                assert_eq!(text, td_string!(l, common.errors.outdated), "{l:?}");
                assert!(filled(&text) && !text.contains('<'), "{l:?}: {text}");
            }
        }
    }

    /// The server's codes still have their own messages, however the client wraps them.
    #[test]
    fn error_codes_keep_their_messages() {
        let l = Locale::en;
        for code in ApiError::ALL {
            let sent = from_body(&format!("ServerError|E:{}", code.code()));
            assert_eq!(
                error_text(l, &sent),
                error_text(l, &ServerFnError::from(code))
            );
        }
        assert_eq!(
            error_text(l, &ServerFnError::from(ApiError::SessionExpired)),
            td_string!(l, common.errors.session_expired)
        );
    }

    /// Other errors show their last part, short and without markup.
    #[test]
    fn unknown_errors_are_short_and_plain() {
        let l = Locale::en;
        let offline = ServerFnError::Request("TypeError: Failed to fetch".into());
        assert_eq!(
            error_text(l, &offline),
            td_string!(l, common.errors.unknown, detail = "Failed to fetch")
        );
        let long = error_text(l, &ServerFnError::new("x".repeat(5000)));
        assert!(long.chars().count() < 150 && long.ends_with('…'), "{long}");
        for body in ["Bad gateway: <b>upstream</b> down", "Response|"] {
            assert_eq!(
                error_text(l, &from_body(body)),
                td_string!(l, common.errors.internal),
                "{body}"
            );
        }
    }

    /// Dates and months name the day, the month and the year, in every language and month.
    #[test]
    fn dates_are_filled_in() {
        for &l in Locale::get_all() {
            let mut months = std::collections::BTreeSet::new();
            for day in (LAUNCH_DAY..LAUNCH_DAY + 366).step_by(9) {
                let (year, _, d) = crate::model::civil(day);
                for text in [date_long(l, day), date_short(l, day)] {
                    assert!(filled(&text), "{l:?} {day}: {text}");
                    assert!(text.contains(&year.to_string()), "{l:?}: {text}");
                    assert!(text.contains(&d.to_string()), "{l:?}: {text}");
                }
                let month = month_long(l, month_of(day));
                assert!(
                    filled(&month) && month.contains(&year.to_string()),
                    "{l:?}: {month}"
                );
                months.insert(month);
            }
            // Twelve different month names (and the 13th month is next year's)
            assert_eq!(months.len(), 13, "{l:?}: {months:?}");
        }
    }
}
