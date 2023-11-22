use std::{collections::HashMap, vec, iter};
use std::time::Instant;

use itertools::Itertools;

use crate::{
    dfs::dfs,
    parse_input::{Bridge, GameBoard, Island},
};

// TODO: most vars here could be of lower size
pub(crate) type BridgeCoord = (usize, usize, usize, usize, usize);

pub fn generate(game: &GameBoard) -> (Vec<Vec<i32>>, HashMap<i32, BridgeCoord>) {
    let start = Instant::now();
    let mut dimacs: Vec<Vec<i32>> = vec![];
    let bridge_iter = game
        .bridges
        .iter()
        .zip(game.bridges.iter())
        .flat_map(|(lhs, rhs)| {
            [
                (lhs.from.0, lhs.from.1, lhs.to.0, lhs.to.1, 1),
                (rhs.from.0, rhs.from.1, rhs.to.0, rhs.to.1, 2),
            ]
        })
        .enumerate();

    let from_idx = bridge_iter
        .clone()
        .map(|(idx, var)| (idx as i32 + 1, var))
        .collect::<HashMap<i32, BridgeCoord>>();

    let from_var = bridge_iter
        .map(|(idx, var)| (var, idx as i32 + 1))
        .collect::<HashMap<BridgeCoord, i32>>();

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
        dimacs.append(&mut outgoing_bridges((island, bridges), from_var.clone()))
    }
    dimacs.append(&mut avoid_crosses(game.bridges.clone(), &from_var));
    dimacs.append(&mut connected_bridges(
        &game.bridges,
        &game.islands,
        &from_var,
    ));
    let duration = start.elapsed();
    println!("Time elapsed in generate() is: {:?}", duration);
    return (dimacs, from_idx);
}

// Rule 1
fn outgoing_bridges(
    island: (Island, Vec<Bridge>),
    var_map: HashMap<BridgeCoord, i32>,
) -> Vec<Vec<i32>> {
    let possible_bridges = island
        .1
        .iter()
        .zip(island.1.iter())
        .flat_map(|(lhs, rhs)| vec![lhs_bridge(lhs, &var_map), rhs_bridge(rhs, &var_map)]);
    let bridge_nr = island.0.connections as i8;
    exactly_k_of_n_true(bridge_nr, possible_bridges.collect())
}

fn exactly_k_of_n_true(k: i8, vars: Vec<i32>) -> Vec<Vec<i32>> {
    let n = vars.len() as i8;
    let k = k;
    let min_true_vars = n - k + 1;
    let min_false_vars = k + 1;
    let lower: Vec<Vec<i32>> =
        Itertools::combinations((vars.clone()).into_iter(), min_true_vars as usize).collect();
    let upper: Vec<Vec<i32>> = Itertools::combinations((vars).into_iter(), min_false_vars as usize)
        .map(|v| v.iter().map(|i| -*i).collect())
        .collect();
    [lower, upper].concat()
}

// Rule 2
fn avoid_crosses(bridges: Vec<Bridge>, var_map: &HashMap<BridgeCoord, i32>) -> Vec<Vec<i32>> {
    // Assuming all bridges are give from left to right and top to bottom
    // Assuming bridges that cross islands are already excluded
    let (vert, horiz): (Vec<Bridge>, Vec<Bridge>) = bridges
        .into_iter()
        .partition(|bridge| bridge.from.1 < bridge.to.1);
    let mut clauses: Vec<Vec<i32>> = vec![];
    for v in vert {
        for h in horiz.clone() {
            if v.from.1 < h.from.1 && h.to.1 < v.to.1 && h.from.0 < v.from.0 && v.to.0 < h.to.0 {
                let a_1 = lhs_bridge(&v, &var_map);
                let a_2 = rhs_bridge(&v, &var_map);
                let b_1 = lhs_bridge(&h, &var_map);
                let b_2 = rhs_bridge(&h, &var_map);
                clauses.append(&mut vec![
                    // (a_1 \/ a_2) XOR (b_1 \/ b_2) XOR (¬a_1 /\ ¬a_2 /\ ¬b_1 /\ ¬b_2)
                    // <=> (¬a_1 ∨ ¬b_1) /\ (¬a_1 ∨ ¬b2) /\ (¬a_2 ∨ ¬b_1) /\ (¬a_2 ∨ ¬b_2)
                    vec![-a_1, -b_1],
                    vec![-a_1, -b_2],
                    vec![-a_2, -b_1],
                    vec![-a_2, -b_2],
                ]);
            }
        }
    }
    clauses
}

// Rule 3
fn connected_bridges(
    edges: &Vec<Bridge>,
    nodes: &Vec<Island>,
    from_var: &HashMap<(usize, usize, usize, usize, usize), i32>,
) -> Vec<Vec<i32>> {
    let mut clauses = vec![];
    let (mut bridges, visited) = find_bridges(edges);

    // Case there aren't even possible bridges to connect the puzzle (invalid puzzle)
    if !visited.values().fold(true, |acc, v| acc && *v) {
        // Forcing empty clause that is only revealed after initial SAT check
        clauses.push(vec![1, 2]);
        clauses.push(vec![-1]);
        clauses.push(vec![-2]);
        return clauses;
    }

    // Case every node has exactly number "2", allowing pairwise double bridges.
    // If at least one number is "1" or greater than "2" connectedness will be
    // enforced by bridges / Rule 1. In this case we (a_1 \/ a_2) all pairs.
    if nodes.iter().fold(true, |acc, n| acc && n.connections == 2) {
        let mut tmp_edges = edges.clone();
        while bridges.is_empty() && tmp_edges.len() >= nodes.len() {
            let iterations = tmp_edges.len().clone();
            for i in 0..iterations {
                let removed = tmp_edges.get_mut(i).unwrap().clone();
                tmp_edges.remove(i);
                let (tmp_bridges, _) = find_bridges(&tmp_edges);
                if tmp_bridges.is_empty() {
                    break;
                }
                tmp_edges.insert(i, removed.clone());
            }
            if tmp_edges.len() == iterations {
                tmp_edges.pop();
            }
            let (tmp_bridges, _) = find_bridges(&tmp_edges);
            bridges = tmp_bridges;
        }
    }

    // Case otherwise => Find bridges
    bridges.iter().for_each(|bridge| {
        let (from_x, from_y, to_x, to_y) = (bridge.from.0, bridge.from.1, bridge.to.0, bridge.to.1);
        if let Some(&lhs) = from_var.get(&(from_x, from_y, to_x, to_y, 1)) {
            // We trust that unwrap because every node is inserted as lhs and rhs
            let &rhs = from_var.get(&(from_x, from_y, to_x, to_y, 2)).unwrap();
            clauses.push(vec![lhs, rhs])
        } else {
            // If the undirected dfs found an edge and it wasn't a->b it must be b->a
            let &lhs = from_var.get(&(to_x, to_y, from_x, from_y, 1)).unwrap();
            let &rhs = from_var.get(&(to_x, to_y, from_x, from_y, 2)).unwrap();
            clauses.push(vec![lhs, rhs]);
        }
    });
    clauses
}

fn find_bridges(edges: &Vec<Bridge>) -> (Vec<Bridge>, HashMap<(usize, usize), bool>) {
    let mut adj_list: HashMap<(_, _), Vec<(_, _)>> = HashMap::new();
    for edge in edges.iter() {
        if let Some(neighbors) = adj_list.get_mut(&edge.from) {
            neighbors.push(edge.to);
        } else {
            adj_list.insert(edge.from, vec![edge.to]);
        }
        // reverse (undirected graph)
        if let Some(neighbors) = adj_list.get_mut(&edge.to) {
            neighbors.push(edge.from);
        } else {
            adj_list.insert(edge.to, vec![edge.from]);
        }
    }
    let mut visited = adj_list
        .keys()
        .map(|k| (*k, false))
        .collect::<HashMap<(usize, usize), bool>>();
    let mut distances = HashMap::new();
    let mut lowest = HashMap::new();
    let mut vec = vec![];
    let (bridges, visited) = dfs(
        *visited.keys().next().unwrap(),
        0,
        &mut adj_list,
        &mut visited,
        &mut distances,
        &mut lowest,
        None,
        &mut vec,
    );
    (bridges, visited.clone())
}

fn lhs_bridge(bridge: &Bridge, var_map: &HashMap<BridgeCoord, i32>) -> i32 {
    *var_map
        .get(&(bridge.from.0, bridge.from.1, bridge.to.0, bridge.to.1, 1))
        .expect(&format!(
            "The bridge {:?} was not parsed as a possible bridge.",
            bridge
        ))
}

fn rhs_bridge(bridge: &Bridge, var_map: &HashMap<BridgeCoord, i32>) -> i32 {
    *var_map
        .get(&(bridge.from.0, bridge.from.1, bridge.to.0, bridge.to.1, 2))
        .expect(&format!(
            "The bridge {:?} was not parsed as a possible bridge.",
            bridge
        ))
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
fn should_have_four_cnf_positive_three_cnf_negative() {
    let vars = vec![1, 2, 3, 4, 5];
    let clauses = exactly_k_of_n_true(2, vars);
    assert_eq!(
        clauses,
        [
            // Each var occurs exactly two times => at least two must be true
            vec![1, 2, 3, 4],
            vec![1, 2, 3, 5],
            vec![1, 2, 4, 5],
            vec![1, 3, 4, 5],
            vec![2, 3, 4, 5],
            // At least three must be false
            vec![-1, -2, -3],
            vec![-1, -2, -4],
            vec![-1, -2, -5],
            vec![-1, -3, -4],
            vec![-1, -3, -5],
            vec![-1, -4, -5],
            vec![-2, -3, -4],
            vec![-2, -3, -5],
            vec![-2, -4, -5],
            vec![-3, -4, -5]
        ]
    )
}

#[test]
fn should_have_one_cnf_positive() {
    let vars = 1..=8;
    let clauses = exactly_k_of_n_true(8, vars.collect_vec());
    assert_eq!(clauses, [[1], [2], [3], [4], [5], [6], [7], [8]])
}
