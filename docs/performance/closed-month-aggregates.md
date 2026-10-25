# Stored standings of closed months

A cold profile used to count the points race of every month the player has finished a daily
in, to find out whether they won it (the champion stamp): every ranked time of each month,
with window functions, on every cold request. A month that is over hardly ever changes, so
its standings are now counted once and stored in the database (migration 21,
`closed_months` + `month_standings`); later reads take the stored rows. The running month is
still counted on every read.

## Design

- **Persisted, filled lazily.** The first read of a month that is over (`month_of(today())`
  is later) counts it (`fame::count_points`, the code every read ran before) and stores the
  ordered list in the same `BEGIN IMMEDIATE` transaction. Both the champion (place 1) and the
  board of an old month (`month_board(Some(month))`) read from there. Turso is the database of
  every Fly machine, so one machine's count serves all of them; an in-process cache would
  count again on every machine and every deploy, and would need cross-machine invalidation.
- **Invalidation by triggers on `attempts`.** The standings depend only on the
  `user_id, day, finished_at, revealed, flag, time_ms` of the month's attempts. An `INSERT`,
  a `DELETE`, or an `UPDATE OF` one of those columns deletes the `closed_months` row whose day
  range holds the old or the new day; a trigger on `closed_months` takes its standings along.
  The next read counts again. This is in the same transaction as the change, whichever
  code path (or machine) made it, so no caller has to remember to invalidate.
- **No stale fill.** Counting and storing happen under the write lock. A change committed
  before the lock is in the count; one after it fires the trigger and drops the stored month.
  The read of stored standings is one statement (`closed_months LEFT JOIN month_standings`), so
  it never sees a month without its rows.
- **"Closed" is only an optimization.** Correctness doesn't depend on the clock: a machine
  whose clock is ahead may store the running month, and the next result in it drops it again.

Events that change a closed month, each covered by a test in
`src/server/daily/closed_month_tests.rs` against the full count:

| Event | Code path | What the trigger sees |
| --- | --- | --- |
| Late submission (started on the month's last day, finished after midnight) | `daily::submit` | `UPDATE OF finished_at, time_ms, flag` |
| Review flag / restore (`/admin/flag`) | `daily::set_flag` | `UPDATE OF flag` |
| Ban / unban | `admin::ban`, `admin::unban` | `UPDATE OF flag` (`'banned'`) on all their attempts |
| Account deletion / anonymization | `auth::erase` | `DELETE` of unranked attempts (ranked ones stay, unchanged) |
| Guest cleanup | `auth::cleanup_guests` | as deletion |
| Guest to account transfer | `auth::merge_guest` | `UPDATE OF user_id`, `DELETE` of the rest |
| Reveal | `daily::reveal` | `UPDATE OF revealed` |
| Any other insert, delete, time or day change | | `INSERT`, `DELETE`, `UPDATE OF time_ms, day` |

Renames (`admin::rename_player`, username changes) change no standings: they are stored by
user id and names are joined at read time. Neither do move logs, runs, dayparts, hints,
checks, receipts or device columns (penalties are in `time_ms` already); the tests check those
keep the stored months.

One behaviour change: players tied on points *and* on their first finish down to the
millisecond were ordered by `HashMap` iteration, a different order per request. They are now
ordered by user id, in both the count and the stored rows.

## Measurements

FILLED IN BELOW
