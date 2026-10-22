//! Replays (再生 saisei): the move logs of a day's daily played back, every member of a group at
//! once on one board, all starting at the same moment. What the server sends, and the clock and
//! the boards of the replay theatre, which the browser works out from the logs.
//!
//! Logs are only sent for days that are over, or today's once the viewer finished it themselves
//! ([`ReplayData::locked`]), and only to the group's members (`server::replay`).

use common::Puzzle;
use serde::{Deserialize, Serialize};

use super::moves::MoveEvent;

/// How much faster than real time the replay can run.
pub const SPEEDS: [u32; 3] = [1, 4, 16];

/// A paid help during a solve, a marker on the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarkKind {
    Hint,
    Check,
    /// A help of a newer server, read by a page or an app that doesn't know it yet: no marker.
    #[serde(other)]
    Unknown,
}

/// When a member took a hint or a check, ms after their start like [`MoveEvent::t`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub t: u32,
    pub kind: MarkKind,
}

/// How a member's day ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaneOutcome {
    /// Solved: the official time (penalties included), and whether it's on the boards.
    Solved {
        time_ms: i64,
        hints: i64,
        checks: i64,
        ranked: bool,
    },
    /// Gave up: the solver finished it, there is no log to play.
    GaveUp,
    /// Not finished (still playing today, or not played that day): no log either.
    Missing,
}

/// A member's solve: who, in which colour, and their moves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lane {
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    /// Their colour slot in the group, see `model::colour_var`.
    pub colour: u8,
    pub outcome: LaneOutcome,
    /// In time order, only edges of the day's board (see [`clean_log`]). Empty unless solved.
    pub log: Vec<MoveEvent>,
    pub marks: Vec<Mark>,
}

impl Lane {
    /// When they crossed the line: the move that solved it, the last of the log.
    pub fn finish(&self) -> Option<u32> {
        match self.outcome {
            LaneOutcome::Solved { .. } => self.log.last().map(|e| e.t),
            LaneOutcome::GaveUp | LaneOutcome::Missing => None,
        }
    }
}

/// The group a replay belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayGroup {
    pub id: i64,
    pub name: String,
    /// The day the group was made: replays go back to it, not further.
    pub first_day: i64,
}

/// Everything the replay theatre needs for a day.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayData {
    pub day: i64,
    pub today: i64,
    /// `None`: the viewer's own solve, without a group.
    pub group: Option<ReplayGroup>,
    /// The viewer.
    pub me: i64,
    /// Today's daily, which the viewer hasn't finished yet: no puzzle and no logs, those would
    /// spoil it.
    pub locked: bool,
    /// The day's puzzle, `None` when locked or nobody played the day.
    pub puzzle: Option<Puzzle>,
    /// A lane per member, the group's order (longest standing first).
    pub lanes: Vec<Lane>,
}

impl ReplayData {
    /// When the last one crossed the line, where the timeline ends.
    pub fn end(&self) -> u32 {
        self.lanes
            .iter()
            .filter_map(|l| l.finish().or_else(|| l.log.last().map(|e| e.t)))
            .max()
            .unwrap_or(0)
    }

    pub fn lane(&self, user_id: i64) -> Option<&Lane> {
        self.lanes.iter().find(|l| l.user_id == user_id)
    }
}

/// A stored move log made safe to play: moves on edges the board doesn't have, or more than
/// two bridges, left out, in time order (the order of equal times kept).
pub fn clean_log(log: &[MoveEvent], edges: usize) -> Vec<MoveEvent> {
    let mut clean: Vec<MoveEvent> = log
        .iter()
        .copied()
        .filter(|e| usize::from(e.edge) < edges && e.count <= 2)
        .collect();
    clean.sort_by_key(|e| e.t);
    clean
}

/// The bridges of a log at time `t`: every move up to and including `t` made.
pub fn board_at(log: &[MoveEvent], edges: usize, t: u32) -> Vec<u8> {
    let mut cursor = Cursor::new(edges);
    cursor.seek(log, t);
    cursor.bridges
}

/// A board following a log as the replay's clock moves: forward one move at a time, and back
/// from the start, which is as quick for logs of a few thousand moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// The moves made so far.
    pub applied: usize,
    pub bridges: Vec<u8>,
}

impl Cursor {
    pub fn new(edges: usize) -> Self {
        Self {
            applied: 0,
            bridges: vec![0; edges],
        }
    }

    /// Moves the board to time `t` of `log` (a clean one, in time order).
    pub fn seek(&mut self, log: &[MoveEvent], t: u32) {
        let target = log.partition_point(|e| e.t <= t);
        if target < self.applied {
            self.applied = 0;
            self.bridges.fill(0);
        }
        for e in &log[self.applied..target] {
            if let Some(slot) = self.bridges.get_mut(usize::from(e.edge)) {
                *slot = e.count;
            }
        }
        self.applied = target;
    }
}

/// The moments of all logs together, in time order: `(t, lane)` for every move. Stepping through
/// the replay goes from one to the next.
pub fn merged(logs: &[&[MoveEvent]]) -> Vec<(u32, usize)> {
    let mut all: Vec<(u32, usize)> = logs
        .iter()
        .enumerate()
        .flat_map(|(lane, log)| log.iter().map(move |e| (e.t, lane)))
        .collect();
    all.sort_unstable();
    all
}

/// The first moment after `t` a shown lane moves, for stepping forward.
pub fn next_moment(moments: &[(u32, usize)], t: u32, shown: impl Fn(usize) -> bool) -> Option<u32> {
    let from = moments.partition_point(|&(m, _)| m <= t);
    moments[from..]
        .iter()
        .find(|&&(_, lane)| shown(lane))
        .map(|&(m, _)| m)
}

/// The last moment before `t` a shown lane moved, for stepping back.
pub fn previous_moment(
    moments: &[(u32, usize)],
    t: u32,
    shown: impl Fn(usize) -> bool,
) -> Option<u32> {
    let to = moments.partition_point(|&(m, _)| m < t);
    moments[..to]
        .iter()
        .rev()
        .find(|&&(_, lane)| shown(lane))
        .map(|&(m, _)| m)
}

/// The replay's clock after `real_ms` of real time at `speed`, stopping at `end`.
pub fn advance(t: f64, real_ms: f64, speed: u32, end: u32) -> f64 {
    (t + real_ms.max(0.0) * f64::from(speed)).min(f64::from(end))
}

/// A member who crossed the line in the replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Finisher {
    /// Index into the lanes.
    pub lane: usize,
    /// 1 for the first across the line.
    pub place: usize,
    pub at: u32,
}

/// Who crossed the line by time `t`, in the order they did: earlier first, the same moment in
/// lane order. The places count everyone, shown or not.
pub fn finish_order(lanes: &[Lane], t: u32) -> Vec<Finisher> {
    let mut done: Vec<(u32, usize)> = lanes
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.finish().map(|at| (at, i)))
        .collect();
    done.sort_unstable();
    done.into_iter()
        .enumerate()
        .filter(|&(_, (at, _))| at <= t)
        .map(|(i, (at, lane))| Finisher {
            lane,
            place: i + 1,
            at,
        })
        .collect()
}

/// Bridges a puzzle's solution has: every bridge counts at both its islands.
pub fn total_bridges(puzzle: &Puzzle) -> u32 {
    puzzle
        .islands
        .iter()
        .map(|i| u32::from(i.required))
        .sum::<u32>()
        / 2
}

/// How far along a board is, in percent of the solution's bridges (placed ones, right or not,
/// at most 100).
pub fn progress(bridges: &[u8], total: u32) -> u32 {
    if total == 0 {
        return 0;
    }
    let placed: u32 = bridges.iter().map(|&b| u32::from(b)).sum();
    (placed.min(total) * 100 / total).min(100)
}

/// Where a moment is on the timeline, 0 to 1.
pub fn fraction(t: u32, end: u32) -> f64 {
    if end == 0 {
        0.0
    } else {
        (f64::from(t) / f64::from(end)).clamp(0.0, 1.0)
    }
}

/// The widest space between two members' lines on a bridge, in art pixels.
pub const LANE_PITCH: f64 = 4.5;
/// How wide all lines on one bridge together may be, in art pixels: about an island's width.
pub const LANE_BAND: f64 = 12.0;

/// The distance between two lines when `lanes` members share a bridge.
pub fn lane_pitch(lanes: usize) -> f64 {
    if lanes == 0 {
        return LANE_PITCH;
    }
    #[expect(clippy::cast_precision_loss, reason = "a group has at most 50 members")]
    let lanes = lanes as f64;
    LANE_PITCH.min(LANE_BAND / lanes)
}

/// How far the line of the `i`th of `lanes` shown members lies from the middle of a bridge, in
/// art pixels: every member has their own lane on every bridge, the lanes centred.
pub fn lane_offset(i: usize, lanes: usize) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "a group has at most 50 members")]
    let (i, n) = (i as f64, lanes.max(1) as f64);
    (i - (n - 1.0) / 2.0) * lane_pitch(lanes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(t: u32, edge: u16, count: u8) -> MoveEvent {
        MoveEvent {
            t,
            edge,
            count,
            tap: true,
        }
    }

    fn lane(user_id: i64, log: Vec<MoveEvent>, solved: bool) -> Lane {
        Lane {
            user_id,
            name: format!("p{user_id}"),
            guest: false,
            colour: 0,
            outcome: if solved {
                LaneOutcome::Solved {
                    time_ms: 0,
                    hints: 0,
                    checks: 0,
                    ranked: true,
                }
            } else {
                LaneOutcome::Missing
            },
            log,
            marks: Vec::new(),
        }
    }

    #[test]
    fn logs_are_cleaned_before_they_play() {
        let log = [
            mv(500, 1, 1),
            mv(200, 0, 1),
            mv(300, 9, 1),
            mv(400, 0, 3),
            mv(200, 2, 2),
        ];
        // Edge 9 isn't on a board of 3, 3 bridges can't be; equal times keep their order
        assert_eq!(
            clean_log(&log, 3),
            [mv(200, 0, 1), mv(200, 2, 2), mv(500, 1, 1)]
        );
        assert_eq!(clean_log(&[], 3), []);
    }

    /// Seeking anywhere gives the board of that moment, forward step by step or back again.
    #[test]
    fn seeking_gives_the_board_of_the_moment() {
        let log = [
            mv(100, 0, 1),
            mv(250, 0, 2),
            mv(250, 1, 1),
            mv(900, 0, 0),
            mv(1200, 2, 2),
        ];
        assert_eq!(board_at(&log, 3, 0), [0, 0, 0]);
        assert_eq!(board_at(&log, 3, 100), [1, 0, 0]);
        // Both moves of the same moment
        assert_eq!(board_at(&log, 3, 250), [2, 1, 0]);
        assert_eq!(board_at(&log, 3, 899), [2, 1, 0]);
        assert_eq!(board_at(&log, 3, 900), [0, 1, 0]);
        assert_eq!(board_at(&log, 3, u32::MAX), [0, 1, 2]);

        let mut cursor = Cursor::new(3);
        for t in [0, 120, 260, 1000, 1300] {
            cursor.seek(&log, t);
            assert_eq!(cursor.bridges, board_at(&log, 3, t), "forward to {t}");
        }
        assert_eq!(cursor.applied, log.len());
        for t in [950, 250, 99, 600] {
            cursor.seek(&log, t);
            assert_eq!(cursor.bridges, board_at(&log, 3, t), "back to {t}");
        }
    }

    #[test]
    fn the_logs_merge_into_one_timeline() {
        let a = [mv(100, 0, 1), mv(700, 1, 1)];
        let b = [mv(100, 2, 1), mv(400, 0, 1), mv(1000, 1, 2)];
        let moments = merged(&[&a, &b, &[]]);
        assert_eq!(moments, [(100, 0), (100, 1), (400, 1), (700, 0), (1000, 1)]);
        let all = |_| true;
        assert_eq!(next_moment(&moments, 0, all), Some(100));
        assert_eq!(next_moment(&moments, 100, all), Some(400));
        assert_eq!(next_moment(&moments, 1000, all), None);
        assert_eq!(previous_moment(&moments, 700, all), Some(400));
        assert_eq!(previous_moment(&moments, 100, all), None);
        // Hidden lanes are stepped over
        let only_a = |lane| lane == 0;
        assert_eq!(next_moment(&moments, 100, only_a), Some(700));
        assert_eq!(previous_moment(&moments, 1000, only_a), Some(700));
        assert_eq!(previous_moment(&moments, 700, only_a), Some(100));
    }

    #[test]
    fn the_clock_runs_at_the_chosen_speed_and_stops_at_the_end() {
        assert!((advance(0.0, 16.0, 1, 10_000) - 16.0).abs() < 1e-9);
        assert!((advance(1000.0, 16.0, 4, 10_000) - 1064.0).abs() < 1e-9);
        assert!((advance(1000.0, 100.0, 16, 10_000) - 2600.0).abs() < 1e-9);
        assert!((advance(9_990.0, 100.0, 16, 10_000) - 10_000.0).abs() < 1e-9);
        // A clock going back (a tab woken from sleep) doesn't run the replay backwards
        assert!((advance(500.0, -40.0, 4, 10_000) - 500.0).abs() < 1e-9);
        assert!((fraction(2_500, 10_000) - 0.25).abs() < 1e-9);
        assert!((fraction(20_000, 10_000) - 1.0).abs() < 1e-9);
        assert!(fraction(5, 0).abs() < 1e-9);
    }

    /// The finish order fills in as the clock passes each finish.
    #[test]
    fn the_finish_order_fills_in() {
        let lanes = [
            lane(1, vec![mv(100, 0, 1), mv(3000, 1, 1)], true),
            lane(2, vec![mv(200, 0, 1), mv(1800, 1, 1)], true),
            // Not finished: its moves never cross the line
            lane(3, vec![mv(50, 0, 1)], false),
            lane(4, vec![mv(900, 0, 2), mv(3000, 1, 1)], true),
        ];
        assert_eq!(finish_order(&lanes, 1000), []);
        assert_eq!(
            finish_order(&lanes, 1800),
            [Finisher {
                lane: 1,
                place: 1,
                at: 1800
            }]
        );
        let all: Vec<_> = finish_order(&lanes, 5000)
            .iter()
            .map(|f| (f.lane, f.place))
            .collect();
        // The same moment: in lane order
        assert_eq!(all, [(1, 1), (0, 2), (3, 3)]);
        let data = ReplayData {
            day: 1,
            today: 2,
            group: None,
            me: 1,
            locked: false,
            puzzle: None,
            lanes: lanes.to_vec(),
        };
        assert_eq!(data.end(), 3000);
        assert_eq!(data.lane(2).map(Lane::finish), Some(Some(1800)));
        assert_eq!(data.lane(3).and_then(Lane::finish), None);
    }

    #[test]
    fn progress_counts_the_bridges_placed() {
        let mut puzzle = Puzzle::empty(5, 5);
        puzzle.set(common::Pos::new(0, 0), 2);
        puzzle.set(common::Pos::new(4, 0), 3);
        puzzle.set(common::Pos::new(4, 4), 1);
        assert_eq!(total_bridges(&puzzle), 3);
        assert_eq!(progress(&[0, 0], 3), 0);
        assert_eq!(progress(&[1, 0], 3), 33);
        assert_eq!(progress(&[2, 1], 3), 100);
        // Wrong extra bridges don't go past the finish
        assert_eq!(progress(&[2, 2, 2], 3), 100);
        assert_eq!(progress(&[1], 0), 0);
    }

    /// Every member keeps their lane on every bridge, the lanes centred and within an island's
    /// width.
    #[test]
    fn lanes_share_a_bridge_side_by_side() {
        assert!(lane_offset(0, 1).abs() < 1e-9);
        assert!((lane_offset(0, 2) + LANE_PITCH / 2.0).abs() < 1e-9);
        assert!((lane_offset(1, 2) - LANE_PITCH / 2.0).abs() < 1e-9);
        for n in [3, 6, 12, 50] {
            let first = lane_offset(0, n);
            let last = lane_offset(n - 1, n);
            assert!((first + last).abs() < 1e-9, "centred for {n}");
            assert!(last - first <= LANE_BAND + 1e-9, "{n} lanes too wide");
            assert!(lane_offset(1, n) > first);
        }
        assert!((lane_pitch(0) - LANE_PITCH).abs() < 1e-9);
    }
}
