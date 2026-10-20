//! The command line (`src/main.rs`) as a user runs it: the real binary on files in a temporary
//! directory, checked by its output and exit status.

use std::path::Path;
use std::process::{Command, Output};

use backend::{api, reconstruct, solver};
use common::{Board, Difficulty, GenerateParams, GeneratedPuzzle, Puzzle};

/// Runs the CLI with `args`, in `dir` (the default output paths are relative).
fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_backend"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run the backend binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("utf-8 output")
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("utf-8 output")
}

/// Runs the CLI and expects it to succeed.
fn ok(dir: &Path, args: &[&str]) -> Output {
    let out = run(dir, args);
    assert!(out.status.success(), "{args:?}: {}", stderr(&out));
    out
}

/// A puzzle with exactly one solution, so every solver has to draw the same picture.
fn unique_puzzle() -> GeneratedPuzzle {
    let generated = api::generate(&GenerateParams {
        width: 6,
        height: 5,
        difficulty: Difficulty::Easy,
        seed: 2,
    });
    assert!(generated.unique);
    generated
}

fn satisfies(model: &[i32], clauses: &[Vec<i32>]) -> bool {
    clauses.iter().all(|c| c.iter().any(|l| model.contains(l)))
}

#[test]
fn esr_draws_the_solution_with_every_solver() {
    let generated = unique_puzzle();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("p.txt"), generated.puzzle.to_txt()).unwrap();
    let drawn = reconstruct::render(&Board::new(generated.puzzle), &generated.solution);

    for (args, config) in [
        (&[][..], "splr"),
        (&["-s", "cdcl"][..], "Own CDCL · VSIDS · Luby sequence"),
        (
            &[
                "-s",
                "cdcl",
                "-H",
                "arbitrary",
                "-r",
                "luby",
                "--threshold",
                "8",
            ][..],
            "Own CDCL · Arbitrary · Luby sequence",
        ),
        (
            &[
                "-s",
                "cdcl",
                "-r",
                "Geometric",
                "--threshold",
                "5",
                "--factor",
                "120",
            ][..],
            "Own CDCL · VSIDS · Geometric",
        ),
        (
            &[
                "-s",
                "cdcl",
                "-r",
                "fixed",
                "--threshold",
                "3",
                "--subsumption",
            ][..],
            "Own CDCL · VSIDS · Fixed interval · subsumption",
        ),
        (
            &["-s", "cdcl", "-r", "none"][..],
            "Own CDCL · VSIDS · No restarts",
        ),
        (&["-s", "dpll", "-H", "dlis"][..], "Own DPLL · DLIS"),
    ] {
        let out = ok(dir.path(), &[&["-m", "esr", "-i", "p.txt"], args].concat());
        assert_eq!(
            stdout(&out),
            format!("{drawn}(unique solution)\n"),
            "{args:?}"
        );
        // The statistics go to stderr, with the configuration the flags asked for
        let stats = stderr(&out);
        assert!(
            stats.starts_with(&format!("{config} | ")),
            "{args:?}: {stats}"
        );
        assert!(stats.contains(" variables, "), "{stats}");
    }

    // Into a file instead, "encodesolvereconstruct" is the long name of the mode
    let out = ok(
        dir.path(),
        &["-m", "encodesolvereconstruct", "-i", "p.txt", "-o", "s.txt"],
    );
    assert_eq!(stdout(&out), "Solution written to s.txt\n");
    let written = std::fs::read_to_string(dir.path().join("s.txt")).unwrap();
    assert_eq!(written, format!("{drawn}(unique solution)\n"));
}

#[test]
fn esr_reports_other_solutions_and_unsatisfiable_puzzles() {
    let dir = tempfile::tempdir().unwrap();
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("input/test1.txt");
    let out = ok(dir.path(), &["-m", "esr", "-i", input.to_str().unwrap()]);
    assert!(
        stdout(&out).ends_with("(NOT unique, there are other solutions)\n"),
        "{}",
        stdout(&out)
    );

    // The corners need more bridges than they can get
    std::fs::write(dir.path().join("u.txt"), "3 3\n8.8\n...\n8.8\n").unwrap();
    let out = ok(dir.path(), &["-m", "esr", "-i", "u.txt", "-s", "dpll"]);
    assert_eq!(stdout(&out), "Problem is UNSAT\n");
}

#[test]
fn encode_and_solve_the_dimacs() {
    let generated = unique_puzzle();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("p.txt"), generated.puzzle.to_txt()).unwrap();

    let out = ok(dir.path(), &["-m", "encode", "-i", "p.txt"]);
    assert_eq!(stdout(&out), "Successfully generated p.txt.cnf\n");
    let dimacs = std::fs::read_to_string(dir.path().join("p.txt.cnf")).unwrap();
    assert_eq!(dimacs, api::to_dimacs(&generated.puzzle).unwrap());
    let (clauses, _) = solver::parse_dimacs(&dimacs).unwrap();

    for solver in ["splr", "cdcl", "dpll"] {
        let out = ok(
            dir.path(),
            &["-m", "solve", "-i", "p.txt.cnf", "-s", solver],
        );
        let text = stdout(&out);
        let model = text
            .strip_prefix("SAT\n")
            .unwrap_or_else(|| panic!("{solver}: {text}"))
            .split_whitespace()
            .map(|l| l.parse::<i32>().unwrap())
            .collect::<Vec<_>>();
        assert!(satisfies(&model, &clauses), "{solver}: {text}");
    }

    let out = ok(
        dir.path(),
        &["-m", "solve", "-i", "p.txt.cnf", "-o", "m.txt"],
    );
    assert_eq!(stdout(&out), "Solution written to m.txt\n");
    let written = std::fs::read_to_string(dir.path().join("m.txt")).unwrap();
    assert!(written.starts_with("SAT\n"), "{written}");

    std::fs::write(dir.path().join("u.cnf"), "p cnf 1 2\n1 0\n-1 0\n").unwrap();
    let out = ok(dir.path(), &["-m", "solve", "-i", "u.cnf", "-s", "dpll"]);
    assert_eq!(stdout(&out), "UNSAT\n");
}

/// The pigeonhole formula: `n + 1` pigeons in `n` holes. Unsatisfiable, and resolution (so
/// DPLL and CDCL) needs exponentially many steps to show it.
fn pigeonhole(n: i32) -> String {
    let var = |pigeon: i32, hole: i32| pigeon * n + hole + 1;
    let mut clauses = (0..=n)
        .map(|p| {
            (0..n)
                .map(|h| var(p, h).to_string() + " ")
                .collect::<String>()
                + "0"
        })
        .collect::<Vec<_>>();
    for h in 0..n {
        for p in 0..=n {
            for q in p + 1..=n {
                clauses.push(format!("-{} -{} 0", var(p, h), var(q, h)));
            }
        }
    }
    format!(
        "p cnf {} {}\n{}\n",
        (n + 1) * n,
        clauses.len(),
        clauses.join("\n")
    )
}

#[test]
fn solve_gives_up_after_the_timeout() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("php.cnf"), pigeonhole(9)).unwrap();
    for solver in ["cdcl", "dpll"] {
        let out = run(
            dir.path(),
            &["-m", "solve", "-i", "php.cnf", "-s", solver, "-t", "50"],
        );
        assert_eq!(out.status.code(), Some(1), "{solver}");
        assert!(stderr(&out).contains("timed out"), "{}", stderr(&out));
    }
}

#[test]
fn generate_is_reproducible_with_a_seed() {
    let dir = tempfile::tempdir().unwrap();
    let args = [
        "-m", "generate", "-d", "Easy", "--seed", "7", "-x", "6", "-y", "5",
    ];
    let first = ok(dir.path(), &args);
    let second = ok(dir.path(), &args);
    assert_eq!(stdout(&first), stdout(&second));

    let params = GenerateParams {
        width: 6,
        height: 5,
        difficulty: Difficulty::Easy,
        seed: 7,
    };
    let generated = api::generate(&params);
    let drawn = reconstruct::render(&Board::new(generated.puzzle.clone()), &generated.solution);
    // The puzzle, then its solution
    assert_eq!(
        stdout(&first),
        format!("{}\n{drawn}", generated.puzzle.to_txt())
    );
    let summary = stderr(&first);
    assert!(
        summary.starts_with("6x5 Easy (seed 7): ") && summary.contains("unique: true"),
        "{summary}"
    );

    let out = ok(dir.path(), &[&args[..], &["-o", "g.txt"]].concat());
    assert_eq!(stdout(&out), "Puzzle written to g.txt\n");
    let written = std::fs::read_to_string(dir.path().join("g.txt")).unwrap();
    assert_eq!(Puzzle::parse_txt(&written).unwrap(), generated.puzzle);

    // Sizes outside of what the generator supports are clamped
    let out = ok(
        dir.path(),
        &["-m", "generate", "-d", "easy", "-x", "1", "-y", "999"],
    );
    assert!(
        stderr(&out).starts_with("3x30 Easy (seed "),
        "{}",
        stderr(&out)
    );
}

#[test]
fn wrong_arguments_exit_with_2() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("p.txt"), "3 3\n1.1\n...\n...\n").unwrap();
    let esr = ["-m", "esr", "-i", "p.txt"];
    for (args, message) in [
        (&["-i", "p.txt"][..], "--mode <MODE>"),
        (
            &["-m", "fly"][..],
            "Use 'encode', 'solve', 'esr' or 'generate' as mode",
        ),
        (&["-m", "esr"][..], "mode 'esr' needs an input file (-i)"),
        (
            &["-m", "encode"][..],
            "mode 'encode' needs an input file (-i)",
        ),
        (&["-m", "solve"][..], "mode 'solve' needs a CNF file (-i)"),
        (
            &[&esr[..], &["-s", "minisat"]].concat()[..],
            "Unknown solver 'minisat'",
        ),
        (
            &[&esr[..], &["-H", "coin"]].concat()[..],
            "Unknown heuristic 'coin'",
        ),
        (
            &[&esr[..], &["-s", "cdcl", "-H", "dlis"]].concat()[..],
            "Own CDCL doesn't support the heuristic 'dlis'",
        ),
        (
            &[&esr[..], &["-r", "often"]].concat()[..],
            "Unknown restart policy 'often'",
        ),
        (
            &[&esr[..], &["-t", "soon"]].concat()[..],
            "invalid value 'soon'",
        ),
        (
            &["-m", "generate", "-d", "impossible"][..],
            "unknown difficulty 'impossible'",
        ),
    ] {
        let out = run(dir.path(), args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(stderr(&out).contains(message), "{args:?}: {}", stderr(&out));
        assert!(stdout(&out).is_empty(), "{args:?}");
    }
}

/// These printed the error but exited with 0, a script couldn't tell they failed.
#[test]
fn failures_exit_with_1() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("bad.txt"), "2 2\n1x\n..\n").unwrap();
    std::fs::write(dir.path().join("bad.cnf"), "1 2 0\n").unwrap();
    std::fs::write(dir.path().join("f.cnf"), "p cnf 1 1\n1 0\n").unwrap();
    std::fs::write(dir.path().join("p.txt"), "3 3\n1.1\n...\n...\n").unwrap();
    for (args, message) in [
        (
            &["-m", "esr", "-i", "missing.txt"][..],
            "Error: missing.txt: ",
        ),
        (&["-m", "encode", "-i", "bad.txt"][..], "Error: "),
        (
            &["-m", "solve", "-i", "bad.cnf"][..],
            "Error: bad.cnf: Missing 'p cnf' line",
        ),
        (
            &["-m", "solve", "-i", "f.cnf", "-o", "m.json"][..],
            "Error: m.json: Only .txt files are allowed",
        ),
        (
            &["-m", "esr", "-i", "p.txt", "-o", "no/s.txt"][..],
            "Error: no/s.txt: ",
        ),
        (
            &["-m", "generate", "-d", "easy", "-x", "5", "-o", "no/g.txt"][..],
            "Error: no/g.txt: ",
        ),
    ] {
        let out = run(dir.path(), args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {}", stderr(&out));
        assert!(stderr(&out).contains(message), "{args:?}: {}", stderr(&out));
        assert!(!dir.path().join("bad.txt.cnf").exists());
    }
}
