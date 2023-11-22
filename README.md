# Hashi - Bridges Puzzle

## Folder Structure

The project is organized into three main folders:

- **backend**: Contains the backend code for the Hashi - Bridges Puzzle game.
- **benches**: Contains the code required for benchmarking.
- **common**: Shared code and resources that are used by both the backend and frontend.
- **frontend**: Houses the frontend code, responsible for the user interface and interactions.
- **solutions**: Contains the .cnf and solution .txt files for all initial test problems.

## How to Run

To run the project, follow these steps:

To run the project, open a terminal in the project root directory and execute the following command for frontend:

`cargo run --package frontend`

or for backend:

`cargo run --package backend -- --mode [MODE] --input [FILEPATH] --output [FILEPATH]`

### MODE

We currently support three modes:

|Flag|Alias|Functionality|
|-|-|-|
|encode||Reads a .txt file containing an ASCII hashi puzzle and generates an according .cnf in that directory|
|solve||Reads a CNF file and solves it using minisat|
|encodesolvereconstruct|esr|Reads a .txt file containing an ASCII hashi puzzle, generates a .cnf, solves the CNF file using minisat, reconstructs the solution and prints the reconstructed solved hasi puzzle. 

To build the project, open a terminal in the project root directory and execute:

`cargo build --release`

To execute the rlease build for `test1` and print the reconstructed solution:

`../target/release/backend  --mode esr --input ./input/test1.txt --output ./input/test1.minisat-output.txt`

To execute the integration test containing all given test scenarios and some aditional edge cases:

`cargo test --package backend --test integration_test -- test_integration --exact --nocapture`

### Benchmarking

To execute the criterion benchmarking open a terminal in the project backend directory and execute: 

`cargo bench`

See [Evaluation](hashi_report.pdf)

## SAT Conversion

See [Clauses](Clauses.md)

## Team Responsibilities

**Noel:**

- [x] Initialize the project.
- [x] Develop the Command Line Interface (CLI) for our application.
- [x] Implement SAT-Solver integration and the ability to select the appropriate solver based on input data.
- [x] Clap integration encode or solve.
- [x] Solver Error no DIMACS File
- [ ] Frontend Leptos/Yew integration.
- [x] Generator

**Laura:**

- [x] Handle the parsing of input files.
- [x] Error Handling of input files.
- [x] Establish a data structure for passing input data within the application.
- [x] Manage the creation and formatting of output files.
- [x] Test structure
- [x] Benchmarking
- [x] Game Board Format
- [x] Evaluation of Performance


**Florian:**

- [x] Translate input data into clauses.
- [x] Implement parsing functionality for calculating DIMACS.
- [x] Parsing for CLI output
- [x] Rule 3 in CNF
- [x] Reproduce / Fix unwrap errors in Rule 3
- [x] Update instructions on how to run
