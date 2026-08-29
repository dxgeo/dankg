//! Phase 2: assign each node to a layer, and split the edges that skip one.
//!
//! Longest-path layering: a node sits one layer below the lowest of everything
//! that points at it. That is what makes a heading appear directly under its
//! parent, and it is cheap and exact. It can leave an edge spanning several
//! layers, which the ordering and coordinate phases cannot reason about, so
//! such an edge is split into a chain of one-layer segments joined by virtual
//! nodes. Those become the bend points of the drawn polyline.

use super::acyclic::{self, Role};
use super::{Dag, Route, Segment};

/// Longest-path layering over the acyclic orientation.
pub(crate) fn assign(dag: &mut Dag, edges: &[(usize, usize, i32)], roles: &[Role]) {
    let oriented = acyclic::oriented(edges, roles);
    let count = dag.real;

    let mut incoming = vec![0usize; count];
    for (_, to, _) in &oriented {
        incoming[*to] += 1;
    }

    // Kahn's algorithm, taking the lowest ready index each time so the order is
    // the same on every run.
    let mut ready: Vec<usize> = (0..count).filter(|n| incoming[*n] == 0).collect();
    let mut rank = vec![0u32; count];
    let mut placed = 0usize;

    while let Some(node) = pop_lowest(&mut ready) {
        placed += 1;
        for (from, to, _) in &oriented {
            if *from != node {
                continue;
            }
            rank[*to] = rank[*to].max(rank[node] + 1);
            incoming[*to] -= 1;
            if incoming[*to] == 0 {
                ready.push(*to);
            }
        }
    }
    debug_assert_eq!(placed, count, "phase 1 left a cycle behind");

    dag.rank = rank;
}

fn pop_lowest(ready: &mut Vec<usize>) -> Option<usize> {
    let at = ready.iter().enumerate().min_by_key(|(_, n)| **n)?.0;
    Some(ready.swap_remove(at))
}

/// Replace every edge that spans more than one layer with a chain of virtual
/// nodes, and record the route each input edge ended up taking.
pub(crate) fn split_long_edges(
    dag: &mut Dag,
    edges: &[(usize, usize, i32)],
    roles: &[Role],
) -> Vec<Route> {
    let mut routes = Vec::with_capacity(edges.len());

    for ((from, to, weight), role) in edges.iter().zip(roles) {
        if *role == Role::SelfLoop {
            routes.push(Route { role: *role, path: vec![*from] });
            continue;
        }
        let (source, target) = match role {
            Role::Reversed => (*to, *from),
            _ => (*from, *to),
        };

        let mut path = vec![source];
        // Every intermediate layer gets a bend point, so no segment ever skips
        // a layer and the ordering phase only compares neighbouring ranks.
        for rank in (dag.rank[source] + 1)..dag.rank[target] {
            let bend = dag.rank.len();
            dag.rank.push(rank);
            dag.width.push(0);
            dag.key.push(format!("~{}:{}", routes.len(), rank));
            path.push(bend);
        }
        path.push(target);

        for pair in path.windows(2) {
            dag.segments.push(Segment { from: pair[0], to: pair[1], weight: *weight });
        }
        routes.push(Route { role: *role, path });
    }

    routes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dag(count: usize, edges: &[(usize, usize, i32)]) -> (Dag, Vec<Route>) {
        let mut dag = Dag {
            real: count,
            rank: vec![0; count],
            width: vec![80; count],
            key: (0..count).map(|i| i.to_string()).collect(),
            ..Dag::default()
        };
        let roles = acyclic::break_cycles(count, edges);
        assign(&mut dag, edges, &roles);
        let routes = split_long_edges(&mut dag, edges, &roles);
        (dag, routes)
    }

    fn e(from: usize, to: usize) -> (usize, usize, i32) {
        (from, to, 1)
    }

    #[test]
    fn a_node_sits_one_layer_below_what_points_at_it() {
        let (d, _) = dag(3, &[e(0, 1), e(1, 2)]);
        assert_eq!(d.rank[..3], [0, 1, 2]);
    }

    #[test]
    fn longest_path_wins_over_shortest() {
        // 0 -> 2 directly, and 0 -> 1 -> 2. Node 2 goes below both.
        let (d, _) = dag(3, &[e(0, 2), e(0, 1), e(1, 2)]);
        assert_eq!(d.rank[..3], [0, 1, 2]);
    }

    #[test]
    fn unconnected_nodes_all_start_at_the_top() {
        let (d, _) = dag(3, &[]);
        assert_eq!(d.rank[..3], [0, 0, 0]);
    }

    #[test]
    fn an_edge_across_two_layers_gets_one_bend_point() {
        let (d, routes) = dag(3, &[e(0, 1), e(1, 2), e(0, 2)]);
        let long = &routes[2];
        assert_eq!(long.path.len(), 3, "source, bend, target");
        let bend = long.path[1];
        assert!(d.is_virtual(bend));
        assert_eq!(d.rank[bend], 1);
        assert_eq!(d.width[bend], 0, "a bend point takes no space");
    }

    #[test]
    fn no_segment_ever_skips_a_layer() {
        let (d, _) = dag(5, &[e(0, 1), e(1, 2), e(2, 3), e(3, 4), e(0, 4)]);
        for segment in &d.segments {
            assert_eq!(
                d.rank[segment.to] - d.rank[segment.from],
                1,
                "segment {segment:?} spans more than one layer"
            );
        }
    }

    #[test]
    fn a_short_edge_is_not_split() {
        let (_, routes) = dag(2, &[e(0, 1)]);
        assert_eq!(routes[0].path, vec![0, 1]);
    }

    #[test]
    fn a_reversed_edge_is_routed_down_the_page() {
        let (d, routes) = dag(2, &[e(0, 1), e(1, 0)]);
        let back = routes.iter().find(|r| r.role == Role::Reversed).unwrap();
        assert_eq!(d.rank[back.path[0]], 0, "the route starts at the upper node");
        assert_eq!(d.rank[*back.path.last().unwrap()], 1);
    }

    #[test]
    fn a_self_loop_keeps_its_node_and_makes_no_segment() {
        let (d, routes) = dag(2, &[(0, 0, 1), e(0, 1)]);
        assert_eq!(routes[0].path, vec![0]);
        assert_eq!(d.segments.len(), 1, "only the real edge becomes a segment");
    }

    #[test]
    fn a_cycle_still_produces_a_layering() {
        let (d, _) = dag(3, &[e(0, 1), e(1, 2), e(2, 0)]);
        assert_eq!(d.rank[..3], [0, 1, 2]);
    }

    #[test]
    fn weights_survive_the_split() {
        let (d, _) = dag(3, &[e(0, 1), e(1, 2), (0, 2, 2)]);
        let heavy: Vec<&Segment> = d.segments.iter().filter(|s| s.weight == 2).collect();
        assert_eq!(heavy.len(), 2, "both halves of the split edge keep the weight");
    }
}
