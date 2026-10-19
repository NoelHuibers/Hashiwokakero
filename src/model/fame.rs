//! The long run rankings: the hall of fame and the monthly board.

use serde::{Deserialize, Serialize};

use super::{Stamp, civil, days_from_civil};

/// A player in the hall of fame. Medals only count for days that are over.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllTimeEntry {
    pub rank: i64,
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub solved: i64,
    pub gold: i64,
    pub silver: i64,
    pub bronze: i64,
    pub best_streak: i64,
    pub current_streak: i64,
    /// The stamp the player chose to show next to their name.
    pub stamp: Option<Stamp>,
}

/// The hall of fame, one ranking per tab.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HallOfFame {
    /// Most dailies solved.
    pub solved: Vec<AllTimeEntry>,
    /// Most gold, then silver, then bronze medals.
    pub medals: Vec<AllTimeEntry>,
    /// Longest streak of days in a row.
    pub streaks: Vec<AllTimeEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonthEntry {
    pub rank: i64,
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub points: i64,
    pub solved: i64,
    pub stamp: Option<Stamp>,
}

/// The points race of one calendar month (UTC).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonthBoard {
    /// See [`month_of`].
    pub month: i64,
    pub entries: Vec<MonthEntry>,
    /// The current user's entry, also when it isn't in `entries`.
    pub me: Option<MonthEntry>,
    /// Everyone with points this month.
    pub players: i64,
}

/// Points for finishing a daily.
pub const FINISH_POINTS: i64 = 10;
/// Most extra points for the placement, the fastest of the day gets all of them.
pub const PLACE_POINTS: i64 = 10;

/// Points a ranked daily earns on the monthly board: 10 for finishing, and up to 10 more for
/// beating the others, relative to how many finished. 3rd of 40 is worth more than 3rd of 4.
pub fn points(place: i64, finishers: i64) -> i64 {
    if finishers <= 1 {
        return FINISH_POINTS + PLACE_POINTS;
    }
    let beaten = (finishers - place).clamp(0, finishers - 1);
    // Rounded to the nearest point
    FINISH_POINTS + (PLACE_POINTS * beaten * 2 + finishers - 1) / ((finishers - 1) * 2)
}

/// Months counted since year 0, `year * 12 + month - 1`, so the next month is `+ 1`.
pub fn month_of(day: i64) -> i64 {
    let (y, m, _) = civil(day);
    y * 12 + i64::from(m) - 1
}

/// `(year, month)` of a month number, month 1..=12.
#[expect(
    clippy::cast_possible_truncation,
    reason = "`rem_euclid(12) + 1` is in 1..=12"
)]
pub fn month_civil(month: i64) -> (i64, u32) {
    (month.div_euclid(12), (month.rem_euclid(12) + 1) as u32)
}

/// The first and last day of a month.
pub fn month_days(month: i64) -> (i64, i64) {
    let (y, m) = month_civil(month);
    let (ny, nm) = month_civil(month + 1);
    (days_from_civil(y, m, 1), days_from_civil(ny, nm, 1) - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LAUNCH_DAY, parse_date};

    #[test]
    fn points_for_placements() {
        // Alone or fastest: everything, last: only the finish
        assert_eq!(points(1, 1), 20);
        assert_eq!(points(1, 40), 20);
        assert_eq!(points(40, 40), 10);
        // Halfway rounds up
        assert_eq!(points(2, 3), 15);
        assert_eq!(points(2, 4), 17);
        // 3rd of 40 beats 3rd of 4
        assert!(points(3, 40) > points(3, 4));
    }

    #[test]
    fn months() {
        let launch = month_of(LAUNCH_DAY);
        assert_eq!(month_civil(launch), (2026, 10));
        assert_eq!(
            month_days(launch),
            (
                parse_date("2026-10-01").unwrap(),
                parse_date("2026-10-31").unwrap()
            )
        );
        // Over the turn of the year, and February of a leap year
        assert_eq!(month_civil(launch + 3), (2027, 1));
        let feb = month_of(parse_date("2028-02-10").unwrap());
        assert_eq!(month_days(feb).1, parse_date("2028-02-29").unwrap());
        assert_eq!(month_of(month_days(feb).1 + 1), feb + 1);
    }
}
