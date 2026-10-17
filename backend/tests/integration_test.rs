use backend::{
    generate_clauses::generate, parse_input::parse_input, reconstruct, solver,
    writer::generate_dimacs,
};

#[test]
fn test_integration() {
    for i in 1..=24 {
        println!("Test {i}");
        let input_file = format!("./input/test{i}.txt");
        let output_file = format!("./output/test{i}.txt");
        let game_board = match parse_input(&input_file) {
            Ok(game_board) => game_board,
            Err(err) => {
                eprintln!("Error: {err}");
                continue;
            }
        };

        let (clauses, var_map) = generate(&game_board);
        let out_file = format!("./output/test{i}.cnf");
        if let Err(e) = generate_dimacs(&clauses, var_map.keys().len(), &out_file) {
            eprint!("{e}");
            continue;
        }

        let certificate = match solver::solve(&out_file) {
            Ok(certificate) => certificate,
            Err(err) => {
                eprintln!("Error: {err}");
                continue;
            }
        };

        if let Err(err) = solver::write_solution(certificate, &output_file) {
            eprintln!("Error: {err}");
            continue;
        }

        let content = reconstruct::get_content(&output_file);
        let res = reconstruct::reconstruct_puzzle(&content, &var_map, &game_board);
        print!("{res}");
    }
}
