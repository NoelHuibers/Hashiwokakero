# Repository review fixes

This document describes the security and consistency guarantees introduced by the repository
review fixes, along with the deployment requirements.

## Deployment

Before deploying the website in production, configure `HASHI_DAILY_SECRET` and `DOJO_SECRET`
with independent random values of at least 32 characters. If `DOJO_URL` is set, `DOJO_TOKEN`
also needs at least 32 characters, matching the worker. Missing/empty/development secrets now
stop production startup. Existing nonempty production seeds should be preserved: rotating them
changes future puzzles and hidden benchmark sets.

`HASHI_ADMINS` must refer to existing registered accounts. Create a normal account first, then
assign its existing numeric user id and restart the server. Never reserve a future numeric id.
Startup verifies every configured grant. Legacy username grants remain supported for existing
owners, but those names cannot be registered or taken through a rename. Deleted account ids
remain private, empty tombstones and cannot be reassigned. Remove grants for deleted accounts
before restarting. Numeric grants are preferred to legacy username grants.

Deploy the worker and website from the same revision, worker first when deploying separately.
Requests and callbacks carry `dojo::EVALUATION_VERSION`, which includes the actual locked
Wasmtime version. The website refuses mismatched reports and hides incompatible results.
Housekeeping requeues submissions from previous evaluators and invalidates their credentials.
The worker's benchmark-cache filenames include the evaluation version, so the first job after
an evaluator update regenerates the set. Allow time for this warm-up.

Bump the generator/scoring parts of `EVALUATION_VERSION` whenever benchmark generation, sandbox
configuration, metering, or scoring changes; engine patch updates are recorded automatically.
A season number alone is insufficient to identify comparable results.

Migrations take a database write transaction before checking the current schema version. Schema
changes and their migration records commit together; an interrupted migration rolls back and
can be retried. Back up production data before a deployment that changes the schema, and test
restoring that backup independently. For local SQLite, use a consistent backup or stop the
server before copying the database; copying only the main file while WAL writes are pending
can miss committed data.

## Execution and request limits

The WASI sandbox uses asynchronous host functions, an epoch deadline for running Wasm, and an
outer timeout that also cancels blocked host calls. Validation bounds module size, types,
locals, functions, and operators before full validation or compilation. Runs additionally have
fuel, memory, table, stdout, and wall-time budgets. The worker admits at most two pending jobs.

Public puzzle work and password hashing have separate bounded pools. A permit stays inside the
blocking closure until the computation actually exits, including after HTTP cancellation.
Waiting is bounded as well. Per-network request budgets precede expensive endpoints, and the
network is a parsed IPv4 address or IPv6 /64, not a textual prefix. These counters and CPU
pools are per website process; persistent daily/upload quotas and worker claims are coordinated
through the database.

Only the Fly-supplied `fly-client-ip` header is trusted when running on Fly, following the
[Fly request-header contract](https://docs.fly.io/networking/request-headers). Direct deployments
use the socket peer for admission; arbitrary `X-Forwarded-For` headers are ignored. Another
reverse proxy needs an explicit trusted-proxy integration rather than enabling arbitrary headers.
A local development server may set `LEPTOS_ENV=DEV` and `HASHI_RATE_LIMITS=off` for parallel
browser journeys from one loopback address. Production, including Fly, ignores that switch.

## Integrity and consistency

- The dispatcher claims a job and checks the global running count in one database statement
  before sending it. A fast completion is never overwritten by the HTTP acceptance response.
  Reports are guarded by both evaluation lifecycle and per-job token inside the update, so a
  requeue between authorization and application also rejects the old report. Terminal states
  invalidate their tokens. Late progress reports stop retrying when their credential is refused.
- Daily hint, check, reveal, and submit transitions share a write transaction from the running
  state read through the update. Finishing uses one authoritative penalty snapshot and verifies
  the update succeeded. Device reuse checks, submission/contact quotas, and guest merges are
  also atomic.
- Generation never returns a known-ambiguous candidate. If its bounded search cannot prove a
  candidate unique, it uses the small proven-unique fallback. Daily publishing separately checks
  the uniqueness contract.
- Daily identity ignores empty borders, translation, spacing of occupied rows/columns, rotation,
  and reflection. This closes the server solver/CNF shortcut. Client move logs and device ids
  remain abuse heuristics; they cannot prove that a person rather than an external solver played.
- Hidden solver inputs contain no family/index/answer comments, and randomness is seeded from
  formula content instead of public names. Names remain only in host-side reports.
- `Puzzle::validate` centralizes geometric invariants before solving/canonicalization and storage
  restoration. `Board::check` rejects malformed bridge counts or vector lengths without indexing
  outside the vector. The parser and solver both accept valid 2×2 puzzles.
- DPLL tests duplicate literals and tautologies with the review's exact regression. Branch
  and backtrack conflicts are handled immediately; assigned variables cannot be overwritten by
  a queued unit, and pure literals use the correct signed value. Property tests compare results
  and models against brute force.

## Client and read paths

Solver/generator responses check their puzzle generation before updating reports, busy state,
solutions, or toasts. Editing/replacing a puzzle invalidates earlier requests. A public-profile
request also checks that its route still names the requested player before publishing a response.

Public statistics reads do not award stamps, remove awards, or backfill logs. Reconciliation
happens on changes and in background work, including Dojo-only participants. Public aggregate
snapshots have a 30-second TTL and a maximum of 512 entries; writes invalidate them locally.
Every profile request still revalidates the current identity/deletion in the database. Partial
indexes cover ranked attempts. Contact history uses pages of 100 messages and a stable id cursor,
with an explicit control to fetch older messages.

Interactive boards are keyboard-focusable. In play mode, arrows select a possible bridge,
Enter/Space increases its count, and Shift+Enter/minus decreases it. In the editor, arrows move
between cells, 1–8 sets the number, and 0/Delete clears the cell. Live descriptions announce
coordinates, current/required island degrees, and bridge counts. Instructions are translated
in every supported locale. Editor hover shortcuts ignore text inputs and modifier shortcuts.

## Regression coverage

Focused tests cover blocking WASI sleeps, compilation complexity, cancelled CPU work, migration
rollback/retry and starting replicas, stale/replaced callbacks, completion before dispatch
acceptance, evaluator upgrades, concurrent daily penalties, immutable account ids, reserved admin
names and grants, padded daily identity, malformed puzzle/vector inputs, unique fallback,
read-only profile awards, paginated contact history, and obsolete/disposed solver requests.
Browser journeys cover keyboard play and editing in addition to the repository's existing flows.
