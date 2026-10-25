# Coalescing concurrent identical generations

The first requests of a new day all need today's daily. `daily::get_or_create` found no stored
puzzle for each of them, and each generated it again: the same parameters, the same puzzle,
then `INSERT OR IGNORE` kept the first. Every one of those generations also took a slot of the
puzzle CPU pool (`limits::PUZZLES`: 2 running, 16 waiting, 5 s to start), so a burst of cold
`start_daily` requests made players wait for copies of one computation and then refused some of
them with `too_many_attempts`.

Concurrent requests for the same generation now share one computation
(`server::coalesce::Flights`). Two places use it:

- **The daily** (`daily::get_or_create`), keyed by its `GenerateParams`. Only the generation is
  shared, a pure function of the parameters since f36de4e (work budget, no wall clock). Every
  request still stores the result itself (`INSERT OR IGNORE`, then reads the stored row): the
  owner's connection may be inside a transaction, and another machine may have stored first.
- **Open Graph images** (`og::cached`), keyed by `Image`: the previews of a shared link are
  fetched by several apps at once. The owner puts the PNG into the existing image cache before
  its flight ends, and a new owner looks into the cache again before rendering, so a request
  that missed the cache just before a rendering ended doesn't render again.

## What is not coalesced

- `generate_puzzle` (practice, editor, lab): the browser picks a random 64-bit seed for every new
  puzzle (`state::random_seed`), and there are no shared seed links. Two different players never
  ask for the same parameters, so coalescing would only add bookkeeping. Results of one player
  are never handed to another.
- Group puzzles, kadai and the Dojo don't generate on the server: they check uniqueness of
  puzzles players post (`solve_unique`), which differ per request.
- The decoration board of a day's OG image is the same in every language. Each language's image
  generates it again (a 12×10 board, a small part of a render); the image as a whole is
  coalesced and cached.
- Public profile statistics keep their own cache and flights (PR #96, `daily::stats`). They
  could move onto `Flights` plus their TTL/invalidation layer later; this change leaves that
  code alone.

## Lifecycle and limits

The design of the profile statistics (see [profile-cache-coalescing.md](profile-cache-coalescing.md)),
without a cache of results:

- The first request of a key owns the computation, later ones follow it through a `watch`
  channel. No task is spawned and no synchronous mutex spans an await.
- Owner cancellation removes its flight and closes the channel: the followers wake, one becomes
  the new owner and computes with its own closure. A cancelled follower cancels nothing.
  (The cancelled owner's blocking job still runs to its end in the CPU pool, as before.)
- Results and errors go to everybody waiting at that moment and are not kept. The next request
  for the key computes again, or, for the daily, finds the puzzle in the database. An owner
  refused by the CPU pool (`too_many_attempts`) refuses its followers too; before, each of them
  queued separately and most were refused the same way.
- At most 16 daily and 64 image keys are in flight. A request for a new key at capacity computes
  alone, as before; keys in flight can still be joined. Daily keys only exist for days somebody
  plays (today, and older days with an attempt), image keys for 20 languages × the days up to
  tomorrow.
- A small window remains for the daily: a request that read "no puzzle" after the owner's flight
  ended but before any request's insert committed generates once more. The result is still the
  stored puzzle.

## Measurements

Measured 2026-10-25 (a Sunday: the Oni 24×24 daily, the most expensive one) on `9606899` (before)
and this branch (after), with `examples/generation_burst.py`: a debug build of the server
(`cargo leptos build`; the generator, `sat_solver` and `splr` are built with opt-level 3 in dev,
the rest of the server isn't), a fresh database per run, rate limits off, before and after runs
interleaved. The machine is a 16-core desktop that ran other builds at the same time (load 7–13);
the server's CPU time is its process's user + system time during the burst.

| Burst | Before: wall / server CPU | After: wall / server CPU | Before: answered |
| --- | ---: | ---: | --- |
| 20 concurrent cold `start_daily` (5 runs) | 5.87 s / 11.68 s | 1.15 s / 1.17 s | 60 of 100 (40 `too_many_attempts`) |
| 1 cold `start_daily` (3 runs) | 0.91 s / 0.89 s | 0.88 s / 0.87 s | 3 of 3 |
| 10 concurrent cold OG images (3 runs) | 5.93 s / 11.66 s | 2.79 s / 2.77 s | 12 of 30 (18 errors) |
| 1 cold OG image (2 runs) | 2.81 s / 2.79 s | 2.81 s / 2.80 s | 2 of 2 |

Medians; after the change every request was answered in every run, and all answers of a run
were the same puzzle (before too). Before, the 20-request burst ran about 12 generations of
~0.95 s, two at a time, until the remaining requests' 5 s wait for a CPU slot ran out. After,
it runs one, and the burst costs what a single cold request costs. A single request is
unchanged.

The OG render is slow here because the card code is unoptimized in a debug build; in release
it is much cheaper, but the ratio (one render instead of up to ten) is the same. These numbers
show duplicated work and refusals on cold bursts; they are not production latencies (the
production VM has one shared CPU, where duplicated generations cost proportionally more), and
how often such bursts happen in production is unknown.

[Results](generation-coalescing-results.json) keep every run. Reproduce from the repository root:

```sh
cargo leptos build
cp -r target/site ~/burst-site   # a later build replaces target/site
python3 -I examples/generation_burst.py target/debug/hashi ~/burst-site 3171 20 5 ~/burst-scratch
python3 -I examples/generation_burst.py target/debug/hashi ~/burst-site 3171 10 3 ~/burst-scratch \
  /og/daily/$(date -u +%F)/en.png
```

The day's difficulty decides the cost of a daily generation (Monday Easy … Sunday Oni).

## Validation

`server::coalesce` tests drive the flights with manually polled futures and gates: ten
overlapping requests share one computation and nothing stays behind; another key computes while
one is paused; a cancelled owner hands over to a follower, and a cancelled follower cancels
nothing; an error reaches every waiter and the next request retries; at capacity a new key
computes alone while keys in flight can still be joined. `daily` tests run eight concurrent
`get_or_create` calls on separate connections of one database: one generation, and every
request gets the stored puzzle; a failed generation fails the waiting request too, stores
nothing, and the next request generates. An `og` test renders one image once for six concurrent
requests and keeps it. All of these fail with coalescing switched off.
