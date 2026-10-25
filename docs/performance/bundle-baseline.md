# Bundle size and hydration baseline

What a browser downloads for the site, and how long its pages take to become interactive, so a
change that makes either worse shows up in CI instead of on a phone.
[bundle-baseline.json](bundle-baseline.json) holds the numbers; the "Bundle size" job of CI
compares every build with it and writes both tables to its job summary.

## What is measured

- **Size** (`scripts/bundle-size.mjs`): every WASM, JS and CSS file in `target/site/pkg` of a
  release build, by its name without the content hash. Raw, gzip (level 9) and brotli (quality
  11), the best each compression does; the server compresses on the fly
  (`tower_http::CompressionLayer`), possibly a little less. The fonts aren't counted, a page only
  loads the subsets its text needs.
- **Hydration** (`e2e/perf/hydration.spec.ts`): the time from navigation to the `data-hydrated`
  mark that App sets once the WASM has hydrated the server's HTML (`mark_ready` in `src/lib.rs`),
  the moment the page reacts to clicks. A `MutationObserver` turns the mark into a performance
  mark, so nothing in the app changes for it. The daily (home), the practice board and the ranks,
  each loaded cold (a new browser context, nothing cached) five times with the CPU
  slowed down 4× through DevTools (`Emulation.setCPUThrottlingRate`), after one load to warm up
  the browser; the median counts. The server is the browser tests' release build on their seeded
  database, on the same machine, so the network adds next to nothing.

## The gate

The job fails when the gzipped WASM and JS together grow more than `tolerancePercent` (5 %) over
the baseline. CSS is reported but not gated, it's small. `BUNDLE_TOLERANCE=<percent>` overrides
the tolerance for one run.

The hydration times are only reported, never fail the job: a shared CI runner's speed varies
from run to run, and a slow run would fail builds that changed nothing. Read them as a trend.
The job is not a required check and not in Release compatibility, so it holds up no merge or
deploy; a red "Bundle size" says the bundle grew, look before merging.

## Baseline

Recorded 2026-10-25 on `9606899` (main after #130) by the "Bundle size" job of CI: the release
build of "Build site (browser)" (cargo-leptos 0.3.11), hydration in the Playwright container on an
`ubuntu-24.04` runner, the CPU slowed down 4×.

| Asset | Raw | gzip | brotli |
| --- | --: | --: | --: |
| hashi.wasm | 11,984 KB | 3,890 KB | 2,627 KB |
| hashi.js | 35.2 KB | 9.7 KB | 8.4 KB |
| hashi.css | 106.3 KB | 18.8 KB | 15.7 KB |
| **WASM + JS** | 12,019 KB | 3,900 KB | 2,635 KB |
| **total** | 12,126 KB | 3,919 KB | 2,651 KB |

| Page | Hydrated (median of 5) | Range | Local, 16-core desktop |
| --- | --: | --: | --: |
| `/` (daily) | 943 ms | 920–1,117 ms | 880 ms |
| `/practice` | 1,064 ms | 853–1,122 ms | 872 ms |
| `/ranks` | 777 ms | 720–802 ms | 672 ms |

The WASM is nearly all of it: 12 MB raw, about 2.6 MB over the wire with brotli. A local build
came out within 0.5 KB of CI's. The hydration times are on loopback; on a phone, with a real
network, the download of the WASM comes on top.

## Updating the baseline

When the bundle grew (or shrank) on purpose, store the new numbers and commit them with the
change that caused them, so the review sees why:

- From CI (preferred, the baseline's numbers are CI's): download the `bundle-size` artifact of
  the pull request's "Bundle size" job and copy its `bundle.json` over
  `docs/performance/bundle-baseline.json`. It has the sizes and the hydration times of that run.
- Locally, on a release build:

  ```sh
  cargo leptos build --release
  pnpm run perf:hydration          # optional: the hydration times, e2e/test-results-perf/
  pnpm run bundle-size --update    # writes docs/performance/bundle-baseline.json
  ```

  A local build's WASM differs from CI's by a few bytes (the paths compiled into it), and a
  local machine's hydration times aren't CI's: without `perf:hydration` the baseline keeps the
  times it had.

Without `--update`, `pnpm run bundle-size` only compares (`--site <dir>` for another build's
site, e.g. CI's `dist/site`); its report lands in `target/bundle/summary.md`.
`PERF_RUNS` and `PERF_CPU` change the number of loads per page and the CPU slowdown of
`perf:hydration`.
