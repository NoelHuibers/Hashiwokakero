use std::{fs::OpenOptions, io::Write};

pub fn log_clause(clause: &[i32], operation: char, path: &str) {
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .unwrap();

    // Write a new line to the file
    write!(file, "{} ", operation).unwrap();
    // Write the clause
    for &lit in clause {
        write!(file, "{} ", lit).unwrap();
    }
    // End with 0
    let _ = writeln!(file, "0");
}
