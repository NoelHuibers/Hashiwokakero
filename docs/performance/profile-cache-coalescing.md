# Coalescing cold public-profile statistics

Concurrent cold requests for one public profile previously repeated the full statistics
computation. The existing mutex protected only cache lookup and insertion. The expensive
path includes personal ranks, current-month points, stamp history, and every historical
monthly champion.

The installed HTTP database now shares one in-progress `UserStats` computation per user
within coordination capacity. Each request still independently loads identity, rename,
guest status, deletion and ban fields. Domain `stats` and `profile` helpers remain uncached
and do not share results between independent databases.

## Lifecycle and limits

- Keep the original fixed 30-second TTL and 512 snapshots. Reads do not extend TTL.
- Allow at most 64 active coordinated owners, including obsolete owners after invalidation.
  New keys at capacity use the original uncached computation and add no flight metadata.
  Existing keys can still join. **This is not a limit on concurrent database work.**
- Invalidation prevents new requests joining obsolete work. Generation and owner identity
  prevent obsolete completion inserting a snapshot or removing a replacement flight.
- Publish completion independently of cache admission, including when snapshots are full.
- Retained watch state avoids missed notifications. Errors are shared with current waiters
  but not cached; later requests retry normally.
- The request owns its work. Owner cancellation removes its exact flight and wakes followers
  to elect a replacement. Cancelling a follower does not cancel the owner. No detached task
  or permanent per-user lock is created, and no synchronous mutex spans an await.

## Current-source SQL baseline

Measured 2026-10-21 on `939cf47`, Python SQLite 3.51.2, a synthetic 1,000-player × 180-day
history (180,000 ranked attempts, April 25–October 21). The script reads current rank and
month SQL literals and the merged `stamp_record.sql`; its embedded schema uses the
corresponding ranking indexes. Three warm repetitions were recorded for each query.

| Query | Median | Returned rows |
| --- | ---: | ---: |
| Personal ranks | 6.13 ms | 180 |
| Merged stamp record | 21.19 ms | 180 |
| Current month | 30.24 ms | 21,000 |
| Six closed-month champion queries combined | 229.56 ms | 159,000 |

The measured nine-query subset took **282.99 ms / 180,360 rows once**, versus
**2,871.61 ms / 1,803,600 rows for ten serial duplicate executions**. These demonstrate
repeatable duplicated SQL work. They are not concurrent HTTP or production latency
measurements, and production burst frequency is unknown. They exclude other queries,
Rust monthly aggregation/sorting, serialization and Turso network round trips.

[Original results](profile-cache-coalescing-results.json) preserve all recorded repetitions.
No timings were rerun or substituted while preserving the artifacts. The older October-20
review script constructs the old stamp window query; its ~342 ms result does not describe
the merged stamp query.

Reproduce from the repository root, using Python 3.9+ and its standard-library SQLite:

```sh
git rev-parse HEAD
python3 examples/profile_sql_baseline.py
```

The diagnostic creates only an in-memory database and prints JSON. It needs no application
server, credentials, third-party packages, network or Cargo build. Source-literal extraction
is deliberately small and will fail if the SQL moves; record the revision when comparing
results. Fixed distinct times avoid the stamp query's tie fallback; separate equivalence
fixtures cover ties and NULLs.

## Validation

The focused tests use local cache instances and controlled computation gates. They require
one expensive computation for ten overlapping same-key requests and compare complete
snapshots. They cover unrelated users, fixed TTL, invalidation, owner/follower cancellation,
registered wakeups, errors and recovery, full-cache publication, both capacity limits,
identity changes and independent domain databases. A real two-connection SQLite regression
reproduces stale snapshots during cancellation/re-election; finishing the identity query
before cache coordination prevents stale insertion into the new generation. An actual statistics fixture compares
cached HTTP assembly with uncached domain outputs.

Parent test/lint results and final CI are reported in the PR. These regressions establish
coalescing and lifecycle behavior; they do not establish production throughput or p95 latency.
