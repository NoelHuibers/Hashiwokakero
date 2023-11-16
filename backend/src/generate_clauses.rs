use std::{
    cmp::{max, min},
    collections::{HashMap, HashSet},
    hash::Hash,
    vec,
};

use itertools::Itertools;
use splr::cdb::ClauseDBIF;

use crate::parse_input::{Bridge, GameBoard, Island};

// TODO: rhs/lhs could be bool and coord u8
type BridgeCoord = (usize, usize, usize, usize, usize);
type AdjList = HashMap<(usize, usize), Vec<(usize, usize)>>;

pub fn generate(game: &GameBoard) -> (Vec<Vec<i32>>, HashMap<i32, BridgeCoord>) {
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
    dimacs.append(&mut connected_bridges(game.bridges.clone(), &from_var));
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
        Itertools::combinations((vars.clone()).into_iter(), min_true_vars as usize)
            .collect();
    let upper: Vec<Vec<i32>> =
        Itertools::combinations((vars).into_iter(), min_false_vars as usize)
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

// Rule 3
fn connected_bridges(
    edges: Vec<Bridge>,
    from_var: &HashMap<(usize, usize, usize, usize, usize), i32>,
) -> Vec<Vec<i32>> {
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
    let d = dfs(
        *visited.keys().next().unwrap(),
        0,
        &mut adj_list,
        &mut visited,
        &mut HashMap::new(),
        &mut HashMap::new(),
        None,
        &mut vec![],
    );
    print!("{:?}", d);

    vec![vec![]]
}

fn dfs(
    current: (usize, usize),
    mut distance: usize,
    adj_list: &mut AdjList,
    visited: &mut HashMap<(usize, usize), bool>,
    distances: &mut HashMap<(usize, usize), usize>,
    lowest: &mut HashMap<(usize, usize), usize>,
    parent: Option<(usize, usize)>,
    bridges: &mut Vec<Bridge>,
) -> Vec<Bridge> {
    distances.insert(current, distance);
    lowest.insert(current, distance);
    visited.insert(current, true);
    distance = distance + 1;
    for &next in adj_list.clone().get(&current).unwrap() {
        if parent.is_some_and(|p| p == next) {
            continue;
        }
        match visited.get(&next) {
            Some(false) => {
                dfs(
                    next,
                    distance,
                    adj_list,
                    visited,
                    distances,
                    lowest,
                    Some(current),
                    bridges,
                );
                let &next_dist = lowest.get(&next).unwrap();
                lowest
                    .entry(current)
                    .and_modify(|v| *v = min(*v, next_dist))
                    .or_insert(next_dist);
                if lowest.get(&next).unwrap() > distances.get(&current).unwrap() {
                    bridges.push(Bridge {
                        from: current,
                        to: next,
                    });
                }
            }
            Some(true) => {
                if let Some(&next_lowest) = lowest.get(&next) {
                    lowest
                        .entry(current)
                        .and_modify(|v| *v = min(v.clone(), next_lowest))
                        .or_insert(next_lowest);
                }
            }
            None => continue,
        }
    }
    bridges.to_vec()
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

#[test]
fn should_find_bridges() {
    connected_bridges(
        vec![
            Bridge {
                from: (0, 0),
                to: (0, 1),
            },
            Bridge {
                from: (0, 0),
                to: (1, 0),
            },
            Bridge {
                from: (1, 0),
                to: (2, 0),
            },
        ],
        &HashMap::new(),
    );
}

#[test]
fn should_not_find_bridges() {
    connected_bridges(
        vec![
            Bridge {
                from: (0, 0),
                to: (0, 1),
            },
            Bridge {
                from: (0, 1),
                to: (1, 1),
            },
            Bridge {
                from: (1, 1),
                to: (1, 0),
            },
            Bridge {
                from: (1, 0),
                to: (0, 0),
            },
        ],
        &HashMap::new(),
    );
}
