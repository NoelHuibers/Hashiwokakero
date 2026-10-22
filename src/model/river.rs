//! The river 川: a group's shared goal, next to the competition of the boards. Every bridge a
//! member builds in a solved daily is a plank of a bridge the whole group builds across a river.
//! Once it reaches the other side the group completes the level, and a wider water appears:
//! stream, river, wide river, strait, and then the ocean, again and again.
//!
//! How long a level takes scales with the group (see [`level_target`]), so a pair of friends and
//! a whole office both get across now and then. The server keeps the completed levels
//! (`server::groups::river`), the pages draw the scene (`pages::groups::river`).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// About how many bridges a solved daily has: a Monday's easy one some 15, a Sunday's oni some
/// 220, most players don't solve every one of the hard ones. What a level takes is counted in
/// these.
pub const BRIDGES_PER_DAILY: i64 = 60;

/// Days of dailies a level takes a group whose crew ([`crew_quarters`]) all solve every one:
/// a few days for the stream, then a week, two, three and four. Every ocean after the first one
/// takes as long as it.
pub const LEVEL_DAYS: [i64; 5] = [3, 7, 14, 21, 28];

/// Targets are rounded up to this, a number that reads well under the scene.
pub const TARGET_STEP: i64 = 50;

/// The waters to cross, one per level, the ocean from the fifth on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Waters {
    Stream,
    River,
    WideRiver,
    Strait,
    Ocean,
}

impl Waters {
    pub const ALL: [Self; 5] = [
        Self::Stream,
        Self::River,
        Self::WideRiver,
        Self::Strait,
        Self::Ocean,
    ];

    /// The waters of a level (0 is the first).
    pub fn of(level: u32) -> Self {
        Self::ALL[usize::try_from(level)
            .unwrap_or(usize::MAX)
            .min(Self::ALL.len() - 1)]
    }

    /// The kanji on the level's stamp: 沢 a mountain stream, 川 a river, 河 a wide river, 峡 a
    /// strait (海峡), 海 the sea.
    pub const fn kanji(self) -> &'static str {
        match self {
            Self::Stream => "沢",
            Self::River => "川",
            Self::WideRiver => "河",
            Self::Strait => "峡",
            Self::Ocean => "海",
        }
    }

    /// For the browser tests and the CSS.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Stream => "stream",
            Self::River => "river",
            Self::WideRiver => "wide-river",
            Self::Strait => "strait",
            Self::Ocean => "ocean",
        }
    }
}

/// Which ocean a level is, 1 for the first: the levels after the strait are all oceans.
pub fn ocean_number(level: u32) -> u32 {
    let first = u32::try_from(Waters::ALL.len() - 1).unwrap_or(4);
    level.saturating_sub(first) + 1
}

/// How many members a group's river counts, in quarters of a member.
///
/// The first four count fully, the fifth to twelfth half, every one after a quarter; a group
/// counts as two at least (quarters keep the arithmetic in whole numbers).
///
/// Big groups have more members who play now and then, so a level doesn't take them longer
/// than a small group, and a player alone still crosses a stream in a week.
pub fn crew_quarters(members: usize) -> i64 {
    let members = i64::try_from(members).unwrap_or(i64::MAX / 8);
    let full = members.min(4);
    let half = (members - 4).clamp(0, 8);
    let quarter = (members - 12).max(0);
    (4 * full + 2 * half + quarter).max(8)
}

/// The bridges a level takes a group of `members`:
///
/// `BRIDGES_PER_DAILY × LEVEL_DAYS[level] × crew`, with the crew from [`crew_quarters`], rounded
/// up to [`TARGET_STEP`]. Six members (a crew of five) cross the stream in 900 bridges and the
/// first ocean in 8,400; a pair (a crew of two) in 400 and 3,400.
pub fn level_target(level: u32, members: usize) -> i64 {
    let days = LEVEL_DAYS[usize::try_from(level)
        .unwrap_or(usize::MAX)
        .min(LEVEL_DAYS.len() - 1)];
    let quarters = BRIDGES_PER_DAILY * days * crew_quarters(members);
    // Quarters to bridges, rounded up to the step
    let step = 4 * TARGET_STEP;
    (quarters + step - 1) / step * TARGET_STEP
}

/// A level the group completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiverLevel {
    /// 0 for the first.
    pub level: u32,
    /// The bridges it took.
    pub target: i64,
    /// When the last plank was laid (ms).
    pub completed_at: i64,
}

/// What a member laid this week.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contribution {
    pub user_id: i64,
    pub bridges: i64,
}

/// A group's river, as a member sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupRiver {
    pub group_id: i64,
    /// The level being built, the number of levels completed.
    pub level: u32,
    /// The bridges the completed levels took.
    pub base: i64,
    /// Bridges laid of the current level, less than its target.
    pub progress: i64,
    /// What the current level takes.
    pub target: i64,
    /// Every bridge of the river, members who left included.
    pub total: i64,
    /// Oldest first.
    pub completed: Vec<RiverLevel>,
    /// Who laid the most this week (the UTC week, from Monday), most first, nobody without one.
    pub week: Vec<Contribution>,
    /// The river's total when the viewer saw it last: the planks after it are new to them.
    pub seen: i64,
    /// A level completed since the viewer saw the river last, to celebrate with them (the
    /// newest, if it was more than one). Only levels completed after they joined.
    pub celebrate: Option<RiverLevel>,
}

impl GroupRiver {
    /// Of the current level's bridges, how many the viewer has seen.
    pub fn seen_progress(&self) -> i64 {
        (self.seen - self.base).clamp(0, self.progress)
    }
}

/// How many of a level's `planks` lie for `progress` of `target` bridges: whole planks, the last
/// one only once the level is complete.
pub fn planks_laid(progress: i64, target: i64, planks: usize) -> usize {
    if target <= 0 || progress <= 0 {
        return 0;
    }
    let planks_i = i64::try_from(planks).unwrap_or(i64::MAX);
    let laid = (progress.min(target).saturating_mul(planks_i) / target).clamp(0, planks_i);
    usize::try_from(laid).unwrap_or(planks)
}

/// The members by what they laid, most first, then by id; nobody without a bridge.
pub fn ranking(mut laid: Vec<Contribution>) -> Vec<Contribution> {
    laid.retain(|c| c.bridges > 0);
    laid.sort_by(|a, b| b.bridges.cmp(&a.bridges).then(a.user_id.cmp(&b.user_id)));
    laid
}

/// The bridges of a solved daily of a member, as the river counts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Laid {
    /// When the daily was finished (ms).
    pub at: i64,
    pub user_id: i64,
    pub bridges: i64,
}

/// A level completed, found by [`crossings`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crossing {
    pub level: u32,
    pub target: i64,
    pub at: i64,
    /// Who laid planks of it, by id.
    pub crew: Vec<i64>,
}

/// The levels completed from level `level` on, whose bridges start after `base`.
///
/// `before` is the river's total before the first of `laid` (bridges of members who left
/// included, they have no time), `laid` the dailies after that in the order they were finished.
/// A level is complete at the daily that brings it to its target, with everyone who laid planks
/// of it since the level before as its crew; one daily can complete more than one. A river that
/// is past its target before the first of `laid` (when a member left and the targets got
/// smaller) completes at the next daily, or `now` without one.
pub fn crossings(
    level: u32,
    base: i64,
    before: i64,
    laid: &[Laid],
    members: usize,
    now: i64,
) -> Vec<Crossing> {
    let mut out = Vec::new();
    let mut level = level;
    let mut base = base;
    let mut total = before;
    let mut crew = BTreeSet::new();
    let mut complete = |total: i64, at: i64, last: Option<i64>, crew: &mut BTreeSet<i64>| {
        let mut target = level_target(level, members);
        while total - base >= target {
            out.push(Crossing {
                level,
                target,
                at,
                crew: crew.iter().copied().collect(),
            });
            base += target;
            level += 1;
            target = level_target(level, members);
            crew.clear();
            // The daily's bridges beyond the target are planks of the next level
            if let Some(user) = last
                && total > base
            {
                crew.insert(user);
            }
        }
    };
    for l in laid.iter().filter(|l| l.bridges > 0) {
        total += l.bridges;
        crew.insert(l.user_id);
        complete(total, l.at, Some(l.user_id), &mut crew);
    }
    complete(total, now, None, &mut crew);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_groups_count_as_two_and_big_ones_less_per_member() {
        let crews: Vec<_> = [0, 1, 2, 3, 4, 5, 6, 8, 12, 13, 20, 50]
            .iter()
            .map(|&m| crew_quarters(m))
            .collect();
        assert_eq!(crews, [8, 8, 8, 12, 16, 18, 20, 24, 32, 33, 40, 70]);
        // Never less for more members
        for m in 1..60 {
            assert!(crew_quarters(m + 1) >= crew_quarters(m), "{m}");
        }
    }

    /// The numbers the docs promise, and every level takes longer than the one before.
    #[test]
    fn levels_scale_with_the_group() {
        assert_eq!(level_target(0, 6), 900);
        assert_eq!(level_target(4, 6), 8_400);
        assert_eq!(level_target(0, 2), 400);
        assert_eq!(level_target(4, 2), 3_400);
        // Alone, like a pair
        assert_eq!(level_target(0, 1), level_target(0, 2));
        for members in [1, 2, 6, 12, 50] {
            let targets: Vec<_> = (0..7).map(|l| level_target(l, members)).collect();
            assert!(
                targets.windows(2).take(4).all(|w| w[0] < w[1]),
                "{targets:?}"
            );
            // The oceans after the first one take as long as it
            assert_eq!(targets[4], targets[6]);
            assert!(targets.iter().all(|t| t % TARGET_STEP == 0), "{targets:?}");
        }
        // Fifty members don't take 25 times as long as two, nor as long as fifty would alone
        assert!(level_target(0, 50) < 10 * level_target(0, 2));
        assert!(level_target(0, 50) > level_target(0, 12));
    }

    /// A crew that solves every daily gets across in about the days of its level.
    #[test]
    fn a_level_takes_about_its_days() {
        for (level, &days) in LEVEL_DAYS.iter().enumerate() {
            let level = u32::try_from(level).unwrap();
            for members in [2, 4] {
                let a_day = BRIDGES_PER_DAILY * i64::try_from(members).unwrap();
                let target = level_target(level, members);
                // Rounded up to the step: a day more at most
                let takes = (target + a_day - 1) / a_day;
                assert!(
                    (days..=days + 1).contains(&takes),
                    "{level}, {members}: {target}"
                );
            }
        }
    }

    #[test]
    fn levels_after_the_strait_are_oceans() {
        let waters: Vec<_> = (0..7).map(Waters::of).collect();
        assert_eq!(
            waters,
            [
                Waters::Stream,
                Waters::River,
                Waters::WideRiver,
                Waters::Strait,
                Waters::Ocean,
                Waters::Ocean,
                Waters::Ocean
            ]
        );
        assert_eq!(Waters::of(u32::MAX), Waters::Ocean);
        assert_eq!([4, 5, 6].map(ocean_number), [1, 2, 3]);
        let kanji: BTreeSet<_> = Waters::ALL.iter().map(|w| w.kanji()).collect();
        assert_eq!(kanji.len(), Waters::ALL.len());
    }

    #[test]
    fn planks_are_laid_whole() {
        assert_eq!(planks_laid(0, 900, 30), 0);
        assert_eq!(planks_laid(29, 900, 30), 0);
        assert_eq!(planks_laid(30, 900, 30), 1);
        assert_eq!(planks_laid(899, 900, 30), 29);
        assert_eq!(planks_laid(900, 900, 30), 30);
        assert_eq!(planks_laid(5_000, 900, 30), 30);
        assert_eq!(planks_laid(-3, 900, 30), 0);
        assert_eq!(planks_laid(10, 0, 30), 0);
    }

    #[test]
    fn the_most_bridges_lead() {
        let c = |user_id, bridges| Contribution { user_id, bridges };
        assert_eq!(
            ranking(vec![c(3, 40), c(1, 0), c(2, 90), c(5, 40), c(4, 7)]),
            [c(2, 90), c(3, 40), c(5, 40), c(4, 7)]
        );
        assert_eq!(ranking(vec![]), []);
    }

    fn laid(at: i64, user_id: i64, bridges: i64) -> Laid {
        Laid {
            at,
            user_id,
            bridges,
        }
    }

    /// A level completes at the daily that brings it to its target, once.
    #[test]
    fn a_level_completes_at_the_plank_that_reaches_the_other_side() {
        // Two members: the stream takes 400
        let target = level_target(0, 2);
        assert_eq!(target, 400);
        let dailies = [
            laid(10, 1, 150),
            laid(20, 2, 0),
            laid(30, 2, 150),
            laid(40, 1, 120),
            laid(50, 2, 60),
        ];
        let found = crossings(0, 0, 0, &dailies, 2, 99);
        assert_eq!(
            found,
            [Crossing {
                level: 0,
                target: 400,
                at: 40,
                crew: vec![1, 2],
            }]
        );
        // Not enough yet: nothing
        assert_eq!(crossings(0, 0, 0, &dailies[..3], 2, 99), []);
        // Seen again from the completed level on, it's not complete again
        assert_eq!(crossings(1, 400, 420, &dailies[4..], 2, 99), []);
    }

    /// One big day can complete two levels, the planks beyond the first count for the second.
    #[test]
    fn one_daily_can_complete_more_than_one_level() {
        // A pair: 400 for the stream, then 850 for the river
        assert_eq!(level_target(1, 2), 850);
        let found = crossings(0, 0, 350, &[laid(5, 7, 1_000)], 2, 99);
        assert_eq!(
            found,
            [
                Crossing {
                    level: 0,
                    target: 400,
                    at: 5,
                    crew: vec![7],
                },
                Crossing {
                    level: 1,
                    target: 850,
                    at: 5,
                    crew: vec![7],
                },
            ]
        );
        // Exactly the target: the next level has no planks of it yet
        let found = crossings(0, 0, 0, &[laid(5, 7, 400), laid(6, 8, 10)], 2, 99);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].crew, [7]);
    }

    /// Bridges of members who left have no time: a river past its target without a daily
    /// completes at the next daily, or now.
    #[test]
    fn a_river_already_across_completes_with_the_next_daily_or_now() {
        let found = crossings(0, 0, 450, &[], 2, 99);
        assert_eq!(
            found,
            [Crossing {
                level: 0,
                target: 400,
                at: 99,
                crew: vec![],
            }]
        );
        let found = crossings(0, 0, 450, &[laid(5, 3, 2)], 2, 99);
        assert_eq!((found[0].at, found[0].crew.clone()), (5, vec![3]));
    }

    #[test]
    fn the_viewer_sees_new_planks_of_the_current_level_only() {
        let river = GroupRiver {
            group_id: 1,
            level: 1,
            base: 400,
            progress: 300,
            target: 900,
            total: 700,
            completed: vec![],
            week: vec![],
            seen: 500,
            celebrate: None,
        };
        assert_eq!(river.seen_progress(), 100);
        // Last seen before the level started: all of it is new
        assert_eq!(
            GroupRiver {
                seen: 120,
                ..river.clone()
            }
            .seen_progress(),
            0
        );
        // Seen more than there is now (a result taken back): nothing new
        assert_eq!(GroupRiver { seen: 900, ..river }.seen_progress(), 300);
    }
}
