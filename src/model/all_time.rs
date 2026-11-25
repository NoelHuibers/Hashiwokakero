//! A group's all time records: who won the most dailies and races, who was yokozuna most often,
//! and everyone's streak. Worked out by `server::groups::all_time` from the same data as the
//! banzuke, nothing is stored.
//!
//! Like the banzuke, only the group's current members count: someone who left takes their
//! results along (the others are compared as if they hadn't been there), someone who joined
//! brings theirs. Dailies count from the group's first banzuke week on.
//!
//! - A daily win: the fastest ranked daily (finished, not given up, not flagged, the banzuke's
//!   rule) of a day among the members who have one, on days with at least two of them. Equal
//!   times (by `score_time`, like the banzuke's places) all win.
//! - A race win: the fastest placed finish (solved, not given up, not flagged, see
//!   [`RaceResult::placed`]) of an ended race of the group, among the members who joined it, in
//!   races at least two of them joined. Equal times all win.
//! - A yokozuna week: a week over in which the member was the banzuke's yokozuna, with at least
//!   [`CROWN_MIN_ACTIVE`](super::CROWN_MIN_ACTIVE) members on it (the week's [`champions`], who wear the crown).

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{GroupMember, RaceResult, WeekSolve, champions, monday_of, score_time, standings};

/// Daily wins of `members`: per day, the fastest of their ranked dailies, on days at least two
/// of them have one. Members without a win are left out.
pub fn daily_wins(members: &[i64], solves: &[WeekSolve]) -> HashMap<i64, i64> {
    let members: HashSet<i64> = members.iter().copied().collect();
    let mut days: BTreeMap<i64, Vec<(i64, i64)>> = BTreeMap::new();
    for s in solves.iter().filter(|s| members.contains(&s.user)) {
        days.entry(s.day)
            .or_default()
            .push((s.user, score_time(s.day, s.time_ms)));
    }
    let mut wins = HashMap::new();
    for times in days.values().filter(|t| t.len() >= 2) {
        for user in fastest(times) {
            *wins.entry(user).or_insert(0) += 1;
        }
    }
    wins
}

/// Race wins of `members` from the results of a group's ended races, `(race, result)`: per race
/// at least two of them joined, the fastest of their placed finishes. Members without a win are
/// left out.
pub fn race_wins(members: &[i64], results: &[(i64, RaceResult)]) -> HashMap<i64, i64> {
    let members: HashSet<i64> = members.iter().copied().collect();
    let mut races: BTreeMap<i64, Vec<&RaceResult>> = BTreeMap::new();
    for (race, result) in results.iter().filter(|(_, r)| members.contains(&r.user_id)) {
        races.entry(*race).or_default().push(result);
    }
    let mut wins = HashMap::new();
    for racers in races.values().filter(|r| r.len() >= 2) {
        let times: Vec<(i64, i64)> = racers
            .iter()
            .filter(|r| r.placed())
            .filter_map(|r| Some((r.user_id, r.time_ms?)))
            .collect();
        for user in fastest(&times) {
            *wins.entry(user).or_insert(0) += 1;
        }
    }
    wins
}

/// The users with the lowest time of `(user, time)`, all of them if they're equal.
fn fastest(times: &[(i64, i64)]) -> Vec<i64> {
    let Some(best) = times.iter().map(|&(_, t)| t).min() else {
        return Vec::new();
    };
    times
        .iter()
        .filter(|&&(_, t)| t == best)
        .map(|&(u, _)| u)
        .collect()
}

/// Weeks `members` were yokozuna, of the weeks from the one of `first` (a Monday) to the one
/// before `current` (a Monday): the banzuke's [`champions`] of each. Members never yokozuna are
/// left out.
pub fn yokozuna_weeks(
    members: &[i64],
    solves: &[WeekSolve],
    first: i64,
    current: i64,
) -> HashMap<i64, i64> {
    let mut weeks: BTreeMap<i64, Vec<WeekSolve>> = BTreeMap::new();
    for s in solves {
        let monday = monday_of(s.day);
        if (first..current).contains(&monday) {
            weeks.entry(monday).or_default().push(*s);
        }
    }
    let mut won = HashMap::new();
    for (&monday, solves) in &weeks {
        for user in champions(&standings(members, solves, monday)) {
            *won.entry(user).or_insert(0) += 1;
        }
    }
    won
}

/// A column of the all time table, what it can be sorted by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AllTimeColumn {
    DailyWins,
    RaceWins,
    Yokozuna,
    Streak,
}

impl AllTimeColumn {
    pub const ALL: [Self; 4] = [
        Self::DailyWins,
        Self::RaceWins,
        Self::Yokozuna,
        Self::Streak,
    ];

    /// For the browser tests.
    pub const fn id(self) -> &'static str {
        match self {
            Self::DailyWins => "daily",
            Self::RaceWins => "races",
            Self::Yokozuna => "yokozuna",
            Self::Streak => "streak",
        }
    }

    pub const fn of(self, row: &GroupAllTimeRow) -> i64 {
        match self {
            Self::DailyWins => row.daily_wins,
            Self::RaceWins => row.race_wins,
            Self::Yokozuna => row.yokozuna,
            Self::Streak => row.streak,
        }
    }
}

/// A member's line of the all time table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupAllTimeRow {
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub colour: u8,
    pub daily_wins: i64,
    pub race_wins: i64,
    /// Weeks over they were the group's yokozuna.
    pub yokozuna: i64,
    /// Their current daily streak, as their own page shows it.
    pub streak: i64,
}

/// A group's all time table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupAllTime {
    /// A row per member, by daily wins, then race wins (see [`sort_all_time`]).
    pub rows: Vec<GroupAllTimeRow>,
    /// The first day whose dailies count, the Monday of the group's first banzuke week.
    pub first: i64,
    /// Today's dailies aren't counted yet: the viewer hasn't finished today's daily (the wins
    /// would tell who was quicker, like the banzuke's).
    pub today_hidden: bool,
}

impl GroupAllTime {
    /// Nobody has won anything yet.
    pub fn empty(&self) -> bool {
        self.rows
            .iter()
            .all(|r| r.daily_wins == 0 && r.race_wins == 0 && r.yokozuna == 0)
    }
}

/// The rows of `members` (in their order) from the counts per member, sorted by daily wins, then
/// race wins.
pub fn all_time_rows<S: std::hash::BuildHasher>(
    members: &[GroupMember],
    daily: &HashMap<i64, i64, S>,
    races: &HashMap<i64, i64, S>,
    yokozuna: &HashMap<i64, i64, S>,
    streaks: &HashMap<i64, i64, S>,
) -> Vec<GroupAllTimeRow> {
    let of = |m: &HashMap<i64, i64, S>, user| m.get(&user).copied().unwrap_or(0);
    let mut rows: Vec<GroupAllTimeRow> = members
        .iter()
        .map(|m| GroupAllTimeRow {
            user_id: m.user_id,
            name: m.name.clone(),
            guest: m.guest,
            colour: m.colour,
            daily_wins: of(daily, m.user_id),
            race_wins: of(races, m.user_id),
            yokozuna: of(yokozuna, m.user_id),
            streak: of(streaks, m.user_id),
        })
        .collect();
    sort_all_time(&mut rows, AllTimeColumn::DailyWins, false);
    rows
}

/// Sorts the rows by `column`, the most first (`reversed`: the fewest), then by the other
/// columns in their order (the most first), then by name.
pub fn sort_all_time(rows: &mut [GroupAllTimeRow], column: AllTimeColumn, reversed: bool) {
    rows.sort_by_cached_key(|r| {
        let rest: Vec<Reverse<i64>> = AllTimeColumn::ALL
            .iter()
            .filter(|&&c| c != column)
            .map(|c| Reverse(c.of(r)))
            .collect();
        let first = if reversed {
            column.of(r)
        } else {
            -column.of(r)
        };
        (first, rest, r.name.to_lowercase(), r.user_id)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MS_PER_DAY;

    /// 2026-10-19, a Monday.
    const MONDAY: i64 = 20_745;

    fn solve(user: i64, day: i64, time_ms: i64) -> WeekSolve {
        WeekSolve {
            user,
            day,
            time_ms,
            finished_at: day * MS_PER_DAY + 10 * 3_600_000,
            clean: false,
        }
    }

    fn result(user_id: i64, time_ms: Option<i64>) -> RaceResult {
        RaceResult {
            user_id,
            finished_at: time_ms.map(|t| 1_000_000 + t),
            revealed: false,
            time_ms,
            flag: None,
        }
    }

    fn sorted(map: HashMap<i64, i64>) -> Vec<(i64, i64)> {
        let mut v: Vec<_> = map.into_iter().collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn the_fastest_of_a_day_wins_and_ties_all_win() {
        let solves = [
            // 2 beats 1
            solve(1, MONDAY, 60_000),
            solve(2, MONDAY, 50_000),
            // 1 and 3 tie, 2 is slower
            solve(1, MONDAY + 1, 40_000),
            solve(3, MONDAY + 1, 40_000),
            solve(2, MONDAY + 1, 45_000),
            // Since shared places, hundredths decide, the milliseconds below don't (score_time)
            solve(2, MONDAY + 7, 30_001),
            solve(3, MONDAY + 7, 30_004),
        ];
        assert_eq!(
            sorted(daily_wins(&[1, 2, 3], &solves)),
            [(1, 1), (2, 2), (3, 2)]
        );
    }

    #[test]
    fn a_day_with_one_member_doesnt_count() {
        let solves = [
            solve(1, MONDAY, 60_000),
            solve(2, MONDAY + 1, 50_000),
            // 4 isn't a member: 1 alone on the day
            solve(1, MONDAY + 2, 60_000),
            solve(4, MONDAY + 2, 10_000),
        ];
        assert!(daily_wins(&[1, 2, 3], &solves).is_empty());
    }

    /// Someone who left takes their results along: whoever was second wins the day if two
    /// members are left on it.
    #[test]
    fn who_left_is_out_of_the_days() {
        let solves = [
            solve(1, MONDAY, 60_000),
            solve(2, MONDAY, 50_000),
            solve(9, MONDAY, 10_000),
            solve(1, MONDAY + 1, 60_000),
            solve(9, MONDAY + 1, 10_000),
        ];
        assert_eq!(sorted(daily_wins(&[1, 2], &solves)), [(2, 1)]);
        let wins = daily_wins(&[1, 2, 9], &solves);
        assert_eq!(sorted(wins), [(9, 2)]);
    }

    #[test]
    fn the_fastest_placed_racer_wins_ties_all_win() {
        let revealed = RaceResult {
            revealed: true,
            ..result(3, Some(10_000))
        };
        let flagged = RaceResult {
            flag: Some(crate::model::Flag::TooFast),
            ..result(4, Some(5_000))
        };
        let results = [
            // 1 beats 2; 3 gave up and 4 is flagged, quicker but no winners
            (10, result(1, Some(60_000))),
            (10, result(2, Some(70_000))),
            (10, revealed),
            (10, flagged),
            // 1 and 2 tie
            (11, result(1, Some(50_000))),
            (11, result(2, Some(50_000))),
            // 2 alone finished, 1 ran out of time: 2 wins
            (12, result(1, None)),
            (12, result(2, Some(90_000))),
        ];
        assert_eq!(sorted(race_wins(&[1, 2, 3, 4], &results)), [(1, 2), (2, 2)]);
    }

    /// A race needs two members who joined it: one alone, or one with someone who left since,
    /// isn't won.
    #[test]
    fn a_race_of_one_member_doesnt_count() {
        let results = [
            (10, result(1, Some(60_000))),
            (11, result(1, Some(60_000))),
            (11, result(9, Some(30_000))),
            // Nobody finished: nobody wins
            (12, result(1, None)),
            (12, result(2, None)),
        ];
        assert!(race_wins(&[1, 2], &results).is_empty());
        // With 9 still a member, 9 won race 11
        assert_eq!(sorted(race_wins(&[1, 2, 9], &results)), [(9, 1)]);
    }

    #[test]
    fn yokozuna_weeks_are_the_crowned_weeks_over() {
        let solves = [
            // Week 1: 1 before 2
            solve(1, MONDAY, 50_000),
            solve(2, MONDAY, 60_000),
            // Week 2: 2 alone, too few for a yokozuna
            solve(2, MONDAY + 7, 60_000),
            // Week 3: 1 and 2 tie on points and days won: both yokozuna
            solve(1, MONDAY + 14, 50_000),
            solve(2, MONDAY + 14, 60_000),
            solve(1, MONDAY + 15, 60_000),
            solve(2, MONDAY + 15, 50_000),
            // Week 4, the current one: not over
            solve(2, MONDAY + 21, 10_000),
            solve(1, MONDAY + 21, 20_000),
            // Before the group's first week
            solve(2, MONDAY - 7, 10_000),
            solve(1, MONDAY - 7, 20_000),
        ];
        let weeks = yokozuna_weeks(&[1, 2], &solves, MONDAY, MONDAY + 21);
        assert_eq!(sorted(weeks), [(1, 2), (2, 1)]);
        // Without 1, nobody else played week 1 and 3 enough for a crown
        assert!(yokozuna_weeks(&[2], &solves, MONDAY, MONDAY + 21).is_empty());
    }

    fn member(user_id: i64, name: &str) -> GroupMember {
        GroupMember {
            user_id,
            name: name.into(),
            guest: false,
            colour: 0,
            joined_at: 0,
            owner: false,
            stamp: None,
            done_today: false,
        }
    }

    #[test]
    fn rows_go_by_daily_wins_then_race_wins_and_sort_by_any_column() {
        let members = [
            member(1, "aiko"),
            member(2, "bram"),
            member(3, "chen"),
            member(4, "dara"),
        ];
        let map = |pairs: &[(i64, i64)]| pairs.iter().copied().collect::<HashMap<_, _>>();
        let mut rows = all_time_rows(
            &members,
            &map(&[(1, 2), (2, 5), (3, 2)]),
            &map(&[(1, 1), (3, 4)]),
            &map(&[(1, 3)]),
            &map(&[(4, 9), (2, 1)]),
        );
        let order = |rows: &[GroupAllTimeRow]| rows.iter().map(|r| r.user_id).collect::<Vec<_>>();
        assert_eq!(order(&rows), [2, 3, 1, 4]);
        let all = GroupAllTime {
            rows: rows.clone(),
            first: MONDAY,
            today_hidden: false,
        };
        assert!(!all.empty());

        sort_all_time(&mut rows, AllTimeColumn::Streak, false);
        assert_eq!(order(&rows), [4, 2, 3, 1]);
        sort_all_time(&mut rows, AllTimeColumn::RaceWins, false);
        assert_eq!(order(&rows), [3, 1, 2, 4]);
        sort_all_time(&mut rows, AllTimeColumn::Yokozuna, true);
        assert_eq!(order(&rows), [2, 3, 4, 1]);

        // Streaks alone aren't wins
        let fresh = all_time_rows(
            &members,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &map(&[(1, 4)]),
        );
        // Then the streak, then the name
        assert_eq!(order(&fresh), [1, 2, 3, 4]);
        assert!(
            GroupAllTime {
                rows: fresh,
                first: MONDAY,
                today_hidden: false
            }
            .empty()
        );
    }
}
