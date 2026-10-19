# Hashi SAT Dojo starter

A minimal DPLL solver that speaks the [SAT Dojo](https://hashi.rs/dojo) protocol. Copy this folder,
make the solver faster, and submit it.

```sh
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1
# the file to submit:
ls target/wasm32-wasip1/release/dojo-starter.wasm
```

## Protocol

- Input: a DIMACS CNF formula on **stdin**. There is no file system, network or environment.
- Output on **stdout**, as in the SAT competition:
  - `s SATISFIABLE` followed by the model on `v` lines, ending with `0`
  - `s UNSATISFIABLE`
  - `s UNKNOWN` if you give up
- Any language that compiles to WebAssembly with WASI preview 1 works: Rust, C/C++ (wasi-sdk), Zig, Go...

## Scoring

Every formula gets a budget of 10 billion *fuel*: one unit per executed WebAssembly instruction,
one per byte for bulk memory operations. Time and machine load don't matter, so scores are exactly
reproducible. Clocks and random numbers are deterministic too: `random_get` is seeded per formula
and set.

Score = PAR-2: the mean fuel per formula, where unsolved formulas count twice the budget.
**A single wrong answer disqualifies the submission**, so never guess UNSAT.

## Testing locally

The `dojo` tool runs your solver exactly like the leaderboard, on the public development set:

```sh
cargo install --git https://github.com/NoelHuibers/Hashiwokakero dojo --features worker
dojo run target/wasm32-wasip1/release/dojo-starter.wasm          # all families
dojo run my.wasm --family pigeonhole --stderr                      # one family, with your logs
dojo export dev-set/                                               # the formulas as .cnf files
dojo run my.wasm --set dojo-season-1.tar.gz                        # a past season's hidden set
```

Finished seasons are archived at https://hashi.rs/dojo/seasons: the seed of the hidden set (its
SHA-256 was published when the season started), the set itself and every result per formula.
