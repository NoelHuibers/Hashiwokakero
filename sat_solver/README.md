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

### CPU Time Measurement
To measure the CPU time for each `.cnf` file in `dimacs-files/input`, run the `cputime` binary:

`cargo run --bin cputime --time-limit [TIME-LIMIT] --heuristic [HEURISTIC]`

### TIME-LIMIT

Time-Limit arguments:
- true: 60sec execution time frame
- false: measures the duration of the execution without timeout

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
- [ ] Cactus plots
- [x] Heuristics

**Flo:**

- [x] DPLL Algorithm
- [x] Pure literal Elimination
- [ ] Better mem allocations
