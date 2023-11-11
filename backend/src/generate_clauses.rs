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
        dimacs.append(&mut outgoing_bridges((island, bridges)))
    }
    dimacs.append(&mut connected_bridges(game.bridges.clone(), game.islands));
    dimacs.append(&mut avoid_crosses(game.bridges));
    return dimacs;
}

// Rule 1
fn outgoing_bridges(island: (Island, Vec<Bridge>)) -> Vec<Vec<i32>> {
    let possible_bridges = island
        .1
        .iter()
        .zip(island.1.iter())
        .flat_map(|(lhs, rhs)| vec![gen_bridge_name(lhs, 1), gen_bridge_name(rhs, 2)]);
    let bridge_nr = island.0.connections as i8;
    exactly_k_of_n_true(bridge_nr, possible_bridges.collect())
}

fn exactly_k_of_n_true(k: i8, vars: Vec<i32>) -> Vec<Vec<i32>> {
    let n = vars.len() as i8;
    let k = k;
    let min_true_vars = n - k + 1;
    let min_false_vars = k + 1;
    let lower: Vec<Vec<i32>> =
        itertools::Itertools::combinations((vars.clone()).into_iter(), min_true_vars as usize)
            .collect();
    let upper: Vec<Vec<i32>> =
        itertools::Itertools::combinations((vars).into_iter(), min_false_vars as usize)
            .map(|v| v.iter().map(|i| -*i).collect())
            .collect();
    [lower, upper].concat()
}

// Rule 2
fn connected_bridges(mut bridges: Vec<Bridge>, islands: Vec<Island>) -> Vec<Vec<i32>> {
    (0..islands.len() - 1)
        .into_iter()
        .map(|i| {
            bridges
                .iter()
                .map(|bridge| (gen_bridge_name(bridge, 1), gen_bridge_name(bridge, 2)))
                .collect::<Vec<(i32, i32)>>()[i..]
                .into_iter()
                .flat_map(|(a, b)| vec![*a, *b])
                .collect::<Vec<i32>>()
        })
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
    let bridge = &Bridge {
        from: (2, 0),
        to: (2, 3),
    };
    let bridge_name = gen_bridge_name(bridge, 1);
    assert_eq!(31341, bridge_name);
}

#[test]
fn should_gen_fst_rule() {
    let game = crate::parse_input::parse_input("./backend/input/test1.txt").unwrap();
    println!("Clauses: \n{:?}", generate(game));
}

#[test]
fn should_have_two_cnf_positivie_one_cnf_negative() {
    let vars = vec![1, 2, 3, 4];
    let clauses = exactly_k_of_n_true(3, vars);
    assert_eq!(
        clauses,
        [
            // Each var occurs exactly three times => at least three must be true
            vec![1, 2],
            vec![1, 3],
            vec![1, 4],
            vec![2, 3],
            vec![2, 4],
            vec![3, 4],
            // At least one must be false
            vec![-1, -2, -3, -4]
        ]
    )
}

#[test]
fn should_have_() {
    let vars = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let clauses = exactly_k_of_n_true(2, vars);
    assert_eq!(
        clauses,
        [
            // Each var occurs exactly three times => at least three must be true
            vec![1, 2],
            vec![1, 3],
            vec![1, 4],
            vec![2, 3],
            vec![2, 4],
            vec![3, 4],
            // At least one must be false
            vec![-1, -2, -3, -4]
        ]
    )
}
