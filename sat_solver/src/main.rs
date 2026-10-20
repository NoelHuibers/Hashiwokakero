use std::{
    fs,
    process::ExitCode,
    time::{Duration, Instant},
};

// The library's modules, not a copy of them: compiled (and tested) once
use sat_solver::{cdcl, cli, dpll, parse, profiling};

fn main() -> ExitCode {
    let arguments = cli::cli();

    // clap made sure there is an output path with --DRUP
    let logger = arguments
        .outputpath
        .as_ref()
        .filter(|_| arguments.drup)
        .map(|path| format!("{path}.log"));

    if arguments.flamegraph {
        profiling::enable();
        profiling::start("main");
    }

    let (vars, v_count, c_count) = match parse::parse(&arguments.inputpath) {
        Ok(formula) => formula,
        Err(e) => {
            eprintln!("Error: {}: {e}", arguments.inputpath);
            return ExitCode::FAILURE;
        }
    };

    if arguments.flamegraph {
        profiling::start("main_solve");
    }

    // A limit too far away to represent is no limit (adding it used to panic)
    let deadline = arguments
        .timeout
        .and_then(|t| Duration::try_from_secs_f64(t).ok())
        .and_then(|t| Instant::now().checked_add(t));
    let cert = if arguments.solver == "dpll" {
        dpll::DPLL::new(vars, v_count, c_count, arguments.heuristic, arguments.depth)
            .with_deadline(deadline)
            .solve()
    } else {
        cdcl::CDCL::new(
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
        .solve()
    };

    if arguments.flamegraph {
        profiling::end("main_solve");
    }

    match cert {
        Ok(sol) => {
            println!("{sol}");
            if let Some(output) = arguments.outputpath
                && let Err(why) = fs::write(&output, format!("{sol}\n"))
            {
                eprintln!("Error: Couldn't write {output}: {why}");
                return ExitCode::FAILURE;
            }
        }
        Err(e) => {
            eprintln!("Sat-Solver failed to solve: {e}");
            return ExitCode::FAILURE;
        }
    }

    if arguments.flamegraph {
        profiling::end("main");
        //to create flamegraph for profiling. Portrayal does not work properly with usage of multithreading (-> cputime.rs)
        if let Err(why) = profiling::dump_html("flamegraph.html") {
            eprintln!("Error: Couldn't write flamegraph.html: {why}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
