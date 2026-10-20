# Historical stamp record query

The stamp record now starts from one player's eligible attempts. Existing
indexes supply finisher counts, strictly better ranking keys, the
first finish, and adjacent solve times. This avoids running
multiple windows over every finisher on every day that player has played.

`ROW_NUMBER` in the original query gives distinct places even when both solve
time and finish timestamp are identical. A strict correlated rank count alone
would change those places. For any such day involving the requested player, the
query retains the original windows, including their unspecified tie order, rather
than introducing a new tie-break. Legacy NULL solve times also take that path;
NULL times belonging to other finishers still precede non-NULL times in the rank.
The query remains one statement, including its fallback, with no cache or writes.
Only needed attempt fields are materialized, excluding logs and bridge payloads.

## Correctness

`tests/fixtures/stamp-record-original.sql` freezes the pre-change SQL as an independent
oracle used by `src/server/daily/stamp_query_tests.rs` and the diagnostic. Tests
compare all 13 output columns (including both directional gaps,
place, first finish, and run/daypart), not just the nearest gap or stamp count.
Fixtures cover tied times, identical finish timestamps, fully identical ranking
keys, shuffled insertion/user order, first/last/solo finishers, unfinished attempts,
reveals, review and ban flags, NULL legacy times, missing days, absent users,
ban/unban, flag restoration, identity reassignment, and query plans after ANALYZE.
A 32-player/73-day history is compared for every player after each mutation.
Decoded records and stamp progress also agree at day and month boundaries.
Existing daily tests cover stamp awards, sticky stamps, moderation revocation,
announcements, and background reconciliation.

## Reproduction

Run from this checkout; only fresh temporary databases are created. No server,
browser assets, credentials, or production data are required.

```sh
CARGO_BUILD_JOBS=2 cargo test --locked -p hashi --lib --no-default-features --features ssr stamp_query
# Bounded smoke/equivalence run: five modes, 32 players × 20 days, three samples each.
CARGO_BUILD_JOBS=2 cargo run --locked -p hashi --no-default-features --features ssr --example stamp_record_query
# Configurable bounded run, with the pinned engine's plans for the first workload.
CARGO_BUILD_JOBS=2 cargo run --locked -p hashi --no-default-features --features ssr --example stamp_record_query -- 32 20 3 --explain
# Reproduce the nine original workload sizes and five samples (up to 365,000 attempts).
CARGO_BUILD_JOBS=2 cargo run --locked -p hashi --no-default-features --features ssr --example stamp_record_query -- --baseline
```

The standalone example requires the `ssr` feature and uses the pinned production
libSQL library with `db::open_local` and `db::migrate`, not Python's SQLite build.
It creates uniquely owned temporary directories and removes the database/WAL files
after connections close, including on an assertion failure. It does not consult the
application's database settings. The server and diagnostic both `include_str!` the
same private production asset, `src/server/daily/stamp_record.sql`; the unit tests
use the server's constant. No optimized SQL is copied into the diagnostic, and no
benchmark API is exposed. Its only assertions are result equivalence;
there are no timing thresholds. The baseline uses five alternating warm runs,
which include statement preparation and reading every result column. Setup, seeding, and equality assertions
are outside the measured interval. Every measured output is checked against the
original query. The deterministic distinct-time data matches the earlier review's
`(user * 173 + day * 97) % 1000000 + 60000` generator.

Normal unit-test runs contain no ignored or environment-skipped benchmark. The
manual timing workload is an example target and does not run during `cargo test`.
The configurable form takes three positive integers: players, days, and sample
count, and runs all five modes; append `--explain` to print both first-workload plans.
The bounded default uses the same sparse-day selection (days 0, 180, 364), so only
day 0 remains when fewer than 181 days are seeded. With even sample counts the
reported speedup uses the upper middle sorted sample. Use the five-sample baseline
for comparisons with the table below.

The refined example was smoke-tested with `32 20 3 --explain` on libSQL 0.10.0 /
SQLite 3.47.0: all 13 columns matched in all five modes and every measured run.
The pinned plan used `attempts_user_ranked` for `mine`, `attempts_day_ranked` for
tie checks/counts/neighbors (including time bounds on neighbor lookups), and
`attempts_device (day=?)` for first-finish aggregation on this small fixture.
The latter still checks all eligibility predicates; index selection is the
planner's choice, not a guarantee that every subquery uses the partial rank index.
The small exact-tie workload measured about 8.50 ms original versus 9.20 ms
optimized, illustrating the extra fallback/preparation work on small histories.
This smoke run does not replace the nine-workload baseline below.

## Measurements (2026-10-20, round one)

The table and raw JSON preserve the original **round-one ignored-test harness**
measurements; they have not been replaced with results from the extracted example.
The SQL tokens and frozen oracle are unchanged; extraction only adds formatting
and comments. The new example retains the seed, workload mutations, migrations, alternating query order,
and equality checks (including the original current-thread Tokio runtime), but
runs as a debug example rather than the test-profile binary, so timing comparisons across harnesses need fresh measurements.

Pinned libSQL 0.10.0 reports SQLite 3.47.0. These are local debug/test-build
measurements on a shared 30 GiB Linux desktop; other worktrees were active. Absolute
timings differ from the earlier Python SQLite 3.51.2 review benchmark. All nine
workloads returned identical rows in every run. Medians of five warm samples:

| Players × days | Workload | Rows | Original ms | Optimized ms | Speedup |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 × 1 | distinct | 1 | 0.425 | 0.900 | 0.47× |
| 10 × 30 | distinct | 30 | 5.008 | 1.773 | 2.82× |
| 100 × 180 | distinct | 180 | 265.184 | 29.695 | 8.93× |
| 1000 × 180 | distinct | 180 | 2072.945 | 191.042 | 10.85× |
| 1000 × 365 | distinct | 365 | 4735.486 | 435.714 | 10.87× |
| 1000 × 365 | sparse | 3 | 34.703 | 4.033 | 8.60× |
| 1000 × 180 | equal times | 180 | 2280.925 | 214.495 | 10.63× |
| 1000 × 180 | exact ties | 180 | 2130.907 | 2130.047 | 1.00× |
| 1000 × 180 | large logs | 180 | 2427.449 | 225.911 | 10.75× |

[Raw sorted samples and methodology](stamp-record-query-results.json) are committed.
Sparse participation retains only days 0, 180, and 364 for the requested player;
other players keep their complete histories. Equal-time groups contain up to ten
players with distinct finish timestamps. The exact-tie workload gives the requested
player identical ranking keys to another player on every day, forcing the original
windows. Large logs add a 50 KB log and 2 KB bridge payload to each requested-player
attempt, demonstrating that the materialized input omits those fields.

The solo-attempt regression is about **0.475 ms** of additional preparation/work.
The larger statement is retained to preserve exact tie behavior in a single database
round trip. No additional query or cache was added to route this sub-millisecond
case; already at ten players over thirty days, the measured query is 2.82× faster.
The exact-tie workload preserves the original cost rather than promising a speedup.

## Limits and follow-ups

Review of the tuple/NULL behavior: `day` is NOT NULL, so the ambiguous-day
`NOT IN` cannot eliminate rows through a NULL day. A non-NULL own time counts
NULL competitor times as earlier ranks; excluding them from the predecessor
lookup still returns NULL for the gap when only NULL times precede it. Exact ties
among other players do not affect this player's place or neighbor *time*. NULL
own times and exact ties involving this player retain whole-day window partitions.
The fallback repeats the original window definitions and eligibility predicates;
its `ROW_NUMBER` tie order remains unspecified, as in the original query. A future
engine/index change should rerun the oracle tests rather than assume a stable new
tie-break policy.

These are local synthetic SQL measurements, not production or end-to-end daily
info latency. Remote region/network latency, concurrent load, and monthly champion
aggregation are excluded. Daily count/first-finish work still grows with daily
participation. Exact ties deliberately retain the old window cost; small boards
may expose the extra statement preparation cost. NULL finished solve times retain
the old query behavior rather than being repaired here.

Historical month champions, global profile-cache invalidation, and coalescing daily
generation remain follow-ups. Any future cache must account for moderation, bans,
identity merges, and cross-replica updates before treating historical ranks as
immutable. This change requires no schema, API, frontend, or move-log changes.
