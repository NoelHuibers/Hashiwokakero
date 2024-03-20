use std::io::{BufWriter, Write};

pub(crate) struct ProofLogger {
    writer: BufWriter<std::fs::File>,
}

impl ProofLogger {
    pub fn new(path: &str) -> std::io::Result<Self> {
        let file = std::fs::File::create(path)?;
        let writer = BufWriter::new(file);
        Ok(ProofLogger { writer })
    }

    pub fn log_clause(&mut self, clause: &[i32], operation: char) -> std::io::Result<()> {
        // Prefix with 'a' for add or 'd' for delete
        write!(self.writer, "{} ", operation)?;
        // Write the clause
        for &lit in clause {
            write!(self.writer, "{} ", lit)?;
        }
        // End with 0
        writeln!(self.writer, "0")
    }
}
