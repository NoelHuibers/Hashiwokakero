use std::{collections::HashMap, fs::{File, remove_file}, io::Read, time::Instant};
use crate::{generate_clauses::BridgeCoord, parse_input::GameBoard};

pub fn reconstruct_puzzle(
    sat_output_path: &String,
    var_map: &HashMap<i32, BridgeCoord>,
    game_board: &GameBoard,
) -> String {
    let start = Instant::now();
    let mut output = String::new();
    let mut file = File::open(sat_output_path.clone())
        .expect(&format!("File {} does not exist", sat_output_path));
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect(&format!("Could not read contents of {}", sat_output_path));
    if contents.contains("UNSAT") {
        return "Problem is UNSAT\n".into()
    }
    let line = contents.lines().skip(1).next();
    if let Some(vars) = line {
        let vars = vars.split(' ').filter_map(|v| v.parse::<i32>().ok());
        let bridges: Vec<BridgeCoord> = vars
            .filter(|i| i.is_positive())
            .filter_map(|i| var_map.get(&i))
            .map(|v| *v)
            .collect();
        let mut bridge_map = HashMap::new();
        for bridge in bridges.clone() {
            let mut bridge_symbols = ('-', '|');
            if bridge.4 == 1 {
                let mut current = bridge;
                current.4 = 2;
                if bridges.contains(&current) {
                    bridge_symbols = ('=', '‖');
                }
            }
            if bridge.0 == bridge.2 {
                for y in bridge.1..bridge.3 {
                    bridge_map.entry((bridge.0, y)).or_insert(bridge_symbols.1);
                }
            } else {
                for x in bridge.0..bridge.2 {
                    bridge_map.entry((x, bridge.1)).or_insert(bridge_symbols.0);
                }
            }
        }
        let island_map = game_board
            .islands
            .iter()
            .map(|i| ((i.x, i.y), i.connections))
            .collect::<HashMap<_, _>>();
        for row in 0..game_board.rows {
            for col in 0..game_board.cols {
                if let Some(num) = island_map.get(&(col, row)) {
                    output.push(format!("{}", num).chars().next().unwrap());
                    continue;
                }
                if let Some(bridge) = bridge_map.get(&(col.into(), row.into())) {
                    output.push(*bridge);
                    continue;
                }
                output.push('.');
            }
            output.push('\n');
        }
    } else {
        output ="Problem had no variables".into();
    }
    let duration = start.elapsed();
    //println!("Time elapsed in reconstruct_puzzle() is: {:?}", duration);
    output
}

#[test]
fn should_parse_sat_output() {
    let path = "test.txt";
    let mut file = File::create(path).unwrap();
    let content = "SAT\n1 2 3 -4 5 -6 7 -8";
    std::io::Write::write_all(&mut file, content.as_bytes()).unwrap();
    let var_map = HashMap::from([
        (1, (0, 0, 0, 2, 1)),
        (2, (0, 0, 0, 2, 2)),
        (3, (0, 0, 2, 0, 1)),
        (4, (0, 0, 2, 0, 2)),
        (5, (0, 2, 2, 2, 1)),
        (6, (0, 2, 2, 2, 2)),
        (7, (2, 0, 2, 2, 1)),
        (8, (2, 0, 2, 2, 2)),
    ]);
    let game_board = GameBoard {
        rows: 5,
        cols: 3,
        islands: vec![
            crate::parse_input::Island {
                x: 0,
                y: 0,
                connections: 3,
            },
            crate::parse_input::Island {
                x: 0,
                y: 2,
                connections: 3,
            },
            crate::parse_input::Island {
                x: 2,
                y: 0,
                connections: 2,
            },
            crate::parse_input::Island {
                x: 2,
                y: 2,
                connections: 2,
            },
        ],
        bridges: vec![
            crate::parse_input::Bridge {
                from: (0, 0),
                to: (0, 2),
            },
            crate::parse_input::Bridge {
                from: (0, 0),
                to: (2, 0),
            },
            crate::parse_input::Bridge {
                from: (2, 0),
                to: (2, 2),
            },
            crate::parse_input::Bridge {
                from: (0, 2),
                to: (2, 2),
            },
        ],
    };
    reconstruct_puzzle(&path.to_string(), &var_map, &game_board);
    remove_file(path).unwrap();
}
