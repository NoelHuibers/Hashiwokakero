# DPLL Solver

## Running the SAT-Solver

Once built via `cargo build --release` the solver can be executed the following:

`./target/release/dpll_solver -- bin dpll --mode [MODE] --input [FILEPATH] --heuristic [HEURISTIC] --depth [DEPTH]`


### MODE

Currently, only the DPLL mode is available.

### DEPTH

The depth flag can be set to true or false and determines whether the currently lowest backtracking depth is printed to the terminal. This gives some idea of the progress.

### HEURISTIC

Supported Heuristics are:

- arbitrary
- dlis
- dlcs 
- mom 
- boehm 
- jeroslaw_wang 
- custom (in progress)

## CPU Time Measurement
To measure the CPU time for each `.cnf` file in `src/inputs`, run the `cputime` binary:

`cargo run --bin cputime --time-limit [TIME-LIMIT] --heuristic [HEURISTIC]`


### TIME-LIMIT

Time-Limit arguments:
- true: 60sec execution time frame
- false: measures the duration of the execution without timeout

## Plotting
To create the plots for the existing `.csv` files in  `src/cputime`, run the `plot` binary:
`cargo run --bin plot`


## Team Responsibilities

**Noel:**

- [x] Init
- [ ] CLI
- [x] Parser
- [ ] Edge cases for parser (e.G. newlines before 0 was found as in Zulip question)
- [ ] Random Restarts

**Laura:**

- [x] Testing / Profiling
- [x] Benchmarks / CPU Time
- [x] Test: Cactus plots (small test set)
- [x] Heuristics
- [ ] Complete benchmarking of test data + plots

**Flo:**

- [x] DPLL Algorithm
- [x] Pure literal Elimination
- [ ] Better mem allocations
