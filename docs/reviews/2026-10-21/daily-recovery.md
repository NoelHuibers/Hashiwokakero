# Daily recovery implementation

Base: `f66f556`. Branch: `fix/daily-recovery`. Parent integrated current main (`b994bfe`), preserving its win modal, rivals, grouped rankings and About content. Matching checks are pending. No general play layout or Dojo changes.

## Behavior and ownership

- Metadata and ranking have independent pending/error states and named, translated Retry actions. Failed refreshes retain the existing board/ranking. Focus/visibility never retries a failed metadata request; a manual retry dispatches once, with a synchronous pending guard.
- Fetch generations supersede old identity/day requests and reject disposed owners. Metadata retains the full `RefreshGuard` check, including paid-action revision/counters. A submit advances that revision before dispatch so an older quiet read cannot overwrite it. Ranking is independent of metadata/counter changes, but a saved finish supersedes an older pending ranking response.
- Submission freezes the original log, move count and UTC offset in page state after the synchronous final-move flush. A failed write makes one authoritative recovery read. Unknown, mismatched, or still-running replies keep the solved board locked and expose Retry result. A retry first reads the exact attempt; only a successful running reply permits a write. There are no timers/effects that retry an error.
- Bounds: first dispatch is at most one write plus one recovery read; each explicit retry is at most one read, one write, and one recovery read. Double clicks cannot create overlapping dispatches. A lost successful response can be recovered entirely by the read.
- A reload restores a locally solved but server-unfinished board locked with Retry result, rather than unlocking it or automatically resubmitting. Sharing and claims of leaderboard membership wait for confirmed completion. If the same attempt was revealed elsewhere, authoritative recovery shows that outcome.
- Start callbacks also carry a generation, so an obsolete completion cannot clear a newer operation's busy flag. Auth loading/identity is memoized: a refresh of unchanged identity cannot restart a failed submission.

## Why additive server endpoints

The old `submit_daily` commits the finish before rank/finisher/stamp queries. Failure after commit is therefore ambiguous. Its legacy replay response is AlreadyFinished, and cookie identity can change across tabs without the displayed board changing. Client-only retries cannot establish ownership or assume a second write is safe.

`daily_attempt_info(day, owner)` and `submit_daily_attempt(..., owner)` assert both authenticated user and stable attempt ID. The recovery read validates ownership and reads the captured day under one transaction, including across UTC midnight. Guarded submission checks stable identity under the existing immediate write lock. It replays a saved, non-revealed finish only for the same attempt and bridges, returning stored time/current rank without rewriting finish, log, moves, penalties, run or daypart. Concurrent replays serialize. Legacy submit, hint/check, response DTOs, saved-log fixtures and paid-intent storage/wire formats are unchanged.

## Validation

Final commands/results are recorded in the implementation handoff. All Rust checks use `CARGO_BUILD_JOBS=2`.

New focused regressions cover immediate fetch/submit guards, superseded generations/disposal, same-attempt reconciliation, wrong day/start/attempt/puzzle rejection, solved-board/log preservation, old quiet reads, identity changes, replay immutability and concurrent replay, stable-ID replacement, and HTTP user/attempt assertions. Existing paid-intent and synchronous final-move tests remain in the daily suite.

Browser cases added to `e2e/tests/daily.spec.ts` are **unexecuted** pending the parent's matching server/WASM build:

1. Initial metadata failure → manual retry, same-turn double click, no focus retry.
2. Independent ranking failure → ranking-only retry while preserving running progress.
3. Lost successful submit response + failed reconciliation + failed explicit read → later read-only confirmation, preserving board/log/penalties.
4. Unsent submission → exact-payload retry even if another tab rewrites the shared log key.
5. Submit response after navigation/disposal.
6. Transient metadata refresh failure while running → manual recovery with progress intact.
7. Hard reload of an unconfirmed solved board → locked explicit retry.
8. Solved SPA return, including an initial metadata failure and manual retry in the same document.

The existing final-paid-hint submission counter now observes the new guarded endpoint. No cargo-leptos, full release/server, coverage, or browser runs were performed here.

## Limits and tradeoffs

- Retry result intentionally performs a read before writing. An unavailable recovery endpoint blocks a write even if the earlier request never reached the server. Wrong/missing ownership or expired sessions retain a generic recovery error until the player restores the appropriate identity; server exception text is not displayed.
- The frozen submission payload is page-owned. Hard reload uses the existing saved board and daily log, whose storage remains best-effort and shared across tabs. A separate durable per-attempt submission journal/storage migration is outside this change: another tab overwriting that legacy log before a hard reload can still cause an unverifiable/unranked result. Within the live page, retries retain the original payload even if shared storage changes. Paid-action durable intents and their recovery rules are unchanged.
- Replay returns current rank/finisher counts, not a historical rank receipt. Stamp announcement delivery remains best-effort under the existing announcement semantics; a lost notification does not remove an earned stamp.
- All 20 locale files contain all seven recovery strings; wording has not had native-speaker review.

## Parent integration review

An independent QA review found metadata discovery could be invalidated by a submit with no
capturable attempt, or by a later scope effect on a day/identity transition. Submission now
requires loaded metadata before advancing its revision; metadata retry remains available when
shared game state is Won but metadata is absent. Reload clears previous metadata and synchronizes
scope before capturing the new read guard. This also prevents old-attempt auto-submission under
a newly authenticated user. First metadata establishes ranking-day ownership and supersedes
initial unknown-day requests; leaderboard responses must match the displayed day.

Successful guarded writes call main's `submitted()` helper and retain the win modal. Read-only
finish reconciliation behaves like a finish observed on page load, without inventing a win
receipt or making an extra write for animation. The superseded `unsent` retry machine and its
helper-specific tests were replaced by owned recovery; its browser scenario and modal tests
were retained and adapted. Focused regressions now also cover concurrent reveal and real SPA
return. The final static re-review found no remaining blocker in these changes.

Inherited follow-ups identified during review: group-ranking fetches still lack the world
ranking's ownership guards; same-puzzle board/log progress can carry across an account change.
The latter predates this change and supports guest transfers; resetting progress by stable
attempt ownership needs a separate design. Neither inherited behavior is claimed fixed here.
