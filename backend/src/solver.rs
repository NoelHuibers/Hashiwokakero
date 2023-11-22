use splr::*;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::time::Instant;

pub fn solve(filepath: &str) -> io::Result<Certificate> {
    let start = Instant::now();
    let config = Config::from(filepath);
    let duration = start.elapsed();
    //println!("Time elapsed in solve() is: {:?}", duration);
    match Solver::build(&config) {
        Ok(mut s) => match s.solve() {
            Ok(ans) => Ok(ans),
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidInput, e.to_string())),
        },
        Err(e) => Err(io::Error::new(io::ErrorKind::InvalidInput, e.to_string())),
    }
}

pub fn write_solution(certificate: Certificate, output_file: &str) -> io::Result<()> {
    let start = Instant::now();
    if !output_file.ends_with(".txt") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Only .txt files are allowed as output from a solver.",
        ));
    }
    let file = File::create(output_file)?;
    let mut writer = BufWriter::new(file);

    match certificate {
        Certificate::SAT(model) => {
            writeln!(writer, "SAT")?;
            for literal in model {
                write!(writer, "{} ", literal)?;
            }
        }
        Certificate::UNSAT => {
            write!(writer, "UNSAT")?;
        }
    }
    let duration = start.elapsed();
    //println!("Time elapsed in write_solution() is: {:?}", duration);
    Ok(())
}
