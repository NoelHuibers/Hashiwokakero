use splr::*;

pub fn solve(clauses: Vec<Vec<i32>>) {
    match Certificate::try_from(clauses) {
        Ok(Certificate::SAT(ans)) => println!("s SATISFIABLE: {:?}", ans),
        Ok(Certificate::UNSAT) => println!("s UNSATISFIABLE"),
        Err(e) => panic!("s UNKNOWN; {}", e),
    }
}

pub fn parse(input_file: &str) {
    //TODO: parse input file
}
