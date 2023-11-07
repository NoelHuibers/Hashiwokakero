use std::{collections::HashMap, vec};

use crate::parse_input::{Bridge, GameBoard, Island};

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
            bridges,
        )))
    }
    dimacs.append(&mut connected_bridges(game.bridges.clone(), game.islands));
    dimacs.append(&mut avoid_crosses(game.bridges));
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
    let possible_bridges = [lhs_bridges, rhs_bridges].concat();
    let bridge_nr = island.0.connections;
    for i in 0..bridge_nr {
        // Conversion shouldn't fail as our max connections limit is 8
        bounds.push(possible_bridges[i as usize..].to_vec());
    }
    for i in 0..possible_bridges.len() - bridge_nr as usize {
        bounds.push(possible_bridges[i as usize..].iter().map(|v| -v).collect());
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
    // TODO: Maybe not the right approach...
    let total_bridges = islands.len()-1;
    bridges[..total_bridges as usize]
        .iter()
        .map(|bridge| vec![gen_bridge_name(bridge, 1), gen_bridge_name(bridge, 2)])
        .collect::<Vec<Vec<i32>>>()
}

// Rule 3
fn avoid_crosses(bridges: Vec<Bridge>) -> Vec<Vec<i32>> {
    // Assuming all bridges are give from left to right and top to bottom
    // Assuming bridges that cross islands are already excluded
    let (vert, horiz): (Vec<Bridge>, Vec<Bridge>) = bridges
        .into_iter()
        .partition(|bridge| bridge.from.1 < bridge.to.1);
    let mut clauses: Vec<Vec<i32>> = vec![];
    for v in vert {
        for h in horiz.clone() {
            if v.from.1 < h.from.1 && h.to.1 < v.to.1 && h.from.0 < v.from.0 && v.to.0 < h.to.0 {
                let a_1 = gen_bridge_name(&v, 1);
                let a_2 = gen_bridge_name(&v, 2);
                let b_1 = gen_bridge_name(&h, 1);
                let b_2 = gen_bridge_name(&h, 2);
                clauses.append(&mut vec![
                    // XOR for a_1, b_1
                    vec![-a_1, -b_1],
                    vec![a_1, b_1],
                    // XOR for a_2, b_1
                    vec![-a_2, -b_1],
                    vec![a_2, b_1],
                    // XOR for a_1, b_2
                    vec![-a_1, -b_2],
                    vec![a_1, b_2],
                    // XOR for a_2, b_2
                    vec![-a_2, -b_2],
                    vec![a_2, b_2],
                ]);
            }
        }
    }
    print!("nop");
    clauses
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
fn should_gen_bridge_name() {
    let bridge = &Bridge { from: (2, 0), to: (2, 3) };
    let bridge_name = gen_bridge_name(bridge, 1);
    assert_eq!(31341, bridge_name);
}

#[test]
fn should_gen_fst_rule() {
    let game = crate::parse_input::parse_input("./backend/input/test1.txt").unwrap();
    println!("Clauses: \n{:?}", generate(game));
}
