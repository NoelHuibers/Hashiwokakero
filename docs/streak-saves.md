# Streak saves

A missed daily doesn't have to end a streak. Players earn **saves** by playing, and a save
lets them **catch up** a daily they missed: they solve it a day or two late, unranked, and it
counts for the streak.

## Rules

- **Earning.** Every 7th day in a row earns a save, at most 2 held at once. Only days from
  [`SAVES_FROM`](#rollout) on count, so nobody starts with saves.
- **Missing a day.** The streak stays open while the days missed since the last counted day
  are no more than the saves held and all lie within the last 2 days. Today is never missed
  before it ends.
- **Catching up.** The daily page offers the oldest missed day: "You missed Tuesday: catch up
  (1 of 2 saves)". The player solves that day's daily. When they finish, one save is spent and
  the day counts: **+1** to the streak, as any solved day.
- **Too late.** Once a gap is longer than the saves held, or its oldest day is more than 2
  days back, the streak breaks. Unspent saves stay.

Details:

- **Oldest first.** Only the oldest missed day of the open gap can be caught up, so a save is
  never spent on a day after a gap that can't be filled.
- **Spent at the finish.** Starting a catch-up and leaving costs nothing. The finish is
  checked again: if the window closed meanwhile it doesn't count and no save is spent.
- **No catch-up after "show solution" or a flag.** A day with a revealed or flagged run can't
  be caught up (otherwise "show solution, then catch up" gets around the rule). A day started
  and not finished can.
- **Unranked, private.** A catch-up has no rank, medal, monthly points, banzuke place, rival
  head-to-head, race or public replay. The player sees their own time.
- **Streaks only.** Caught-up days count for the current and best streak, the Streak 7/30/100
  stamps, the hall of fame streaks and the `best_streak` leaderboard on Google Play and Game
  Center. Everything else (solved count, other stamps, times, averages) stays ranked-only.
- **Earning includes caught-up days.** A caught-up day is a day of the streak, so it can be
  the 7th of a run and earn the next save.
- **Copying.** Yesterday's replays are public, so a catch-up could be copied. With at most one
  save a week and nothing ranked at stake, that's accepted.

## How it's computed

Saves aren't stored. They follow from the counted days, walked in day order, by one pure
function in `model/calendar.rs` (it replaces `streaks()`):

```
streak_state(ranked, caught_up, today, saves_from) -> StreakState {
    current, best,   // as today
    saves,           // held now, 0..=2
    missed,          // the open gap that can still be caught up, oldest first, else empty
}
```

- `ranked`: days with a ranked finish (as today: finished, not revealed, not flagged);
  `caught_up`: days from `catch_ups`. A day is in at most one of them.
- Walking the days: a day right after the previous counted day extends the run, any other
  starts a new one. On days from `saves_from` on, every 7th day of the run (counted from
  `saves_from` or the run's start, whichever is later) earns a save up to 2; a caught-up day
  spends one where it lies. Saves carry over when a run ends.
- The open gap: the latest run of missed days before today. It stays open if all its days
  lie in `today - 2 ..= today - 1`, none of them has a revealed or flagged run, and it isn't
  longer than the saves held. Its days are `missed`, and the current streak is the run before
  it plus the counted days after it (a gap that isn't open breaks the run, as now).
- Every reader goes through this function: `UserStats` (record tiles, profile), rivals,
  stamps, games leaderboards, the catch-up checks and the last call. The hall of fame ranks
  everyone in one SQL query: it takes the union of ranked and caught-up days, so its best
  streaks are exact; its current streak skips the open gap (at most 2 days, a tie-breaker).

## Storage

Catch-ups have their own table instead of a column on `attempts`: some 66 queries read
`attempts` and decide what's ranked with `revealed = 0 AND flag IS NULL`, and any that missed
a new column would leak late solves into rankings.

```sql
CREATE TABLE IF NOT EXISTS catch_ups (
    user_id     INTEGER NOT NULL,
    day         INTEGER NOT NULL,   -- the daily caught up
    started_at  INTEGER NOT NULL,
    finished_at INTEGER,            -- NULL while playing
    time_ms     INTEGER,
    PRIMARY KEY (user_id, day)
);
```

Account deletion deletes the player's rows, guest merging moves them like `attempts`, guest
cleanup removes them with the guest. The privacy page lists them with the daily results.
A finish invalidates the player's cached profile.

## Shown

- **Calendar and recent results:** a caught-up day shows its time without a rank and a 補
  ("make up") stamp (`DayResult::caught_up`).
- **Streak tile:** the saves held as small marks (●○), with a tooltip.
- **Daily page:** with an open gap, the catch-up offer above or after today's daily.
- **Last call** ([notifications](notifications.md)): goes to players whose streak is open,
  not only to those who solved yesterday, when today's daily isn't done or the open gap closes
  at the end of today. With an open gap the text says so: "Catch up Tuesday to keep your
  12-day streak".

## Rollout

`SAVES_FROM: Option<i64>` in `model/calendar.rs`. `None` behaves exactly as before (no saves,
no gap stays open). The first PR adds everything behind `None`; the second, which makes catch-ups
playable, sets it to the UTC day it ships. History before that day never changes.

1. **Core:** `streak_state` with tests, `catch_ups` table, deletion/merge/cleanup, every
   reader switched over (stats, rivals, stamps, games leaderboards, hall of fame, last call
   eligibility). No visible change.
2. **Catch-up:** server functions to start and finish a catch-up (checks above, solution
   verified on the server), daily page offer and unranked play, 補 on the calendar, saves on
   the streak tile, last call text, all languages, e2e. Sets `SAVES_FROM`.
