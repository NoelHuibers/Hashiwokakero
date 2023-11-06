use std::{collections::HashMap, vec};

use crate::parse_input::{parse_input, Bridge, GameBoard, Island};

const MAX_BRIDGES: u8 = 8;

fn generate(game: GameBoard) -> Vec<Vec<i32>> {
    let mut dimacs: Vec<Vec<i32>> = vec![];

    for island in game.islands.clone() {
        let bridges: Vec<Bridge> = game
            .bridges
            .clone()
            .into_iter()
            .filter_map(|bridge| {
                if bridge.from != (island.x, island.y) && bridge.to != (island.x, island.y) {
                    return None;
                } else {
                    return Some(bridge);
                }
            })
            .collect();
        // Bridges are duplicated because we allow two bridges between islands
        dimacs.append(&mut outgoing_bridges((
            island,
            [bridges.clone(), bridges].concat(),
        )))
    }
    dimacs.append(&mut connected_bridges(game.bridges, game.islands));
    return dimacs;
}

// Rule 1
fn outgoing_bridges(island: (Island, Vec<Bridge>)) -> Vec<Vec<i32>> {
    let mut bounds: Vec<Vec<i32>> = vec![];
    let lhs_bridges: Vec<i32> = island
        .1
        .iter()
        .map(|bridge| gen_bridge_name(bridge, 1))
        .collect();
    let rhs_bridges: Vec<i32> = island
        .1
        .iter()
        .map(|bridge| gen_bridge_name(&bridge, 2))
        .collect();
    let clauses = [lhs_bridges, rhs_bridges].concat();
    let bridge_nr = island.0.connections;
    for i in 0..bridge_nr {
        // Conversion shouldn't fail as our max connections limit is 8
        bounds.push(clauses[i as usize..].to_vec());
    }
    for i in 0..MAX_BRIDGES - bridge_nr as u8 {
        bounds.push(clauses[i as usize..].iter().map(|v| -v).collect());
    }
    return bounds;
}

// Rule 2
fn connected_bridges(mut bridges: Vec<Bridge>, islands: Vec<Island>) -> Vec<Vec<i32>> {
    let island_map = islands
        .iter()
        .map(|island| ((island.x, island.y), island.connections))
        .collect::<HashMap<_, _>>();
    bridges.sort_by(|a, b| {
        let bridge_sum_a = island_map.get(&(a.from.0, a.from.1)).unwrap()
            + island_map.get(&(a.to.0, a.to.1)).unwrap();
        let bridge_sum_b = island_map.get(&(b.from.0, b.from.1)).unwrap()
            + island_map.get(&(b.to.0, b.to.1)).unwrap();
        bridge_sum_b.cmp(&bridge_sum_a)
    });
    let total_bridges = islands.iter().fold(0, |acc, i| acc + i.connections) / 2;
    bridges[..total_bridges as usize]
        .iter()
        .map(|bridge| vec![gen_bridge_name(bridge, 1), gen_bridge_name(bridge, 2)])
        .collect::<Vec<Vec<i32>>>()
}

fn gen_bridge_name(bridge: &Bridge, idx: i32) -> i32 {
    format!(
        "{}{}{}{}{}",
        // Indices are incremented in order to avoid illegal DIMACS vars
        bridge.from.0 + 1,
        bridge.from.1 + 1,
        bridge.to.0 + 1,
        bridge.to.1 + 1,
        idx
    )
    .parse()
    .unwrap()
}

#[test]
fn should_gen_fst_rule() {
    let game = parse_input("./backend/input/test1.txt").unwrap();
    println!("Clauses: \n{:?}", generate(game));
}
