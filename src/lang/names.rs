//! Translated names: dates, difficulties, solver settings and server errors.

use leptos::prelude::ServerFnError;

use crate::i18n::{Locale, td_string};
use crate::model::ApiError;

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
        ApiError::Forbidden => td_string!(locale, common.errors.forbidden),
    }
    .to_string()
}
