//! The weekly banzuke 番付 of a group: the standings of a UTC week (Monday to Sunday) written up
//! like the ranking list of a sumo tournament, and the week's awards 賞, so that not only the
//! fastest have something to aim for. Worked out by `server::groups::banzuke` from the ranked
//! dailies of the group's current members: someone who left is no longer on the earlier weeks
//! either, someone who joined is on them with what they played before.
//!
//! Scoring: every day, the members with a ranked result (finished, not given up, not flagged,
//! like the global board) get points by their place among them, the first as many as the group
//! has members, every place after one less. Equal times share a place. A day not played is worth
//! nothing. The week goes by points, then by days won; members equal on both share the place,
//! and the title.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::hash::BuildHasher;

use serde::{Deserialize, Serialize};

use super::{GroupMember, MS_PER_DAY};

/// Ranked dailies of a week that the steadiest and the early bird need.
pub const AWARD_MIN_SOLVES: i64 = 3;
/// Ranked dailies of the week for the most improved...
pub const IMPROVED_MIN_SOLVES: i64 = 2;
/// ...and in the [`HISTORY_WEEKS`] before, to compare with.
pub const IMPROVED_HISTORY: i64 = 3;
/// Weeks before a week the most improved is compared with.
pub const HISTORY_WEEKS: i64 = 4;
/// Members with a ranked daily in a week for its yokozuna and award winners to earn a stamp.
pub const HONOUR_MIN_ACTIVE: usize = 3;
/// The weeks over the stamp sync looks at for [`honours`], the last ones: a player who comes back
/// within them still gets their stamp (it stays, see `Stamp::sticky`).
pub const HONOUR_WEEKS: i64 = 4;
/// Members with a ranked daily in last week for its yokozuna to wear the crown.
pub const CROWN_MIN_ACTIVE: usize = 2;
/// The early bird's usual time is shown this coarsely: when someone plays is their business.
pub const EARLY_ROUND_MS: i64 = 300_000;

/// A ranked daily of a member: finished, not given up, not flagged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeekSolve {
    pub user: i64,
    pub day: i64,
    pub time_ms: i64,
    pub finished_at: i64,
    /// Without hints and checks.
    pub clean: bool,
}

/// A rank of the banzuke, from the week's place: the four of the top (sanyaku) and the
/// maegashira after them, numbered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Title {
    /// 横綱, the grand champion.
    Yokozuna,
    /// 大関, champion.
    Ozeki,
    /// 関脇
    Sekiwake,
    /// 小結
    Komusubi,
    /// 前頭 and their number, 1 for the fifth place.
    Maegashira(i64),
}

impl Title {
    pub const fn of_place(place: i64) -> Self {
        match place {
            ..=1 => Self::Yokozuna,
            2 => Self::Ozeki,
            3 => Self::Sekiwake,
            4 => Self::Komusubi,
            n => Self::Maegashira(n - 4),
        }
    }

    pub const fn kanji(self) -> &'static str {
        match self {
            Self::Yokozuna => "横綱",
            Self::Ozeki => "大関",
            Self::Sekiwake => "関脇",
            Self::Komusubi => "小結",
            Self::Maegashira(_) => "前頭",
        }
    }

    /// For CSS and the browser tests.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Yokozuna => "yokozuna",
            Self::Ozeki => "ozeki",
            Self::Sekiwake => "sekiwake",
            Self::Komusubi => "komusubi",
            Self::Maegashira(_) => "maegashira",
        }
    }
}

/// A number in kanji, like a maegashira's: 一 … 九十九. Larger ones stay digits.
pub fn kanji_number(n: i64) -> String {
    const DIGITS: [&str; 10] = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    let digit = |d: i64| DIGITS[usize::try_from(d).unwrap_or(0)];
    match n {
        1..=9 => digit(n).into(),
        10..=99 => {
            let tens = if n / 10 == 1 { "" } else { digit(n / 10) };
            format!("{tens}十{}", digit(n % 10))
        }
        _ => n.to_string(),
    }
}

/// How a member's place changed since the week before.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Movement {
    Up(i64),
    Down(i64),
    Stayed,
    /// Not on the week before's banzuke: 新 new.
    New,
}

/// The movement from the place `before` (`None`: not placed) to the place `now`.
pub fn movement(now: i64, before: Option<i64>) -> Movement {
    match before {
        None => Movement::New,
        Some(b) if b > now => Movement::Up(b - now),
        Some(b) if b < now => Movement::Down(now - b),
        Some(_) => Movement::Stayed,
    }
}

/// A member's week, before names and titles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    pub user: i64,
    pub points: i64,
    /// Days they were the fastest (shared or not).
    pub wins: i64,
    /// Their place among the group each day, Monday to Sunday, `None` without a ranked daily.
    pub days: [Option<i64>; 7],
    /// The week's place, `None` without a single ranked daily (kyūjō 休場, sitting out).
    pub place: Option<i64>,
}

impl Standing {
    pub fn played(&self) -> i64 {
        count(self.days.iter().flatten().count())
    }
}

fn count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// Index of `day` in the week of `monday`, if it's in it.
fn week_index(day: i64, monday: i64) -> Option<usize> {
    usize::try_from(day - monday).ok().filter(|&i| i < 7)
}

/// Points for a day's place in a group of `members`: as many as there are members for the
/// first, one less for every place after, at least one for playing.
pub fn day_points(place: i64, members: usize) -> i64 {
    (count(members) - place + 1).max(1)
}

/// The week of `monday` of every one of `members` (in their order), from their ranked dailies
/// (other weeks' and non-members' are ignored).
pub fn standings(members: &[i64], solves: &[WeekSolve], monday: i64) -> Vec<Standing> {
    let mut out: Vec<Standing> = members
        .iter()
        .map(|&user| Standing {
            user,
            points: 0,
            wins: 0,
            days: [None; 7],
            place: None,
        })
        .collect();
    let at: HashMap<i64, usize> = members.iter().enumerate().map(|(i, &u)| (u, i)).collect();
    for i in 0..7 {
        let day: Vec<(usize, i64)> = solves
            .iter()
            .filter(|s| week_index(s.day, monday) == Some(i))
            .filter_map(|s| at.get(&s.user).map(|&m| (m, s.time_ms)))
            .collect();
        for &(m, time) in &day {
            // Equal times share the place
            let place = 1 + count(day.iter().filter(|&&(_, t)| t < time).count());
            let s = &mut out[m];
            s.days[i] = Some(place);
            s.points += day_points(place, members.len());
            s.wins += i64::from(place == 1);
        }
    }
    let keys: Vec<(i64, i64)> = out
        .iter()
        .filter(|s| s.points > 0)
        .map(|s| (s.points, s.wins))
        .collect();
    for s in &mut out {
        if s.points > 0 {
            let better = keys.iter().filter(|&&k| k > (s.points, s.wins)).count();
            s.place = Some(1 + count(better));
        }
    }
    out
}

/// Members placed on a week's banzuke, the ones who played.
pub fn active(standings: &[Standing]) -> usize {
    standings.iter().filter(|s| s.place.is_some()).count()
}

/// The yokozuna of a week, for the crown on the archipelago: none if too few played.
pub fn champions(standings: &[Standing]) -> Vec<i64> {
    if active(standings) < CROWN_MIN_ACTIVE {
        return Vec::new();
    }
    standings
        .iter()
        .filter(|s| s.place == Some(1))
        .map(|s| s.user)
        .collect()
}

/// The ways to win a week, besides the banzuke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AwardKind {
    /// The lowest total time of those who played the most days, like the yellow jersey.
    Fastest,
    /// The most dailies without hints or checks.
    Cleanest,
    /// The paces closest together, like the Metronome stamp ([`AWARD_MIN_SOLVES`]).
    Steadiest,
    /// Usually done the soonest after the daily came out (UTC midnight).
    EarlyBird,
    /// The pace that improved most over the [`HISTORY_WEEKS`] before.
    Improved,
    /// 🏮 The slowest total of those who played all seven days: a fond prize for not giving up.
    Lantern,
}

impl AwardKind {
    pub const ALL: [Self; 6] = [
        Self::Fastest,
        Self::Cleanest,
        Self::Steadiest,
        Self::EarlyBird,
        Self::Improved,
        Self::Lantern,
    ];

    pub const fn kanji(self) -> &'static str {
        match self {
            Self::Fastest => "速",
            Self::Cleanest => "清",
            Self::Steadiest => "律",
            Self::EarlyBird => "早",
            // 進歩, progress
            Self::Improved => "進",
            // The lantern at the back of the race
            Self::Lantern => "灯",
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::Fastest => "fastest",
            Self::Cleanest => "cleanest",
            Self::Steadiest => "steadiest",
            Self::EarlyBird => "early_bird",
            Self::Improved => "improved",
            Self::Lantern => "lantern",
        }
    }
}

/// What won an award, for its one line of reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Feat {
    Fastest {
        solves: i64,
        total_ms: i64,
    },
    Cleanest {
        clean: i64,
        solves: i64,
    },
    /// Paces within `spread` percent of each other.
    Steadiest {
        solves: i64,
        spread: i64,
    },
    /// Usually done this long after the daily came out, rounded to [`EARLY_ROUND_MS`].
    EarlyBird {
        usual_ms: i64,
    },
    /// The usual pace this many percent quicker than in the weeks before.
    Improved {
        percent: i64,
    },
    /// The slowest total of the `of` who played every day.
    Lantern {
        total_ms: i64,
        of: i64,
    },
}

impl Feat {
    pub const fn kind(self) -> AwardKind {
        match self {
            Self::Fastest { .. } => AwardKind::Fastest,
            Self::Cleanest { .. } => AwardKind::Cleanest,
            Self::Steadiest { .. } => AwardKind::Steadiest,
            Self::EarlyBird { .. } => AwardKind::EarlyBird,
            Self::Improved { .. } => AwardKind::Improved,
            Self::Lantern { .. } => AwardKind::Lantern,
        }
    }
}

/// An award of a week and who won it, everyone equally good shares it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Award {
    pub winners: Vec<i64>,
    pub feat: Feat,
}

/// The candidates with the smallest key, and the key.
fn best<K: Ord + Copy>(candidates: impl IntoIterator<Item = (i64, K)>) -> Option<(Vec<i64>, K)> {
    let candidates: Vec<(i64, K)> = candidates.into_iter().collect();
    let key = candidates.iter().map(|&(_, k)| k).min()?;
    let winners = candidates
        .iter()
        .filter(|&&(_, k)| k == key)
        .map(|&(u, _)| u)
        .collect();
    Some((winners, key))
}

/// The lower middle of some numbers.
fn median(mut values: Vec<i64>) -> Option<i64> {
    values.sort_unstable();
    values.get(values.len().checked_sub(1)? / 2).copied()
}

/// A time against the median time of all players of that day, in thousandths: 1000 is the
/// usual player, 800 a fifth quicker. Makes Monday's easy daily and Sunday's oni comparable.
pub fn pace<S: BuildHasher>(solve: &WeekSolve, medians: &HashMap<i64, i64, S>) -> Option<i64> {
    let median = *medians.get(&solve.day)?;
    (median > 0).then(|| solve.time_ms * 1000 / median)
}

/// The awards of the week of `monday` among `members`.
///
/// From their ranked dailies of that week and the [`HISTORY_WEEKS`] before, and every day's
/// median time of all players (see [`pace`]). Awards nobody qualifies for are left out.
pub fn awards<S: BuildHasher>(
    members: &[i64],
    solves: &[WeekSolve],
    medians: &HashMap<i64, i64, S>,
    monday: i64,
) -> Vec<Award> {
    let mut week: HashMap<i64, Vec<WeekSolve>> = HashMap::new();
    let mut before: HashMap<i64, Vec<WeekSolve>> = HashMap::new();
    for s in solves.iter().filter(|s| members.contains(&s.user)) {
        if week_index(s.day, monday).is_some() {
            week.entry(s.user).or_default().push(*s);
        } else if (monday - 7 * HISTORY_WEEKS..monday).contains(&s.day) {
            before.entry(s.user).or_default().push(*s);
        }
    }
    let played = |user: &i64| count(week.get(user).map_or(0, Vec::len));
    let total = |user: &i64| {
        week.get(user)
            .map_or(0, |w| w.iter().map(|s| s.time_ms).sum())
    };
    let paces = |solves: Option<&Vec<WeekSolve>>| -> Vec<i64> {
        solves
            .into_iter()
            .flatten()
            .filter_map(|s| pace(s, medians))
            .collect()
    };
    let mut out = Vec::new();

    // Fastest: the lowest total of those who played the most days
    let most = members.iter().map(played).max().unwrap_or(0);
    if most > 0
        && let Some((winners, total_ms)) = best(
            members
                .iter()
                .filter(|u| played(u) == most)
                .map(|&u| (u, total(&u))),
        )
    {
        out.push(Award {
            winners,
            feat: Feat::Fastest {
                solves: most,
                total_ms,
            },
        });
    }

    // Cleanest: the most without hints or checks, then the fewest with them
    let clean = |user: &i64| {
        count(
            week.get(user)
                .map_or(0, |w| w.iter().filter(|s| s.clean).count()),
        )
    };
    if let Some((winners, (Reverse(clean), unclean))) = best(
        members
            .iter()
            .filter(|u| clean(u) > 0)
            .map(|&u| (u, (Reverse(clean(&u)), played(&u) - clean(&u)))),
    ) {
        out.push(Award {
            winners,
            feat: Feat::Cleanest {
                clean,
                solves: clean + unclean,
            },
        });
    }

    // Steadiest: the narrowest spread of paces, in ten thousandths
    if let Some((winners, (spread, solves))) = best(members.iter().filter_map(|&u| {
        let p = paces(week.get(&u));
        let (lo, hi) = (*p.iter().min()?, *p.iter().max()?);
        (count(p.len()) >= AWARD_MIN_SOLVES && lo > 0)
            .then(|| (u, ((hi - lo) * 10_000 / lo, Reverse(count(p.len())))))
    })) {
        out.push(Award {
            winners,
            feat: Feat::Steadiest {
                solves: solves.0,
                spread: (spread + 50) / 100,
            },
        });
    }

    // Early bird: usually done the soonest after midnight UTC
    if let Some((winners, usual)) = best(members.iter().filter_map(|&u| {
        let w = week.get(&u)?;
        let usual = median(
            w.iter()
                .map(|s| (s.finished_at - s.day * MS_PER_DAY).clamp(0, MS_PER_DAY - 1))
                .collect(),
        )?;
        (count(w.len()) >= AWARD_MIN_SOLVES).then_some((u, usual))
    })) {
        out.push(Award {
            winners,
            feat: Feat::EarlyBird {
                usual_ms: usual / EARLY_ROUND_MS * EARLY_ROUND_MS,
            },
        });
    }

    // Most improved: this week's usual pace against the weeks before, in ten thousandths
    if let Some((winners, Reverse(gain))) = best(members.iter().filter_map(|&u| {
        let now = paces(week.get(&u));
        let then = paces(before.get(&u));
        if count(now.len()) < IMPROVED_MIN_SOLVES || count(then.len()) < IMPROVED_HISTORY {
            return None;
        }
        let (now, then) = (median(now)?, median(then)?);
        let gain = (then - now) * 10_000 / then.max(1);
        // At least a percent quicker
        (gain >= 100).then_some((u, Reverse(gain)))
    })) {
        out.push(Award {
            winners,
            feat: Feat::Improved {
                percent: gain / 100,
            },
        });
    }

    // Lanterne rouge: the slowest total of those who played every day, if more than one did
    let full: Vec<i64> = members.iter().copied().filter(|u| played(u) == 7).collect();
    if full.len() >= 2
        && let Some((winners, Reverse(total_ms))) =
            best(full.iter().map(|&u| (u, Reverse(total(&u)))))
        && winners.len() < full.len()
    {
        out.push(Award {
            winners,
            feat: Feat::Lantern {
                total_ms,
                of: count(full.len()),
            },
        });
    }
    out
}

/// A member's line on the banzuke.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BanzukeRow {
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub colour: u8,
    pub points: i64,
    pub wins: i64,
    /// Their place among the group each day, Monday to Sunday.
    pub days: [Option<i64>; 7],
    /// `None`: no ranked daily this week, kyūjō 休場.
    pub place: Option<i64>,
    pub title: Option<Title>,
    /// Since the week before, `None` for kyūjō.
    pub movement: Option<Movement>,
}

impl BanzukeRow {
    pub fn played(&self) -> i64 {
        count(self.days.iter().flatten().count())
    }
}

/// The banzuke's lines: the week (`now`) of `members` with the movement since the week before
/// (`before`), by place and then name, the ones sitting out at the end.
pub fn banzuke_rows(
    members: &[GroupMember],
    now: &[Standing],
    before: &[Standing],
) -> Vec<BanzukeRow> {
    let mut rows: Vec<BanzukeRow> = members
        .iter()
        .filter_map(|m| {
            let s = now.iter().find(|s| s.user == m.user_id)?;
            let last = before
                .iter()
                .find(|b| b.user == m.user_id)
                .and_then(|b| b.place);
            Some(BanzukeRow {
                user_id: m.user_id,
                name: m.name.clone(),
                guest: m.guest,
                colour: m.colour,
                points: s.points,
                wins: s.wins,
                days: s.days,
                place: s.place,
                title: s.place.map(Title::of_place),
                movement: s.place.map(|p| movement(p, last)),
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        (a.place.is_none(), a.place)
            .cmp(&(b.place.is_none(), b.place))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then(a.user_id.cmp(&b.user_id))
    });
    rows
}

/// A group's banzuke and awards of a week.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Banzuke {
    /// The day number of the week's Monday.
    pub monday: i64,
    pub today: i64,
    /// The Monday of the group's first week.
    pub first: i64,
    /// Today's results aren't in yet: the viewer hasn't finished today's daily (they'd tell who
    /// was quicker, see `model::spoiler_safe`).
    pub today_hidden: bool,
    pub rows: Vec<BanzukeRow>,
    pub awards: Vec<Award>,
    /// Last week's yokozuna (of the week before the current one), crowned on the archipelago.
    pub champions: Vec<i64>,
}

impl Banzuke {
    /// The week is over: the banzuke and the awards are final.
    pub const fn over(&self) -> bool {
        self.monday + 6 < self.today
    }

    pub fn award(&self, kind: AwardKind) -> Option<&Award> {
        self.awards.iter().find(|a| a.feat.kind() == kind)
    }
}

/// What a week brought a member, for the stamps: the yokozuna, and any award. Only weeks with
/// [`HONOUR_MIN_ACTIVE`] players count.
pub fn honours(user: i64, standings: &[Standing], awards: &[Award]) -> (bool, bool) {
    if active(standings) < HONOUR_MIN_ACTIVE {
        return (false, false);
    }
    let yokozuna = standings
        .iter()
        .any(|s| s.user == user && s.place == Some(1));
    let award = awards.iter().any(|a| a.winners.contains(&user));
    (yokozuna, award)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn at(user: i64, day: i64, time_ms: i64, hour: i64) -> WeekSolve {
        WeekSolve {
            finished_at: day * MS_PER_DAY + hour * 3_600_000,
            ..solve(user, day, time_ms)
        }
    }

    fn clean(user: i64, day: i64, time_ms: i64) -> WeekSolve {
        WeekSolve {
            clean: true,
            ..solve(user, day, time_ms)
        }
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

    /// Every day of the week of `monday` with the same median.
    fn flat_medians(median: i64) -> HashMap<i64, i64> {
        (MONDAY - 7 * HISTORY_WEEKS..MONDAY + 7)
            .map(|d| (d, median))
            .collect()
    }

    fn places(s: &[Standing]) -> Vec<(i64, i64, Option<i64>)> {
        s.iter().map(|s| (s.user, s.points, s.place)).collect()
    }

    fn award(awards: &[Award], kind: AwardKind) -> Option<&Award> {
        awards.iter().find(|a| a.feat.kind() == kind)
    }

    #[test]
    fn titles_follow_the_place() {
        let titles: Vec<Title> = (1..=7).map(Title::of_place).collect();
        assert_eq!(
            titles,
            [
                Title::Yokozuna,
                Title::Ozeki,
                Title::Sekiwake,
                Title::Komusubi,
                Title::Maegashira(1),
                Title::Maegashira(2),
                Title::Maegashira(3),
            ]
        );
        assert_eq!(Title::Ozeki.kanji(), "大関");
        let numbers: Vec<String> = [1, 9, 10, 11, 20, 46, 99, 100]
            .into_iter()
            .map(kanji_number)
            .collect();
        assert_eq!(
            numbers,
            ["一", "九", "十", "十一", "二十", "四十六", "九十九", "100"]
        );
    }

    /// The first of a day gets as many points as the group has members, every place after one
    /// less; nothing for not playing.
    #[test]
    fn points_by_the_place_of_the_day() {
        let members = [1, 2, 3, 4];
        let solves = [
            solve(1, MONDAY, 60_000),
            solve(2, MONDAY, 90_000),
            solve(3, MONDAY, 75_000),
            solve(1, MONDAY + 1, 80_000),
            solve(2, MONDAY + 1, 70_000),
        ];
        let s = standings(&members, &solves, MONDAY);
        // 1: 4 + 3, 2: 2 + 4, 3: 3, 4: nothing
        assert_eq!(
            places(&s),
            [
                (1, 7, Some(1)),
                (2, 6, Some(2)),
                (3, 3, Some(3)),
                (4, 0, None)
            ]
        );
        assert_eq!(s[0].days[..2], [Some(1), Some(2)]);
        assert_eq!((s[0].wins, s[1].wins, s[2].wins), (1, 1, 0));
        assert_eq!(s[0].played(), 2);
        // The last place still gets a point
        assert_eq!(day_points(4, 4), 1);
        assert_eq!(day_points(9, 4), 1);
    }

    #[test]
    fn equal_times_share_the_place_and_ties_go_by_wins() {
        let members = [1, 2, 3];
        let solves = [
            solve(1, MONDAY, 60_000),
            solve(2, MONDAY, 60_000),
            solve(3, MONDAY, 70_000),
        ];
        let s = standings(&members, &solves, MONDAY);
        // Both first: 3 points each, the next is third
        assert_eq!(s[2].days[0], Some(3));
        assert_eq!(
            places(&s),
            [(1, 3, Some(1)), (2, 3, Some(1)), (3, 1, Some(3))]
        );

        // Equal points: the one with a day won is ahead. Four members: 4, 3, 2, 1 points
        let mut solves = Vec::new();
        for day in [MONDAY, MONDAY + 1] {
            solves.extend([
                solve(3, day, 50_000),
                solve(4, day, 55_000),
                solve(2, day, 60_000),
            ]);
        }
        solves.push(solve(1, MONDAY + 2, 90_000));
        let s = standings(&[1, 2, 3, 4], &solves, MONDAY);
        // 1: 4 (a win), 2: 2 + 2 (no win), 3: 8, 4: 6
        assert_eq!(
            places(&s),
            [
                (1, 4, Some(3)),
                (2, 4, Some(4)),
                (3, 8, Some(1)),
                (4, 6, Some(2))
            ]
        );
        // Same points, same wins: shared
        let solves = [
            solve(1, MONDAY, 60_000),
            solve(2, MONDAY, 70_000),
            solve(2, MONDAY + 1, 60_000),
            solve(1, MONDAY + 1, 70_000),
        ];
        let s = standings(&[1, 2], &solves, MONDAY);
        assert_eq!(places(&s), [(1, 3, Some(1)), (2, 3, Some(1))]);
    }

    /// Only the week's days count, and only members.
    #[test]
    fn the_week_runs_from_monday_to_sunday() {
        let solves = [
            solve(1, MONDAY - 1, 60_000),
            solve(1, MONDAY + 7, 60_000),
            solve(9, MONDAY, 10_000),
            solve(2, MONDAY + 6, 60_000),
            solve(1, MONDAY, 99_000),
        ];
        let s = standings(&[1, 2], &solves, MONDAY);
        assert_eq!(places(&s), [(1, 2, Some(1)), (2, 2, Some(1))]);
        assert_eq!(s[0].days, [Some(1), None, None, None, None, None, None]);
        assert_eq!(s[1].days[6], Some(1));
        // An empty week: nobody placed, no awards, no crown
        let s = standings(&[1, 2], &[], MONDAY);
        assert_eq!(places(&s), [(1, 0, None), (2, 0, None)]);
        assert_eq!(active(&s), 0);
        assert_eq!(champions(&s), Vec::<i64>::new());
        assert_eq!(awards(&[1, 2], &[], &flat_medians(100_000), MONDAY), []);
    }

    #[test]
    fn movement_since_last_week() {
        assert_eq!(movement(1, Some(3)), Movement::Up(2));
        assert_eq!(movement(4, Some(2)), Movement::Down(2));
        assert_eq!(movement(2, Some(2)), Movement::Stayed);
        assert_eq!(movement(2, None), Movement::New);

        let members = [
            member(1, "aiko"),
            member(2, "bram"),
            member(3, "chen"),
            member(4, "Dan"),
        ];
        let before = standings(
            &[1, 2, 3, 4],
            &[solve(1, MONDAY - 7, 50_000), solve(2, MONDAY - 7, 60_000)],
            MONDAY - 7,
        );
        let now = standings(
            &[1, 2, 3, 4],
            &[
                solve(2, MONDAY, 50_000),
                solve(1, MONDAY, 60_000),
                solve(3, MONDAY, 70_000),
            ],
            MONDAY,
        );
        let rows = banzuke_rows(&members, &now, &before);
        let seen: Vec<_> = rows
            .iter()
            .map(|r| (r.name.as_str(), r.place, r.title, r.movement))
            .collect();
        assert_eq!(
            seen,
            [
                (
                    "bram",
                    Some(1),
                    Some(Title::Yokozuna),
                    Some(Movement::Up(1))
                ),
                ("aiko", Some(2), Some(Title::Ozeki), Some(Movement::Down(1))),
                ("chen", Some(3), Some(Title::Sekiwake), Some(Movement::New)),
                // Sitting out: no title, no movement, at the end
                ("Dan", None, None, None),
            ]
        );
        assert_eq!(rows[0].played(), 1);
    }

    /// Shared places are listed by name.
    #[test]
    fn shared_places_by_name() {
        let members = [member(1, "zoe"), member(2, "Anna"), member(3, "bea")];
        let now = standings(
            &[1, 2, 3],
            &[solve(1, MONDAY, 50_000), solve(2, MONDAY, 50_000)],
            MONDAY,
        );
        let rows = banzuke_rows(&members, &now, &[]);
        let seen: Vec<_> = rows.iter().map(|r| (r.user_id, r.title)).collect();
        assert_eq!(
            seen,
            [
                (2, Some(Title::Yokozuna)),
                (1, Some(Title::Yokozuna)),
                (3, None)
            ]
        );
    }

    #[test]
    fn the_crown_needs_two_players_and_stamps_three() {
        let one = standings(&[1, 2], &[solve(1, MONDAY, 50_000)], MONDAY);
        assert_eq!(champions(&one), Vec::<i64>::new());
        let two = standings(
            &[1, 2, 3],
            &[solve(1, MONDAY, 50_000), solve(2, MONDAY, 60_000)],
            MONDAY,
        );
        assert_eq!(champions(&two), [1]);
        let fastest = Award {
            winners: vec![2],
            feat: Feat::Fastest {
                solves: 1,
                total_ms: 60_000,
            },
        };
        assert_eq!(
            honours(1, &two, std::slice::from_ref(&fastest)),
            (false, false)
        );
        let three = standings(
            &[1, 2, 3],
            &[
                solve(1, MONDAY, 50_000),
                solve(2, MONDAY, 60_000),
                solve(3, MONDAY, 70_000),
            ],
            MONDAY,
        );
        assert_eq!(
            honours(1, &three, std::slice::from_ref(&fastest)),
            (true, false)
        );
        assert_eq!(
            honours(2, &three, std::slice::from_ref(&fastest)),
            (false, true)
        );
        assert_eq!(honours(3, &three, &[fastest]), (false, false));
    }

    /// The fastest is the lowest total of those who played the most days: playing more beats a
    /// quicker but shorter week, like the yellow jersey.
    #[test]
    fn fastest_by_the_total_of_the_most_days() {
        let solves = [
            solve(1, MONDAY, 100_000),
            solve(1, MONDAY + 1, 100_000),
            solve(2, MONDAY, 10_000),
            solve(3, MONDAY, 90_000),
            solve(3, MONDAY + 1, 90_000),
        ];
        let a = awards(&[1, 2, 3], &solves, &flat_medians(100_000), MONDAY);
        assert_eq!(
            award(&a, AwardKind::Fastest),
            Some(&Award {
                winners: vec![3],
                feat: Feat::Fastest {
                    solves: 2,
                    total_ms: 180_000
                }
            })
        );
        // Equal totals share it
        let solves = [solve(1, MONDAY, 100_000), solve(2, MONDAY, 100_000)];
        let a = awards(&[1, 2], &solves, &flat_medians(100_000), MONDAY);
        assert_eq!(award(&a, AwardKind::Fastest).unwrap().winners, [1, 2]);
    }

    #[test]
    fn cleanest_by_dailies_without_help() {
        let solves = [
            clean(1, MONDAY, 100_000),
            clean(1, MONDAY + 1, 100_000),
            solve(1, MONDAY + 2, 100_000),
            clean(2, MONDAY, 100_000),
            clean(2, MONDAY + 1, 100_000),
            clean(3, MONDAY, 100_000),
            solve(4, MONDAY, 100_000),
        ];
        let a = awards(&[1, 2, 3, 4], &solves, &flat_medians(100_000), MONDAY);
        // Two clean each, but 2 needed no help at all
        assert_eq!(
            award(&a, AwardKind::Cleanest),
            Some(&Award {
                winners: vec![2],
                feat: Feat::Cleanest {
                    clean: 2,
                    solves: 2
                }
            })
        );
        // Nobody clean: no award
        let a = awards(&[4], &[solve(4, MONDAY, 1)], &flat_medians(100_000), MONDAY);
        assert_eq!(award(&a, AwardKind::Cleanest), None);
    }

    /// The steadiest compares paces (against each day's median), so a slow oni and a quick
    /// Monday can be just as steady.
    #[test]
    fn steadiest_needs_three_dailies_and_compares_paces() {
        let mut medians = flat_medians(100_000);
        // Sunday's oni takes everyone five times as long
        medians.insert(MONDAY + 6, 500_000);
        let solves = [
            // Paces 1.00, 1.02, 1.04: within 4 %
            solve(1, MONDAY, 100_000),
            solve(1, MONDAY + 1, 102_000),
            solve(1, MONDAY + 6, 520_000),
            // Paces 0.5, 0.6, 0.55: within 20 %, quicker but less steady
            solve(2, MONDAY, 50_000),
            solve(2, MONDAY + 1, 60_000),
            solve(2, MONDAY + 2, 55_000),
            // Two dailies only, perfectly steady
            solve(3, MONDAY, 70_000),
            solve(3, MONDAY + 1, 70_000),
        ];
        let a = awards(&[1, 2, 3], &solves, &medians, MONDAY);
        assert_eq!(
            award(&a, AwardKind::Steadiest),
            Some(&Award {
                winners: vec![1],
                feat: Feat::Steadiest {
                    solves: 3,
                    spread: 4
                }
            })
        );
        // Too few dailies: nobody
        let a = awards(&[3], &solves, &medians, MONDAY);
        assert_eq!(award(&a, AwardKind::Steadiest), None);
        // Equally steady: the one with more dailies
        let more = [
            solve(4, MONDAY, 100_000),
            solve(4, MONDAY + 1, 104_000),
            solve(4, MONDAY + 2, 102_000),
            solve(4, MONDAY + 3, 100_000),
        ];
        let all = [solves.to_vec(), more.to_vec()].concat();
        let a = awards(&[1, 4], &all, &medians, MONDAY);
        let steadiest = award(&a, AwardKind::Steadiest).unwrap();
        assert_eq!(steadiest.winners, [4]);
    }

    #[test]
    fn the_early_bird_is_usually_done_first() {
        let solves = [
            at(1, MONDAY, 100_000, 7),
            at(1, MONDAY + 1, 100_000, 6),
            at(1, MONDAY + 2, 100_000, 22),
            at(2, MONDAY, 100_000, 8),
            at(2, MONDAY + 1, 100_000, 8),
            at(2, MONDAY + 2, 100_000, 8),
            // Up the earliest, but only twice
            at(3, MONDAY, 100_000, 1),
            at(3, MONDAY + 1, 100_000, 1),
        ];
        let a = awards(&[1, 2, 3], &solves, &flat_medians(100_000), MONDAY);
        // The usual time is the median: one late night doesn't spoil it
        assert_eq!(
            award(&a, AwardKind::EarlyBird),
            Some(&Award {
                winners: vec![1],
                feat: Feat::EarlyBird {
                    usual_ms: 7 * 3_600_000
                }
            })
        );
        // Rounded down to five minutes
        let odd = [
            WeekSolve {
                finished_at: MONDAY * MS_PER_DAY + 7 * 3_600_000 + 299_999,
                ..solve(4, MONDAY, 1)
            },
            WeekSolve {
                finished_at: (MONDAY + 1) * MS_PER_DAY + 7 * 3_600_000 + 299_999,
                ..solve(4, MONDAY + 1, 1)
            },
            WeekSolve {
                finished_at: (MONDAY + 2) * MS_PER_DAY + 7 * 3_600_000 + 299_999,
                ..solve(4, MONDAY + 2, 1)
            },
        ];
        let a = awards(&[4], &odd, &flat_medians(100_000), MONDAY);
        assert_eq!(
            award(&a, AwardKind::EarlyBird).unwrap().feat,
            Feat::EarlyBird {
                usual_ms: 7 * 3_600_000
            }
        );
    }

    #[test]
    fn most_improved_against_the_four_weeks_before() {
        let medians = flat_medians(100_000);
        let mut solves = Vec::new();
        // 1 used to take 120 % of the median, now 90 %: 25 % quicker
        for d in [MONDAY - 20, MONDAY - 13, MONDAY - 6] {
            solves.push(solve(1, d, 120_000));
        }
        solves.extend([solve(1, MONDAY, 90_000), solve(1, MONDAY + 1, 90_000)]);
        // 2 went from 100 % to 95 %: 5 % quicker
        for d in [MONDAY - 27, MONDAY - 26, MONDAY - 1] {
            solves.push(solve(2, d, 100_000));
        }
        solves.extend([solve(2, MONDAY, 95_000), solve(2, MONDAY + 1, 95_000)]);
        // 3 got slower
        for d in [MONDAY - 3, MONDAY - 2, MONDAY - 1] {
            solves.push(solve(3, d, 50_000));
        }
        solves.extend([solve(3, MONDAY, 60_000), solve(3, MONDAY + 1, 60_000)]);
        // 4 improved most, but has only two dailies before (one more was too long ago)
        for d in [MONDAY - 29, MONDAY - 2, MONDAY - 1] {
            solves.push(solve(4, d, 300_000));
        }
        solves.extend([solve(4, MONDAY, 50_000), solve(4, MONDAY + 1, 50_000)]);
        let a = awards(&[1, 2, 3, 4], &solves, &medians, MONDAY);
        assert_eq!(
            award(&a, AwardKind::Improved),
            Some(&Award {
                winners: vec![1],
                feat: Feat::Improved { percent: 25 }
            })
        );
        // Nobody improved: no award
        let a = awards(&[3], &solves, &medians, MONDAY);
        assert_eq!(award(&a, AwardKind::Improved), None);
        // One daily this week isn't enough
        let one: Vec<WeekSolve> = solves
            .iter()
            .copied()
            .filter(|s| s.user == 1 && s.day != MONDAY + 1)
            .collect();
        assert_eq!(
            award(&awards(&[1], &one, &medians, MONDAY), AwardKind::Improved),
            None
        );
    }

    #[test]
    fn the_lanterne_rouge_played_every_day() {
        let medians = flat_medians(100_000);
        let week = |user, time| (MONDAY..MONDAY + 7).map(move |d| solve(user, d, time));
        let mut solves: Vec<WeekSolve> = week(1, 60_000).chain(week(2, 90_000)).collect();
        // 3 is slower still, but missed a day
        solves.extend(week(3, 200_000).skip(1));
        let a = awards(&[1, 2, 3], &solves, &medians, MONDAY);
        assert_eq!(
            award(&a, AwardKind::Lantern),
            Some(&Award {
                winners: vec![2],
                feat: Feat::Lantern {
                    total_ms: 630_000,
                    of: 2
                }
            })
        );
        // Alone with every day: first and last, no lantern
        let alone: Vec<WeekSolve> = week(1, 60_000).collect();
        assert_eq!(
            award(&awards(&[1], &alone, &medians, MONDAY), AwardKind::Lantern),
            None
        );
        // Everyone equal: nobody is last
        let even: Vec<WeekSolve> = week(1, 60_000).chain(week(2, 60_000)).collect();
        assert_eq!(
            award(
                &awards(&[1, 2], &even, &medians, MONDAY),
                AwardKind::Lantern
            ),
            None
        );
    }

    #[test]
    fn paces_need_a_median() {
        let s = solve(1, MONDAY, 80_000);
        assert_eq!(pace(&s, &flat_medians(100_000)), Some(800));
        assert_eq!(pace(&s, &HashMap::new()), None);
        assert_eq!(pace(&s, &HashMap::from([(MONDAY, 0)])), None);
        assert_eq!(median(vec![3, 1, 2, 4]), Some(2));
        assert_eq!(median(vec![]), None);
    }

    #[test]
    fn a_week_is_over_after_its_sunday() {
        let banzuke = |today| Banzuke {
            monday: MONDAY,
            today,
            first: MONDAY,
            today_hidden: false,
            rows: Vec::new(),
            awards: Vec::new(),
            champions: Vec::new(),
        };
        assert!(!banzuke(MONDAY + 6).over());
        assert!(banzuke(MONDAY + 7).over());
        assert_eq!(banzuke(MONDAY).award(AwardKind::Fastest), None);
        let kanji: std::collections::HashSet<_> =
            AwardKind::ALL.iter().map(|k| k.kanji()).collect();
        assert_eq!(kanji.len(), AwardKind::ALL.len());
    }
}
