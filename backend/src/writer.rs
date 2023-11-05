use std::fs::File;
use std::io::{BufWriter, Write};

fn generate_dimacs(
    clauses: &Vec<Vec<i32>>,
    variables: i32,
    output_filename: &str,
) -> std::io::Result<()> {
    let file = File::create(output_filename)?;
    let mut writer = BufWriter::new(file);

    writeln!(writer, "c DIMACS file generated")?;
    writeln!(writer, "p cnf {} {}", variables, clauses.len())?;

    for clause in clauses {
        for literal in clause {
            write!(writer, "{} ", literal)?;
        }
        writeln!(writer, "0")?;
    }

    Ok(())
}
