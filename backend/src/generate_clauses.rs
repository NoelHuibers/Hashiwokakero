use crate::parse_input::{GameBoard, Bridge, Island};

const MAX_BRIDGES: u8 = 8;

fn generate(game: GameBoard) {
    let mut dimacs: Vec<Vec<i32>> = vec![];

    for island in game.islands {
        let bridges: Vec<Bridge> = game.bridges.clone().into_iter().filter_map(|bridge| {
            if bridge.from != (island.x, island.y) && bridge.to != (island.x, island.y) {
                return None;
            } else {
                return Some(bridge)
            }
        }).collect();
        dimacs.append(&mut outgoing_bridges((island, bridges)))
    }
    
}

// Rule 1
fn outgoing_bridges(island: (Island, Vec<Bridge>)) -> Vec<Vec<i32>> {
    let mut bounds: Vec<Vec<i32>> = vec![];
    let lhs_bridges: Vec<i32> = island.1.iter().map(|bridge| format!("{}{}{}{}1", bridge.from.0, bridge.from.1, bridge.to.0, bridge.to.1).parse().unwrap()).collect();
    let rhs_bridges: Vec<i32> = island.1.iter().map(|bridge| format!("{}{}{}{}2", bridge.from.0, bridge.from.1, bridge.to.0, bridge.to.1).parse::<i32>().unwrap()).collect();
    let clauses = [lhs_bridges, rhs_bridges].concat();
    for i in 0..island.0.connections {
        // Conversion shouldn't fail as our max connections limit is 8
        let mut lower_bound = clauses[i as usize..].to_vec();
        lower_bound.push(0);
        bounds.push(lower_bound);
    }
    for i in 0..MAX_BRIDGES - island.0.connections as u8 {
        let mut upper_bound: Vec<i32> = clauses[i as usize..].iter().map(|v| -v).collect();
        upper_bound.push(0);
        bounds.push(upper_bound);
    }
    return bounds;
}
