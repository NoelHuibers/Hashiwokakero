//! The `dojo` command line as a user runs it: `export`, `run`, `calibrate` and `serve` on small
//! sets in a temporary directory, checked by their output, the files they write and their exit
//! status. cargo-llvm-cov counts the coverage of the spawned binary too.

#![cfg(feature = "worker")]

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use dojo::calibrate::{self, FuelRow, TimeRow};
use dojo::community::{BenchmarkReport, Refusal, Verdict};
use dojo::fuel::{Category, Counts, Weights};
use dojo::proof::{TempDir, Tools};
use dojo::{Answer, Family, RULES_VERSION};

/// A fresh directory under cargo's temporary directory for tests, removed at the end.
/// The worker's token: it refuses one shorter than 32 characters
const TOKEN: &str = "test-token-of-at-least-32-characters";

fn scratch(name: &str) -> TempDir {
    TempDir::new(Path::new(env!("CARGO_TARGET_TMPDIR")), name).expect("a scratch directory")
}

fn dojo(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dojo"));
    command.current_dir(dir);
    command
}

/// Runs `dojo` with `args` in `dir`.
fn run(dir: &Path, args: &[&str]) -> Output {
    dojo(dir).args(args).output().expect("run the dojo binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("utf-8 output")
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("utf-8 output")
}

/// Runs `dojo` and expects it to succeed, returns its standard output.
fn ok(dir: &Path, args: &[&str]) -> Output {
    let out = run(dir, args);
    assert!(out.status.success(), "{args:?}: {}", stderr(&out));
    out
}

/// Runs `dojo` and expects it to fail with a message containing `message`: an error it
/// reports, not a panic.
fn fails(dir: &Path, args: &[&str], message: &str) -> String {
    let out = run(dir, args);
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(1), "{args:?}: {err}");
    assert!(!err.contains("panicked"), "{args:?}: {err}");
    assert!(err.contains(message), "{args:?}: {err}");
    err
}

fn minisat() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("reference/minisat.wasm")
        .to_string_lossy()
        .into_owned()
}

/// A solver that prints `answer` and exits, whatever the formula.
fn answering(answer: &str) -> Vec<u8> {
    let data = answer.bytes().fold(String::new(), |mut data, b| {
        let _ = write!(data, "\\{b:02x}");
        data
    });
    wat::parse_str(format!(
        r#"(module
            (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 64) "{data}")
            (func (export "_start")
                (i32.store (i32.const 0) (i32.const 64))
                (i32.store (i32.const 4) (i32.const {len}))
                (drop (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))))"#,
        len = answer.len()
    ))
    .unwrap()
}

/// A solver that traps at once.
fn crashing() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "_start") unreachable))"#,
    )
    .unwrap()
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> String {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path.to_string_lossy().into_owned()
}

/// `[ i/n] name expected outcome ...`: the progress lines of `dojo run`, as (name, expected,
/// outcome).
fn progress(err: &str) -> Vec<(String, String, String)> {
    err.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix('[')?.split_once(']')?.1;
            let mut words = rest.split_whitespace();
            let name = words.next()?.to_string();
            let expected = words.next()?.to_string();
            let outcome = match words.next()? {
                "no" => "no proof".to_string(),
                other => other.to_string(),
            };
            Some((name, expected, outcome))
        })
        .collect()
}

#[test]
fn usage_and_bad_arguments() {
    let dir = scratch("dojo-cli-usage");
    let d = dir.path();
    let solver = write(d, "unsat.wasm", &answering("s UNSATISFIABLE\n"));
    let junk = write(d, "junk.wasm", b"not webassembly");
    let broken = write(d, "broken.tar.gz", b"not a tarball");

    fails(d, &[], "usage: dojo run");
    fails(d, &["fly"], "usage: dojo run");
    fails(d, &["export"], "usage: dojo export");
    fails(
        d,
        &["export", "out", "--season", "soon"],
        "--season takes a number",
    );
    // An unknown family is an error, before any formula is generated
    let err = fails(
        d,
        &["export", "out", "--family", "sudoku"],
        "unknown family sudoku, one of hashi, ",
    );
    assert!(!err.contains("formulas, ready"), "{err}");
    assert!(!d.join("out").exists());

    fails(d, &["run"], "usage: dojo run <solver.wasm>");
    fails(d, &["run", "missing.wasm"], "missing.wasm: ");
    fails(d, &["run", &junk], "rejected: Invalid");
    for (flag, value, message) in [
        ("--budget", "lots", "--budget takes a number"),
        ("--threads", "many", "--threads takes a number"),
        ("--checker", "magic", "--checker takes verified or builtin"),
        ("--set", "nothing.tar.gz", "nothing.tar.gz: "),
        ("--set", &broken, "broken.tar.gz: "),
        ("--family", "sudoku", "unknown family sudoku"),
    ] {
        fails(d, &["run", &solver, flag, value], message);
    }
    // The leaderboard's checkers, insisted on but not installed
    let empty = d.join("empty");
    std::fs::create_dir(&empty).unwrap();
    let out = dojo(d)
        .args(["run", &solver, "--checker", "verified"])
        .env_remove("DOJO_DRAT_TRIM")
        .env_remove("DOJO_CAKE_LPR")
        .env("PATH", &empty)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("drat-trim and cake_lpr weren't found"),
        "{}",
        stderr(&out)
    );

    fails(
        d,
        &["calibrate"],
        "usage: dojo calibrate fuel | time | analyze",
    );
    fails(d, &["calibrate", "fuel"], "no solvers: give --solver");
    fails(
        d,
        &["calibrate", "fuel", "--solver", "minisat"],
        "--solver NAME=WASM[:NATIVE], not minisat",
    );
    fails(
        d,
        &["calibrate", "time", "--solver", "a=missing.wasm"],
        "missing.wasm: ",
    );
    let named = format!("a={solver}");
    fails(
        d,
        &["calibrate", "fuel", "--solver", &named],
        "no formulas: give --set SET.tar.gz or --dev",
    );
    fails(
        d,
        &["calibrate", "fuel", "--solver", &named, "--set", &broken],
        "broken.tar.gz: ",
    );
    fails(
        d,
        &["calibrate", "fuel", "--cap", "x"],
        "--cap takes a number",
    );
    fails(
        d,
        &["calibrate", "analyze", "fuel.csv"],
        "usage: dojo calibrate analyze fuel.csv time.csv",
    );
    fails(
        d,
        &["calibrate", "analyze", "fuel.csv", "time.csv"],
        "fuel.csv: ",
    );

    // The worker needs its token, and the leaderboard's checkers unless told otherwise
    let out = dojo(d)
        .arg("serve")
        .env_remove("DOJO_TOKEN")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(stderr(&out).contains("DOJO_TOKEN is not set"));
    let out = dojo(d)
        .arg("serve")
        .env("DOJO_TOKEN", "secret")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(stderr(&out).contains("DOJO_TOKEN must contain at least 32 characters"));
    let out = dojo(d)
        .arg("serve")
        .env("DOJO_TOKEN", TOKEN)
        .env_remove("DOJO_CHECKER")
        .env_remove("DOJO_DRAT_TRIM")
        .env_remove("DOJO_CAKE_LPR")
        .env("PATH", &empty)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("DOJO_CHECKER=builtin checks proofs with the built-in checker"),
        "{}",
        stderr(&out)
    );
}

/// Exports a small hidden set, then scores solvers on it: `MiniSat`, which solves and proves, and
/// solvers that answer wrong, without a proof, crash or run out of fuel.
#[test]
fn exports_sets_and_scores_solvers() {
    let dir = scratch("dojo-cli-run");
    let d = dir.path();

    // A season's set is a hidden one, exactly what the library generates for the seed
    let out = ok(
        d,
        &[
            "export",
            "set.tar.gz",
            "--seed",
            "cli-test",
            "--season",
            "3",
            "--family",
            "pigeonhole",
        ],
    );
    assert!(stderr(&out).contains("written to set.tar.gz"));
    let archive = std::fs::read(d.join("set.tar.gz")).unwrap();
    let (info, set) = dojo::archive::read_tar_gz(&archive).unwrap();
    assert_eq!(
        (info.seed.as_str(), info.season, info.hidden),
        ("cli-test", Some(3), true)
    );
    // The hidden set's formulas of the family, which has more of them than the dev set
    let pigeonholes = |spec: &[dojo::bench::Spec]| {
        spec.iter()
            .filter(|s| s.family == Family::Pigeonhole)
            .count()
    };
    assert_eq!(
        set.len(),
        pigeonholes(&dojo::bench::hidden_spec("cli-test"))
    );
    assert!(set.len() > pigeonholes(dojo::bench::DEV_SPEC));
    assert!(set.iter().all(|i| i.family == Family::Pigeonhole));
    let sat = set.iter().filter(|i| i.expected == Answer::Sat).count();
    let n = set.len();

    // MiniSat with the built-in checker, its proofs kept
    let out = ok(
        d,
        &[
            "run",
            &minisat(),
            "--set",
            "set.tar.gz",
            "--checker",
            "builtin",
            "--threads",
            "2",
            "--proofs",
            "proofs",
        ],
    );
    let (text, err) = (stdout(&out), stderr(&out));
    assert!(
        err.contains("proofs are checked with the built-in checker"),
        "{err}"
    );
    assert!(
        err.contains(&format!("set cli-test: {n} formulas")),
        "{err}"
    );
    let lines = progress(&err);
    assert_eq!(lines.len(), n, "{err}");
    let solved: Vec<_> = lines.iter().filter(|l| l.2 == "solved").collect();
    assert!(
        !solved.is_empty() && !lines.iter().any(|l| l.2 == "WRONG"),
        "{err}"
    );
    assert!(
        text.contains(&format!("solved {}/{n}  PAR-2", solved.len())),
        "{text}"
    );
    assert!(text.contains("pigeonhole"), "{text}");
    // Small formulas: MiniSat solves every satisfiable one
    assert!(sat > 0 && sat < n);
    assert!(
        text.contains(&format!("SAT    {sat}/{sat}  PAR-2")),
        "{text}"
    );
    assert!(
        text.contains(&format!("UNSAT  {}/{}  PAR-2", solved.len() - sat, n - sat)),
        "{text}"
    );
    assert!(!text.contains("WRONG answers"), "{text}");
    // A directory per formula, a proof in it for every solved UNSAT one
    for (name, expected, outcome) in &lines {
        assert!(d.join("proofs").join(name).is_dir(), "{name}");
        if expected == "UNSAT" && outcome == "solved" {
            let proof = d.join("proofs").join(name).join("proof.out");
            assert!(std::fs::metadata(&proof).unwrap().len() > 0, "{name}");
        }
    }

    // The leaderboard's checkers accept exactly the proofs the built-in one does
    if let Some(tools) = Tools::find() {
        let out = ok(
            d,
            &[
                "run",
                &minisat(),
                "--set",
                "set.tar.gz",
                "--checker",
                "verified",
            ],
        );
        let err = stderr(&out);
        assert!(err.contains(&tools.describe()), "{err}");
        let mut verified = progress(&err);
        let mut builtin = lines.clone();
        verified.sort();
        builtin.sort();
        assert_eq!(verified, builtin);
    } else {
        assert!(
            std::env::var_os("DOJO_REQUIRE_CHECKERS").is_none(),
            "DOJO_REQUIRE_CHECKERS is set but drat-trim and cake_lpr weren't found"
        );
    }

    // Out of fuel at once: nothing solved, every formula a timeout
    let out = ok(
        d,
        &[
            "run",
            &minisat(),
            "--set",
            "set.tar.gz",
            "--budget",
            "1_000",
        ],
    );
    assert!(
        progress(&stderr(&out)).iter().all(|l| l.2 == "timeout"),
        "{}",
        stderr(&out)
    );
    assert!(stdout(&out).contains(&format!("solved 0/{n}")));

    // UNSAT on everything, without a proof: wrong on the satisfiable formulas, unsolved on the
    // others
    write(d, "unsat.wasm", &answering("s UNSATISFIABLE\n"));
    let out = ok(d, &["run", "unsat.wasm", "--set", "set.tar.gz"]);
    let (text, err) = (stdout(&out), stderr(&out));
    for (_, expected, outcome) in progress(&err) {
        let want = if expected == "SAT" {
            "WRONG"
        } else {
            "no proof"
        };
        assert_eq!(outcome, want, "{err}");
    }
    assert!(text.contains(&format!("solved 0/{n}")), "{text}");
    assert!(
        text.contains(&format!(
            "{} UNSAT answers without an accepted proof count as unsolved",
            n - sat
        )),
        "{text}"
    );
    assert!(text.contains(&format!("{sat} WRONG answers")), "{text}");
    assert!(
        text.contains(&format!("{} UNSAT without an accepted proof", n - sat)),
        "{text}"
    );

    // A crash is no answer, not a wrong one
    write(d, "crash.wasm", &crashing());
    let out = ok(d, &["run", "crash.wasm", "--set", "set.tar.gz", "--stderr"]);
    assert!(
        progress(&stderr(&out)).iter().all(|l| l.2 == "crashed"),
        "{}",
        stderr(&out)
    );
    assert!(!stdout(&out).contains("WRONG"));

    // The development set, written as files
    ok(d, &["export", "dev", "--family", "queens"]);
    let answers = std::fs::read_to_string(d.join("dev/answers.txt")).unwrap();
    let readme = std::fs::read_to_string(d.join("dev/README.txt")).unwrap();
    assert!(
        readme.contains(&format!("seed: {}", dojo::DEV_SEED)),
        "{readme}"
    );
    assert!(readme.contains("kind: dev"), "{readme}");
    assert_ne!(answers, "");
    for line in answers.lines() {
        let (name, answer) = line.split_once(' ').unwrap();
        assert!(name.starts_with("queens-"), "{line}");
        assert!(Answer::from_id(answer).is_some(), "{line}");
        assert!(
            d.join("dev").join(format!("{name}.cnf")).is_file(),
            "{line}"
        );
    }
}

/// A run in fuel.csv: `ops` operators, a share `loads` of them loads, the rest ALU.
fn fuel_row(solver: &str, instance: &str, ops: u64, loads: f64) -> FuelRow {
    let mut counts = Counts::default();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "small synthetic counts"
    )]
    let load = (ops as f64 * loads) as u64;
    counts.ops[Category::Load as usize] = load;
    counts.ops[Category::Alu as usize] = ops - load;
    FuelRow {
        solver: solver.into(),
        instance: instance.into(),
        family: "queens".into(),
        expected: Answer::Sat,
        solved: true,
        fuel: Weights::UNIFORM.fuel(&counts),
        counts: Some(counts),
    }
}

/// The three steps of the calibration study: fuel and time of `MiniSat` on a small archived set,
/// then the analysis.
#[cfg(unix)]
#[test]
fn calibration_study() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch("dojo-cli-calibrate");
    let d = dir.path();
    ok(d, &["export", "small.tar.gz", "--family", "queens"]);
    let (_, set) =
        dojo::archive::read_tar_gz(&std::fs::read(d.join("small.tar.gz")).unwrap()).unwrap();
    let solver = format!("minisat={}", minisat());
    let junk = format!("junk={}", write(d, "junk.wasm", b"not webassembly"));
    fails(
        d,
        &[
            "calibrate",
            "fuel",
            "--solver",
            &junk,
            "--set",
            "small.tar.gz",
        ],
        "junk: Invalid",
    );

    // Fuel: a row per formula, named after the archive, the categories adding up to the fuel
    let out = ok(
        d,
        &[
            "calibrate",
            "fuel",
            "--solver",
            &solver,
            "--set",
            "small.tar.gz",
            "--cap",
            "4_000_000_000",
            "--threads",
            "2",
            "--checker",
            "builtin",
            "--out",
            "fuel.csv",
        ],
    );
    assert_eq!(stdout(&out), "");
    assert!(
        stderr(&out).contains(&format!("[{0}/{0}] minisat small/", set.len())),
        "{}",
        stderr(&out)
    );
    let text = std::fs::read_to_string(d.join("fuel.csv")).unwrap();
    assert_eq!(text.lines().next(), Some(calibrate::fuel_header().as_str()));
    let rows = calibrate::read_csv(&text, FuelRow::parse).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.instance.clone()).collect::<Vec<_>>(),
        set.iter()
            .map(|i| format!("small/{}", i.name))
            .collect::<Vec<_>>()
    );
    for row in &rows {
        assert!(row.solved, "{row:?}");
        let counts = row.counts.expect("a finished run is counted");
        assert_eq!(Weights::UNIFORM.fuel(&counts), row.fuel, "{row:?}");
    }

    // Time: a native "solver" that always answers UNSAT (wrong here, every formula is
    // satisfiable). With --fuel only what fuel measured: the first run as if it hadn't
    // finished (once, no sandbox), the last one not at all.
    let native = d.join("native");
    std::fs::write(
        &native,
        "#!/bin/sh\ncat >/dev/null\necho 's UNSATISFIABLE'\n",
    )
    .unwrap();
    std::fs::set_permissions(&native, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut measured = rows.clone();
    measured[0].counts = None;
    measured.pop();
    let mut csv = calibrate::fuel_header() + "\n";
    for row in &measured {
        csv += &(row.csv() + "\n");
    }
    std::fs::write(d.join("measured.csv"), csv).unwrap();
    let with_native = format!("minisat={}:{}", minisat(), native.display());
    let out = ok(
        d,
        &[
            "calibrate",
            "time",
            "--solver",
            &with_native,
            "--set",
            "small.tar.gz",
            "--fuel",
            "measured.csv",
            "--runs",
            "2",
            "--timeout",
            "20",
            "--scratch",
            "runs",
        ],
    );
    let times = calibrate::read_csv(&stdout(&out), TimeRow::parse).unwrap();
    assert_eq!(times.len(), rows.len() - 1, "{}", stdout(&out));
    assert_eq!(
        (times[0].native_ms.len(), times[0].wasm_ms),
        (1, None),
        "{:?}",
        times[0]
    );
    for t in &times[1..] {
        assert_eq!(t.native_ms.len(), 2, "{t:?}");
        assert!(t.wasm_ms.is_some(), "{t:?}");
    }
    assert!(times.iter().all(|t| !t.native_solved));
    std::fs::write(d.join("time.csv"), stdout(&out)).unwrap();

    // Too few usable runs for an analysis (none was solved natively)
    fails(
        d,
        &["calibrate", "analyze", "fuel.csv", "time.csv"],
        "no time for minisat on small/",
    );
    fails(
        d,
        &[
            "calibrate",
            "analyze",
            "measured.csv",
            "time.csv",
            "--min-ms",
            "0",
        ],
        "only 0 usable runs",
    );
    fails(
        d,
        &[
            "calibrate",
            "analyze",
            "fuel.csv",
            "time.csv",
            "--min-ms",
            "soon",
        ],
        "--min-ms takes a number",
    );

    // Synthetic runs of two solvers, enough of them: the report has every section
    let (mut fuel, mut time) = (Vec::new(), Vec::new());
    for (solver, loads) in [("a", 0.1), ("b", 0.4)] {
        for i in 1..=12u32 {
            let row = fuel_row(
                solver,
                &format!("set/f{i}"),
                u64::from(i) * 1_000_000,
                loads,
            );
            let counts = row.counts.unwrap();
            #[expect(clippy::cast_precision_loss, reason = "small synthetic counts")]
            let ms = 2e-6 * counts.ops[Category::Load as usize] as f64
                + 0.5e-6 * counts.ops[Category::Alu as usize] as f64;
            time.push(TimeRow {
                solver: solver.into(),
                instance: row.instance.clone(),
                native_ms: vec![ms],
                native_solved: true,
                wasm_ms: Some(2.0 * ms),
            });
            fuel.push(row);
        }
    }
    let mut fuel_csv = calibrate::fuel_header() + "\n";
    for row in &fuel {
        fuel_csv += &(row.csv() + "\n");
    }
    let mut time_csv = String::from(calibrate::TIME_HEADER) + "\n";
    for row in &time {
        time_csv += &(row.csv() + "\n");
    }
    std::fs::write(d.join("synthetic-fuel.csv"), fuel_csv).unwrap();
    std::fs::write(d.join("synthetic-time.csv"), time_csv).unwrap();
    let out = ok(
        d,
        &[
            "calibrate",
            "analyze",
            "synthetic-fuel.csv",
            "synthetic-time.csv",
            "--budget",
            "50_000_000",
            "--min-ms",
            "0",
        ],
    );
    let report = stdout(&out);
    assert!(
        report.starts_with("24 runs of 2 solvers on 12 formulas; 24 solved"),
        "{report}"
    );
    for section in [
        "### Fuel against native time",
        "### Weights",
        "### Rankings by PAR-2",
        "### Sandbox against native",
        "| a | 2.00× |",
        "- queens: 12",
    ] {
        assert!(report.contains(section), "{section}\n{report}");
    }
}

/// Kills the worker if the test fails before it exits by itself.
struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A request to the worker, `None` if it doesn't answer (yet): the status and the body.
fn http(port: u16, method: &str, path: &str, token: &str, body: &[u8]) -> Option<(u16, String)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .ok()?;
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\
         Authorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).ok()?;
    stream.write_all(body).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let status = response.split(' ').nth(1)?.parse().ok()?;
    let body = response.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    Some((status, body.to_string()))
}

/// A request the stub website got: its request line, authorization and body.
type Received = (String, String, Vec<u8>);

/// A stub of the website on a free port: answers every request with 204 and passes it on.
fn website() -> (u16, mpsc::Receiver<Received>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sent, received) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            let header_end = loop {
                let Ok(n) = stream.read(&mut buffer) else {
                    break None;
                };
                if n == 0 {
                    break None;
                }
                request.extend_from_slice(&buffer[..n]);
                if let Some(i) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    break Some(i + 4);
                }
            };
            let Some(header_end) = header_end else {
                continue;
            };
            let head = String::from_utf8_lossy(&request[..header_end]).to_string();
            let header = |name: &str| {
                head.lines()
                    .find_map(|l| {
                        let (key, value) = l.split_once(':')?;
                        key.eq_ignore_ascii_case(name)
                            .then(|| value.trim().to_string())
                    })
                    .unwrap_or_default()
            };
            let length: usize = header("content-length").parse().unwrap_or(0);
            while request.len() < header_end + length {
                let Ok(n) = stream.read(&mut buffer) else {
                    break;
                };
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..n]);
            }
            let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
            let line = head.lines().next().unwrap_or_default().to_string();
            let body = request[header_end..].to_vec();
            if sent.send((line, header("authorization"), body)).is_err() {
                break;
            }
        }
    });
    (port, received)
}

/// `dojo serve` with every setting: it answers, verifies a community benchmark, reports the
/// verdict to the website with the check's token and exits once it has been idle.
#[test]
fn serve_verifies_benchmarks_and_exits_when_idle() {
    let dir = scratch("dojo-cli-serve");
    let d = dir.path();
    let (site, reports) = website();
    let port = {
        let free = TcpListener::bind("127.0.0.1:0").unwrap();
        free.local_addr().unwrap().port()
    };
    let log = std::fs::File::create(d.join("worker.log")).unwrap();
    let started = Instant::now();
    let mut worker = Worker(
        dojo(d)
            .arg("serve")
            .env("DOJO_TOKEN", TOKEN)
            .env("APP_URL", format!("http://127.0.0.1:{site}/"))
            .env("PORT", port.to_string())
            .env("DOJO_IDLE_EXIT_SECS", "0")
            .env("DOJO_CHECKER", "builtin")
            .env("DOJO_THREADS", "2")
            .env("DOJO_MEMORY_MB", "256")
            .env("DOJO_CACHE_DIR", d)
            .env("DOJO_CHECK_SECS", "30")
            .env("DOJO_CHECK_MEMORY_MB", "512")
            .env("DOJO_CHECKS", "2")
            .env("DOJO_PROOF_DIR", d.join("proofs"))
            .env("DOJO_PROOF_MAX_MB", "4")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let health = loop {
        if let Some(answer) = http(port, "GET", "/health", "", b"") {
            break answer;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the worker didn't start"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(health, (200, "ok".into()));
    let pool = format!("/pools/{}", "a".repeat(64));
    assert_eq!(http(port, "GET", &pool, TOKEN, b"").unwrap().0, 404);
    assert_eq!(http(port, "GET", &pool, "guess", b"").unwrap().0, 401);
    let job = format!(
        "/jobs?id=1&season=1&seed=s&report_token=t&rules={}",
        RULES_VERSION + 1
    );
    assert_eq!(http(port, "POST", &job, TOKEN, b"\0asm").unwrap().0, 409);

    // MiniSat solves this one at once: too easy for a hidden set
    let check = format!("/benchmarks?id=5&report_token=check-5&rules={RULES_VERSION}");
    let formula = b"p cnf 2 2\n1 2 0\n-1 0\n";
    assert_eq!(http(port, "POST", &check, TOKEN, formula).unwrap().0, 202);
    let (line, auth, body) = reports
        .recv_timeout(Duration::from_secs(120))
        .expect("a verdict in time");
    assert!(line.starts_with("POST /dojo/benchmark-report "), "{line}");
    assert_eq!(auth, "Bearer check-5");
    let report: BenchmarkReport = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        report,
        BenchmarkReport {
            id: 5,
            verdict: Verdict::Refused {
                reason: Refusal::TooEasy
            }
        }
    );

    // Nothing left to do: it exits by itself (it looks every 10 seconds)
    let status = loop {
        if let Some(status) = worker.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "the idle worker didn't exit"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    let log = std::fs::read_to_string(d.join("worker.log")).unwrap();
    assert!(status.success(), "{log}");
    for line in [
        "checking proofs with the built-in checker",
        &format!("dojo worker on :{port}, reporting to http://127.0.0.1:{site}/"),
        "benchmark #5: \"Refused { reason: TooEasy }\"",
        "s, exiting",
    ] {
        assert!(log.contains(line), "{line}\n{log}");
    }
    assert!(d.join("proofs").is_dir());
    drop(reports);
}
