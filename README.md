# Hashi 橋をかけろ — Bridges Puzzle

Play [Hashiwokakero](https://en.wikipedia.org/wiki/Hashiwokakero) in the browser, in cute Japanese pixel art. Every puzzle is generated with a unique solution, proven by a SAT solver, and you can let our own CDCL and DPLL solvers (written in the SAT solving lab, WS 2023/24) solve, hint and check it in different modes.

- **遊ぶ Play** – generated puzzles in five difficulties (Easy 7×7 … Oni 24×24) and a daily puzzle. Build bridges by clicking the water between islands or dragging from an island, undo/redo, hints and checks from the SAT solution, timer and progress, saved in the browser.
- **作る Create** – a puzzle workshop: place islands, import/export the `.txt` format, check solvability and uniqueness with the SAT solver, download the CNF, and play your puzzle.
- **研究 Lab** – race one puzzle through many solver configurations (CDCL with VSIDS/arbitrary branching, no/fixed/geometric/Luby restarts, subsumption; DPLL with eight branching heuristics; splr) and compare time, decisions, restarts and SAT calls.
- **遊び方 Rules** – the rules and how the SAT encoding works.

The frontend is [Leptos](https://leptos.dev) 0.8 (SSR + hydration) with Tailwind CSS v4 and [rust-ui](https://rust-ui.com) components, the shadcn/ui port for Leptos (copied into `src/components/ui/` like shadcn intends). The board and all sprites are pixel art drawn as ASCII in `src/components/sprites.rs`.

## Folder Structure

- **src**: The Leptos web app, a folder per area with a `mod.rs` that re-exports what the rest of the app uses:
  - `app.rs` the routes, `pages/<page>/` one folder per page (its cards and widgets as files), `components/` shared components (the board, layout, leaderboards, rust-ui in `ui/`)
  - `state/` client state (game, settings, toasts, solver calls), `api/` server functions per area, `model/` types shared by server and browser
  - `server/` server only code: database, accounts, the daily (`server/daily/`) and the SAT Dojo (`server/dojo/`)
- **style**: `tailwind.css`, the Tailwind v4 entry point and the theme (day 昼 / night 夜).
- **public**: Static assets (favicon).
- **common**: Types shared by the frontend and the backend: puzzle, board geometry and validation, solver/generator options and results.
- **backend**: Encoding to CNF, solving (splr or our own solvers), uniqueness checks, the generator and the CLI.
- **sat_solver**: Our own DPLL/CDCL SAT solver from the SAT solving lab.
- **dojo**: The SAT Dojo: benchmark sets, the WebAssembly sandbox and the worker that scores submitted solvers. `dojo/house` builds our own solvers as entries, `dojo/starter` is a small solver to start from.
- **locales**: The translations, one folder per language with a JSON file per page.
- **e2e**: Browser tests of the built site (Playwright), see [Browser tests](#browser-tests).
- **solutions**: The .cnf and solution .txt files for all initial test problems.

## Running the web app

Requirements: Rust stable (installed automatically via `rust-toolchain.toml`, incl. the `wasm32-unknown-unknown` target), Node.js 24+ (26 recommended, see `.node-version`) , [pnpm](https://pnpm.io) (the version in `packageManager` in `package.json`) and [cargo-leptos](https://github.com/leptos-rs/cargo-leptos):

```sh
cargo install --locked cargo-leptos   # or: cargo binstall cargo-leptos
pnpm install                          # Tailwind, tw-animate-css and the fonts (public/fonts)
cargo leptos watch                    # http://127.0.0.1:3000 (or pnpm run dev, see below)
```

cargo-leptos runs the Tailwind CLI from `node_modules` (so its version is the one in `package.json`) and installs the wasm-bindgen CLI matching `Cargo.lock` by itself.

The fonts are self-hosted, no request goes to Google Fonts: `pnpm install` runs `scripts/fonts.mjs`, which copies the woff2 files of the `@fontsource` packages to `public/fonts` and writes their `@font-face` rules (`pnpm run fonts` does it again).

A release build: `cargo leptos build --release`, the server binary is `target/release/hashi` and serves `target/site`.

### Lighter local builds

The translations are the heaviest code to compile: `i18n/build.rs` generates one module from all of `locales/` (20 languages, about 7 MB of code), and the compiler works through it on one core. They live in a crate of their own, `hashi_i18n` (`i18n/`), which only compiles again when `locales/` changes, not with every edit of the website. A small edit then takes `cargo leptos build` (server and browser) 17 s instead of 25 s, a build of the website crate from scratch 36 s instead of 67 s when the translations are already built. To keep the rest in check:

```sh
pnpm run dev                          # cargo leptos watch, 4 compiler jobs, at most 10 GB
HASHI_LOCALES=en,ja pnpm run dev      # only English and Japanese translated, a lighter build
pnpm run dev --hot-reload             # arguments go to cargo leptos watch
```

- `pnpm run dev` runs `cargo leptos watch` with `CARGO_BUILD_JOBS=4`, and, where `systemd-run` is there, in a systemd scope with `MemoryMax=10G` and no swap: a build that needs more fails (the compiler is killed) instead of pushing the desktop into swap. `CARGO_BUILD_JOBS` and `HASHI_DEV_MEMORY` (e.g. `14G`, or `infinity`) override them.
- `HASHI_LOCALES` (comma separated, English is always in) works with every cargo command, also without `pnpm run dev`. The other languages stay in the language switcher and their URLs work, but they show English. Changing it rebuilds both crates. With `en`, `cargo leptos build` of both from scratch took 45 s instead of 61 s, a small edit 13 s and 1.3 GB instead of 17 s and 1.9 GB. Leave it unset for the tests (some check that every language is translated) and for anything you ship; CI, release and Docker builds never set it and build every language.
- cargo-leptos compiles debug builds with `--cfg erase_components`, which keeps Leptos' view types small. A plain `cargo build --no-default-features --features ssr` doesn't: its server binary took the compiler over 10 GB, with any `HASHI_LOCALES`. Prefer `pnpm run dev`/`cargo leptos build` for running the site.
- Debug builds have only line tables for our crates and no debug info for dependencies (`[profile.dev]` in `Cargo.toml`): panics and backtraces still name file and line, and the compiler and the linker need less (a first debug build of the server: 2:50 instead of 3:11 minutes, 3.2 instead of 3.7 GB, 3.5 instead of 6.7 GB in `target/`). For a debugger session with variables: `CARGO_PROFILE_DEV_DEBUG=full`.

Every checkout and git worktree has its own `target/`, so each compiles all dependencies again. [sccache](https://github.com/mozilla/sccache) shares them: it caches the compiler's output per crate, and a checkout that needs a dependency another one already compiled (same version, features, profile and flags) copies it from the cache. It's opt-in, for your user, not set in the repository (CI caches its own way):

```sh
cargo binstall -y sccache             # or cargo install sccache
```

```toml
# ~/.cargo/config.toml: every cargo command, in every checkout
[build]
rustc-wrapper = "sccache"
```

`RUSTC_WRAPPER=sccache` in the shell's profile does the same. `RUSTC_WRAPPER= cargo …` (empty) turns it off for one command, and switching it on or off doesn't rebuild anything already in a `target/`. In a fresh target dir with a warm cache, `cargo check -p hashi --no-default-features --features ssr` took 55 s instead of 118 s, `cargo build -p backend` 3.2 s instead of 9.6 s, the same from another checkout. The first build that fills the cache is about as fast as one without it (126 s). It can't cache our own crates (incremental builds), proc macros, or running build scripts, so the website crate and the translations still compile as before; it pays off for the first build of a new worktree, after `cargo clean`, and after a toolchain update (only the first checkout compiles the dependencies again). `sccache --show-stats` shows the hits.

The cache lives in `~/.cache/sccache` (`SCCACHE_DIR` moves it) and holds at most 10 GB by default, evicting the least recently used entries; `SCCACHE_CACHE_SIZE=20G` (in the environment of the first cargo command, which starts sccache's server) changes the limit. Each profile, target (server, browser, tests, coverage) and flag set has entries of its own, a check of the server's dependencies alone took 250 MB. `sccache --stop-server` stops the server, it also stops on its own after 10 idle minutes.

`target/` grows with every build, and every worktree has its own. `pnpm run clean` lists what can go, with sizes, and deletes nothing; `pnpm run clean --yes` deletes it (`pnpm clean` without `run` is pnpm's own command):

- huge fingerprint outputs (`target/*/.fingerprint/*/output-*` over 50 MB, `--min-output-mb`): the compiler's messages, replayed when a crate is fresh;
- incremental caches except the newest of each crate and profile;
- artifacts in `target/*/deps` neither written nor read for 30 days (`--days`), like cargo-sweep;
- with `--worktrees`, git worktrees whose branch is merged into origin/main, with their target dirs. Never one with uncommitted or untracked changes, and never one without commits of its own.

It only deletes inside target dirs that cargo made (with its `CACHEDIR.TAG`): this checkout's (or `CARGO_TARGET_DIR`), or the ones given with `--target DIR`. Cargo rebuilds whatever it needs again.

### Browser tests

`e2e/` has [Playwright](https://playwright.dev) tests of the built site: every page in every language loads, hydrates and reacts without an error in the browser, the 404 page, and journeys through the site, each as a player of its own: practice from generating to the win screen (with the mouse, and by tapping on a phone), the daily as a guest and with a new account up to the leaderboard and the stamp book, signing in, deleting an account, the workshop, the lab, the contact form, the settings, the Dojo and the admin dashboard. The daily's solution comes from the server's database, the page never has it. Each run starts the server binary on port 3150 with a fresh database in `e2e/.run/`, seeded by `e2e/seed.ts` (players with a few weeks of dailies, signed in sessions, a Dojo entry, contact messages).

```sh
pnpm exec playwright install chromium  # once, the browser
cargo leptos build --release
pnpm run e2e                          # pnpm run e2e --ui, or -g "practice" for some tests
```

`E2E_BIN` and `E2E_SITE_ROOT` point it at another build (e.g. `target/debug/hashi`), `E2E_PORT` at another port. A failed run leaves a report with traces (`pnpm exec playwright show-report e2e/playwright-report`) and the server's log in `e2e/.run/server.log`.

`e2e/tests/visual.spec.ts` compares a few pages with screenshots (`e2e/tests/__screenshots__/`): the home page, a practice board, the ranks, the rules in Japanese and Arabic, and a daily's result. They catch what breaks the layout without an error, like a Tailwind or font update. Screenshots only match where they were taken, so they are taken and compared in the official Playwright image of the version in `pnpm-lock.yaml`, in CI and locally (podman or docker, the image is pulled on first use), with a server and database of their own on the next port:

```sh
pnpm run e2e:container                # every test, the screenshots included
pnpm run e2e:update-snapshots         # take the screenshots again, after a change on purpose
```

`pnpm run e2e` outside the container leaves them out. CI also builds the deploy image (`Dockerfile.prebuilt`) from its binary and site and checks that it serves the site (`scripts/docker-smoke.sh`).

## Deployment

Pushes to `main` deploy to [fly.io](https://fly.io) once CI passes: CI builds the site and the dojo worker with its warm cache and ships them in slim Debian trixie images (`Dockerfile.prebuilt`, `dojo/Dockerfile.prebuilt`), built on the runner. The multi-stage `Dockerfile` (Node 26 and pnpm for the npm packages, Rust 1.99 + cargo-leptos for the build, Debian trixie slim at runtime) builds everything from source, for a manual `fly deploy` or a local image. Dependabot keeps Cargo, npm (pnpm-lock.yaml), Docker and GitHub Actions dependencies up to date, CI tests and lints every pull request and runs the browser tests on its release build, in two shards side by side (one HTML report, the `e2e-report` artifact). A pull request that only changes docs (`*.md`, `*.pdf`, `LICENSE` files) skips the lints, tests and builds; their checks show as skipped, which counts as passed.

## SAT Dojo

At [hashi.rs/dojo](https://hashi.rs/dojo) anyone can submit a SAT solver compiled to WebAssembly (WASI preview 1). Solvers read DIMACS on stdin, answer in the SAT competition format and are scored by *fuel*, the number of executed WebAssembly instructions, on a hidden set that changes every season. A run is fully deterministic: clocks and random numbers are seeded, so the same solver always gets the same score.

```sh
cargo build -p house --release --target wasm32-wasip1           # our own solvers as .wasm
cargo run -p dojo --release --features worker -- run target/wasm32-wasip1/release/splr.wasm
cargo run -p dojo --release --features bench -- export dev-set/  # the public dev set as .cnf files
```

The website stores submissions and sends them to the worker (`dojo serve`, its own Fly app in `dojo/fly.toml`), which starts on demand, evaluates and reports back. See `src/server/dojo.rs` and `dojo/src/worker.rs` for the settings.

## Backend CLI

The backend can also be used from the command line:

`cargo run --release --package backend -- --mode [MODE] --input [FILEPATH] --output [FILEPATH] --solver [SOLVER]`

### MODE

|Flag|Alias|Functionality|
|-|-|-|
|encode||Reads a .txt file containing an ASCII hashi puzzle and writes the CNF (DIMACS, with comments naming the variables) to `<input>.cnf`|
|solve||Reads a DIMACS CNF file and solves it with the chosen SAT solver (prints or writes the model)|
|encodesolvereconstruct|esr|Reads a .txt puzzle, encodes and solves it in memory (with lazy connectivity), checks whether the solution is unique and prints the solved puzzle plus solver statistics|
|generate||Generates a puzzle with a unique solution (`-x`/`-y` grid size, `-d` difficulty, `--seed`), prints it with its solution or writes the puzzle to `--output`|

### SOLVER

|Solver|Functionality|Options|
|-|-|-|
|splr|The [splr](https://crates.io/crates/splr) crate (default for the CLI)||
|cdcl|Our own CDCL solver from `sat_solver` (SAT Solving lab WS 2023/24)|`--heuristic vsids\|arbitrary`, `--restarts none\|fixed\|geometric\|luby` (`--threshold N`, `--factor PERCENT`), `--subsumption`|
|dpll|Our own DPLL solver from `sat_solver`|`--heuristic vsids\|arbitrary\|dlis\|dlcs\|mom\|boehm\|jeroslaw_wang\|custom`|

`--timeout MS` limits the solving time (default 60000). Restart policies: `fixed` restarts every
`threshold` conflicts (default 100), `geometric` grows the threshold by `factor` percent after each
restart (default 100 / 150 %), `luby` restarts after `threshold * luby(i)` conflicts (default 32).

### Generator

|Difficulty|Default size|What the player needs (measured with a human-style deduction solver)|
|-|-|-|
|easy|7x7|basic deductions only (island capacities, isolation of 1-1 / 2=2, no crossing)|
|medium|10x10|mostly basic deductions, some connectivity reasoning|
|hard|14x14|large parts undecided by basic deductions, some trial and error|
|expert|18x18|more trial and error|
|oni|24x24|the hardest of the generated candidates, often beyond one step of lookahead|

The same seed and parameters always give the same puzzle. Uniqueness is shown by a backtracking
search and confirmed by the SAT solver. See `backend/src/generator.rs` for the algorithm.

### Examples

From the repository root:

```sh
cargo build --release -p backend
# Solve test1 with our CDCL (VSIDS, Luby restarts) and print the solution
target/release/backend -m esr -i backend/input/test1.txt -s cdcl -H vsids -r luby
# Encode a puzzle, then solve the CNF with DPLL
target/release/backend -m encode -i backend/input/test8.txt
target/release/backend -m solve -i backend/input/test8.txt.cnf -s dpll -H dlis
# Generate a hard 14x14 puzzle
target/release/backend -m generate -d hard --seed 42
```

### Tests and benchmarks

```sh
cargo test -p common -p backend -p sat_solver
# Slow or timing tests (generator at default sizes, timing, statistics, perturbed puzzles)
cargo test --release -p backend -- --ignored --nocapture
# Criterion benchmarks (encoding, solving with all solvers, generating), in backend/
cargo bench
# Line coverage of the tests CI runs (cargo binstall cargo-llvm-cov first), report in
# target/llvm-cov/html/index.html. CI fails when it drops more than a percentage point below
# scripts/coverage-baseline.json, `pnpm run coverage --update` stores new numbers there.
pnpm run coverage
```

See [Evaluation](hashi_report.pdf)

## SAT Conversion

See [Clauses](Clauses.md)

See [repository review fixes and deployment requirements](docs/repository-review.md).

## License

[MIT](LICENSE) © Noel Huibers
