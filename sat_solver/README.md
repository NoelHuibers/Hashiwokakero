# DPLL Solver

## Running the SAT-Solver

Once built via `cargo build --release` the solver can be executed the following:

`./target/release/dpll_solver --mode [MODE] --input [FILEPATH] --heuristic [HEURISTIC] --depth [DEPTH]`

### MODE

Currently, only the DPLL mode is available.

### DEPTH

The depth flag can be set to true or false and determines whether the currently lowest backtracking depth is printed to the terminal. This gives some idea of the progress.

### HEURISTIC

Supported Heuristics are:

+ TODO

## Team Responsibilities

**Noel:**

- [x] Init
- [ ] CLI
- [x] Parser
- [ ] Edge cases for parser (e.G. newlines before 0 was found as in Zulip question)
- [ ] Random Restarts

**Laura:**

- [ ] Testing & Benchmarks
- [ ] Cactus plots
- [ ] Heuristics

**Flo:**

- [x] DPLL Algorithm
- [x] Pure literal Elimination
- [ ] Better mem allocations
