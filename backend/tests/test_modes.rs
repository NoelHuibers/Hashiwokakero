//! The command line's modes (`backend::modes`), on files in a temporary directory.

use std::path::{Path, PathBuf};

use backend::{api, modes, solver};
use common::{Difficulty, GenerateParams, Puzzle, SolverConfig, SolverKind};

/// A file named `name` with `content` in a fresh directory (kept alive by the guard).
fn file(name: &str, content: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(name);
    std::fs::write(&path, content).expect("write");
    (dir, path)
}

fn path(p: &Path) -> &str {
    p.to_str().expect("utf-8 path")
}

fn read(p: impl AsRef<Path>) -> String {
    std::fs::read_to_string(p).expect("read")
}

const PUZZLE: &str = "5 5\n2.1..\n.....\n4.3.1\n.....\n3...2\n";
/// Its numbers can't be reached: the corners need more bridges than they can get
const UNSAT: &str = "3 3\n8.8\n...\n8.8\n";

#[test]
fn encode_writes_the_dimacs_next_to_the_puzzle() {
    let (dir, puzzle) = file("p.txt", PUZZLE);
    modes::encode_mode(path(&puzzle)).unwrap();
    let expected = api::to_dimacs(&Puzzle::parse_txt(PUZZLE).unwrap()).unwrap();
    assert_eq!(read(dir.path().join("p.txt.cnf")), expected);

    // Puzzles that can't be read or encoded are errors and give no file
    let (dir, bad) = file("bad.txt", "2 2\n1x\n..\n");
    assert!(modes::encode_mode(path(&bad)).is_err());
    assert!(!dir.path().join("bad.txt.cnf").exists());
    let missing = dir.path().join("missing.txt");
    let err = modes::encode_mode(path(&missing)).unwrap_err();
    assert!(err.starts_with(path(&missing)), "{err}");
    assert!(!dir.path().join("missing.txt.cnf").exists());
}

#[test]
fn solve_writes_the_model() {
    let (dir, cnf) = file("f.cnf", "p cnf 2 3\n1 -2 0\n-1 2 0\n1 2 0\n");
    let out = dir.path().join("out.txt");
    for kind in SolverKind::ALL {
        let config = SolverConfig {
            kind,
            ..SolverConfig::default()
        };
        modes::solve_mode(path(&cnf), Some(path(&out).to_string()), config).unwrap();
        assert_eq!(read(&out), "SAT\n1 2 ", "{kind:?}");
        std::fs::remove_file(&out).unwrap();
    }
    let (dir, unsat) = file("u.cnf", "p cnf 1 2\n1 0\n-1 0\n");
    let out = dir.path().join("out.txt");
    modes::solve_mode(
        path(&unsat),
        Some(path(&out).to_string()),
        SolverConfig::default(),
    )
    .unwrap();
    assert_eq!(read(&out), "UNSAT");
    // Only .txt output files, and errors in the input write nothing
    let other = dir.path().join("out.json");
    let err = modes::solve_mode(
        path(&unsat),
        Some(path(&other).to_string()),
        SolverConfig::default(),
    )
    .unwrap_err();
    assert!(err.contains("Only .txt files"), "{err}");
    assert!(!other.exists());
    let (dir, broken) = file("b.cnf", "1 2 0\n");
    let out = dir.path().join("out.txt");
    let err = modes::solve_mode(
        path(&broken),
        Some(path(&out).to_string()),
        SolverConfig::default(),
    )
    .unwrap_err();
    assert!(err.contains("Missing 'p cnf' line"), "{err}");
    assert!(!out.exists());
    // Without an output file the answer goes to stdout
    modes::solve_mode(path(&cnf), None, SolverConfig::default()).unwrap();
    assert_eq!(
        solver::get_content(splr::Certificate::SAT(vec![1, -2])),
        "SAT\n1 -2 "
    );
}

#[test]
fn esr_draws_the_solution() {
    let generated = api::generate(&GenerateParams {
        width: 6,
        height: 5,
        difficulty: Difficulty::Easy,
        seed: 2,
    });
    let (dir, puzzle) = file("p.txt", &generated.puzzle.to_txt());
    let out = dir.path().join("solution.txt");
    modes::esr_mode(
        path(&puzzle),
        Some(path(&out).to_string()),
        SolverConfig::splr(),
    )
    .unwrap();
    let drawn = read(&out);
    assert!(drawn.ends_with("(unique solution)\n"), "{drawn}");
    assert_eq!(drawn.lines().count(), 6, "{drawn}");
    assert!(drawn.contains(['-', '=', '|', '‖']), "{drawn}");
    // This one has several
    let (dir, puzzle) = file("p.txt", PUZZLE);
    let out = dir.path().join("solution.txt");
    modes::esr_mode(
        path(&puzzle),
        Some(path(&out).to_string()),
        SolverConfig::default(),
    )
    .unwrap();
    assert!(read(&out).ends_with("(NOT unique, there are other solutions)\n"));

    let (dir, unsat) = file("u.txt", UNSAT);
    let out = dir.path().join("solution.txt");
    modes::esr_mode(
        path(&unsat),
        Some(path(&out).to_string()),
        SolverConfig::default(),
    )
    .unwrap();
    assert_eq!(read(&out), "Problem is UNSAT\n");

    // A missing puzzle is an error and writes nothing
    let none = dir.path().join("none.txt");
    assert!(
        modes::esr_mode(
            path(&dir.path().join("missing.txt")),
            Some(path(&none).to_string()),
            SolverConfig::default(),
        )
        .is_err()
    );
    assert!(!none.exists());
    modes::esr_mode(path(&puzzle), None, SolverConfig::default()).unwrap();
}

#[test]
fn generate_writes_a_puzzle() {
    let params = GenerateParams {
        width: 7,
        height: 7,
        difficulty: Difficulty::Easy,
        seed: 11,
    };
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("generated.txt");
    modes::generate_mode(params, Some(path(&out).to_string())).unwrap();
    assert_eq!(
        Puzzle::parse_txt(&read(&out)).unwrap(),
        api::generate(&params).puzzle
    );
    modes::generate_mode(params, None).unwrap();
    // Into a directory that doesn't exist
    let err = modes::generate_mode(params, Some(path(&dir.path().join("no/p.txt")).to_string()))
        .unwrap_err();
    assert!(err.contains("no/p.txt"), "{err}");
}
