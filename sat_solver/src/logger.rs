//! DRAT proofs of unsatisfiability, as the solvers write them.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// A text DRAT proof being written to a file: learned clauses (lemmas) and deleted ones, and
/// the empty clause once the formula is refuted. Buffered, flushed when dropped.
pub struct ProofLog {
    out: BufWriter<File>,
    line: String,
}

impl std::fmt::Debug for ProofLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProofLog").finish_non_exhaustive()
    }
}

impl ProofLog {
    /// A new (empty) proof file at `path`.
    pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            out: BufWriter::with_capacity(1 << 16, File::create(path)?),
            line: String::new(),
        })
    }

    fn write(&mut self, prefix: &str, clause: &[i32]) {
        use std::fmt::Write as _;
        self.line.clear();
        self.line.push_str(prefix);
        for lit in clause {
            let _ = write!(self.line, "{lit} ");
        }
        self.line.push_str("0\n");
        // A proof that can't be written is no proof, the checker will say so
        let _ = self.out.write_all(self.line.as_bytes());
    }

    /// A lemma: follows from the clauses so far.
    pub fn add(&mut self, clause: &[i32]) {
        self.write("", clause);
    }

    /// A clause that is no longer needed.
    pub fn delete(&mut self, clause: &[i32]) {
        self.write("d ", clause);
    }

    /// The empty clause, the end of a refutation.
    pub fn refuted(&mut self) {
        self.add(&[]);
        let _ = self.out.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_text_drat() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("proof.drat");
        std::fs::write(&path, "old").unwrap();
        {
            let mut log = ProofLog::create(&path).unwrap();
            log.add(&[1, -2]);
            log.delete(&[3]);
            log.refuted();
            assert_eq!(format!("{log:?}"), "ProofLog { .. }");
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "1 -2 0\nd 3 0\n0\n"
        );
        assert!(ProofLog::create(dir.path().join("no/such/dir")).is_err());
    }
}
