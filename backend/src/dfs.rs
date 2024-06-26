use crate::parse_input::Bridge;
use core::cmp::min;
use std::collections::HashMap;

type AdjList = HashMap<(u8, u8), Vec<(u8, u8)>>;
type Node = (u8, u8);

pub fn dfs<'a>(
    current: Node,
    mut distance: u8,
    adj_list: &'a mut AdjList,
    visited: &'a mut HashMap<Node, bool>,
    distances: &'a mut HashMap<Node, u8>,
    lowest: &'a mut HashMap<Node, u8>,
    parent: Option<Node>,
    bridges: &'a mut Vec<Bridge>,
) -> (Vec<Bridge>, &'a mut HashMap<(u8, u8), bool>) {
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
    (bridges.to_vec(), visited)
}

#[test]
fn should_find_bridges() {
    let edges = vec![
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
    ];
    // Sorry for pasting all this code from generate_clauses, but it's just a test...
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
        .collect::<HashMap<(u8, u8), bool>>();
    let (d, v) = dfs(
        *visited.keys().next().unwrap(),
        0,
        &mut adj_list,
        &mut visited,
        &mut HashMap::new(),
        &mut HashMap::new(),
        None,
        &mut vec![],
    );
    assert!(
        d.contains(&Bridge {
            from: (0, 0),
            to: (0, 1)
        }) || d.contains(&Bridge {
            from: (0, 1),
            to: (0, 0)
        })
    );
    assert!(
        d.contains(&Bridge {
            from: (1, 0),
            to: (0, 0)
        }) || d.contains(&Bridge {
            from: (0, 0),
            to: (1, 0)
        })
    );
    assert!(
        d.contains(&Bridge {
            from: (2, 0),
            to: (1, 0)
        }) || d.contains(&Bridge {
            from: (1, 0),
            to: (2, 0)
        })
    );
}

#[test]
fn should_not_find_bridges() {
    let edges = vec![
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
    ];
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
        .collect::<HashMap<(u8, u8), bool>>();
    let (d, _) = dfs(
        *visited.keys().next().unwrap(),
        0,
        &mut adj_list,
        &mut visited,
        &mut HashMap::new(),
        &mut HashMap::new(),
        None,
        &mut vec![],
    );
    assert!(d.is_empty())
}
