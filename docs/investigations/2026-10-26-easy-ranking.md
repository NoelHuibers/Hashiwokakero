# Easy ranking and replay timing — 2026-10-26

Status: implementation complete; release validation and deployment in progress.
Shared hundredth places apply from 2026-10-26; server receipt timing applies from
2026-10-27. Earlier days retain their original ranking policy.

## Verified production results

Read the live daily leaderboard and the authenticated, read-only admin attempt
endpoint on 2026-10-26. Day 20752, Easy. Neither player used hints or checks;
neither attempt is flagged. Both logs contain eleven changes.

| Measurement | Noel | Nele |
| --- | ---: | ---: |
| Current place | 1 | 2 |
| Stored official time | 7,826 ms | 7,831 ms |
| Current leaderboard text | 0:07.8 | 0:07.8 |
| Final logged move | 7,297 ms | 7,161 ms |
| Official elapsed minus final logged move | 529 ms | 670 ms |

The actual ranking difference is **5 ms**, not a displayed-time tie broken by
who played first. The existing earlier-finish tie-breaker is used only when the
stored millisecond times are exactly equal.

The replay puts Nele's final move 136 ms earlier than Noel's. This does not prove
that she actually solved faster: the old log timestamps include clock-estimation
error and use a clock sampled by the display timer. The two gaps above are not
measurements of network latency alone.

## Causes in the current implementation

1. `src/server/daily/attempt.rs::submit_inner` measures elapsed from the server's
   recorded start until after acquiring a transaction and reading/validating the
   puzzle on submission. This includes initial puzzle delivery, submission
   transport, and server work/waiting. It is not just time spent solving.
2. `src/pages/daily/state.rs::clock_ms` reads `Daily.now + skew`. `Daily.now` is
   updated by a 100 ms interval in `src/pages/daily/mod.rs`. `record_moves` therefore
   timestamps changes with the most recent display tick, rather than reading the
   clock at the move. Browser scheduling can make that sample even older.
3. `Daily.apply` estimates skew as `info.server_now - browser_now()` when the
   response is applied. Response delivery is not compensated; refreshes can change
   the offset mid-attempt. Wall-clock changes can also affect this timeline.
4. `src/model/calendar.rs::format_duration` truncates to tenths below ten minutes.
   The admin log's `seconds` formatter instead rounds to tenths. Logs and scores
   consequently do not even use the same rounding convention.
5. `src/model/replay.rs::Lane::finish` uses the last log event, not the official
   finish. `finish_order` breaks equal event times by lane order. In
   `src/pages/replay/legend.rs`, the official time is only shown if there are
   penalties, hiding the discrepancy for today's clean solves.
6. Verification and stamp summaries use `server_times`, which holds backwards
   timestamps and compresses logs that exceed the server duration by more than
   15 seconds. Admin inspection uses the raw log; replay sorts it with `clean_log`.
   These are different interpretations of the same evidence.
7. Ranking policy is duplicated. Global daily, group daily, monthly points,
   profiles/medals and stamps use variants of time/finish ordering. Weekly group
   standings already share places for exact equal times. A leaderboard-only
   change would leave inconsistent rewards.

## Recommended target

### Server-authoritative scoring, accurate client diagnostics

The user's additional requirement is that a client cannot fake a faster time.
Keep ranked time entirely server-authoritative. Do not rank a submitted client
elapsed value, a browser finish timestamp, or the final move in its log. Never
subtract client-reported latency or ping from a score.

The timing contract should be:

- Start: a timestamp assigned by the server and persisted with the owned attempt
  **before the puzzle is released**. Reloads and retries must reuse it.
- Finish: server receipt of the **complete submission payload**, captured before
  waiting for the database or doing expensive validation. Only a correctly owned,
  fully validated solved submission may persist that finish. A failed or partial
  submission does not stop the clock.
- Ranked duration: `received_at - started_at + server_penalties`.
- The timestamp is an internal server value. Request JSON, browser clock, logs,
  headers and client-supplied IDs must never override it. Reject a receipt that
  predates the recorded start instead of accepting a negative duration.

**Do not timestamp the request's headers or connection opening.** An attacker
could open a request early, keep uploading while solving, and finish the body
later. Capture only once all scoring-relevant fields have arrived. In the current
Leptos server function, an internal timestamp at decoded-function entry is a
candidate seam; verify body extraction and app/web paths before relying on it.
Pass the captured value through internal functions, not through the public API.

Retain transactional ownership/attempt assertions, board and log validation,
server-authoritative purchases and penalties, and idempotent submission recovery.
Concurrent or delayed retries must preserve the first accepted result; they must
not reset the start, replace the finish, or select a client-provided earlier time.

For logs, sample a monotonic client clock at each action instead of reading the
100 ms display tick. Anchor its diagnostic timeline once to the attempt's known
server elapsed estimate; refresh must not move old timestamps. Reloads, background
sleep and wall-clock changes need tested continuation rules. These timestamps are
**diagnostic evidence only** and cannot lower the server-measured score. Preserve
raw events and report any reconciliation/normalization.

This removes avoidable server validation/lock delay from the score without
trusting the client. Initial puzzle delivery and submission transport still count.
There is no reliable way to remove that connection bias using only client-supplied
timing. Hundredths define scoring resolution, not a promise of 10 ms fairness.
Protecting the clock also does not prove every submitted log came from human
input: keep existing anti-automation checks and review as a separate defense.

### Hundredths and real shared places

Continue storing integer milliseconds. Introduce one integer centisecond score:

    score_cs = (server_elapsed_ms + server_penalty_ms + 5) / 10

This is nearest-hundredth rounding for nonnegative durations. Format exactly this
value and rank exactly this value; hidden milliseconds must not break displayed
ties. Equal scores share competition places: 1, 1, 3. Playing earlier in the day
must not win a tied score. An ID can stabilize row order without changing place.
Use the same policy across difficulties rather than making Easy obey different
rules; Easy simply benefits most from it. Inspect raw milliseconds in admin.

If applied to today's *existing server numbers*, both would be 0:07.83 and share
first. This is illustrative, not a reconstruction of their actual client solve
times. Hundredths improve readability and define the competition's resolution;
they are not a guarantee of 10 ms measurement accuracy.

Use fixed rounding buckets, not a rule that any two results within 10 ms tie:
pairwise tolerances can make ties non-transitive. Bucket boundaries still exist.

Shared places must also drive global/group boards, podiums (including more than
three medalists), profiles, medals, monthly points, weekly standings, rivals and
rank-dependent stamps. Tied players get identical rank-based rewards. Review
fixed top-50 limits so they do not arbitrarily discard part of a boundary tie.
Keep “first to finish that day” achievements separate from speed ranking.

### Honest log and replay displays

Admin should show separate, named values: raw/normalized move timestamp with
millisecond precision, logged completion, server elapsed, penalty and official score. Show when timing was reconciled or corrected. Preserve original evidence;
share the normalization policy used for verification rather than sorting a
backwards raw log into a different action sequence for replay.

Replay should show the official score for every finisher, including clean solves.
Move playback can follow the diagnostic log, but the official finish marker,
places and result must follow the server-measured score. A solved board may wait
for server acknowledgement in playback; label that interval rather than claiming
it is more solving. With penalties, distinguish board completion from the final
ranked result. Shared official places must not be assigned by lane order. Label
historical logged completion as approximate, with the official result alongside.
Do not stretch client logs into a claim of accurately measured human solve time.

## Delivery sequence

1. Fix timestamp capture and expose the discrepancies in admin/replay. Keep
   existing official times until the new score policy starts. Record the extra
   timing evidence needed to assess the discrepancy, without calling it measured
   network latency.
2. Implement one score/rank policy and update every consumer and tied podium.
   Add persisted timing-version metadata with a clear effective daily boundary.
3. Switch new days to the improved server receipt timing after validating full-body
   capture, concurrency, reloads and retries. Version the effective daily boundary
   so legacy days retain their recorded policy. Keep the web and installed app on
   the same server scoring contract. Preserve previously awarded results by default.

Do not retroactively replace today's scores with the last logged timestamps:
the logs are too imprecise and lack enough timing evidence to reconstruct the
true durations. A historical tie-policy change is a separate product decision,
with consequences for medals, points and sticky stamps.

## Acceptance checks

- Two moves inside one display tick receive their independently measured times.
- Refresh, wall-clock changes, background/sleep and reload do not shorten a solve
  or reorder already captured actions; sleep time counts.
- Client elapsed values, forged log timestamps, clock changes, headers, fake ping,
  unknown JSON fields and replay edits cannot lower the official score.
- Opening a request early and delaying its body does not backdate completion;
  finish is captured only after the entire payload arrives.
- Database waiting/validation does not inflate the captured receipt time. Transport
  delay still counts, and an identical retry preserves the accepted finish.
- Ownership/account changes, parallel submissions, attempts to submit before the
  start, unsolved boards and invalid logs cannot reset or freeze the clock early.
- Legacy days remain explicitly identified; web and app share the server timing
  contract, and clients cannot select a different policy.
- 7,826 and 7,831 ms both score 7.83; rounding boundaries, minute carry, penalties,
  negative/invalid input and display formatting use the same integer policy.
- Equal scores receive 1, 1, 3 and identical rewards everywhere, including ties
  across leaderboard limits and more than three podium finishers.
- Clean and penalized replays display official scores, distinguish playback
  completion from rank, and identify approximate legacy timing.
- Normalized replay reaches the verified submitted board while raw evidence stays
  accessible; invalid logs remain rejected and claimed client durations never enter scoring.

Implementation notes:

- Migration 22 adds server-derived virtual ranking keys and a partial index. It
  preserves all raw attempt timestamps, milliseconds and logs; today's correction
  follows from the policy, without special-casing either player.
- Server-function receipt is captured after full JSON body extraction and before
  authentication/database work. Client-supplied timing fields are ignored.
- Replays receive server-computed group places, use server elapsed for their finish
  markers, and show official scores. Raw move evidence stays visible in admin.
- Move logs use a monotonic action clock; refreshing does not backdate events, and
  sleep recovery can advance the diagnostic anchor. These times never set scores.
- The production backup was restored and checked for integrity. Applying migration
  22 to that restored database yields Noel and Nele at shared rank 1 while retaining
  their original 7,826 and 7,831 ms measurements.

Validation includes HTTP slow-body and forged-timestamp regressions, retries,
shared rewards and leaderboard boundary ties. Release results are recorded in the
pull request and deployment verification.
