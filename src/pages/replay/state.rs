//! The replay theatre's state: the clock, who's shown, and every member's board at the clock's
//! time, worked out from the logs as the clock moves (see `model::replay`).

use leptos::prelude::*;

use crate::model::{
    Cursor, Finisher, Lane, ReplayData, SPEEDS, advance, finish_order, merged, next_moment,
    previous_moment, total_bridges,
};

/// A jump of Shift + an arrow key, in replay time.
pub const JUMP_MS: u32 = 10_000;

/// How the theatre shows the boards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    /// Everyone on one board, side by side on every bridge.
    #[default]
    Together,
    /// Small multiples: a little board per member, all on the same clock.
    Apart,
}

/// More shown members than this start apart: the lanes on one bridge would get too thin.
pub const MAX_TOGETHER: usize = 8;

/// A member's board at the clock's time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaneBoard {
    pub bridges: Vec<u8>,
    /// Moves made so far: the last one is `log[applied - 1]`.
    pub applied: usize,
}

/// The replay theatre of one day. Cheap to copy, everything lives in the reactive arena.
#[derive(Clone, Copy, Debug)]
pub struct Theatre {
    pub data: StoredValue<ReplayData>,
    /// The clock, ms since everybody's start.
    pub time: RwSignal<f64>,
    pub playing: RwSignal<bool>,
    pub speed: RwSignal<u32>,
    /// Per lane, whether it's hidden (the legend's toggles).
    pub hidden: RwSignal<Vec<bool>>,
    pub layout: RwSignal<Layout>,
    /// The clock in whole ms: what the boards follow.
    pub at: Memo<u32>,
    /// Every lane's board at the clock's (whole ms) time.
    pub boards: Memo<Vec<LaneBoard>>,
    /// Who crossed the line by now, in their order.
    pub finished: Memo<Vec<Finisher>>,
    /// The moves of all lanes, `(t, lane)` in time order, for stepping.
    moments: StoredValue<Vec<(u32, usize)>>,
    /// Counts the plays, so an old frame loop stops when a new one starts.
    pub run: StoredValue<u32>,
    /// When the last lane finished, the end of the timeline.
    pub end: u32,
    /// Bridges of the day's solution.
    pub total: u32,
    pub edges: usize,
}

impl Theatre {
    /// A theatre for `data`, showing everyone with a log, or only `only` (the viewer and a
    /// rival, for a duel) when given.
    pub fn new(data: ReplayData, only: Option<&[i64]>) -> Self {
        let board = data.puzzle.clone().map(common::Board::new);
        let edges = board.as_ref().map_or(0, |b| b.edges.len());
        let total = data.puzzle.as_ref().map_or(0, total_bridges);
        let end = data.end();
        let logs: Vec<&[crate::model::MoveEvent]> =
            data.lanes.iter().map(|l| l.log.as_slice()).collect();
        let moments = merged(&logs);
        let hidden: Vec<bool> = data
            .lanes
            .iter()
            .map(|l| only.is_some_and(|ids| !ids.contains(&l.user_id)))
            .collect();
        let shown = data
            .lanes
            .iter()
            .zip(&hidden)
            .filter(|(l, h)| !**h && !l.log.is_empty())
            .count();
        let lanes = data.lanes.len();
        let data = StoredValue::new(data);
        let time = RwSignal::new(0.0_f64);
        let cursors = StoredValue::new(vec![Cursor::new(edges); lanes]);
        let at = Memo::new(move |_| whole_ms(time.get()));
        let boards = Memo::new(move |_| {
            let t = at.get();
            data.with_value(|d| {
                cursors.update_value(|cursors| {
                    for (cursor, lane) in cursors.iter_mut().zip(&d.lanes) {
                        cursor.seek(&lane.log, t);
                    }
                });
            });
            cursors.with_value(|cursors| {
                cursors
                    .iter()
                    .map(|c| LaneBoard {
                        bridges: c.bridges.clone(),
                        applied: c.applied,
                    })
                    .collect()
            })
        });
        let finished = Memo::new(move |_| {
            let t = at.get();
            data.with_value(|d| finish_order(&d.lanes, t))
        });
        Self {
            data,
            time,
            at,
            finished,
            playing: RwSignal::new(false),
            speed: RwSignal::new(SPEEDS[1]),
            hidden: RwSignal::new(hidden),
            layout: RwSignal::new(if shown > MAX_TOGETHER {
                Layout::Apart
            } else {
                Layout::Together
            }),
            boards,
            moments: StoredValue::new(moments),
            run: StoredValue::new(0),
            end,
            total,
            edges,
        }
    }

    /// The clock in whole ms.
    pub fn now(self) -> u32 {
        self.at.get()
    }

    pub fn lanes(self) -> usize {
        self.data.with_value(|d| d.lanes.len())
    }

    pub fn lane(self, i: usize) -> Option<Lane> {
        self.data.with_value(|d| d.lanes.get(i).cloned())
    }

    /// Lanes with moves to play: solved with a log.
    pub fn playable(self, i: usize) -> bool {
        self.data
            .with_value(|d| d.lanes.get(i).is_some_and(|l| !l.log.is_empty()))
    }

    pub fn shown(self, i: usize) -> bool {
        self.playable(i) && !self.hidden.with(|h| h.get(i).copied().unwrap_or(false))
    }

    /// The shown lanes with moves, in lane order: their place on every bridge.
    pub fn shown_lanes(self) -> Vec<usize> {
        (0..self.lanes()).filter(|&i| self.shown(i)).collect()
    }

    pub fn toggle(self, i: usize) {
        self.hidden.update(|h| {
            if let Some(hidden) = h.get_mut(i) {
                *hidden = !*hidden;
            }
        });
    }

    /// Shows only the lanes of these players, or everyone for `None`.
    pub fn show_only(self, ids: Option<&[i64]>) {
        let hidden = self.data.with_value(|d| {
            d.lanes
                .iter()
                .map(|l| ids.is_some_and(|ids| !ids.contains(&l.user_id)))
                .collect()
        });
        self.hidden.set(hidden);
    }

    pub fn play(self) {
        if self.end == 0 {
            return;
        }
        if self.time.get_untracked() >= f64::from(self.end) {
            self.time.set(0.0);
        }
        self.run.update_value(|r| *r += 1);
        self.playing.set(true);
    }

    pub fn pause(self) {
        self.playing.set(false);
    }

    pub fn toggle_play(self) {
        if self.playing.get_untracked() {
            self.pause();
        } else {
            self.play();
        }
    }

    /// Real time passed while playing: the clock moves on at the speed, and stops at the end.
    pub fn tick(self, real_ms: f64) {
        let t = advance(
            self.time.get_untracked(),
            real_ms,
            self.speed.get_untracked(),
            self.end,
        );
        self.time.set(t);
        if t >= f64::from(self.end) {
            self.playing.set(false);
        }
    }

    pub fn seek(self, t: f64) {
        self.time.set(t.clamp(0.0, f64::from(self.end)));
    }

    /// To the next (`forward`) or previous move of a shown lane, pausing: step mode.
    pub fn step(self, forward: bool) {
        self.pause();
        let t = whole_ms(self.time.get_untracked());
        let shown = |lane| self.shown_untracked(lane);
        let to = self.moments.with_value(|m| {
            if forward {
                next_moment(m, t, shown)
            } else {
                previous_moment(m, t, shown)
            }
        });
        match (to, forward) {
            (Some(to), _) => self.time.set(f64::from(to)),
            (None, true) => self.time.set(f64::from(self.end)),
            (None, false) => self.time.set(0.0),
        }
    }

    /// Shift + an arrow: [`JUMP_MS`] on or back.
    pub fn jump(self, forward: bool) {
        let t = self.time.get_untracked();
        let delta = f64::from(JUMP_MS);
        self.seek(if forward { t + delta } else { t - delta });
    }

    fn shown_untracked(self, i: usize) -> bool {
        self.playable(i)
            && !self
                .hidden
                .with_untracked(|h| h.get(i).copied().unwrap_or(false))
    }

    /// Who crossed the line by now, `(lane, place, at)` in their order.
    pub fn finishers(self) -> Vec<Finisher> {
        self.finished.get()
    }
}

/// A time of the clock in whole ms.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the clock is clamped to the timeline, a few hours at most"
)]
pub fn whole_ms(t: f64) -> u32 {
    t.max(0.0).min(f64::from(u32::MAX)) as u32
}

#[cfg(test)]
mod tests {
    use common::{Pos, Puzzle};

    use super::*;
    use crate::model::{LaneOutcome, MoveEvent};

    fn mv(t: u32, edge: u16, count: u8) -> MoveEvent {
        MoveEvent {
            t,
            edge,
            count,
            tap: true,
        }
    }

    fn lane(user_id: i64, log: Vec<MoveEvent>) -> Lane {
        Lane {
            user_id,
            name: format!("p{user_id}"),
            guest: false,
            colour: u8::try_from(user_id).unwrap(),
            outcome: if log.is_empty() {
                LaneOutcome::GaveUp
            } else {
                LaneOutcome::Solved {
                    time_ms: 1,
                    hints: 0,
                    checks: 0,
                    ranked: true,
                }
            },
            log,
            marks: Vec::new(),
        }
    }

    /// Two islands by a double bridge (edge 0), a third below the second by one (edge 1).
    fn data() -> ReplayData {
        let mut puzzle = Puzzle::empty(5, 5);
        puzzle.set(Pos::new(0, 0), 2);
        puzzle.set(Pos::new(4, 0), 3);
        puzzle.set(Pos::new(4, 4), 1);
        ReplayData {
            day: 10,
            today: 11,
            group: None,
            me: 1,
            locked: false,
            puzzle: Some(puzzle),
            lanes: vec![
                lane(1, vec![mv(1000, 0, 1), mv(2000, 0, 2), mv(6000, 1, 1)]),
                lane(2, vec![mv(500, 1, 1), mv(3000, 0, 2)]),
                lane(3, Vec::new()),
            ],
            group_puzzle: None,
        }
    }

    fn bridges(t: &Theatre) -> Vec<Vec<u8>> {
        t.boards.get().into_iter().map(|b| b.bridges).collect()
    }

    #[test]
    fn seeking_moves_every_board_to_the_moment() {
        Owner::new().with(|| {
            let t = Theatre::new(data(), None);
            assert_eq!((t.end, t.total, t.edges), (6000, 3, 2));
            assert_eq!(bridges(&t), [vec![0, 0], vec![0, 0], vec![0, 0]]);
            t.seek(2500.0);
            assert_eq!(bridges(&t), [vec![2, 0], vec![0, 1], vec![0, 0]]);
            t.seek(9999.0);
            assert_eq!(t.now(), 6000);
            assert_eq!(bridges(&t), [vec![2, 1], vec![2, 1], vec![0, 0]]);
            // And back
            t.seek(-5.0);
            assert_eq!(t.now(), 0);
            assert_eq!(bridges(&t)[0], [0, 0]);
            t.seek(1000.0);
            assert_eq!(t.boards.get()[0].applied, 1);
        });
    }

    #[test]
    fn playing_runs_the_clock_at_the_speed_until_the_end() {
        Owner::new().with(|| {
            let t = Theatre::new(data(), None);
            t.speed.set(4);
            t.play();
            assert!(t.playing.get());
            t.tick(250.0);
            assert_eq!(t.now(), 1000);
            t.speed.set(16);
            t.tick(1000.0);
            assert_eq!(t.now(), 6000);
            assert!(!t.playing.get(), "stops at the end");
            // Playing again starts over
            t.play();
            assert_eq!(t.now(), 0);
            t.toggle_play();
            assert!(!t.playing.get());
        });
    }

    #[test]
    fn steps_go_from_move_to_move_of_the_shown() {
        Owner::new().with(|| {
            let t = Theatre::new(data(), None);
            t.play();
            t.step(true);
            assert!(!t.playing.get(), "stepping pauses");
            assert_eq!(t.now(), 500);
            t.step(true);
            assert_eq!(t.now(), 1000);
            // Lane 2 hidden: its moves are skipped
            t.toggle(1);
            assert!(!t.shown(1));
            t.step(true);
            assert_eq!(t.now(), 2000);
            t.step(true);
            assert_eq!(t.now(), 6000);
            t.step(true);
            assert_eq!(t.now(), 6000);
            t.step(false);
            assert_eq!(t.now(), 2000);
            t.seek(0.0);
            t.step(false);
            assert_eq!(t.now(), 0);
            t.jump(true);
            assert_eq!(t.now(), 6000);
            t.jump(false);
            assert_eq!(t.now(), 0);
        });
    }

    #[test]
    fn who_is_shown() {
        Owner::new().with(|| {
            let t = Theatre::new(data(), None);
            // Lane 3 gave up: nothing to show
            assert_eq!(t.shown_lanes(), [0, 1]);
            assert!(!t.playable(2));
            t.show_only(Some(&[2]));
            assert_eq!(t.shown_lanes(), [1]);
            t.show_only(None);
            assert_eq!(t.shown_lanes(), [0, 1]);
            // A duel: the viewer and a rival
            let duel = Theatre::new(data(), Some(&[1, 2]));
            assert_eq!(duel.shown_lanes(), [0, 1]);
            let alone = Theatre::new(data(), Some(&[1]));
            assert_eq!(alone.shown_lanes(), [0]);
            assert_eq!(alone.layout.get(), Layout::Together);
        });
    }

    #[test]
    fn the_finish_line_fills_in() {
        Owner::new().with(|| {
            let t = Theatre::new(data(), None);
            assert_eq!(t.finishers(), []);
            t.seek(3000.0);
            let first: Vec<_> = t.finishers().iter().map(|f| (f.lane, f.place)).collect();
            assert_eq!(first, [(1, 1)]);
            t.seek(6000.0);
            assert_eq!(t.finishers().len(), 2);
        });
    }

    #[test]
    fn big_groups_start_apart() {
        Owner::new().with(|| {
            let mut big = data();
            big.lanes = (1..=10).map(|i| lane(i, vec![mv(100, 0, 1)])).collect();
            assert_eq!(Theatre::new(big, None).layout.get(), Layout::Apart);
            // Nobody played: no timeline to run
            let mut empty = data();
            empty.lanes.clear();
            let t = Theatre::new(empty, None);
            t.play();
            assert!(!t.playing.get());
        });
    }
}
