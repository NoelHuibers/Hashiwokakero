# Hashi - Bridges Puzzle

## Folder Structure

The project is organized into three main folders:

- **backend**: Contains the backend code for the Hashi - Bridges Puzzle game.
- **frontend**: Houses the frontend code, responsible for the user interface and interactions.
- **common**: Shared code and resources that are used by both the backend and frontend.

## How to Run

To run the project, follow these steps:

To run the project, open a terminal in the project root directory and execute the following command for frontend:

`cargo run --package frontend`

or for backend:

`cargo run --package backend`

Additionally, you can provide an input file using the `--input` flag like this:

`cargo run --package backend -- --input [FILEPATH]`

To build the project, open a terminal in the project root directory and execute:

`cargo build`

## Team Responsibilities

**Noel:**

- [x] Initialize the project.
- [x] Develop the Command Line Interface (CLI) for our application.
- [x] Implement SAT-Solver integration and the ability to select the appropriate solver based on input data.
- [x] Clap integration encode or solve.
- [ ] CLI for displaying solution.
- [ ] Frontend Leptos/Yew integration.
- [ ] Generator

**Laura:**

- [x] Handle the parsing of input files.
- [x] Error Handling of input files.
- [x] Establish a data structure for passing input data within the application.
- [x] Manage the creation and formatting of output files.
- [ ] Test structure
- [ ] Benchmarking

**Florian:**

- [x] Translate input data into clauses.
- [x] Implement parsing functionality for calculating DIMACS.
- [ ] Parsing for CLI output

**Unsolved:**

- [ ] Rule 2 in CNF
