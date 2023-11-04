use crate::parse_input::GameBoard;

pub fn print_infos(game_board: &GameBoard) {
    println!("Puzzle Infos:");
    println!("Rows: {}", game_board.rows);
    println!("Cols: {}", game_board.cols);
    println!("Islands:");
    for island in &game_board.islands {
        println!("Island at ({}, {}), Allowed connections: {}", island.x, island.y, island.connections);
    }
}
