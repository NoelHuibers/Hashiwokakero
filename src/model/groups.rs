//! Groups (仲間 nakama): friends and colleagues comparing their dailies. What the group page
//! shows, the member colours and the archipelago's layout, shared by the server and the pages.
//!
//! Later features of a group (replays, reactions, a weekly banzuke, ...) build on these: the
//! page data is [`GroupPage`], a member's colour is [`GroupMember::colour`] (draw it with
//! [`colour_var`]), and the server checks membership with `server::groups::require_member`.

use serde::{Deserialize, Serialize};

use super::stamps::Stamp;
use super::{MS_PER_DAY, weekday};

/// Members a group can have at most.
pub const MAX_MEMBERS: usize = 50;
/// Groups a player can be in at most.
pub const MAX_GROUPS: usize = 10;
/// Characters a group name can have at most (after trimming).
pub const GROUP_NAME_MAX: usize = 32;
/// The days the archipelago's bridges count (today and the ones before it).
pub const BRIDGE_DAYS: i64 = 28;
/// Days both solved for a double bridge between two members, one is enough for a single one.
pub const DOUBLE_BRIDGE_DAYS: i64 = 7;
/// Characters of an invite code.
pub const INVITE_LEN: usize = 10;
/// What invite codes are made of: lower case letters and digits that can't be mistaken for one
/// another (no 0/o, 1/l/i), 32 of them, so a code has 50 random bits.
pub const INVITE_ALPHABET: &[u8; 32] = b"abcdefghjkmnpqrstuvwxyz23456789_";

/// The palette member colours come from, as many as there are `--nakama-<n>` variables in
/// tailwind.css (each in a day and a night shade).
pub const MEMBER_COLOURS: u8 = 12;

/// The traditional name of each member colour, a kanji: 朱 vermilion, 藍 indigo, ...
pub const COLOUR_KANJI: [&str; MEMBER_COLOURS as usize] = [
    "朱", "藍", "抹", "黄", "桜", "藤", "浅", "茶", "柿", "鼠", "苔", "梅",
];

/// The CSS colour of a member colour slot, follows the day and night theme.
pub fn colour_var(slot: u8) -> String {
    format!("var(--nakama-{})", slot % MEMBER_COLOURS)
}

/// The colour a new member gets: the first one nobody in the group has, or, once all are taken,
/// the first of the least used. Stored with the membership, so a member keeps theirs.
pub fn free_colour(taken: &[u8]) -> u8 {
    let mut uses = [0usize; MEMBER_COLOURS as usize];
    for &slot in taken {
        uses[usize::from(slot % MEMBER_COLOURS)] += 1;
    }
    let fewest = uses.iter().copied().min().unwrap_or(0);
    uses.iter()
        .position(|&n| n == fewest)
        .and_then(|i| u8::try_from(i).ok())
        .unwrap_or(0)
}

/// A group name as it is stored: trimmed, inner runs of white space as one space. `None` if it
/// is empty, too long or has control characters.
pub fn clean_group_name(name: &str) -> Option<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let length = name.chars().count();
    ((1..=GROUP_NAME_MAX).contains(&length) && !name.chars().any(char::is_control)).then_some(name)
}

/// Whether a string can be an invite code, before asking the database.
pub fn valid_invite(code: &str) -> bool {
    code.len() == INVITE_LEN && code.bytes().all(|b| INVITE_ALPHABET.contains(&b))
}

/// A group in the list of the player's groups.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSummary {
    pub id: i64,
    pub name: String,
    pub members: i64,
    /// Members who finished today's daily.
    pub done_today: i64,
    pub owner: bool,
    /// Islands of the first members, for a small picture.
    pub colours: Vec<u8>,
}

/// A member of a group, as the other members see them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMember {
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    /// Their colour slot in this group, see [`colour_var`].
    pub colour: u8,
    pub joined_at: i64,
    pub owner: bool,
    /// The stamp they show next to their name.
    pub stamp: Option<Stamp>,
    /// Finished today's daily (solved or given up). Everyone may know that, not how.
    pub done_today: bool,
}

/// How many days two members both solved recently, for the bridge between their islands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedDays {
    pub a: i64,
    pub b: i64,
    pub days: i64,
}

/// What a member did with today's daily, as far as the viewer may know it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodayState {
    /// Not finished yet (or not started).
    Waiting,
    /// Finished, but the viewer hasn't: no time, no place.
    Done,
    /// A ranked result, the place among the group's ranked results.
    Ranked {
        rank: i64,
        time_ms: i64,
        hints: i64,
        checks: i64,
    },
    /// Finished, not on the boards (gave up, or flagged): like the global board, no time.
    Unranked,
}

impl TodayState {
    pub fn done(self) -> bool {
        self != Self::Waiting
    }
}

/// One member's line on the group's board of today.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodayEntry {
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub colour: u8,
    pub stamp: Option<Stamp>,
    pub state: TodayState,
}

/// The group's board of today's daily. Spoiler safe: until the viewer finished it themselves,
/// it only says who is done ([`TodayState::Done`]), see [`spoiler_safe`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupToday {
    pub group_id: i64,
    pub group_name: String,
    pub day: i64,
    /// The viewer finished today's daily: the board has times.
    pub open: bool,
    pub entries: Vec<TodayEntry>,
    /// The hanko 判子 the members pressed on each other's results (`model::hanko`), only once
    /// the board is open.
    #[serde(default)]
    pub hanko: Vec<super::HankoMark>,
}

impl GroupToday {
    pub fn done(&self) -> usize {
        self.entries.iter().filter(|e| e.state.done()).count()
    }
}

/// The board as the viewer may see it.
///
/// Before they finished, results are only "done", in name order (the order of finishing would
/// tell who was quick); after, ranked results by place, then the unranked, then who's still to
/// play.
pub fn spoiler_safe(mut entries: Vec<TodayEntry>, open: bool) -> Vec<TodayEntry> {
    if !open {
        for e in &mut entries {
            if e.state.done() {
                e.state = TodayState::Done;
            }
        }
    }
    let key = |e: &TodayEntry| match e.state {
        TodayState::Ranked { rank, .. } => (0, rank),
        TodayState::Done | TodayState::Unranked => (1, 0),
        TodayState::Waiting => (2, 0),
    };
    entries.sort_by(|a, b| {
        key(a)
            .cmp(&key(b))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then(a.user_id.cmp(&b.user_id))
    });
    entries
}

/// Everything the group page shows when it opens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupPage {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    pub owner_id: i64,
    /// The viewer.
    pub me: i64,
    /// The invite code, for the link `/g/<code>`.
    pub invite: String,
    /// Longest standing first.
    pub members: Vec<GroupMember>,
    /// Pairs of members with at least one day both solved in the last [`BRIDGE_DAYS`].
    pub shared: Vec<SharedDays>,
    pub today: GroupToday,
}

impl GroupPage {
    pub fn is_owner(&self) -> bool {
        self.owner_id == self.me
    }

    pub fn member(&self, user_id: i64) -> Option<&GroupMember> {
        self.members.iter().find(|m| m.user_id == user_id)
    }
}

/// What the join page shows of a group before joining.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvitePreview {
    /// For the archipelago's layout, the same as on the group page.
    pub id: i64,
    pub name: String,
    pub members: Vec<GroupMember>,
    /// The visitor is a member already: the group's id, to go there.
    pub joined: Option<i64>,
    /// No room for another member.
    pub full: bool,
}

/// A member's row of the week grid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeekRow {
    pub user_id: i64,
    pub name: String,
    pub colour: u8,
    /// Monday to Sunday: the daily of the day solved.
    pub solved: [bool; 7],
}

/// Members × days of a UTC week, Monday to Sunday.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupWeek {
    /// The day number of the week's Monday.
    pub monday: i64,
    pub today: i64,
    /// The Monday of the first week there is something to see: the group's first.
    pub first: i64,
    pub rows: Vec<WeekRow>,
}

/// The Monday of the week `day` is in.
pub fn monday_of(day: i64) -> i64 {
    day - i64::try_from(weekday(day)).unwrap_or(0)
}

/// The week grid's rows from `(user, day)` pairs of solved dailies: the members in their order,
/// days outside the week ignored.
pub fn week_rows(members: &[GroupMember], solved: &[(i64, i64)], monday: i64) -> Vec<WeekRow> {
    members
        .iter()
        .map(|m| {
            let mut days = [false; 7];
            for &(user, day) in solved {
                if user == m.user_id
                    && let Ok(i) = usize::try_from(day - monday)
                    && i < 7
                {
                    days[i] = true;
                }
            }
            WeekRow {
                user_id: m.user_id,
                name: m.name.clone(),
                colour: m.colour,
                solved: days,
            }
        })
        .collect()
}

/// The day a group was made, for its first week.
pub fn group_day(created_at: i64) -> i64 {
    created_at.div_euclid(MS_PER_DAY)
}

/// Bridges between two members' islands: none, one (a day both solved) or two (a week of
/// them), like a Hashi bridge.
pub fn bridge_count(days: i64) -> u8 {
    match days {
        ..=0 => 0,
        d if d < DOUBLE_BRIDGE_DAYS => 1,
        _ => 2,
    }
}

/// Where a member's island lies in the archipelago, in cells of the pixel art (16 px).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spot {
    pub user_id: i64,
    pub x: i32,
    pub y: i32,
}

/// A possible bridge between two neighbouring islands, `a` left of or above `b`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Strait {
    pub a: Spot,
    pub b: Spot,
    pub horizontal: bool,
}

/// The archipelago: an island per member on a grid like a Hashi puzzle's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Archipelago {
    /// Size in cells.
    pub width: i32,
    pub height: i32,
    pub spots: Vec<Spot>,
}

/// Cells between the starts of two neighbouring islands' slots, across and down.
const SLOT: i32 = 3;

/// Islands per row: a short line for a few, a block for many.
fn columns(members: usize) -> usize {
    match members {
        0..=3 => members.max(1),
        // About as many rows as columns, at most eight across
        n => (1..=8).find(|c| c * c >= n).unwrap_or(8).clamp(3, 8),
    }
}

/// A number from a group and a member that stays the same: islands keep their place.
fn wobble(group: i64, user: i64) -> u64 {
    let mut h = (group.cast_unsigned() ^ 0x9e37_79b9_7f4a_7c15).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= user.cast_unsigned().wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    h.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 32
}

/// Lays out the islands of `members` (longest standing first) in rows.
///
/// Every island moves a cell sideways or not, decided by the group and member ids, so the
/// islands don't line up like a spreadsheet and still never move between visits; a row that
/// isn't full is centred.
pub fn archipelago(group: i64, members: &[i64]) -> Archipelago {
    let cols = columns(members.len());
    let width = i32::try_from(cols).unwrap_or(8) * SLOT;
    let mut spots = Vec::with_capacity(members.len());
    for (row, chunk) in members.chunks(cols).enumerate() {
        let missing = i32::try_from(cols - chunk.len()).unwrap_or(0);
        let indent = missing * SLOT / 2;
        let y = i32::try_from(row).unwrap_or(0) * SLOT;
        for (col, &user) in chunk.iter().enumerate() {
            let shift = i32::from(wobble(group, user) % 2 == 1);
            spots.push(Spot {
                user_id: user,
                x: indent + i32::try_from(col).unwrap_or(0) * SLOT + shift,
                y,
            });
        }
    }
    let rows = i32::try_from(members.len().div_ceil(cols)).unwrap_or(0);
    Archipelago {
        width,
        height: (rows * SLOT - 1).max(1),
        spots,
    }
}

impl Archipelago {
    /// Neighbours a bridge can join: the next island in the same row, and the island right
    /// below in the next row. Bridges between them never cross.
    pub fn straits(&self) -> Vec<Strait> {
        let mut out = Vec::new();
        for (i, &a) in self.spots.iter().enumerate() {
            if let Some(&b) = self.spots.get(i + 1).filter(|b| b.y == a.y) {
                out.push(Strait {
                    a,
                    b,
                    horizontal: true,
                });
            }
            if let Some(&b) = self.spots.iter().find(|b| b.y == a.y + SLOT && b.x == a.x) {
                out.push(Strait {
                    a,
                    b,
                    horizontal: false,
                });
            }
        }
        out
    }
}

/// The days two members both solved, from the shared days of the page.
pub fn shared_days(shared: &[SharedDays], a: i64, b: i64) -> i64 {
    shared
        .iter()
        .find(|s| (s.a, s.b) == (a, b) || (s.a, s.b) == (b, a))
        .map_or(0, |s| s.days)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn entry(user_id: i64, name: &str, state: TodayState) -> TodayEntry {
        TodayEntry {
            user_id,
            name: name.into(),
            guest: false,
            colour: 0,
            stamp: None,
            state,
        }
    }

    fn ranked(rank: i64, time_ms: i64) -> TodayState {
        TodayState::Ranked {
            rank,
            time_ms,
            hints: 0,
            checks: 0,
        }
    }

    #[test]
    fn new_members_get_a_colour_nobody_has() {
        assert_eq!(free_colour(&[]), 0);
        assert_eq!(free_colour(&[0, 1, 3]), 2);
        assert_eq!(free_colour(&[1, 2]), 0);
        // All taken: the first of the least used
        let all: Vec<u8> = (0..MEMBER_COLOURS).collect();
        assert_eq!(free_colour(&all), 0);
        let mut more = all;
        more.extend([0, 1]);
        assert_eq!(free_colour(&more), 2);
        assert_eq!(colour_var(3), "var(--nakama-3)");
        assert_eq!(colour_var(MEMBER_COLOURS + 1), "var(--nakama-1)");
        assert_eq!(COLOUR_KANJI.len(), usize::from(MEMBER_COLOURS));
    }

    #[test]
    fn group_names_are_trimmed_and_limited() {
        assert_eq!(
            clean_group_name("  Team   Hashi \n").as_deref(),
            Some("Team Hashi")
        );
        assert_eq!(clean_group_name("   "), None);
        assert_eq!(clean_group_name(""), None);
        let long = "x".repeat(GROUP_NAME_MAX);
        assert_eq!(clean_group_name(&long), Some(long.clone()));
        assert_eq!(clean_group_name(&format!("{long}y")), None);
        // Characters, not bytes
        let kanji = "橋".repeat(GROUP_NAME_MAX);
        assert_eq!(clean_group_name(&kanji), Some(kanji.clone()));
        assert_eq!(clean_group_name("a\u{0007}b"), None);
    }

    #[test]
    fn invite_codes_are_checked_before_the_database() {
        assert!(valid_invite("abcdefgh23"));
        assert!(!valid_invite("abcdefgh2"));
        assert!(!valid_invite("abcdefgh234"));
        // Lookalikes and upper case aren't in codes
        assert!(!valid_invite("abcdefgh20"));
        assert!(!valid_invite("ABCDEFGH23"));
        assert!(!valid_invite("abcdefgh/."));
    }

    /// Before finishing today's daily the board only says who is done, in name order.
    #[test]
    fn the_board_has_no_times_before_you_finished() {
        let entries = vec![
            entry(1, "zoe", ranked(1, 60_000)),
            entry(2, "Anna", TodayState::Waiting),
            entry(3, "bea", ranked(2, 90_000)),
            entry(4, "cem", TodayState::Unranked),
        ];
        let closed = spoiler_safe(entries.clone(), false);
        let seen: Vec<_> = closed.iter().map(|e| (e.user_id, e.state)).collect();
        assert_eq!(
            seen,
            [
                (3, TodayState::Done),
                (4, TodayState::Done),
                (1, TodayState::Done),
                (2, TodayState::Waiting)
            ]
        );
        let open = spoiler_safe(entries, true);
        let seen: Vec<_> = open.iter().map(|e| (e.user_id, e.state)).collect();
        assert_eq!(
            seen,
            [
                (1, ranked(1, 60_000)),
                (3, ranked(2, 90_000)),
                (4, TodayState::Unranked),
                (2, TodayState::Waiting)
            ]
        );
    }

    #[test]
    fn weeks_start_on_monday() {
        // 2026-10-19 is a Monday, day 20_745
        let monday = 20_745;
        for day in monday..monday + 7 {
            assert_eq!(monday_of(day), monday, "{day}");
        }
        assert_eq!(monday_of(monday - 1), monday - 7);
        assert_eq!(monday_of(monday + 7), monday + 7);
    }

    #[test]
    fn the_week_grid_has_a_row_per_member() {
        let monday = 20_745;
        let members = [member(5, "aiko"), member(2, "bram"), member(9, "chen")];
        let solved = [
            (5, monday),
            (5, monday + 6),
            // The week before and after, and someone who left
            (5, monday - 1),
            (2, monday + 7),
            (7, monday + 2),
            (2, monday + 3),
        ];
        let rows = week_rows(&members, &solved, monday);
        let seen: Vec<_> = rows.iter().map(|r| (r.user_id, r.solved)).collect();
        assert_eq!(
            seen,
            [
                (5, [true, false, false, false, false, false, true]),
                (2, [false, false, false, true, false, false, false]),
                (9, [false; 7]),
            ]
        );
    }

    #[test]
    fn more_days_make_thicker_bridges() {
        assert_eq!(bridge_count(0), 0);
        assert_eq!(bridge_count(1), 1);
        assert_eq!(bridge_count(DOUBLE_BRIDGE_DAYS - 1), 1);
        assert_eq!(bridge_count(DOUBLE_BRIDGE_DAYS), 2);
        assert_eq!(bridge_count(BRIDGE_DAYS), 2);
        let shared = [SharedDays {
            a: 1,
            b: 2,
            days: 4,
        }];
        assert_eq!(shared_days(&shared, 2, 1), 4);
        assert_eq!(shared_days(&shared, 1, 3), 0);
    }

    #[test]
    fn rows_grow_with_the_group() {
        let cols: Vec<_> = [1, 2, 3, 4, 7, 8, 12, 18, 19, 50]
            .iter()
            .map(|&n| columns(n))
            .collect();
        assert_eq!(cols, [1, 2, 3, 3, 3, 3, 4, 5, 5, 8]);
    }

    /// The same members lie in the same places on every visit, every island on its own cell,
    /// in rows of the group's width.
    #[test]
    fn islands_keep_their_places() {
        let members: Vec<i64> = (1..=11).map(|i| i * 7).collect();
        let a = archipelago(42, &members);
        assert_eq!(a, archipelago(42, &members));
        assert_eq!(a.spots.len(), members.len());
        let mut cells: Vec<_> = a.spots.iter().map(|s| (s.x, s.y)).collect();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(cells.len(), members.len(), "two islands on one cell");
        for s in &a.spots {
            assert!(
                (0..a.width).contains(&s.x) && (0..a.height).contains(&s.y),
                "{s:?}"
            );
        }
        // Another group, other wobbles: not every island in the same place
        let b = archipelago(43, &members);
        assert_ne!(a.spots, b.spots);
        // Islands of members who stay don't move when someone joins the last row
        let more = archipelago(42, &[members, vec![99]].concat());
        assert_eq!(more.spots[..8], a.spots[..8]);
    }

    #[test]
    fn bridges_join_neighbours_and_never_cross() {
        let members: Vec<i64> = (1..=20).collect();
        for group in 1..40 {
            let a = archipelago(group, &members);
            let straits = a.straits();
            for s in &straits {
                if s.horizontal {
                    assert_eq!(s.a.y, s.b.y);
                    assert!(s.b.x - s.a.x >= 2, "islands side by side: {s:?}");
                } else {
                    assert_eq!(s.a.x, s.b.x);
                    assert_eq!(s.b.y - s.a.y, SLOT);
                }
            }
            for h in straits.iter().filter(|s| s.horizontal) {
                for v in straits.iter().filter(|s| !s.horizontal) {
                    // The vertical one's column strictly inside the horizontal one, and the
                    // horizontal one's row strictly inside the vertical one
                    let column = (h.a.x + 1..h.b.x).contains(&v.a.x);
                    let row = (v.a.y + 1..v.b.y).contains(&h.a.y);
                    let crosses = column && row;
                    assert!(!crosses, "{h:?} crosses {v:?}");
                }
            }
            // Every row is a chain
            let horizontal = straits.iter().filter(|s| s.horizontal).count();
            let rows = members.len().div_ceil(columns(members.len()));
            assert_eq!(horizontal, members.len() - rows);
        }
        assert_eq!(archipelago(1, &[]).spots, vec![]);
        assert_eq!(archipelago(1, &[5]).straits(), vec![]);
    }
}
