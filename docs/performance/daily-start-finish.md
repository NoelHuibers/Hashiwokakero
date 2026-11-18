# The daily's start and finish

The daily is timed by the server alone: the clock starts when `start_daily` inserts the attempt
(`server/daily/attempt.rs`, after all the work the reply needs, see #173) and stops when the
solved board reaches it (`RECEIPT_TIMING_DAY`). Nothing the browser says moves either end. What
the browser can do is answer the player at once wherever the server isn't needed for that.

## Measured (2026-11-18)

Local release build, CPU 4× slower (`pnpm run perf:daily`, e2e/perf/daily.spec.ts), before the
changes below:

| Moment | Total | Server's reply | Browser |
|---|---|---|---|
| Start → board to play | 89 ms | 13 ms | 67 ms from the reply to the board |
| Last bridge → win card | 216 ms | 10 ms | 174 ms from the reply to the card |

The browser's part of the start is small. In production the start costs, on top of it, the
player's round trip to Frankfurt and the server's few queries (~20–30 ms), and after a while
without traffic also a new TLS connection (2–3 round trips) or a Fly cold start (1–2.7 s, see
issue #176). Only the round trip after the insert and the board's render count on the clock.

The finish waited for the whole round trip of the submit before the win card showed, and the
clock in the header and on the result card went on ticking meanwhile.

After them, same setup: start → board 79 ms (reply 16 ms, reply → board 57 ms), last bridge →
win card 36 ms, no longer waiting for the submit's reply (10 ms here, a round trip to
Frankfurt and the server's work in production).

## What the page does now

- **Finish.** When the board is solved, the tab notes the time on the move log's clock
  (`Daily::solved_ms`, set in `begin_submit`): the header, the result card and the win card show
  that time while the submit is on its way, and the win card shows at once with "Saving your
  result…" instead of the rank. The rank, stamps, sharing, the replay link and the offer of
  notifications come with the server's reply, whose own time then replaces the shown one (a few
  hundredths more, the time the board took to reach the server). A failed submit takes the
  card away and lets the clock run again, as the server will time the board when it gets it. A
  card the player put away during the submit stays away.
- **Start.** Pressing Start lifts the fog over the cover's empty board at once. Pointing at,
  focusing or touching the Start button sends `start_warm_up` (at most every 20 s), which wakes
  the server and opens the browser's connection to it and the server's to the database, so the
  start's request doesn't pay for them. The warm-up reads and changes nothing.

Not done, because each would give up part of the server's timing: showing the board before
the server started the clock, starting the clock when the browser says the board showed, or
taking a measured round trip off the time (a delayed request would make the round trip look
longer). Keeping a machine running all the time (`min_machines_running = 1` in fly.toml) would
take away the cold starts, at the cost of a machine that never stops.

## Guards

- e2e/tests/daily-speed.spec.ts holds every request the moment mustn't wait for: the board after
  Start for a signed-in player (the guest's is in daily.spec.ts), the win card and the stopped
  clock while the submit's reply is held, the warm-up on pointing at Start, and the board kept
  as it is through the hints and the finish.
- Unit tests in `pages/daily/actions.rs` (the clock and the win card around a submit) and
  `pages/daily/cover.rs` (how often the warm-up goes out).
- `pnpm run perf:daily` reports the times above; CI's Bundle size job puts them in its summary.
