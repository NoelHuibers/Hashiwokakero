# Hashi 橋をかけろ — Bridges Puzzle

Play [Hashiwokakero](https://en.wikipedia.org/wiki/Hashiwokakero) in the browser, in cute Japanese pixel art. Every puzzle is generated with a unique solution, proven by a SAT solver, and you can let our own CDCL and DPLL solvers (written in the SAT solving lab, WS 2023/24) solve, hint and check it in different modes.

- **遊ぶ Play** – generated puzzles in five difficulties (Easy 7×7 … Oni 24×24) and a daily puzzle. Build bridges by clicking the water between islands or dragging from an island, undo/redo, hints and checks from the SAT solution, timer and progress, saved in the browser.
- **作る Create** – a puzzle workshop: place islands, import/export the `.txt` format, check solvability and uniqueness with the SAT solver, download the CNF, and play your puzzle.
- **研究 Lab** – race one puzzle through many solver configurations (CDCL with VSIDS/arbitrary branching, no/fixed/geometric/Luby restarts, subsumption; DPLL with eight branching heuristics; splr) and compare time, decisions, restarts and SAT calls.
- **遊び方 Rules** – the rules and how the SAT encoding works.

The frontend is [Leptos](https://leptos.dev) 0.8 (SSR + hydration) with Tailwind CSS v4 and [rust-ui](https://rust-ui.com) components, the shadcn/ui port for Leptos (copied into `src/components/ui/` like shadcn intends). The board and all sprites are pixel art drawn as ASCII in `src/components/sprites.rs`.

## Folder Structure

- **src**: The Leptos web app (`app.rs` routes, `pages/`, `components/` incl. the board and rust-ui components, `state.rs` client state, `api.rs` server functions).
- **style**: `tailwind.css`, the Tailwind v4 entry point and the theme (day 昼 / night 夜).
- **public**: Static assets (favicon).
- **common**: Types shared by the frontend and the backend: puzzle, board geometry and validation, solver/generator options and results.
- **backend**: Encoding to CNF, solving (splr or our own solvers), uniqueness checks, the generator and the CLI.
- **sat_solver**: Our own DPLL/CDCL SAT solver from the SAT solving lab.
- **dojo**: The SAT Dojo: benchmark sets, the WebAssembly sandbox and the worker that scores submitted solvers. `dojo/house` builds our own solvers as entries, `dojo/starter` is a small solver to start from.
- **locales**: The translations, one folder per language with a JSON file per page.
- **solutions**: The .cnf and solution .txt files for all initial test problems.

## Running the web app

Requirements: Rust stable (installed automatically via `rust-toolchain.toml`, incl. the `wasm32-unknown-unknown` target), Node.js 24+ (26 recommended, see `.node-version`) and [cargo-leptos](https://github.com/leptos-rs/cargo-leptos):

```sh
cargo install --locked cargo-leptos   # or: cargo binstall cargo-leptos
npm ci                                # Tailwind, tw-animate-css and the fonts (public/fonts)
cargo leptos watch                    # http://127.0.0.1:3000
```

cargo-leptos runs the Tailwind CLI from `node_modules` (so its version is the one in `package.json`) and installs the wasm-bindgen CLI matching `Cargo.lock` by itself.

The fonts are self-hosted, no request goes to Google Fonts: `npm ci` runs `scripts/fonts.mjs`, which copies the woff2 files of the `@fontsource` packages to `public/fonts` and writes their `@font-face` rules (`npm run fonts` does it again).

A release build: `cargo leptos build --release`, the server binary is `target/release/hashi` and serves `target/site`.

## Deployment

Pushes to `main` deploy to [fly.io](https://fly.io) with the multi-stage `Dockerfile` (Node 26 for the npm packages, Rust 1.99 + cargo-leptos for the build, Debian trixie slim at runtime). Dependabot keeps Cargo, npm, Docker and GitHub Actions dependencies up to date, CI tests and lints every pull request.

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
```

See [Evaluation](hashi_report.pdf)

## SAT Conversion

See [Clauses](Clauses.md)

## License

[MIT](LICENSE) © Noel Huibers
