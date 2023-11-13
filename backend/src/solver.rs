use splr::*;

//minor changes (from println! to Strings) for testing 
pub fn solve(clauses: Vec<Vec<i32>>) -> String {
    let result = match Certificate::try_from(clauses) {
        Ok(Certificate::SAT(ans)) => format!("SATISFIABLE: {:?}", ans),
        Ok(Certificate::UNSAT) => "UNSATISFIABLE".to_string(),
        Err(e) => panic!("UNKNOWN; {}", e),
    };

    println!("{}", result);

    result
}