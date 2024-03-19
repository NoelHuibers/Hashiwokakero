use std::{fs::File, io::Write};

use flame;

pub mod cdcl;
pub mod cli;
pub mod dpll;
mod heuristics;
pub mod parse;
pub mod preprocessing;
pub mod randomrestarts;

fn main() {
    let arguments = cli::cli();

    if arguments.flamegraph {
        flame::start("main");
    }

    let (vars, v_count, c_count) = parse::parse(&arguments.inputpath).unwrap();

    if arguments.flamegraph {
        flame::start("main_solve");
    }

    let cert = match arguments.solver.as_str() {
        "dpll" => {
            dpll::DPLL::new(vars, v_count, c_count, arguments.heuristic, arguments.depth).solve()
        }
        "cdcl" => cdcl::CDCL::new(
            vars,
            v_count,
            c_count,
            arguments.heuristic,
            arguments.subsumed_clauses,
            None,
            false,
            None,
            arguments.k,
            arguments.m,
        )
        .solve(),
        otherwise => panic!("{} is not a valid mode.", otherwise),
    };

    if arguments.flamegraph {
        flame::start("main_solve");
    }

    match cert {
        Ok(sol) => {
            println!("{}", sol);
            if let Some(output) = arguments.outputpath {
                let res = format!("{}\n", sol);
                let mut file = match File::create(output) {
                    Ok(file) => file,
                    Err(why) => panic!("Couldn't create file: {}", why),
                };
                let _ = file.write_all(res.as_bytes());
                // Write the content to the file
            };
        }
        Err(e) => println!("Sat-Solver failed to solve: {:?}", e),
    }

    if arguments.flamegraph {
        flame::end("main");
        //to create flamegraph for profiling. Portrayal does not work properly with usage of multithreading (-> cputime.rs)
        flame::dump_html(std::fs::File::create("src/visuals/graphs/flamegraph.html").unwrap())
            .unwrap();
    }
}
