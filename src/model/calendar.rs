//! Dates: day numbers, the calendar, weekdays, the daily of a day, durations, and streaks with
//! their saves.
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

/// The first day whose dailies earn streak saves, `None` while saves are off: then nobody has
/// saves, no gap stays open and caught-up days don't count, streaks are as before. See
/// `docs/streak-saves.md`.
pub const SAVES_FROM: Option<i64> = None;
/// Every this many days in a row earn a save.
pub const SAVE_EVERY: i64 = 7;
/// The most saves held at once.
pub const MAX_SAVES: i64 = 2;
/// How many days back a missed daily can still be caught up: the open gap lies in
/// `today - CATCH_UP_DAYS ..= today - 1`.
pub const CATCH_UP_DAYS: i64 = 2;

/// A player's streaks and saves, see [`streak_state`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreakState {
    /// The streak that is still going: ending today or yesterday, or before the open gap.
    pub current: i64,
    /// The longest run of counted days. Only gapless runs: while a gap is open, `current` can be
    /// up to [`CATCH_UP_DAYS`] more.
    pub best: i64,
    /// Saves held now, `0..=MAX_SAVES`.
    pub saves: i64,
    /// The open gap, oldest first: the missed days that can still be caught up. Empty without one.
    pub missed: Vec<i64>,
}

/// The streaks and saves of a player, from the days that count.
///
/// Those are `ranked` (a ranked finish) and `caught_up` (a finished catch-up, only from
/// `saves_from` on). `blocked` are the days with a revealed or flagged run, which can't be caught
/// up. Unsorted input is fine; a day both ranked and caught up counts once, as ranked.
///
/// Walking the counted days in order, a day right after the previous one extends the run, any
/// other starts a new one. From `saves_from` on, every [`SAVE_EVERY`]th day of a run (counted
/// from `saves_from` or the run's start, whichever is later) earns a save, at most
/// [`MAX_SAVES`], and a caught-up day spends one. Saves carry over when a run ends.
///
/// The open gap is the latest run of missed days before today (today is never missed before it
/// ends). It stays open if all its days lie in the last [`CATCH_UP_DAYS`] days, none of them is
/// blocked, and it isn't longer than the saves held; the current streak is then the run before
/// it plus the counted days after it. A gap that isn't open breaks the run.
///
/// With `saves_from` `None` the current streak ends today or yesterday and `best` is the longest
/// run of ranked days, as always.
pub fn streak_state(
    ranked: &[i64],
    caught_up: &[i64],
    blocked: &[i64],
    today: i64,
    saves_from: Option<i64>,
) -> StreakState {
    use std::collections::BTreeMap;

    // day -> caught up
    let mut days: BTreeMap<i64, bool> = ranked.iter().map(|&d| (d, false)).collect();
    if let Some(from) = saves_from {
        for &day in caught_up.iter().filter(|&&d| d >= from) {
            days.entry(day).or_insert(true);
        }
    }

    let (mut best, mut run, mut start, mut prev, mut saves) = (0, 0, 0, None::<i64>, 0);
    for (&day, &caught) in &days {
        if prev == Some(day - 1) {
            run += 1;
        } else {
            run = 1;
            start = day;
        }
        best = best.max(run);
        if let Some(from) = saves_from.filter(|&from| day >= from) {
            if caught {
                saves = (saves - 1).max(0);
            }
            if (day - start.max(from) + 1) % SAVE_EVERY == 0 {
                saves = (saves + 1).min(MAX_SAVES);
            }
        }
        prev = Some(day);
    }
    let mut state = StreakState {
        current: match prev {
            Some(last) if last >= today - 1 => run,
            _ => 0,
        },
        best,
        saves,
        missed: Vec::new(),
    };
    if saves_from.is_none() {
        return state;
    }

    let counted = |day: i64| days.contains_key(&day);
    // The latest missed day before today, and the counted day before it
    let mut gap_end = today - 1;
    while counted(gap_end) {
        gap_end -= 1;
    }
    let Some((&before, _)) = days.range(..gap_end).next_back() else {
        return state;
    };
    let gap = before + 1..=gap_end;
    let open = *gap.start() >= today - CATCH_UP_DAYS
        && gap_end - before <= saves
        && !blocked.iter().any(|d| gap.contains(d));
    if open {
        let mut first = before;
        while counted(first - 1) {
            first -= 1;
        }
        let after = today - 1 - gap_end + i64::from(counted(today));
        state.current = before - first + 1 + after;
        state.missed = gap.collect();
    }
    state
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

    /// `(current, best)` without saves, as `streaks()` used to be.
    fn streaks(days: &[i64], today: i64) -> (i64, i64) {
        let s = streak_state(days, &[], &[], today, None);
        assert_eq!((s.saves, s.missed.len()), (0, 0));
        (s.current, s.best)
    }

    #[test]
    fn streak_counting() {
        assert_eq!(streaks(&[], 10), (0, 0));
        assert_eq!(streaks(&[8, 9, 10], 10), (3, 3));
        assert_eq!(streaks(&[8, 9], 10), (2, 2));
        assert_eq!(streaks(&[7, 8], 10), (0, 2));
        assert_eq!(streaks(&[1, 2, 3, 4, 9, 10], 10), (2, 4));
    }

    /// Saves from day 100 on.
    const FROM: Option<i64> = Some(100);

    fn days(range: std::ops::RangeInclusive<i64>) -> Vec<i64> {
        range.collect()
    }

    fn state(ranked: &[i64], caught_up: &[i64], blocked: &[i64], today: i64) -> StreakState {
        streak_state(ranked, caught_up, blocked, today, FROM)
    }

    #[test]
    fn without_saves_nothing_else_counts() {
        // A caught-up day and a missed one: as if saves didn't exist
        let ranked = [days(100..=106), vec![108]].concat();
        let s = streak_state(&ranked, &[107], &[], 109, None);
        assert_eq!(
            s,
            StreakState {
                current: 1,
                best: 7,
                saves: 0,
                missed: vec![]
            }
        );
        let s = streak_state(&days(100..=120), &[], &[], 122, None);
        assert_eq!(
            s,
            StreakState {
                current: 0,
                best: 21,
                saves: 0,
                missed: vec![]
            }
        );
    }

    #[test]
    fn every_7th_day_in_a_row_earns_a_save_up_to_2() {
        assert_eq!(state(&days(100..=105), &[], &[], 106).saves, 0);
        assert_eq!(state(&days(100..=106), &[], &[], 107).saves, 1);
        assert_eq!(state(&days(100..=112), &[], &[], 113).saves, 1);
        assert_eq!(state(&days(100..=113), &[], &[], 114).saves, 2);
        assert_eq!(state(&days(100..=130), &[], &[], 131).saves, 2);
        // Today counts as soon as it's solved
        assert_eq!(state(&days(100..=106), &[], &[], 106).saves, 1);
        // Two runs of 6 earn nothing
        let ranked = [days(100..=105), days(107..=112)].concat();
        assert_eq!(state(&ranked, &[], &[], 113).saves, 0);
    }

    #[test]
    fn the_count_starts_at_saves_from() {
        // 17 days in a row, 7 of them from day 100 on
        let s = state(&days(90..=106), &[], &[], 107);
        assert_eq!((s.current, s.best, s.saves), (17, 17, 1));
        assert_eq!(state(&days(90..=105), &[], &[], 106).saves, 0);
        // Nothing before
        assert_eq!(state(&days(70..=99), &[], &[], 100).saves, 0);
        // A run that starts later counts from its start
        assert_eq!(state(&days(103..=108), &[], &[], 109).saves, 0);
        assert_eq!(state(&days(103..=109), &[], &[], 110).saves, 1);
    }

    #[test]
    fn a_caught_up_day_spends_a_save_and_counts() {
        // 7 days earn a save, day 107 caught up spends it and counts +1
        let ranked = [days(100..=106), vec![108]].concat();
        let s = state(&ranked, &[107], &[], 109);
        assert_eq!(
            s,
            StreakState {
                current: 9,
                best: 9,
                saves: 0,
                missed: vec![]
            }
        );
        // A day both ranked and caught up (a guest merged in) counts once, as ranked
        let s = state(&days(100..=108), &[107], &[], 109);
        assert_eq!((s.current, s.saves), (9, 1));
        // Caught-up days before saves start don't count
        let s = state(&[98, 100], &[99], &[], 101);
        assert_eq!((s.current, s.best), (1, 1));
    }

    #[test]
    fn caught_up_days_count_toward_earning() {
        // A save from the first run, the second run's 7th day is caught up: spends and earns
        let ranked = [days(100..=106), days(110..=115), vec![117]].concat();
        let s = state(&ranked, &[116], &[], 118);
        assert_eq!((s.current, s.best, s.saves), (8, 8, 1));
        // Its 14th day is one too
        let ranked = [days(100..=106), days(110..=122)].concat();
        let s = state(&ranked, &[123], &[], 124);
        assert_eq!((s.current, s.saves), (14, 2));
    }

    #[test]
    fn saves_carry_over_a_broken_run() {
        let ranked = [days(100..=106), days(120..=121)].concat();
        let s = state(&ranked, &[], &[], 122);
        assert_eq!(
            s,
            StreakState {
                current: 2,
                best: 7,
                saves: 1,
                missed: vec![]
            }
        );
        // And are spent in the next one
        let ranked = [days(100..=106), vec![120, 122]].concat();
        let s = state(&ranked, &[121], &[], 123);
        assert_eq!((s.current, s.saves), (3, 0));
    }

    #[test]
    fn a_one_day_gap_stays_open_with_a_save() {
        let ranked = days(100..=106);
        // Yesterday missed, today not done yet
        let s = state(&ranked, &[], &[], 108);
        assert_eq!(
            s,
            StreakState {
                current: 7,
                best: 7,
                saves: 1,
                missed: vec![107]
            }
        );
        // Today done
        let s = state(&[ranked.clone(), vec![108]].concat(), &[], &[], 108);
        assert_eq!(
            s,
            StreakState {
                current: 8,
                best: 7,
                saves: 1,
                missed: vec![107]
            }
        );
        // The day before yesterday missed, yesterday and today done
        let s = state(&[ranked, vec![108, 109]].concat(), &[], &[], 109);
        assert_eq!((s.current, s.missed), (9, vec![107]));
        // Without a save it breaks
        let s = state(&days(100..=105), &[], &[], 107);
        assert_eq!(
            s,
            StreakState {
                current: 0,
                best: 6,
                saves: 0,
                missed: vec![]
            }
        );
        let s = state(&[days(100..=105), vec![107]].concat(), &[], &[], 107);
        assert_eq!((s.current, s.missed), (1, vec![]));
    }

    #[test]
    fn a_two_day_gap_needs_two_saves() {
        let s = state(&days(100..=113), &[], &[], 116);
        assert_eq!(
            s,
            StreakState {
                current: 14,
                best: 14,
                saves: 2,
                missed: vec![114, 115]
            }
        );
        let s = state(&[days(100..=113), vec![116]].concat(), &[], &[], 116);
        assert_eq!((s.current, s.missed), (15, vec![114, 115]));
        // One save isn't enough
        let s = state(&days(100..=112), &[], &[], 115);
        assert_eq!(
            s,
            StreakState {
                current: 0,
                best: 13,
                saves: 1,
                missed: vec![]
            }
        );
        // Catching up the first spends a save and leaves the second open
        let s = state(&days(100..=113), &[114], &[], 116);
        assert_eq!(
            s,
            StreakState {
                current: 15,
                best: 15,
                saves: 1,
                missed: vec![115]
            }
        );
    }

    #[test]
    fn a_gap_too_long_or_too_old_breaks_the_run() {
        // Three days missed
        let s = state(&days(100..=113), &[], &[], 117);
        assert_eq!(
            s,
            StreakState {
                current: 0,
                best: 14,
                saves: 2,
                missed: vec![]
            }
        );
        // One day missed, but three days ago
        let ranked = [days(100..=113), vec![115, 116]].concat();
        let s = state(&ranked, &[], &[], 117);
        assert_eq!(
            s,
            StreakState {
                current: 2,
                best: 14,
                saves: 2,
                missed: vec![]
            }
        );
        // Today done: still the same gap, still too old
        let s = state(&[ranked, vec![117]].concat(), &[], &[], 117);
        assert_eq!((s.current, s.missed), (3, vec![]));
        // Nothing before the gap: no streak to keep
        let s = state(&[], &[], &[], 117);
        assert_eq!(s, StreakState::default());
    }

    #[test]
    fn a_revealed_or_flagged_day_cant_be_caught_up() {
        let s = state(&days(100..=106), &[], &[107], 108);
        assert_eq!(
            s,
            StreakState {
                current: 0,
                best: 7,
                saves: 1,
                missed: vec![]
            }
        );
        let s = state(&days(100..=113), &[], &[115], 116);
        assert_eq!((s.current, s.missed), (0, vec![]));
        // A blocked day elsewhere doesn't matter
        let s = state(&days(100..=106), &[], &[90, 108], 108);
        assert_eq!((s.current, s.missed), (7, vec![107]));
    }

    #[test]
    fn today_is_never_missed() {
        let s = state(&days(100..=106), &[], &[], 107);
        assert_eq!(
            s,
            StreakState {
                current: 7,
                best: 7,
                saves: 1,
                missed: vec![]
            }
        );
        let s = state(&days(100..=105), &[], &[], 106);
        assert_eq!(
            s,
            StreakState {
                current: 6,
                best: 6,
                saves: 0,
                missed: vec![]
            }
        );
    }

    #[test]
    fn durations() {
        assert_eq!(format_duration(65_300), "1:05.3");
        assert_eq!(format_duration(750_000), "12:30");
        assert_eq!(format_duration(3_723_000), "1:02:03");
    }
}
