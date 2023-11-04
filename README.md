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

- Noel:

  - Initialize the project.
  - Develop the Command Line Interface (CLI) for our application.
  - Implement SAT-Solver integration and the ability to select the appropriate solver based on input data.

- Laura:

  - Handle the parsing of input files.
  - Establish a data structure for passing input data within the application.
  - Manage the creation and formatting of output files.

- Florian:
  -Translate input data into clauses.
  -Implement parsing functionality for calculating DMAX.
