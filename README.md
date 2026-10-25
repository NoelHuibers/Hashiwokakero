# Hashi 橋をかけろ — Bridges Puzzle

Play [Hashiwokakero](https://hashi.rs) in the browser with Japanese pixel art,
generated puzzles with a unique solution, and SAT-powered hints and checks.

- **Play:** five difficulties, a daily puzzle, saved progress, and leaderboards.
- **Create:** build puzzles, import/export text files, and check solvability and uniqueness.
- **Lab:** compare splr and our own CDCL/DPLL solvers with different configurations.
- **SAT Dojo:** submit a WebAssembly SAT solver and compete on seasonal benchmarks.

Built with Rust, [Leptos](https://leptos.dev), and Tailwind CSS. Fonts are self-hosted.

## Run locally

Install Rust through rustup, Node.js 24+ (see `.node-version`), and the pnpm version
in `package.json`. `rust-toolchain.toml` specifies the Rust targets and components.

```sh
cargo install --locked cargo-leptos
pnpm install
pnpm run dev
```

Open <http://127.0.0.1:3000>. For lighter development builds:

```sh
HASHI_LOCALES=en,ja pnpm run dev
```

English is always included; other omitted languages fall back to English. Leave
`HASHI_LOCALES` unset for tests and production builds. `CARGO_BUILD_JOBS` and
`HASHI_DEV_MEMORY` override the dev script's defaults of four compiler jobs and a
10 GB memory limit (where systemd is available).

Build a release with `cargo leptos build --release`. The server binary is
`target/release/hashi` and serves `target/site`. See [Deployment](docs/deployment.md)
for production configuration. The Android and iOS apps are planned in
[Mobile apps](docs/mobile.md).

`pnpm run clean` previews removable build artifacts; `pnpm run clean --yes` deletes
them. Optional `sccache` can share dependency builds between checkouts.

The server draws the social preview images (`src/server/og`) with fonts cut down to their
characters, committed in `src/server/og/fonts`. After changing a text on them, run
`pnpm run og-fonts`; the og tests fail until then.

## Tests

```sh
cargo test -p common -p backend -p sat_solver
cargo bench
pnpm run coverage                    # requires cargo-llvm-cov
```

Correctness tests, including default-size generation and solver regressions, run without
`--ignored`. Timing is measured by `cargo bench`; the optional generation report is:

```sh
cargo run --release -p backend --example generator_report
```

The Dojo integration tests require the same proof checkers as production. Build them once
(requires a C compiler, curl and sha256sum), then set their paths before running worker tests
or coverage:

```sh
dojo/checkers/build.sh "$HOME/.local/share/hashi/checkers"
export DOJO_DRAT_TRIM="$HOME/.local/share/hashi/checkers/drat-trim"
export DOJO_CAKE_LPR="$HOME/.local/share/hashi/checkers/cake_lpr"
cargo test -p dojo --features worker
```

Browser tests use a release build and a fresh test database:

```sh
pnpm exec playwright install chromium
cargo leptos build --release
pnpm run e2e
pnpm run e2e:container               # includes visual tests; requires Docker or Podman
pnpm run e2e:update-snapshots        # after intentional visual changes
```

`E2E_BIN`, `E2E_SITE_ROOT`, and `E2E_PORT` override the build and port used by the
browser tests. Inspect failed runs with `pnpm exec playwright show-report e2e/playwright-report`.

`pnpm run smoke:production` checks the deployed site (https://hashi.rs, `SMOKE_URL` for another)
in a browser: the main pages answer, hydrate and react, with all their assets. It is read-only,
it never makes a guest; CI runs it after every deploy and every 6 hours
(`.github/workflows/production-smoke.yml`).

## Backend CLI

Run these commands from the repository root:

```sh
cargo build --release -p backend
target/release/backend -m esr -i backend/input/test1.txt -s cdcl -H vsids -r luby
target/release/backend -m generate -d hard --seed 42
target/release/backend --help
```

Modes: `encode` writes a puzzle's static CNF, `solve` solves a DIMACS file, `esr`
solves a puzzle with connectivity and uniqueness checks, and `generate` creates a
puzzle. Solvers: `splr`, `cdcl`, and `dpll`. See the [SAT encoding](Clauses.md) and
the [standalone solver instructions](sat_solver/README.md).

## SAT Dojo

The [Dojo](https://hashi.rs/dojo) runs WASI preview 1 solvers on seasonal benchmark
sets. Scores use deterministic WebAssembly fuel; SAT answers require a model and
UNSAT answers require a proof. Completed seasons publish their benchmark sets and
results in the [season archive](https://hashi.rs/dojo/seasons).

Start with the [solver starter and protocol](dojo/starter/README.md). The
[calibration study](dojo/CALIBRATION.md) explains fuel scoring, and the
[research notes](dojo/RESEARCH.md) record the supporting sources.

## Repository layout

- `src/`: web app, shared UI, and server code.
- `common/`, `backend/`, `sat_solver/`: puzzle types, generation, encoding, and solvers.
- `dojo/`: benchmark tooling, worker, and solver starter.
- `locales/`, `style/`, `public/`: translations, styles, and static assets.
- `app/`: the mobile app's client-side rendered web bundle, see [app/README.md](app/README.md).
- `e2e/`: browser tests.
- `backend/input/`, `solutions/`: example puzzles and solver outputs.

The original project evaluation is in [hashi_report.pdf](hashi_report.pdf).

## License

[MIT](LICENSE) © Noel Huibers
