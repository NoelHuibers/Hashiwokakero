//! Hanko 判子: the seals members of a group press on each other's daily results, a reaction
//! without words (so nothing to moderate). A fixed set of six, each once per member and result,
//! pressed again to take it back. Never on one's own result, and only once the viewer finished
//! that daily too: before, the board doesn't even show the others' results.
//!
//! The seals belong to the group they were pressed in: a member of two groups gets the ones of
//! each group on that group's board only.

use serde::{Deserialize, Serialize};

use super::{TodayEntry, TodayState};

/// Days a result can get seals: its own and the next one, so a board opened just before
/// midnight (UTC) still takes them.
pub const HANKO_DAYS: i64 = 2;

/// The seals there are, in the order the picker shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HankoKind {
    /// 速 fast.
    Fast,
    /// 凄 amazing.
    Wow,
    /// 惜 so close.
    Close,
    /// 笑 funny.
    Funny,
    /// 粘 perseverance.
    Grit,
    /// 祝 congratulations.
    Cheers,
}

impl HankoKind {
    pub const ALL: [Self; 6] = [
        Self::Fast,
        Self::Wow,
        Self::Close,
        Self::Funny,
        Self::Grit,
        Self::Cheers,
    ];

    /// As stored in the database, never change one.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Wow => "wow",
            Self::Close => "close",
            Self::Funny => "funny",
            Self::Grit => "grit",
            Self::Cheers => "cheers",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }

    /// The character carved into the seal.
    pub const fn kanji(self) -> &'static str {
        match self {
            Self::Fast => "速",
            Self::Wow => "凄",
            Self::Close => "惜",
            Self::Funny => "笑",
            Self::Grit => "粘",
            Self::Cheers => "祝",
        }
    }
}

/// A seal on a result of a group's board: `giver` pressed `kind` on the result of `target`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HankoMark {
    pub target: i64,
    pub giver: i64,
    pub kind: HankoKind,
}

/// The seals on `target`'s result, kind by kind in the picker's order, with who pressed each (in
/// the order they did). Kinds nobody pressed are left out.
pub fn tally(marks: &[HankoMark], target: i64) -> Vec<(HankoKind, Vec<i64>)> {
    HankoKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let givers: Vec<i64> = marks
                .iter()
                .filter(|m| m.target == target && m.kind == kind)
                .map(|m| m.giver)
                .collect();
            (!givers.is_empty()).then_some((kind, givers))
        })
        .collect()
}

/// Whether `viewer` may press seals on `entry` of a board: it's open (the viewer finished the
/// daily), the entry is a finished result (solved or given up), and not the viewer's own.
pub fn can_stamp(open: bool, viewer: i64, entry: &TodayEntry) -> bool {
    open && entry.user_id != viewer
        && matches!(
            entry.state,
            TodayState::Ranked { .. } | TodayState::Unranked
        )
}

/// `marks` with one seal of `viewer` pressed (`on`) or taken back: what the board shows at once,
/// before the server answered.
pub fn toggled(
    mut marks: Vec<HankoMark>,
    viewer: i64,
    target: i64,
    kind: HankoKind,
    on: bool,
) -> Vec<HankoMark> {
    let mark = HankoMark {
        target,
        giver: viewer,
        kind,
    };
    marks.retain(|m| *m != mark);
    if on {
        marks.push(mark);
    }
    marks
}

/// A small tilt for a seal, in degrees (−7 to 7): pressed by hand, never quite straight. The
/// same for the same seal on every render, so it doesn't wobble when the board reloads.
pub fn tilt(target: i64, kind: HankoKind) -> i64 {
    let index = HankoKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
    let seed = target
        .wrapping_mul(2_654_435_761)
        .wrapping_add(i64::try_from(index).unwrap_or(0).wrapping_mul(40_503));
    seed.rem_euclid(15) - 7
}

/// A seal the viewer got on their result: "Keiko pressed 凄 on your result".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HankoReceived {
    pub group_id: i64,
    pub group_name: String,
    /// The day of the daily the result is of.
    pub day: i64,
    pub giver: i64,
    pub giver_name: String,
    /// The giver's colour in that group.
    pub giver_colour: u8,
    pub kind: HankoKind,
    /// When it was pressed, ms.
    pub at: i64,
    /// Not seen before: it was new when it was loaded.
    pub fresh: bool,
}

/// Seals not seen yet on one's results in a group, for the dot on the groups' navigation entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnreadHanko {
    pub group_id: i64,
    pub count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(user_id: i64, state: TodayState) -> TodayEntry {
        TodayEntry {
            user_id,
            name: format!("p{user_id}"),
            guest: false,
            colour: 0,
            stamp: None,
            state,
        }
    }

    const RANKED: TodayState = TodayState::Ranked {
        rank: 1,
        time_ms: 60_000,
        hints: 0,
        checks: 0,
    };

    #[test]
    fn ids_round_trip() {
        for kind in HankoKind::ALL {
            assert_eq!(HankoKind::from_id(kind.id()), Some(kind));
        }
        assert_eq!(HankoKind::from_id("chat"), None);
        let kanji: std::collections::HashSet<_> = HankoKind::ALL.map(HankoKind::kanji).into();
        assert_eq!(kanji.len(), HankoKind::ALL.len());
    }

    #[test]
    fn only_others_finished_results_once_open() {
        // Solved and given up both take seals
        assert!(can_stamp(true, 1, &entry(2, RANKED)));
        assert!(can_stamp(true, 1, &entry(2, TodayState::Unranked)));
        // Never one's own
        assert!(!can_stamp(true, 1, &entry(1, RANKED)));
        // Nothing to stamp yet
        assert!(!can_stamp(true, 1, &entry(2, TodayState::Waiting)));
        // Before the viewer finished: the board is closed, results are only "done"
        assert!(!can_stamp(false, 1, &entry(2, RANKED)));
        assert!(!can_stamp(false, 1, &entry(2, TodayState::Done)));
    }

    #[test]
    fn tally_groups_by_kind_in_picker_order() {
        let mark = |target, giver, kind| HankoMark {
            target,
            giver,
            kind,
        };
        let marks = [
            mark(2, 3, HankoKind::Cheers),
            mark(2, 1, HankoKind::Wow),
            mark(2, 3, HankoKind::Wow),
            mark(4, 1, HankoKind::Fast),
        ];
        assert_eq!(
            tally(&marks, 2),
            vec![(HankoKind::Wow, vec![1, 3]), (HankoKind::Cheers, vec![3])]
        );
        assert_eq!(tally(&marks, 4), vec![(HankoKind::Fast, vec![1])]);
        assert_eq!(tally(&marks, 5), []);
    }

    #[test]
    fn toggling_presses_once_and_takes_back() {
        let on = toggled(Vec::new(), 1, 2, HankoKind::Close, true);
        assert_eq!(on.len(), 1);
        // Pressing again doesn't press twice
        let again = toggled(on.clone(), 1, 2, HankoKind::Close, true);
        assert_eq!(again, on);
        // Another kind is another seal
        assert_eq!(toggled(on.clone(), 1, 2, HankoKind::Grit, true).len(), 2);
        assert_eq!(toggled(on, 1, 2, HankoKind::Close, false), []);
    }

    #[test]
    fn tilts_are_small_and_steady() {
        for target in 0..50 {
            for kind in HankoKind::ALL {
                let t = tilt(target, kind);
                assert!((-7..=7).contains(&t));
                assert_eq!(t, tilt(target, kind));
            }
        }
        // Not all the same
        let tilts: std::collections::HashSet<_> =
            (0..20).map(|t| tilt(t, HankoKind::Wow)).collect();
        assert!(tilts.len() > 3);
    }
}
