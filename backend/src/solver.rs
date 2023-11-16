use splr::*;
use std::io;

pub fn solve(filepath: &str) -> io::Result<Certificate> {
    let config = Config::from(filepath);
    match Solver::build(&config) {
        Ok(mut s) => match s.solve() {
            Ok(ans) => {
                println!("{:?}", ans);
                Ok(ans)
            }
            Err(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Only DIMACS CNF files are supported",
            )),
        },
        Err(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Only DIMACS CNF files are supported",
        )),
    }
}
