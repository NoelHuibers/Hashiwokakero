use std::{
    fs::File,
    io::Write,
    time::{Duration, Instant},
};

// The library's modules, not a copy of them: compiled (and tested) once
use sat_solver::{cdcl, cli, dpll, parse, profiling};

fn main() {
    let arguments = cli::cli();

    if arguments.flamegraph {
        profiling::enable();
        profiling::start("main");
    }

    let (vars, v_count, c_count) = parse::parse(&arguments.inputpath).unwrap();

    if arguments.flamegraph {
        profiling::start("main_solve");
    }

    let logger = match (arguments.outputpath.clone(), arguments.drup) {
        (Some(path), true) => Some(format!("{path}.log")),
        (None, true) => panic!("Please specify an output path when using the drup flag."),
        _ => None,
    };

    let deadline = arguments
        .timeout
        .map(|t| Instant::now() + Duration::from_secs_f64(t));
    let cert = match arguments.solver.as_str() {
        "dpll" => dpll::DPLL::new(vars, v_count, c_count, arguments.heuristic, arguments.depth)
            .with_deadline(deadline)
            .solve(),
        "cdcl" => cdcl::CDCL::new(
            vars,
            v_count,
            c_count,
            arguments.heuristic,
            arguments.subsumed_clauses,
            arguments.threshold,
            arguments.luby,
            arguments.factor,
            arguments.k,
            arguments.m,
            logger,
        )
        .with_deadline(deadline)
        .solve(),
        otherwise => panic!("{otherwise} is not a valid mode."),
    };

    if arguments.flamegraph {
        profiling::end("main_solve");
    }

    match cert {
        Ok(sol) => {
            println!("{sol}");
            if let Some(output) = arguments.outputpath {
                let res = format!("{sol}\n");
                let mut file = match File::create(output) {
                    Ok(file) => file,
                    Err(why) => panic!("Couldn't create file: {why}"),
                };
                let _ = file.write_all(res.as_bytes());
                // Write the content to the file
            }
        }
        Err(e) => println!("Sat-Solver failed to solve: {e:?}"),
    }

    if arguments.flamegraph {
        profiling::end("main");
        //to create flamegraph for profiling. Portrayal does not work properly with usage of multithreading (-> cputime.rs)
        profiling::dump_html("flamegraph.html").unwrap();
    }
}
