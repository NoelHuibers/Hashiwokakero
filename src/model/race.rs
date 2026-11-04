//! Races (勝負 shōbu): members of a group race each other live on the same fresh puzzle.
//!
//! A member opens a lobby on the group page with a difficulty of Play's presets, the others
//! join. The host starts it once at least [`MIN_RACERS`] joined: the server generates the puzzle
//! and the race begins [`COUNTDOWN_MS`] later for everyone at once, on the server's clock. While
//! they play, everyone sees how far the others are (a progress bar, never their bridges). It ends
//! when everyone finished, gave up or is away, or [`RACE_MS`] after the start; then the results
//! with the replay of everyone's moves, and a rematch.
//!
//! What the server sends the race's page ([`RaceView`]) and the group page ([`GroupRaces`]), and
//! the rules both sides need: the phases, the places and the progress shown.

use std::collections::HashMap;

use common::{Difficulty, Puzzle};
use serde::{Deserialize, Serialize};

use super::daily::HelpPenalties;
use super::moves::Flag;

/// How long a lobby waits for its start, from opening it.
pub const LOBBY_MS: i64 = 15 * 60_000;
/// From the host's start to the race's: 3, 2, 1, 始め.
pub const COUNTDOWN_MS: i64 = 5_000;
/// The longest a race runs, from its start.
pub const RACE_MS: i64 = 30 * 60_000;
/// A player not seen for this long is away: dropped out, unless they come back while it runs.
pub const AWAY_MS: i64 = 60_000;
/// How often a player's progress goes to the server at most.
pub const PROGRESS_EVERY_MS: i64 = 1_000;
/// The server takes a player's progress at most this often: a little below
/// [`PROGRESS_EVERY_MS`], for requests that arrive a little early.
pub const PROGRESS_MIN_GAP_MS: i64 = 800;
/// Players a race needs to start.
pub const MIN_RACERS: usize = 2;
/// The finished races the group page shows, the newest first.
pub const HISTORY_SHOWN: usize = 5;
/// The most a progress bar shows before the server verified the finish.
pub const PROGRESS_CAP: u8 = 99;

/// Where a race stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RacePhase {
    /// Waiting for players and the host's start.
    Lobby,
    /// Started, the puzzle comes at `starts_at`.
    Countdown,
    Running,
    /// Over: the results.
    Ended,
}

impl RacePhase {
    /// The phase at `now` of a race that starts at `starts_at` (`None`: not started) and ended
    /// at `ended_at`. Past [`RACE_MS`] it is over, whether the server noted it yet or not.
    pub fn at(now: i64, starts_at: Option<i64>, ended_at: Option<i64>) -> Self {
        match starts_at {
            None => Self::Lobby,
            Some(_) if ended_at.is_some() => Self::Ended,
            Some(start) if now < start => Self::Countdown,
            Some(start) if now >= start + RACE_MS => Self::Ended,
            Some(_) => Self::Running,
        }
    }
}

/// How a player's race went, or goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaceOutcome {
    /// In the lobby, or racing.
    Racing,
    /// Solved, verified by the server: the time with penalties, and the place (`None`: the move
    /// log was flagged, no place).
    Solved {
        time_ms: i64,
        place: Option<u32>,
    },
    GaveUp,
    /// Didn't finish before the race ended: away, or the time ran out.
    Out,
}

impl RaceOutcome {
    /// Done with the race: solved or gave up.
    pub fn done(self) -> bool {
        matches!(self, Self::Solved { .. } | Self::GaveUp)
    }
}

/// A player of a race, as everyone sees them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RacePlayer {
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    /// Their colour slot in the group, see `model::colour_var`.
    pub colour: u8,
    pub joined_at: i64,
    /// Percent of the solution's bridges on their board: what they reported, at most
    /// [`PROGRESS_CAP`] until they finished, 100 once solved.
    pub progress: u8,
    /// Not seen for [`AWAY_MS`].
    pub away: bool,
    pub hints: i64,
    pub checks: i64,
    /// Why a finish has no place.
    pub flag: Option<Flag>,
    pub outcome: RaceOutcome,
}

/// Everything the page of a race shows, as the viewer may see it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceView {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    /// The viewer.
    pub me: i64,
    /// Who opened it, `None`: they deleted their account.
    pub host: Option<i64>,
    pub difficulty: Difficulty,
    /// What a hint and a check cost, the daily's prices for the difficulty.
    pub penalties: HelpPenalties,
    pub created_at: i64,
    /// When the race starts (the clocks of everyone), `None` in the lobby.
    pub starts_at: Option<i64>,
    pub ended_at: Option<i64>,
    /// The ones who joined, the first first.
    pub players: Vec<RacePlayer>,
    /// The puzzle: for the players once it started (and when they asked for it, the page keeps
    /// it), for everyone once it's over.
    pub puzzle: Option<Puzzle>,
    /// The solution, once it's over.
    pub solution: Option<Vec<u8>>,
    /// The race opened as its rematch, and whether it still takes players.
    pub rematch: Option<i64>,
    pub rematch_open: bool,
    /// Server time when the response was made, to correct the browser's clock.
    pub server_now: i64,
}

impl RaceView {
    pub fn phase(&self) -> RacePhase {
        RacePhase::at(self.server_now, self.starts_at, self.ended_at)
    }

    /// When the lobby closes if nobody starts it.
    pub fn expires_at(&self) -> i64 {
        self.created_at + LOBBY_MS
    }

    /// When the race ends at the latest.
    pub fn deadline(&self) -> Option<i64> {
        self.starts_at.map(|s| s + RACE_MS)
    }

    /// The viewer's line, if they joined.
    pub fn mine(&self) -> Option<&RacePlayer> {
        self.players.iter().find(|p| p.user_id == self.me)
    }

    pub fn joined(&self) -> bool {
        self.mine().is_some()
    }

    pub fn is_host(&self) -> bool {
        self.host == Some(self.me)
    }

    /// The host can start: in the lobby, with enough players.
    pub fn can_start(&self) -> bool {
        self.is_host() && self.phase() == RacePhase::Lobby && self.players.len() >= MIN_RACERS
    }

    /// The viewer still races: joined, the race runs and they aren't done.
    pub fn racing(&self) -> bool {
        self.phase() == RacePhase::Running
            && self
                .mine()
                .is_some_and(|p| p.outcome == RaceOutcome::Racing)
    }
}

/// A player of a race on the group page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceEntrant {
    pub user_id: i64,
    pub name: String,
    pub colour: u8,
    /// Over: their place (`None`: no place) and time.
    pub place: Option<u32>,
    pub time_ms: Option<i64>,
}

/// A race on the group page: the open one, or one of the last ones.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceSummary {
    pub id: i64,
    pub difficulty: Difficulty,
    pub host: Option<i64>,
    pub created_at: i64,
    pub starts_at: Option<i64>,
    pub ended_at: Option<i64>,
    /// The players, the first to join first.
    pub entrants: Vec<RaceEntrant>,
    /// The viewer is one of them.
    pub joined: bool,
}

impl RaceSummary {
    pub fn phase(&self, now: i64) -> RacePhase {
        RacePhase::at(now, self.starts_at, self.ended_at)
    }

    /// Who came first, and their time.
    pub fn winner(&self) -> Option<&RaceEntrant> {
        self.entrants.iter().find(|e| e.place == Some(1))
    }
}

/// The races of a group page.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupRaces {
    /// The race in the lobby or running, at most one per group.
    pub open: Option<RaceSummary>,
    /// The last finished ones, the newest first, at most [`HISTORY_SHOWN`].
    pub recent: Vec<RaceSummary>,
    pub server_now: i64,
}

/// How a player's race went, as stored: what the places are made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaceResult {
    pub user_id: i64,
    pub finished_at: Option<i64>,
    pub revealed: bool,
    pub time_ms: Option<i64>,
    pub flag: Option<Flag>,
}

impl RaceResult {
    /// Solved, not given up, not flagged: it gets a place.
    pub fn placed(&self) -> bool {
        self.finished_at.is_some()
            && !self.revealed
            && self.flag.is_none()
            && self.time_ms.is_some()
    }
}

/// The places of a race: the ones who solved it without a flag, by time with penalties, then who
/// finished first. One place each.
pub fn race_places(results: &[RaceResult]) -> HashMap<i64, u32> {
    let mut ranked: Vec<(i64, i64, i64)> = results
        .iter()
        .filter(|r| r.placed())
        .filter_map(|r| Some((r.time_ms?, r.finished_at?, r.user_id)))
        .collect();
    ranked.sort_unstable();
    ranked
        .iter()
        .enumerate()
        .map(|(i, &(_, _, user))| (user, u32::try_from(i + 1).unwrap_or(u32::MAX)))
        .collect()
}

/// When a started race is over, if it is at `now`: the time ran out ([`RACE_MS`] after the start),
/// or nobody is racing any more (everyone `done` or not seen for [`AWAY_MS`]). `players` are
/// `(done, last_seen)`.
pub fn race_over(now: i64, starts_at: i64, players: &[(bool, i64)]) -> Option<i64> {
    let deadline = starts_at + RACE_MS;
    if now >= deadline {
        return Some(deadline);
    }
    let racing = players
        .iter()
        .any(|&(done, seen)| !done && now - seen < AWAY_MS);
    (now >= starts_at && !racing).then_some(now)
}

/// The progress shown of a player: what they reported, at most [`PROGRESS_CAP`] until a
/// verified finish, which is 100.
pub fn shown_progress(reported: i64, solved: bool) -> u8 {
    if solved {
        100
    } else {
        u8::try_from(reported.clamp(0, i64::from(PROGRESS_CAP))).unwrap_or(0)
    }
}

/// The browser's estimate of the server's clock, from the times the server sent (`server_now`).
///
/// A reply's server time was taken somewhere between sending the request and receiving the
/// reply; the middle is the best guess, and the quicker the round trip the better the guess. So
/// the sample with the quickest round trip so far wins: a slow first reply (a server waking up)
/// is replaced by the next quick one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockSync {
    /// Server clock minus browser clock.
    pub skew: i64,
    /// The round trip of the sample `skew` is from, `None` before the first.
    pub rtt: Option<i64>,
}

impl ClockSync {
    /// Takes a reply sent at `sent` and received at `received` (browser clock), with the server's
    /// time `server_now`. Returns whether the estimate changed.
    pub fn sample(&mut self, sent: i64, received: i64, server_now: i64) -> bool {
        let rtt = (received - sent).max(0);
        if self.rtt.is_some_and(|best| rtt > best) {
            return false;
        }
        self.rtt = Some(rtt);
        self.skew = server_now - (sent + rtt / 2);
        true
    }

    /// The server's clock at the browser's `now`.
    pub fn server(&self, now: i64) -> i64 {
        now + self.skew
    }
}

/// What the countdown shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Countdown {
    /// More than three seconds to go: 用意 (get ready).
    Ready,
    /// 3, 2, 1.
    Count(u8),
    /// The first second of the race: 始め.
    Go,
    /// Not before the start, nor just after it.
    Hidden,
}

/// The countdown at the server time `now` of a race that starts at `starts_at`.
pub fn countdown(now: i64, starts_at: i64) -> Countdown {
    let left = starts_at - now;
    match left {
        l if l > 3_000 => Countdown::Ready,
        l if l > 0 => Countdown::Count(u8::try_from((l + 999) / 1_000).unwrap_or(3)),
        l if l > -1_000 => Countdown::Go,
        _ => Countdown::Hidden,
    }
}

/// How long until a race's page asks the server again, at the server time `now`.
///
/// In the lobby
/// every 2 s; in the countdown every second, and right at the start for the puzzle; while it runs
/// every second (a player still without the puzzle sooner); after it, every 2 s for the rematch.
pub fn poll_delay_ms(view: &RaceView, now: i64, have_puzzle: bool) -> u32 {
    let ms = match RacePhase::at(now, view.starts_at, view.ended_at) {
        RacePhase::Lobby | RacePhase::Ended => 2_000,
        RacePhase::Countdown => {
            let until = view.starts_at.unwrap_or(now) - now + 50;
            until.clamp(50, 1_000)
        }
        RacePhase::Running if view.joined() && !have_puzzle => 250,
        RacePhase::Running => 1_000,
    };
    u32::try_from(ms).unwrap_or(1_000)
}

/// The group page's card asks this often.
pub const CARD_POLL_MS: u32 = 10_000;

/// Which progress goes with the next request: a new value, once a [`PROGRESS_EVERY_MS`] passed
/// since the last one went.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProgressReport {
    /// The last value sent, and when (browser clock).
    sent: Option<(u8, i64)>,
}

impl ProgressReport {
    /// The progress to send at `now`, if any: `value` if it changed and the last one went long
    /// enough ago. Counts it as sent.
    pub fn take(&mut self, now: i64, value: u8) -> Option<u8> {
        match self.sent {
            Some((last, _)) if last == value => None,
            Some((_, at)) if now - at < PROGRESS_EVERY_MS => None,
            _ => {
                self.sent = Some((value, now));
                Some(value)
            }
        }
    }

    /// The request with the last value failed: it goes again with the next one.
    pub fn failed(&mut self) {
        self.sent = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(starts_at: Option<i64>, ended_at: Option<i64>, now: i64) -> RaceView {
        RaceView {
            id: 1,
            group_id: 1,
            group_name: "Club".into(),
            me: 2,
            host: Some(1),
            difficulty: Difficulty::Easy,
            penalties: HelpPenalties::default(),
            created_at: 0,
            starts_at,
            ended_at,
            players: vec![
                player(1, RaceOutcome::Racing),
                player(2, RaceOutcome::Racing),
            ],
            puzzle: None,
            solution: None,
            rematch: None,
            rematch_open: false,
            server_now: now,
        }
    }

    fn player(user_id: i64, outcome: RaceOutcome) -> RacePlayer {
        RacePlayer {
            user_id,
            name: format!("p{user_id}"),
            guest: false,
            colour: 0,
            joined_at: user_id,
            progress: 0,
            away: false,
            hints: 0,
            checks: 0,
            flag: None,
            outcome,
        }
    }

    #[test]
    fn a_race_goes_from_the_lobby_through_the_countdown_to_its_end() {
        assert_eq!(RacePhase::at(5, None, None), RacePhase::Lobby);
        assert_eq!(RacePhase::at(5, Some(10), None), RacePhase::Countdown);
        assert_eq!(RacePhase::at(10, Some(10), None), RacePhase::Running);
        assert_eq!(
            RacePhase::at(10 + RACE_MS - 1, Some(10), None),
            RacePhase::Running
        );
        // Over when the time ran out, noted or not, or when the server noted it
        assert_eq!(
            RacePhase::at(10 + RACE_MS, Some(10), None),
            RacePhase::Ended
        );
        assert_eq!(RacePhase::at(20, Some(10), Some(15)), RacePhase::Ended);
    }

    #[test]
    fn the_host_starts_with_two_players() {
        let mut v = view(None, None, 0);
        assert!(!v.can_start(), "only the host");
        v.me = 1;
        assert!(v.can_start() && v.is_host() && v.joined());
        v.players.pop();
        assert!(!v.can_start(), "alone");
        let started = view(Some(10), None, 20);
        assert!(!started.can_start());
        assert!(started.racing());
        assert_eq!(started.deadline(), Some(10 + RACE_MS));
        assert_eq!(view(None, None, 0).expires_at(), LOBBY_MS);
    }

    #[test]
    fn places_go_by_time_then_finish_without_flags_or_giving_up() {
        let result = |user_id, time: Option<i64>, at: Option<i64>| RaceResult {
            user_id,
            finished_at: at,
            revealed: false,
            time_ms: time,
            flag: None,
        };
        let mut flagged = result(5, Some(1_000), Some(1_000));
        flagged.flag = Some(Flag::TooFast);
        let mut gave_up = result(6, Some(500), Some(500));
        gave_up.revealed = true;
        let places = race_places(&[
            result(1, Some(90_000), Some(60_000)),
            result(2, Some(60_000), Some(70_000)),
            // As fast as 1, but finished later
            result(3, Some(90_000), Some(80_000)),
            // Still racing
            result(4, None, None),
            flagged,
            gave_up,
        ]);
        assert_eq!(places.len(), 3);
        assert_eq!(
            (places[&2], places[&1], places[&3]),
            (1, 2, 3),
            "{places:?}"
        );
    }

    #[test]
    fn a_race_is_over_when_nobody_races_or_the_time_is_up() {
        let start = 100_000;
        let now = start + 30_000;
        // Someone still racing and seen
        assert_eq!(
            race_over(now, start, &[(true, now), (false, now - 5_000)]),
            None
        );
        // The one racing is away: over
        assert_eq!(
            race_over(now, start, &[(true, now), (false, now - AWAY_MS)]),
            Some(now)
        );
        // Everyone done
        assert_eq!(race_over(now, start, &[(true, 0), (true, 0)]), Some(now));
        // Before the start nothing ends, whoever is away
        assert_eq!(race_over(start - 1, start, &[(false, 0)]), None);
        // The time is up, ended at the deadline
        assert_eq!(
            race_over(start + RACE_MS + 5, start, &[(false, start + RACE_MS)]),
            Some(start + RACE_MS)
        );
    }

    #[test]
    fn progress_stays_below_full_until_a_verified_finish() {
        assert_eq!(shown_progress(-3, false), 0);
        assert_eq!(shown_progress(42, false), 42);
        assert_eq!(shown_progress(100, false), PROGRESS_CAP);
        assert_eq!(shown_progress(250, false), PROGRESS_CAP);
        assert_eq!(shown_progress(10, true), 100);
    }

    #[test]
    fn the_clock_follows_the_quickest_round_trip() {
        let mut clock = ClockSync::default();
        assert_eq!(clock.server(1_000), 1_000);
        // Sent at 1000, back at 1200: the server's 5100 was taken around 1100
        assert!(clock.sample(1_000, 1_200, 5_100));
        assert_eq!((clock.skew, clock.rtt), (4_000, Some(200)));
        assert_eq!(clock.server(2_000), 6_000);
        // A slower one doesn't count
        assert!(!clock.sample(3_000, 4_000, 99_000));
        assert_eq!(clock.skew, 4_000);
        // A quicker one does
        assert!(clock.sample(5_000, 5_040, 9_050));
        assert_eq!((clock.skew, clock.rtt), (4_030, Some(40)));
        // A clock going back makes no negative round trip
        assert!(clock.sample(7_000, 6_990, 11_000));
        assert_eq!((clock.skew, clock.rtt), (4_000, Some(0)));
    }

    #[test]
    fn the_countdown_says_ready_then_three_two_one_go() {
        let start = 10_000;
        assert_eq!(countdown(start - 5_000, start), Countdown::Ready);
        assert_eq!(countdown(start - 3_001, start), Countdown::Ready);
        assert_eq!(countdown(start - 3_000, start), Countdown::Count(3));
        assert_eq!(countdown(start - 2_001, start), Countdown::Count(3));
        assert_eq!(countdown(start - 2_000, start), Countdown::Count(2));
        assert_eq!(countdown(start - 1, start), Countdown::Count(1));
        assert_eq!(countdown(start, start), Countdown::Go);
        assert_eq!(countdown(start + 999, start), Countdown::Go);
        assert_eq!(countdown(start + 1_000, start), Countdown::Hidden);
    }

    #[test]
    fn pages_ask_often_when_it_matters() {
        let lobby = view(None, None, 0);
        assert_eq!(poll_delay_ms(&lobby, 0, false), 2_000);
        // The countdown: every second, and right after the start
        let start = 10_000;
        let counting = view(Some(start), None, 0);
        assert_eq!(poll_delay_ms(&counting, start - 4_000, false), 1_000);
        assert_eq!(poll_delay_ms(&counting, start - 400, false), 450);
        assert_eq!(poll_delay_ms(&counting, start - 50, false), 100);
        // Running: soon while the puzzle is still to come, every second then
        assert_eq!(poll_delay_ms(&counting, start + 10, false), 250);
        assert_eq!(poll_delay_ms(&counting, start + 10, true), 1_000);
        // Watchers don't wait for a puzzle
        let mut watching = view(Some(start), None, 0);
        watching.me = 9;
        assert_eq!(poll_delay_ms(&watching, start + 10, false), 1_000);
        let over = view(Some(start), Some(start + 60_000), 0);
        assert_eq!(poll_delay_ms(&over, start + 70_000, true), 2_000);
    }

    #[test]
    fn progress_goes_out_when_it_changed_once_a_second() {
        let mut report = ProgressReport::default();
        assert_eq!(report.take(0, 0), Some(0));
        // The same again: nothing
        assert_eq!(report.take(5_000, 0), None);
        assert_eq!(report.take(5_000, 10), Some(10));
        // Changed, but too soon
        assert_eq!(report.take(5_500, 20), None);
        assert_eq!(report.take(5_999, 20), None);
        assert_eq!(report.take(6_000, 20), Some(20));
        // A failed request: the value goes again
        report.failed();
        assert_eq!(report.take(6_100, 20), Some(20));
    }
}
