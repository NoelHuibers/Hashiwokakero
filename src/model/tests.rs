//! Property tests of the models: day and month math across month, year and leap-year
//! boundaries, the player's time of day in every time zone, streaks, move logs and hints.

use common::{Board, MAX_BRIDGES, Puzzle};
use proptest::prelude::*;
use proptest::test_runner::{RngSeed, contextualize_config};

use super::*;

/// The same cases on every run, so CI never fails at random. `PROPTEST_RNG_SEED` (other cases)
/// and `PROPTEST_CASES` (more of them) search further, a failure prints its smallest case.
fn config(cases: u32) -> ProptestConfig {
    contextualize_config(ProptestConfig {
        cases,
        rng_seed: RngSeed::Fixed(0x4a5e),
        // The fixed seed finds a failure again, no files in the source tree needed
        failure_persistence: None,
        ..ProptestConfig::default()
    })
}

/// Days from year 0 to 9999, the ones `date_string` writes with four digits.
fn day() -> impl Strategy<Value = i64> {
    days_from_civil(0, 1, 1)..=days_from_civil(9999, 12, 31)
}

fn days_in_month(y: i64, m: u32) -> u32 {
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A board with possible bridges and bridge counts on it.
fn board_and_counts() -> impl Strategy<Value = (Board, Vec<u8>, Vec<u8>)> {
    (2..=8usize, 2..=8usize)
        .prop_flat_map(|(w, h)| {
            let cell = prop_oneof![2 => Just(0u8), 1 => 1..=8u8];
            prop::collection::vec(prop::collection::vec(cell, w), h)
        })
        .prop_map(|grid| Board::new(Puzzle::from_grid(&grid)))
        .prop_filter("some bridges", |b| !b.edges.is_empty())
        .prop_flat_map(|board| {
            let n = board.edges.len();
            let counts = prop::collection::vec(0..=MAX_BRIDGES, n);
            (Just(board), counts.clone(), counts)
        })
}

fn tap(t: u32, edge: usize, count: u8) -> MoveEvent {
    MoveEvent {
        t,
        edge: u16::try_from(edge).expect("small board"),
        count,
        tap: true,
    }
}

proptest! {
    #![proptest_config(config(256))]

    /// Every day is a valid date, the next day is the next date (also across months, years and
    /// leap days) and the weekdays follow each other.
    #[test]
    fn days_and_dates_round_trip(day in day()) {
        let (y, m, d) = civil(day);
        prop_assert!((1..=12).contains(&m) && (1..=days_in_month(y, m)).contains(&d));
        prop_assert_eq!(days_from_civil(y, m, d), day);
        prop_assert_eq!(parse_date(&date_string(day)), Some(day));
        let next = match (m, d == days_in_month(y, m)) {
            (12, true) => (y + 1, 1, 1),
            (_, true) => (y, m + 1, 1),
            _ => (y, m, d + 1),
        };
        prop_assert_eq!(civil(day + 1), next);
        prop_assert_eq!(weekday(day + 1), (weekday(day) + 1) % 7);
        prop_assert_eq!(daily_difficulty(day + 7), daily_difficulty(day));
        let pretty = pretty_date(day);
        prop_assert!(pretty.starts_with(&WEEKDAY_NAMES[weekday(day)][..3]), "{}", pretty);
        prop_assert!(pretty.contains(&format!(" {d} ")) && pretty.ends_with(&format!(" {y}")), "{}", pretty);
        // A day number of a timestamp is the day it starts at, whatever the time of day
        prop_assert_eq!(day_of(day * MS_PER_DAY), day);
        prop_assert_eq!(day_of((day + 1) * MS_PER_DAY - 1), day);
    }

    /// Dates that don't exist are rejected, not moved to another day.
    #[test]
    fn impossible_dates_are_rejected(y in 0..=9999i64, m in 0..=13u32, d in 0..=32u32) {
        let valid = (1..=12).contains(&m) && (1..=days_in_month(y, m)).contains(&d);
        let parsed = parse_date(&format!("{y:04}-{m:02}-{d:02}"));
        prop_assert_eq!(parsed.is_some(), valid);
        if let Some(day) = parsed {
            prop_assert_eq!(civil(day), (y, m, d));
        }
    }

    /// A day is in its month of the monthly race, the months before and after are the ones
    /// next to it, and a month has 28 to 31 days.
    #[test]
    fn months_contain_their_days(day in day()) {
        let month = month_of(day);
        let (first, last) = month_days(month);
        prop_assert!(first <= day && day <= last);
        prop_assert!((27..=30).contains(&(last - first)));
        prop_assert_eq!(month_of(first - 1), month - 1);
        prop_assert_eq!(month_of(last + 1), month + 1);
        let (y, m, _) = civil(day);
        prop_assert_eq!(month_civil(month), (y, m));
        prop_assert_eq!(civil(first).2, 1);
    }

    /// The time of day is the player's local time in every time zone (UTC-12 to UTC+14), a
    /// day later it is the same, and offsets no time zone has count as unknown.
    #[test]
    fn dayparts_follow_the_local_clock(
        ms in 0..4_000_000_000_000i64,
        offset in -14 * 60..=12 * 60i32,
        bogus in prop_oneof![i32::MIN..-14 * 60, 12 * 60 + 1..=i32::MAX],
    ) {
        let local_hour = (ms - i64::from(offset) * 60_000).rem_euclid(MS_PER_DAY) / 3_600_000;
        let expected = match local_hour {
            0..=3 => Some(Daypart::Night),
            12 | 13 => Some(Daypart::Lunch),
            _ => None,
        };
        prop_assert_eq!(Daypart::at(ms, offset), expected);
        prop_assert_eq!(Daypart::at(ms + MS_PER_DAY, offset), expected);
        // The same moment in UTC: shifted by the offset
        prop_assert_eq!(Daypart::at(ms - i64::from(offset) * 60_000, 0), expected);
        prop_assert_eq!(Daypart::at(ms, bogus), None);
        if let Some(d) = expected {
            prop_assert_eq!(Daypart::from_id(d.id()), Some(d));
        }
    }

    /// Streaks are the longest run of consecutive days, and the current one ends today or
    /// yesterday.
    #[test]
    fn streaks_count_consecutive_days(
        mut days in prop::collection::vec(0..60i64, 0..40),
        today in 0..70i64,
    ) {
        days.sort_unstable();
        days.dedup();
        let solved = |d: i64| days.binary_search(&d).is_ok();
        let run_ending = |end: i64| (0..=end).take_while(|k| solved(end - k)).count();
        let best = days.iter().map(|&d| run_ending(d)).max().unwrap_or(0);
        let current = days
            .last()
            .filter(|&&last| last >= today - 1)
            .map_or(0, |&last| run_ending(last));
        let to_i64 = |n: usize| i64::try_from(n).expect("small");
        prop_assert_eq!(streaks(&days, today), (to_i64(current), to_i64(best)));
    }

    /// Durations read like a clock: tenths under ten minutes, hours from an hour on.
    #[test]
    fn durations_read_back(ms in any::<i64>()) {
        let text = format_duration(ms);
        let ms = ms.max(0);
        let (clock, tenths) = text.split_once('.').map_or((text.as_str(), None), |(c, t)| (c, Some(t)));
        let seconds = clock
            .split(':')
            .map(|p| p.parse::<i64>().expect("digits"))
            .fold(0, |total, part| total * 60 + part);
        prop_assert_eq!(seconds, ms / 1000);
        prop_assert_eq!(tenths.is_some(), ms < 600_000);
        if let Some(t) = tenths {
            prop_assert_eq!(t.parse::<i64>().expect("a digit"), ms % 1000 / 100);
        }
        prop_assert_eq!(clock.matches(':').count(), if ms >= 3_600_000 { 2 } else { 1 });
    }

    /// A log of a player clicking the bridges into place at a human pace passes, one that
    /// doesn't lead to the submitted board or goes back in time doesn't.
    #[test]
    fn honest_logs_pass((board, solution, start) in board_and_counts(), gaps in prop::collection::vec(500..2000u32, 200)) {
        // From the empty board through some other bridges to the solution, one click each, at
        // least half a second apart (two bridges' worth of the floor)
        let mut log = Vec::new();
        let mut t = 1000;
        let mut gaps = gaps.into_iter().cycle();
        for (e, &count) in start.iter().enumerate().chain(solution.iter().enumerate()) {
            if count > 0 || log.iter().any(|m: &MoveEvent| usize::from(m.edge) == e) {
                t += gaps.next().expect("cycled");
                log.push(tap(t, e, count));
            }
        }
        prop_assume!(!log.is_empty());
        let elapsed = i64::from(t) + 500;
        prop_assert_eq!(verify_log(&board, &solution, &solution, &log, elapsed, 0), Ok(()));
        // The summary of what was just checked
        let summary = summarize_log(&log);
        prop_assert_eq!(summary.events as usize, log.len());
        prop_assert_eq!(summary.span_ms, log.last().expect("not empty").t - log[0].t);
        if let (Some(fastest), Some(median)) = (summary.fastest_gap_ms, summary.median_gap_ms) {
            prop_assert!(500 <= fastest && fastest <= median);
        }

        let mut other = solution.clone();
        other[0] = (other[0] + 1) % 3;
        prop_assert_eq!(verify_log(&board, &solution, &other, &log, elapsed, 0), Err(Flag::Log));
        if log.len() > 1 {
            let mut backwards = log.clone();
            backwards.swap(0, 1);
            prop_assert_eq!(verify_log(&board, &solution, &solution, &backwards, elapsed, 0), Err(Flag::Log));
        }
        let mut beyond = log.clone();
        beyond.push(tap(t + 1, board.edges.len(), 1));
        prop_assert_eq!(verify_log(&board, &solution, &solution, &beyond, elapsed, 0), Err(Flag::Log));
    }

    /// Whatever a client sends as its log, checking and summarizing it gives an answer, never
    /// a crash.
    #[test]
    fn any_log_is_checked(
        (board, solution, bridges) in board_and_counts(),
        log in prop::collection::vec((any::<u32>(), 0..40u16, any::<u8>(), any::<bool>()), 0..60),
        elapsed in any::<i64>(),
        hints in any::<i64>(),
    ) {
        let log: Vec<MoveEvent> = log
            .into_iter()
            .map(|(t, edge, count, tap)| MoveEvent { t, edge, count, tap })
            .collect();
        let _ = verify_log(&board, &solution, &bridges, &log, elapsed.clamp(-1 << 40, 1 << 40), hints.clamp(-1 << 40, 1 << 40));
        let summary = summarize_log(&log);
        prop_assert!(summary.taps <= summary.events);
    }

    /// Today's daily is recognised however it is turned or mirrored.
    #[test]
    fn canonical_form_ignores_symmetry((board, _, _) in board_and_counts(), t in 0..8u8) {
        let p = &board.puzzle;
        let (w, h) = (p.width, p.height);
        let mut turned = if t & 4 == 0 { Puzzle::empty(w, h) } else { Puzzle::empty(h, w) };
        for i in &p.islands {
            let (mut x, mut y) = (i.pos.x, i.pos.y);
            if t & 1 != 0 {
                x = w - 1 - x;
            }
            if t & 2 != 0 {
                y = h - 1 - y;
            }
            if t & 4 != 0 {
                (x, y) = (y, x);
            }
            turned.set(common::Pos::new(x, y), i.required);
        }
        prop_assert_eq!(canonical(&turned), canonical(p));
    }

    /// Taking every hint leads to the solution, wrong bridges are fixed first, and `wrong_edges`
    /// lists the bridges beyond the solution.
    #[test]
    fn hints_lead_to_the_solution((board, solution, start) in board_and_counts()) {
        let n = board.edges.len();
        let wrong: Vec<usize> = (0..n).filter(|&e| start[e] > solution[e]).collect();
        prop_assert_eq!(wrong_edges(&start, &solution), wrong);
        let mut current = start;
        let mut steps = 0;
        while let Some((edge, value)) = pick_hint(&board, &current, &solution) {
            prop_assert_eq!(value, solution[edge]);
            prop_assert_ne!(current[edge], value);
            if (0..n).any(|e| current[e] > solution[e]) {
                prop_assert!(current[edge] > value, "a missing bridge before a wrong one");
            }
            current[edge] = value;
            steps += 1;
            prop_assert!(steps <= n);
        }
        prop_assert_eq!(&current, &solution);
        prop_assert!(wrong_edges(&current, &solution).is_empty());
        // Bridges of another puzzle get no hint
        prop_assert_eq!(pick_hint(&board, &current[1..], &solution), None);
    }
}
