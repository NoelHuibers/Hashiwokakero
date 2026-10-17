//! How long each family takes to generate: gen_time <family> <sizes...>
use std::time::Instant;

use dojo::Family;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let family = Family::from_id(&args[1]).expect("unknown family");
    for size in args[2..].iter().map(|s| s.parse::<u32>().expect("size")) {
        let t = Instant::now();
        let set = dojo::bench::generate_with("timing", &[(family, size)]);
        println!(
            "{} {size}: {:?} {:.1?} vars={} clauses={}",
            family.id(),
            set[0].expected,
            t.elapsed(),
            set[0].cnf.vars,
            set[0].cnf.clauses.len()
        );
    }
}
