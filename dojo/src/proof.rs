//! Checking UNSAT proofs, like the main track of the SAT Competition.
//!
//! A solver that answers `s UNSATISFIABLE` has to back it with a proof: it gets the path of a
//! file as its first argument (`/proof/proof.out`, see [`crate::sandbox`]) and writes the proof
//! there. Formats:
//!
//! - DRAT, text or binary (told apart like drat-trim does, see [`crate::drat`]). The default.
//! - LRAT, text or binary, with hints. A solver declares it with the line `c proof lrat` on
//!   stdout (any `c` line before or after the answer).
//!
//! The leaderboard checks with the SAT Competition's pipeline that ends in a formally verified
//! checker: drat-trim (backward checking) turns a DRAT proof into LRAT, which `cake_lpr` checks.
//! `cake_lpr` is compiled by `CakeML` from a checker proven correct in HOL4, so an accepted proof
//! doesn't depend on trusting drat-trim or our own code. LRAT proofs go to `cake_lpr` directly.
//! Without the two tools (a local `dojo run`) the built-in checkers in [`crate::drat`] and
//! [`crate::lrat`] take over: they check every lemma forward, drat-trim only the ones the
//! refutation needs, so they may reject a proof with a wrong but unused lemma that the
//! leaderboard accepts.
//!
//! Verdicts ([`Verdict`]):
//! - verified: the UNSAT answer counts
//! - rejected (the checker read the proof and found it wrong): the answer is wrong, which
//!   disqualifies, like a wrong model
//! - unverified (no proof, larger than the limit, the checker ran out of time or memory, or
//!   failed otherwise): counts as unsolved
//!
//! Checking costs no fuel, it has its own limits ([`CheckLimits`]), and at most
//! [`Checker::new`]'s `slots` checks run at the same time so their memory adds up predictably.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::cnf::Cnf;
use crate::drat::DratError;

/// The proof file's name in the solver's proof directory (the SAT Competition's name).
pub const PROOF_FILE: &str = "proof.out";

/// Where the proof directory appears to the solver.
pub const GUEST_DIR: &str = "/proof";

/// The path a solver gets as its first argument.
pub const GUEST_PATH: &str = "/proof/proof.out";

/// How a proof is written; text or binary is detected from its first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Drat,
    Lrat,
}

impl Format {
    /// The format a solver declared on stdout: `c proof lrat` makes it LRAT, otherwise DRAT.
    pub fn declared(stdout: &str) -> Self {
        let lrat = stdout.lines().any(|line| {
            let mut words = line.split_whitespace();
            words.next() == Some("c")
                && words.next() == Some("proof")
                && words.next().is_some_and(|f| f.eq_ignore_ascii_case("lrat"))
        });
        if lrat { Self::Lrat } else { Self::Drat }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Drat => "DRAT",
            Self::Lrat => "LRAT",
        }
    }
}

/// What checking a proof gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Verified,
    /// The proof is wrong: the UNSAT answer counts as wrong.
    Rejected(String),
    /// No verdict either way: the UNSAT answer counts as unsolved.
    Unverified(String),
}

/// Time and memory for checking one proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckLimits {
    /// For drat-trim and `cake_lpr` together.
    pub time: Duration,
    /// For each of the external checkers (address space), on Linux with `prlimit`.
    pub memory_mb: u64,
}

impl Default for CheckLimits {
    fn default() -> Self {
        Self {
            time: Duration::from_secs(300),
            memory_mb: 1024,
        }
    }
}

/// The programs that check proofs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tools {
    /// drat-trim and `cake_lpr`, as on the leaderboard.
    Verified {
        drat_trim: PathBuf,
        cake_lpr: PathBuf,
    },
    /// Our own checkers ([`crate::drat`], [`crate::lrat`]).
    Builtin,
}

impl Tools {
    /// drat-trim and `cake_lpr` from `DOJO_DRAT_TRIM` and `DOJO_CAKE_LPR`, or else from the
    /// `PATH` (as `drat-trim` and `cake_lpr`); `None` unless both are there.
    pub fn find() -> Option<Self> {
        let tool = |var: &str, name: &str| {
            std::env::var_os(var)
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .or_else(|| on_path(name))
        };
        Some(Self::Verified {
            drat_trim: tool("DOJO_DRAT_TRIM", "drat-trim")?,
            cake_lpr: tool("DOJO_CAKE_LPR", "cake_lpr")?,
        })
    }

    /// The verified tools if they are installed, the built-in checkers otherwise.
    pub fn find_or_builtin() -> Self {
        Self::find().unwrap_or(Self::Builtin)
    }

    pub fn verified(&self) -> bool {
        matches!(self, Self::Verified { .. })
    }

    /// For logs and the CLI.
    pub fn describe(&self) -> String {
        match self {
            Self::Verified {
                drat_trim,
                cake_lpr,
            } => format!(
                "drat-trim ({}) and cake_lpr ({}), like the leaderboard",
                drat_trim.display(),
                cake_lpr.display()
            ),
            Self::Builtin => "the built-in checker (drat-trim and cake_lpr aren't installed; \
                              the leaderboard checks with them, see the starter's README)"
                .into(),
        }
    }
}

/// An executable called `name` in a directory of the `PATH`.
fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// A counting semaphore: how many checks may run at once.
#[derive(Debug)]
struct Slots {
    free: Mutex<usize>,
    freed: Condvar,
}

struct Slot<'a>(&'a Slots);

impl Slots {
    fn take(&self) -> Slot<'_> {
        let mut free = self
            .free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *free == 0 {
            free = self
                .freed
                .wait(free)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *free -= 1;
        drop(free);
        Slot(self)
    }
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let mut free = self
            .0
            .free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *free += 1;
        drop(free);
        self.0.freed.notify_one();
    }
}

/// Checks proofs with the configured tools and limits. Cheap to clone, clones share the slots.
#[derive(Clone, Debug)]
pub struct Checker {
    pub tools: Tools,
    pub limits: CheckLimits,
    /// Where checks keep their files (the formula, drat-trim's LRAT).
    pub scratch: PathBuf,
    slots: Arc<Slots>,
}

impl Checker {
    pub fn new(tools: Tools, limits: CheckLimits, slots: usize) -> Self {
        Self {
            tools,
            limits,
            scratch: std::env::temp_dir(),
            slots: Arc::new(Slots {
                free: Mutex::new(slots.max(1)),
                freed: Condvar::new(),
            }),
        }
    }

    /// The built-in checkers with the default limits.
    pub fn builtin() -> Self {
        Self::new(Tools::Builtin, CheckLimits::default(), 1)
    }

    /// Checks that the proof in the file `proof` shows `cnf` unsatisfiable.
    pub fn check(&self, cnf: &Cnf, proof: &Path, format: Format) -> Verdict {
        let _slot = self.slots.take();
        match &self.tools {
            Tools::Builtin => self.check_builtin(cnf, proof, format),
            Tools::Verified {
                drat_trim,
                cake_lpr,
            } => match TempDir::new(&self.scratch, "dojo-check") {
                Ok(dir) => self.check_verified(cnf, proof, format, drat_trim, cake_lpr, &dir.0),
                Err(e) => Verdict::Unverified(format!("no scratch directory: {e}")),
            },
        }
    }

    fn check_builtin(&self, cnf: &Cnf, proof: &Path, format: Format) -> Verdict {
        let bytes = match std::fs::read(proof) {
            Ok(bytes) => bytes,
            Err(e) => return Verdict::Unverified(format!("the proof can't be read: {e}")),
        };
        let result = match format {
            Format::Drat => crate::drat::check(cnf, &bytes, self.limits.time),
            Format::Lrat => crate::lrat::check(cnf, &bytes, self.limits.time),
        };
        match result {
            Ok(()) => Verdict::Verified,
            Err(DratError::BudgetExceeded) => Verdict::Unverified(format!(
                "checking took longer than {} s",
                self.limits.time.as_secs()
            )),
            Err(e) => Verdict::Rejected(e.to_string()),
        }
    }

    fn check_verified(
        &self,
        cnf: &Cnf,
        proof: &Path,
        format: Format,
        drat_trim: &Path,
        cake_lpr: &Path,
        dir: &Path,
    ) -> Verdict {
        let deadline = Instant::now() + self.limits.time;
        let formula = dir.join("formula.cnf");
        if let Err(e) = std::fs::write(&formula, cnf.dimacs("")) {
            return Verdict::Unverified(format!("the formula can't be written: {e}"));
        }
        let lrat = match format {
            Format::Lrat => proof.to_path_buf(),
            Format::Drat => {
                let lrat = dir.join("proof.lrat");
                let seconds = self.limits.time.as_secs().max(1).to_string();
                let args: Vec<OsString> = vec![
                    formula.clone().into(),
                    proof.into(),
                    "-L".into(),
                    lrat.clone().into(),
                    "-w".into(),
                    "-t".into(),
                    seconds.into(),
                ];
                let run = run_limited(drat_trim, &args, self.limits.memory_mb, deadline, dir);
                match drat_trim_verdict(&run) {
                    Verdict::Verified => lrat,
                    other => return other,
                }
            }
        };
        // cake_lpr allocates its heap and stack at once, within the memory limit
        let heap = self.limits.memory_mb * 3 / 4;
        let stack = (self.limits.memory_mb / 8).max(16);
        let args: Vec<OsString> = vec![
            format!("--CML_HEAP_SIZE={heap}").into(),
            format!("--CML_STACK_SIZE={stack}").into(),
            formula.into(),
            lrat.into(),
        ];
        let run = run_limited(cake_lpr, &args, self.limits.memory_mb, deadline, dir);
        cake_lpr_verdict(&run, format)
    }
}

/// How an external checker ended.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Ended {
    /// Exited with this code (`None`: killed by a signal) and printed this (stdout and stderr).
    Exited(Option<i32>, String),
    TimedOut,
    NotStarted(String),
}

/// `prlimit` (util-linux) if it is installed, to limit the memory of the checkers.
fn prlimit() -> Option<&'static Path> {
    static PRLIMIT: OnceLock<Option<PathBuf>> = OnceLock::new();
    PRLIMIT.get_or_init(|| on_path("prlimit")).as_deref()
}

/// Runs `program` until `deadline` with at most `memory_mb` of address space, output to a file
/// in `dir` (a pipe could fill up and stall it).
fn run_limited(
    program: &Path,
    args: &[OsString],
    memory_mb: u64,
    deadline: Instant,
    dir: &Path,
) -> Ended {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let log = dir.join(format!("out-{}.txt", NEXT.fetch_add(1, Ordering::Relaxed)));
    let (out, err) = match std::fs::File::create(&log).and_then(|f| Ok((f.try_clone()?, f))) {
        Ok(files) => files,
        Err(e) => return Ended::NotStarted(e.to_string()),
    };
    let mut command = match prlimit() {
        Some(prlimit) => {
            let mut c = Command::new(prlimit);
            c.arg(format!("--as={}", memory_mb << 20))
                .arg("--")
                .arg(program);
            c
        }
        None => Command::new(program),
    };
    let child = command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => return Ended::NotStarted(format!("{}: {e}", program.display())),
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Ended::TimedOut;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Ended::NotStarted(e.to_string()),
        }
    };
    // The end of the output says what happened, cap what is read of it
    let output = std::fs::read(&log).unwrap_or_default();
    let tail = &output[output.len().saturating_sub(16 << 10)..];
    Ended::Exited(status.code(), String::from_utf8_lossy(tail).into_owned())
}

fn has_line(output: &str, line: &str) -> bool {
    output.lines().any(|l| l.trim() == line)
}

/// The last lines of a checker's output, for a message.
fn last_lines(output: &str) -> String {
    let lines: Vec<&str> = output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    lines[lines.len().saturating_sub(2)..]
        .iter()
        .fold(String::new(), |mut s, l| {
            if !s.is_empty() {
                s.push_str(" / ");
            }
            let _ = write!(s, "{l}");
            s
        })
}

/// drat-trim says `s VERIFIED`, `s NOT VERIFIED` or `s TIMEOUT`; running out of memory it
/// prints `c MEMOUT` and exits with 0.
fn drat_trim_verdict(run: &Ended) -> Verdict {
    match run {
        Ended::Exited(_, out) if has_line(out, "s VERIFIED") => Verdict::Verified,
        Ended::Exited(_, out) if has_line(out, "s TIMEOUT") => {
            Verdict::Unverified("drat-trim ran out of time".into())
        }
        Ended::Exited(_, out) if has_line(out, "s NOT VERIFIED") => {
            Verdict::Rejected(format!("drat-trim: {}", last_lines(out)))
        }
        Ended::TimedOut => Verdict::Unverified("drat-trim ran out of time".into()),
        Ended::Exited(code, out) => Verdict::Unverified(format!(
            "drat-trim failed ({}): {}",
            code.map_or_else(|| "killed".into(), |c| format!("exit code {c}")),
            last_lines(out)
        )),
        Ended::NotStarted(e) => Verdict::Unverified(format!("drat-trim didn't start: {e}")),
    }
}

/// `cake_lpr` prints `s VERIFIED UNSAT` to stdout, or says what is wrong on stderr and exits with
/// 0; exit codes 1 to 3 mean it ran out of heap, stack or memory. Rejecting the LRAT that
/// drat-trim wrote from a proof it verified is a disagreement between the two, not a wrong proof.
fn cake_lpr_verdict(run: &Ended, format: Format) -> Verdict {
    match run {
        Ended::Exited(_, out) if has_line(out, "s VERIFIED UNSAT") => Verdict::Verified,
        Ended::Exited(Some(0), out) => match format {
            Format::Lrat => Verdict::Rejected(format!("cake_lpr: {}", last_lines(out))),
            Format::Drat => Verdict::Unverified(format!(
                "cake_lpr rejected the LRAT drat-trim wrote: {}",
                last_lines(out)
            )),
        },
        Ended::TimedOut => Verdict::Unverified("cake_lpr ran out of time".into()),
        Ended::Exited(code, out) => Verdict::Unverified(format!(
            "cake_lpr failed ({}): {}",
            code.map_or_else(|| "killed".into(), |c| format!("exit code {c}")),
            last_lines(out)
        )),
        Ended::NotStarted(e) => Verdict::Unverified(format!("cake_lpr didn't start: {e}")),
    }
}

/// The checker that labels benchmark sets: drat-trim and `cake_lpr` if they are installed (the
/// labels come out the same either way, both accept every correct proof).
pub fn labelling() -> &'static Checker {
    static CHECKER: OnceLock<Checker> = OnceLock::new();
    CHECKER.get_or_init(|| {
        let limits = CheckLimits {
            time: Duration::from_secs(120),
            memory_mb: 2048,
        };
        Checker::new(Tools::find_or_builtin(), limits, 2)
    })
}

/// A directory that is removed with everything in it when this goes out of scope (also on a
/// panic).
#[derive(Debug)]
pub struct TempDir(pub PathBuf);

impl TempDir {
    /// A new, empty directory in `root`.
    pub fn new(root: &Path, prefix: &str) -> std::io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = root.join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // Left over from a process with the same id
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    /// The empty directory `root/name`, emptied if it exists; [`TempDir::keep`] keeps it.
    pub fn named(root: &Path, name: &str) -> std::io::Result<Self> {
        let path = root.join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Leaves the directory where it is.
    pub fn keep(self) -> PathBuf {
        let path = self.0.clone();
        std::mem::forget(self);
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (1 ∨ 2), (¬1 ∨ 2), (1 ∨ ¬2), (¬1 ∨ ¬2).
    fn cnf() -> Cnf {
        Cnf::new(2, vec![vec![1, 2], vec![-1, 2], vec![1, -2], vec![-1, -2]])
    }

    fn proof_file(dir: &TempDir, bytes: &[u8]) -> PathBuf {
        let path = dir.path().join(PROOF_FILE);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn formats_are_declared_on_stdout() {
        assert_eq!(Format::declared("s UNSATISFIABLE\n"), Format::Drat);
        assert_eq!(
            Format::declared("c proof lrat\ns UNSATISFIABLE\n"),
            Format::Lrat
        );
        assert_eq!(
            Format::declared("s UNSATISFIABLE\nc  proof   LRAT \n"),
            Format::Lrat
        );
        assert_eq!(Format::declared("c proof drat\n"), Format::Drat);
        assert_eq!(Format::declared("c the proof lrat\n"), Format::Drat);
        assert_eq!((Format::Drat.id(), Format::Lrat.id()), ("DRAT", "LRAT"));
    }

    #[test]
    fn builtin_checks_every_format() {
        let dir = TempDir::new(&std::env::temp_dir(), "dojo-proof-test").unwrap();
        let checker = Checker::builtin();
        let check = |bytes: &[u8], format| checker.check(&cnf(), &proof_file(&dir, bytes), format);
        assert_eq!(check(b"-1 0\n0\n", Format::Drat), Verdict::Verified);
        assert_eq!(check(b"a\x03\x00a\x00", Format::Drat), Verdict::Verified);
        let lrat = b"5 -1 0 2 4 0\n6 0 5 1 3 0\n";
        assert_eq!(check(lrat, Format::Lrat), Verdict::Verified);
        let mut binary = Vec::new();
        for step in [&[5, -1, 0, 2, 4, 0][..], &[6, 0, 5, 1, 3, 0]] {
            binary.push(b'a');
            for &n in step {
                crate::drat::write_varint(&mut binary, n);
            }
        }
        assert_eq!(check(&binary, Format::Lrat), Verdict::Verified);
        assert!(matches!(check(b"0\n", Format::Drat), Verdict::Rejected(_)));
        assert!(matches!(
            check(b"6 0 1 0\n", Format::Lrat),
            Verdict::Rejected(_)
        ));
        // A missing file is no proof
        let missing = checker.check(&cnf(), &dir.path().join("nothing"), Format::Drat);
        assert!(matches!(missing, Verdict::Unverified(_)));
    }

    #[test]
    fn checks_wait_for_a_slot() {
        let checker = Checker::new(Tools::Builtin, CheckLimits::default(), 1);
        let held = checker.slots.take();
        let running = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(|scope| {
            let waiting = scope.spawn(|| {
                let _slot = checker.slots.take();
                running.store(true, Ordering::SeqCst);
            });
            std::thread::sleep(Duration::from_millis(50));
            assert!(!running.load(Ordering::SeqCst));
            drop(held);
            waiting.join().unwrap();
        });
        assert!(running.load(Ordering::SeqCst));
    }

    #[test]
    fn temp_dirs_go_away_unless_kept() {
        let root = TempDir::new(&std::env::temp_dir(), "dojo-proof-root").unwrap();
        let path = TempDir::new(root.path(), "x").unwrap().path().to_path_buf();
        assert!(!path.exists());
        let named = TempDir::named(root.path(), "queens-01").unwrap();
        std::fs::write(named.path().join("f"), b"1").unwrap();
        let kept = named.keep();
        assert!(kept.join("f").exists());
        // Named again: emptied
        let again = TempDir::named(root.path(), "queens-01").unwrap();
        assert!(!again.path().join("f").exists());
    }

    /// Stand-ins for drat-trim and `cake_lpr`: shell scripts that print what the real ones print
    /// in each case.
    #[cfg(unix)]
    fn fake_tools(dir: &Path, drat_trim: &str, cake_lpr: &str) -> Tools {
        use std::os::unix::fs::PermissionsExt;
        let script = |name: &str, body: &str| {
            let path = dir.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        Tools::Verified {
            drat_trim: script("drat-trim", drat_trim),
            cake_lpr: script("cake_lpr", cake_lpr),
        }
    }

    /// The verified pipeline's verdicts, with stand-ins for the tools.
    #[cfg(unix)]
    #[test]
    fn verified_pipeline_verdicts() {
        let dir = TempDir::new(&std::env::temp_dir(), "dojo-fake-tools").unwrap();
        let proof = proof_file(&dir, b"-1 0\n0\n");
        let verdict = |drat_trim: &str, cake_lpr: &str, format, secs| {
            let limits = CheckLimits {
                time: Duration::from_secs(secs),
                memory_mb: 256,
            };
            let mut checker = Checker::new(fake_tools(dir.path(), drat_trim, cake_lpr), limits, 1);
            checker.scratch = dir.path().to_path_buf();
            checker.check(&cnf(), &proof, format)
        };
        let verified = r#"echo "s VERIFIED UNSAT""#;
        // drat-trim gets the formula and the proof, writes LRAT for cake_lpr
        let drat_ok = r#"grep -q "p cnf 2 4" "$1" && cp "$2" "$4" && echo "s VERIFIED""#;
        let cake_ok = r#"grep -q -- "-1 0" "$4" && echo "s VERIFIED UNSAT""#;
        assert_eq!(
            verdict(drat_ok, cake_ok, Format::Drat, 10),
            Verdict::Verified
        );
        // LRAT skips drat-trim
        assert_eq!(
            verdict("exit 9", cake_ok, Format::Lrat, 10),
            Verdict::Verified
        );
        let rejected = |v: Verdict| matches!(v, Verdict::Rejected(_));
        let unverified = |v: Verdict| matches!(v, Verdict::Unverified(_));
        assert!(rejected(verdict(
            "echo 'c ERROR: no conflict'; echo; echo 's NOT VERIFIED'; exit 1",
            verified,
            Format::Drat,
            10
        )));
        assert!(unverified(verdict(
            "echo 's TIMEOUT'",
            verified,
            Format::Drat,
            10
        )));
        assert!(unverified(verdict(
            "echo 'c MEMOUT: reallocation failed'",
            verified,
            Format::Drat,
            10
        )));
        assert!(unverified(verdict(
            "kill -9 $$",
            verified,
            Format::Drat,
            10
        )));
        // Out of time, killed
        let started = Instant::now();
        assert!(unverified(verdict("sleep 30", verified, Format::Drat, 1)));
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(unverified(verdict(
            "echo 's VERIFIED'",
            "sleep 30",
            Format::Lrat,
            1
        )));
        // cake_lpr rejecting the solver's LRAT is a wrong proof, rejecting drat-trim's LRAT a
        // disagreement between the tools
        let cake_no = "echo 'c empty clause not derived at end of proof' >&2";
        assert!(rejected(verdict("exit 9", cake_no, Format::Lrat, 10)));
        assert!(unverified(verdict(
            "echo 's VERIFIED'",
            cake_no,
            Format::Drat,
            10
        )));
        // cake_lpr out of heap
        assert!(unverified(verdict(
            "exit 9",
            "echo 'CakeML heap space exhausted.' >&2; exit 1",
            Format::Lrat,
            10
        )));
        // Tools that aren't there
        let missing = Tools::Verified {
            drat_trim: dir.path().join("none"),
            cake_lpr: dir.path().join("none"),
        };
        let checker = Checker::new(missing, CheckLimits::default(), 1);
        assert!(unverified(checker.check(&cnf(), &proof, Format::Drat)));
        assert!(unverified(checker.check(&cnf(), &proof, Format::Lrat)));
    }

    #[test]
    fn tools_are_found_on_the_path_or_by_variable() {
        assert!(on_path("surely-not-a-program-dojo").is_none());
        assert!(!Tools::Builtin.verified());
        assert!(Tools::Builtin.describe().contains("built-in"));
        let tools = Tools::Verified {
            drat_trim: "/x/drat-trim".into(),
            cake_lpr: "/x/cake_lpr".into(),
        };
        assert!(tools.verified() && tools.describe().contains("/x/cake_lpr"));
        assert_eq!(labelling().limits.time, Duration::from_secs(120));
    }

    /// The real drat-trim and `cake_lpr`, built with `dojo/checkers/build.sh` locally and in CI.
    /// Missing tools must fail rather than silently report an untested integration as passed.
    #[test]
    fn real_tools_check_every_format() {
        let tools = Tools::find().expect(
            "proof checkers missing: run dojo/checkers/build.sh and set DOJO_DRAT_TRIM and DOJO_CAKE_LPR",
        );
        let dir = TempDir::new(&std::env::temp_dir(), "dojo-real-tools").unwrap();
        let checker = Checker::new(tools, CheckLimits::default(), 1);
        let check =
            |cnf: &Cnf, bytes: &[u8], format| checker.check(cnf, &proof_file(&dir, bytes), format);
        assert_eq!(check(&cnf(), b"-1 0\n0\n", Format::Drat), Verdict::Verified);
        assert_eq!(
            check(&cnf(), b"a\x03\x00a\x00", Format::Drat),
            Verdict::Verified
        );
        let lrat = b"5 -1 0 2 4 0\n6 0 5 1 3 0\n";
        assert_eq!(check(&cnf(), lrat, Format::Lrat), Verdict::Verified);
        let mut binary = Vec::new();
        for step in [&[5, -1, 0, 2, 4, 0][..], &[6, 0, 5, 1, 3, 0]] {
            binary.push(b'a');
            for &n in step {
                crate::drat::write_varint(&mut binary, n);
            }
        }
        assert_eq!(check(&cnf(), &binary, Format::Lrat), Verdict::Verified);
        assert!(matches!(
            check(&cnf(), b"0\n", Format::Drat),
            Verdict::Rejected(_)
        ));
        assert!(matches!(
            check(&cnf(), b"", Format::Drat),
            Verdict::Rejected(_)
        ));
        assert!(matches!(
            check(&cnf(), b"5 -1 0 2 0\n6 0 5 1 3 0\n", Format::Lrat),
            Verdict::Rejected(_)
        ));
        // splr's proof of PHP(5,4), and the same proof for the satisfiable PHP(4,4)
        let php = crate::bench::pigeonhole(4, false).0;
        let proof = crate::bench::splr_proof(&php).unwrap();
        assert_eq!(check(&php, &proof, Format::Drat), Verdict::Verified);
        let sat = crate::bench::pigeonhole(4, true).0;
        assert!(!matches!(
            check(&sat, &proof, Format::Drat),
            Verdict::Verified
        ));
    }
}

// In a tests/ directory, so coverage doesn't count the tests themselves
#[cfg(test)]
#[path = "tests/checker_failures.rs"]
mod checker_failures;
