//! Dates: day numbers, the calendar, weekdays, the daily of a day, durations and streaks.
//!
//! Days are counted in UTC since 1970-01-01, the daily puzzle changes at midnight UTC for
//! everyone.

use common::Difficulty;

/// The first daily puzzle, #1.
pub const LAUNCH_DAY: i64 = 20_743; // 2026-10-17
pub const MS_PER_DAY: i64 = 86_400_000;

/// Day number of a unix timestamp in ms.
pub fn day_of(ms: i64) -> i64 {
    ms.div_euclid(MS_PER_DAY)
}

/// (year, month, day) of a day number, Howard Hinnant's `civil_from_days`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "day of month (1..=31) and month (1..=12) are in range by construction"
)]
pub fn civil(day: i64) -> (i64, u32, u32) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Day number of a date, inverse of [`civil`].
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2026-10-17`
pub fn date_string(day: i64) -> String {
    let (y, m, d) = civil(day);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn parse_date(s: &str) -> Option<i64> {
    let mut parts = s.split('-');
    let y = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    let d = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let day = days_from_civil(y, m, d);
    (civil(day) == (y, m, d)).then_some(day)
}

/// 0 = Monday … 6 = Sunday
#[expect(
    clippy::cast_possible_truncation,
    reason = "`rem_euclid(7)` is in 0..7"
)]
pub fn weekday(day: i64) -> usize {
    (day + 3).rem_euclid(7) as usize
}

pub const WEEKDAY_KANJI: [&str; 7] = ["月", "火", "水", "木", "金", "土", "日"];
pub const WEEKDAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// Like a newspaper puzzle the dailies get harder during the week.
pub fn daily_difficulty(day: i64) -> Difficulty {
    match weekday(day) {
        0 => Difficulty::Easy,
        1 | 2 => Difficulty::Medium,
        3 | 4 => Difficulty::Hard,
        5 => Difficulty::Expert,
        _ => Difficulty::Oni,
    }
}

/// Daily puzzle #1 was on the launch day.
pub fn daily_number(day: i64) -> i64 {
    day - LAUNCH_DAY + 1
}

/// `Sat 17 Oct 2026`
pub fn pretty_date(day: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (y, m, d) = civil(day);
    format!(
        "{} {d} {} {y}",
        &WEEKDAY_NAMES[weekday(day)][..3],
        MONTHS[m as usize - 1]
    )
}

/// `1:05.3` / `12:30` / `1:02:03`
pub fn format_duration(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else if total < 600 {
        format!("{m}:{s:02}.{}", ms.max(0) % 1000 / 100)
    } else {
        format!("{m}:{s:02}")
    }
}

/// Current streak (ending today or yesterday) and best streak from sorted solved days.
pub fn streaks(solved_days: &[i64], today: i64) -> (i64, i64) {
    let (mut best, mut run, mut prev) = (0, 0, None::<i64>);
    for &day in solved_days {
        run = if prev == Some(day - 1) { run + 1 } else { 1 };
        best = best.max(run);
        prev = Some(day);
    }
    let current = match prev {
        Some(last) if last >= today - 1 => run,
        _ => 0,
    };
    (current, best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date_string(0), "1970-01-01");
        assert_eq!(date_string(LAUNCH_DAY), "2026-10-17");
        assert_eq!(parse_date("2026-10-17"), Some(LAUNCH_DAY));
        assert_eq!(
            parse_date("2024-02-29").map(date_string).as_deref(),
            Some("2024-02-29")
        );
        assert_eq!(parse_date("2026-02-30"), None);
        assert_eq!(parse_date("nope"), None);
        for day in [-1000, 0, 1, 59, 60, 11_016, LAUNCH_DAY, 100_000] {
            assert_eq!(parse_date(&date_string(day)), Some(day));
        }
    }

    #[test]
    fn weekdays() {
        // 1970-01-01 was a Thursday, 2026-10-17 is a Saturday
        assert_eq!(weekday(0), 3);
        assert_eq!(weekday(LAUNCH_DAY), 5);
        assert_eq!(pretty_date(LAUNCH_DAY), "Sat 17 Oct 2026");
        assert_eq!(daily_difficulty(LAUNCH_DAY), Difficulty::Expert);
        assert_eq!(daily_difficulty(LAUNCH_DAY + 1), Difficulty::Oni);
        assert_eq!(daily_difficulty(LAUNCH_DAY + 2), Difficulty::Easy);
        assert_eq!(daily_number(LAUNCH_DAY), 1);
    }

    #[test]
    fn streak_counting() {
        assert_eq!(streaks(&[], 10), (0, 0));
        assert_eq!(streaks(&[8, 9, 10], 10), (3, 3));
        assert_eq!(streaks(&[8, 9], 10), (2, 2));
        assert_eq!(streaks(&[7, 8], 10), (0, 2));
        assert_eq!(streaks(&[1, 2, 3, 4, 9, 10], 10), (2, 4));
    }

    #[test]
    fn durations() {
        assert_eq!(format_duration(65_300), "1:05.3");
        assert_eq!(format_duration(750_000), "12:30");
        assert_eq!(format_duration(3_723_000), "1:02:03");
    }
}
