//! The `sat` binary as a user runs it: real formulas from `src/inputs` and generated ones in a
//! temporary directory, checked by the output, the written files and the exit status.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sat_solver::parse::parse;

fn input(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/inputs")
        .join(name)
        .to_str()
        .expect("utf-8 path")
        .to_string()
}

/// Runs `sat` with `args`, in `dir` (where `--flamegraph` writes its file).
fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sat"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run the sat binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("utf-8 output")
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("utf-8 output")
}

/// Runs `sat` and expects it to succeed, returns its standard output.
fn ok(dir: &Path, args: &[&str]) -> String {
    let out = run(dir, args);
    assert!(out.status.success(), "{args:?}: {}", stderr(&out));
    stdout(&out)
}

/// The model of a `SAT` answer, checked against the formula in `file`.
fn assert_model(answer: &str, file: &str) {
    let model = answer
        .strip_prefix("SAT\n")
        .unwrap_or_else(|| panic!("expected SAT: {answer}"))
        .split_whitespace()
        .map(|l| l.parse::<i32>().expect("a literal"))
        .collect::<Vec<_>>();
    assert_eq!(model.last(), Some(&0), "{answer}");
    let (clauses, vars, _) = parse(file).unwrap();
    // One value per variable, in order
    assert_eq!(model.len(), vars + 1, "{answer}");
    for clause in &clauses {
        assert!(
            clause.iter().any(|l| model.contains(l)),
            "{clause:?} not satisfied: {answer}"
        );
    }
}

/// The pigeonhole formula: `n + 1` pigeons in `n` holes. Unsatisfiable, and resolution (so
/// DPLL and CDCL) needs exponentially many steps to show it.
fn pigeonhole(dir: &Path, n: i32) -> String {
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
    let text = format!(
        "p cnf {} {}\n{}\n",
        (n + 1) * n,
        clauses.len(),
        clauses.join("\n")
    );
    let path = dir.join(format!("php{n}.cnf"));
    std::fs::write(&path, text).unwrap();
    path.to_str().unwrap().to_string()
}

#[test]
fn dpll_solves_with_every_heuristic() {
    let dir = tempfile::tempdir().unwrap();
    let sat = input("sat/uf50-06.cnf");
    let unsat = pigeonhole(dir.path(), 4);
    for heuristic in [
        "arbitrary",
        "dlis",
        "dlcs",
        "mom",
        "boehm",
        "jeroslaw_wang",
        "vsids",
        "custom",
    ] {
        assert_model(&ok(dir.path(), &["dpll", &sat, "-H", heuristic]), &sat);
        assert_eq!(
            ok(dir.path(), &["dpll", &unsat, "-H", heuristic]),
            "UNSAT\n"
        );
    }
}

#[test]
fn cdcl_solves_with_every_option() {
    let dir = tempfile::tempdir().unwrap();
    let sat = input("sat/aim-50-1_6-yes1-1.cnf");
    let unsat = input("unsat/aim-50-1_6-no-1.cnf");
    for options in [
        &[][..],
        &["-H", "vsids"],
        &["-H", "vsids", "-r", "4", "--luby"],
        &["-r", "3", "-F", "150"],
        &["-r", "2"],
        &["-s", "-k", "2", "-m", "1"],
    ] {
        let answer = ok(dir.path(), &[&["cdcl", sat.as_str()][..], options].concat());
        assert_model(&answer, &sat);
        let answer = ok(
            dir.path(),
            &[&["cdcl", unsat.as_str()][..], options].concat(),
        );
        assert_eq!(answer, "UNSAT\n", "{options:?}");
    }
}

#[test]
fn unknown_heuristics_fall_back_to_arbitrary() {
    let dir = tempfile::tempdir().unwrap();
    let sat = input("sat/uf50-06.cnf");
    let out = run(dir.path(), &["dpll", &sat, "-H", "coin"]);
    assert!(out.status.success());
    // The answer alone on stdout, a script reads it from there
    assert_model(&stdout(&out), &sat);
    assert_eq!(stderr(&out), "Heuristic not found using arbitrary\n");
}

#[test]
fn cdcl_rejects_the_dpll_heuristics() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        dir.path(),
        &["cdcl", &pigeonhole(dir.path(), 3), "-H", "dlis"],
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert!(
        stderr(&out).starts_with(
            "Sat-Solver failed to solve: Error: Heuristic DLIS is not implemented/embedded into CDCL (branch depth: "
        ),
        "{}",
        stderr(&out)
    );
}

#[test]
fn writes_the_answer_to_the_output_file() {
    let dir = tempfile::tempdir().unwrap();
    let sat = input("sat/uf50-06.cnf");
    let out = dir.path().join("out.txt");
    let answer = ok(dir.path(), &["dpll", &sat, out.to_str().unwrap()]);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), answer);
    assert_model(&answer, &sat);
}

/// Whether unit propagation on `clauses` with the negation of `lemma` assigned runs into a
/// conflict, which makes `lemma` a reverse unit propagation (RUP) consequence of `clauses`.
fn rup(clauses: &[Vec<i32>], lemma: &[i32]) -> bool {
    let mut value = lemma
        .iter()
        .map(|&l| (l.abs(), l < 0))
        .collect::<HashMap<i32, bool>>();
    if lemma.iter().any(|&l| value[&l.abs()] != (l < 0)) {
        // A tautology
        return true;
    }
    loop {
        let mut changed = false;
        for clause in clauses {
            let mut free = None;
            let mut free_count = 0;
            let mut satisfied = false;
            for &l in clause {
                match value.get(&l.abs()) {
                    Some(&v) if v == (l > 0) => satisfied = true,
                    Some(_) => {}
                    None => {
                        free_count += 1;
                        free = Some(l);
                    }
                }
            }
            match (satisfied, free_count, free) {
                (false, 0, _) => return true,
                (false, 1, Some(l)) => {
                    value.insert(l.abs(), l > 0);
                    changed = true;
                }
                // Satisfied, or more than one literal left
                _ => {}
            }
        }
        if !changed {
            return false;
        }
    }
}

/// `--DRUP` logs the learned clauses (and the deleted ones) next to the output file. Checked
/// like a proof checker (drat-trim) would: every learned clause follows from the formula and
/// the clauses before it by unit propagation, and in the end unit propagation alone refutes
/// the formula.
#[test]
fn drup_logs_a_valid_refutation() {
    let dir = tempfile::tempdir().unwrap();
    for (file, options) in [
        (input("unsat/aim-50-1_6-no-1.cnf"), &[][..]),
        (pigeonhole(dir.path(), 4), &["-H", "vsids", "-r", "5"][..]),
        // Small k and m, so the solver deletes learned clauses too
        (pigeonhole(dir.path(), 5), &["-k", "2", "-m", "1"][..]),
    ] {
        let out = dir.path().join("out.txt");
        let log = dir.path().join("out.txt.log");
        let _ = std::fs::remove_file(&log);
        let args = [
            &["cdcl", file.as_str(), out.to_str().unwrap(), "-D"][..],
            options,
        ]
        .concat();
        assert_eq!(ok(dir.path(), &args), "UNSAT\n");
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "UNSAT\n");

        let (mut clauses, ..) = parse(&file).unwrap();
        let proof = std::fs::read_to_string(&log).unwrap();
        let mut deletions = 0;
        for line in proof.lines() {
            let (delete, line) = match line.trim().strip_prefix('d') {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            let mut clause = line
                .split_whitespace()
                .map(|l| l.parse::<i32>().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(clause.pop(), Some(0), "{line}");
            if delete {
                let mut sorted = clause.clone();
                sorted.sort_unstable();
                let index = clauses
                    .iter()
                    .position(|c| {
                        let mut c = c.clone();
                        c.sort_unstable();
                        c == sorted
                    })
                    .unwrap_or_else(|| panic!("deleted {clause:?}, which isn't there"));
                clauses.swap_remove(index);
                deletions += 1;
            } else {
                assert!(rup(&clauses, &clause), "{file} {options:?}: {clause:?}");
                clauses.push(clause);
            }
        }
        assert!(!proof.is_empty(), "{file}");
        assert!(rup(&clauses, &[]), "{file} {options:?}: no refutation");
        if options.contains(&"-k") {
            assert!(deletions > 0, "{file} {options:?}: nothing deleted");
        }
    }
}

#[test]
fn drup_needs_an_output_file() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        dir.path(),
        &["cdcl", &input("unsat/aim-50-1_6-no-1.cnf"), "-D"],
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("<output>"), "{}", stderr(&out));
}

#[test]
fn gives_up_after_the_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let hard = pigeonhole(dir.path(), 9);
    for solver in ["dpll", "cdcl"] {
        // At once, and in the middle of the search
        for timeout in ["0", "0.05"] {
            let answer = ok(dir.path(), &[solver, &hard, "-t", timeout]);
            assert_eq!(answer, "UNKNOWN (timeout)\n", "{solver} {timeout}");
        }
    }
}

/// A limit beyond what `Instant` can represent panicked instead of solving without one.
#[test]
fn huge_timeouts_mean_no_limit() {
    let dir = tempfile::tempdir().unwrap();
    let sat = input("sat/uf50-06.cnf");
    for timeout in ["1e30", "inf"] {
        assert_model(&ok(dir.path(), &["cdcl", &sat, "-t", timeout]), &sat);
    }
}

/// Small inputs: every span is kept in memory and written out, which is slow.
#[test]
fn flamegraph_profiles_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let unsat = pigeonhole(dir.path(), 3);
    for (solver, span) in [("dpll", "DPLL::solve"), ("cdcl", "main_solve")] {
        assert_eq!(ok(dir.path(), &[solver, &unsat, "-f"]), "UNSAT\n");
        let html = std::fs::read_to_string(dir.path().join("flamegraph.html")).unwrap();
        for span in ["main", "parse", span] {
            assert!(html.contains(&format!("\"{span}\"")), "{solver}: no {span}");
        }
    }
}

#[test]
fn dpll_shows_the_search_depth() {
    let dir = tempfile::tempdir().unwrap();
    let answer = ok(dir.path(), &["dpll", &pigeonhole(dir.path(), 3), "-d"]);
    let mut lines = answer.lines().collect::<Vec<_>>();
    assert_eq!(lines.pop(), Some("UNSAT"));
    // Whenever the search backtracks to a new minimum depth, down to the root
    let depths = lines
        .iter()
        .map(|l| {
            l.strip_prefix("backtracked to depth ")
                .unwrap()
                .parse::<usize>()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(depths.windows(2).all(|w| w[0] >= w[1]), "{depths:?}");
    assert_eq!(depths.last(), Some(&0));
}

#[test]
fn wrong_arguments_exit_with_2() {
    let dir = tempfile::tempdir().unwrap();
    let file = input("sat/uf50-06.cnf");
    for (args, message) in [
        (&["minisat", file.as_str()][..], "invalid value 'minisat'"),
        (&["dpll"][..], "<input>"),
        (
            &["cdcl", file.as_str(), "-k", "ten"][..],
            "invalid value 'ten'",
        ),
        (
            &["cdcl", file.as_str(), "-m", "-1"][..],
            "unexpected argument '-1'",
        ),
        (
            &["cdcl", file.as_str(), "-r", "1.5"][..],
            "invalid value '1.5'",
        ),
        (&["cdcl", file.as_str(), "-F", "x"][..], "invalid value 'x'"),
        (
            &["cdcl", file.as_str(), "--timeout=-1"][..],
            "non-negative number of seconds",
        ),
        (
            &["cdcl", file.as_str(), "-t", "NaN"][..],
            "non-negative number of seconds",
        ),
    ] {
        let out = run(dir.path(), args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(stderr(&out).contains(message), "{args:?}: {}", stderr(&out));
    }
}

/// Unreadable inputs and outputs are reported, they used to panic.
#[test]
fn bad_files_exit_with_1() {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, text: &str| -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        path
    };
    let not_cnf = write("formula.txt", "p cnf 1 1\n1 0\n");
    let broken = write("broken.cnf", "p cnf 1 1\n1 x 0\n");
    let fine = write("fine.cnf", "p cnf 1 1\n1 0\n");
    let missing = dir.path().join("missing.cnf");
    let nowhere = dir.path().join("no/out.txt");
    for (args, message) in [
        (
            vec![&not_cnf],
            "formula.txt: Only .cnf files are allowed.".to_string(),
        ),
        (vec![&broken], "broken.cnf: Invalid number: ".to_string()),
        (vec![&missing], "missing.cnf: ".to_string()),
        (
            vec![&fine, &nowhere],
            format!("Couldn't write {}: ", nowhere.display()),
        ),
    ] {
        let args = std::iter::once("dpll")
            .chain(args.iter().map(|p| p.to_str().unwrap()))
            .collect::<Vec<_>>();
        let out = run(dir.path(), &args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {}", stderr(&out));
        assert!(
            stderr(&out).contains(&message),
            "{args:?}: {}",
            stderr(&out)
        );
    }
}
