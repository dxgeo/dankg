//! Phase 1: break cycles.
//!
//! Layering needs a DAG, and a knowledge graph is full of cycles -- two notes
//! that link to each other are the normal case, not the exception. A depth-
//! first search finds the back edges and the layout runs them the other way.
//!
//! Nothing is deleted. Each edge remembers that it was turned around, so the
//! renderer draws the arrowhead the way the author wrote it and only the
//! geometry runs backwards. A layout that silently dropped an edge would be
//! drawing a different graph from the one it was given.

/// What the layout had to do with one input edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Forward,
    /// A back edge, run the other way so the graph can be layered.
    Reversed,
    /// `a -> a`. It cannot be layered at all, and is drawn as a loop.
    SelfLoop,
}

/// Classify every edge. `edges` are `(from, to, weight)` over `0..count`.
///
/// The search visits roots and adjacency in index order, so the same input
/// always yields the same set of reversals -- which of a cycle's edges gets
/// turned around is arbitrary, but it must not be arbitrary twice.
pub fn break_cycles(count: usize, edges: &[(usize, usize, i32)]) -> Vec<Role> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Unseen,
        /// On the current search path: an edge back into one of these closes a
        /// cycle.
        Open,
        Done,
    }

    let mut out = vec![Role::Forward; edges.len()];
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (i, (from, to, _)) in edges.iter().enumerate() {
        if from == to {
            out[i] = Role::SelfLoop;
            continue;
        }
        adjacency[*from].push(i);
    }

    let mut mark = vec![Mark::Unseen; count];
    // Iterative, because a corpus is allowed to be deeper than the stack.
    // Each frame is a node and how far through its adjacency we have gone.
    let mut stack: Vec<(usize, usize)> = Vec::new();

    for root in 0..count {
        if mark[root] != Mark::Unseen {
            continue;
        }
        mark[root] = Mark::Open;
        stack.push((root, 0));

        while let Some((node, step)) = stack.pop() {
            if step >= adjacency[node].len() {
                mark[node] = Mark::Done;
                continue;
            }
            stack.push((node, step + 1));

            let edge = adjacency[node][step];
            let target = edges[edge].1;
            match mark[target] {
                Mark::Open => out[edge] = Role::Reversed,
                Mark::Unseen => {
                    mark[target] = Mark::Open;
                    stack.push((target, 0));
                }
                Mark::Done => {}
            }
        }
    }
    out
}

/// The edge list to layer over: self-loops dropped, back edges turned around.
pub fn oriented(edges: &[(usize, usize, i32)], roles: &[Role]) -> Vec<(usize, usize, i32)> {
    edges
        .iter()
        .zip(roles)
        .filter(|(_, role)| **role != Role::SelfLoop)
        .map(|((from, to, weight), role)| match role {
            Role::Reversed => (*to, *from, *weight),
            _ => (*from, *to, *weight),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles(count: usize, edges: &[(usize, usize)]) -> Vec<Role> {
        let weighted: Vec<(usize, usize, i32)> =
            edges.iter().map(|(a, b)| (*a, *b, 1)).collect();
        break_cycles(count, &weighted)
    }

    /// Follows the oriented edges and reports whether any cycle survives.
    fn is_acyclic(count: usize, edges: &[(usize, usize)], roles: &[Role]) -> bool {
        let weighted: Vec<(usize, usize, i32)> =
            edges.iter().map(|(a, b)| (*a, *b, 1)).collect();
        let oriented = oriented(&weighted, roles);

        let mut incoming = vec![0usize; count];
        for (_, to, _) in &oriented {
            incoming[*to] += 1;
        }
        let mut ready: Vec<usize> = (0..count).filter(|n| incoming[*n] == 0).collect();
        let mut seen = 0;
        while let Some(node) = ready.pop() {
            seen += 1;
            for (from, to, _) in &oriented {
                if *from == node {
                    incoming[*to] -= 1;
                    if incoming[*to] == 0 {
                        ready.push(*to);
                    }
                }
            }
        }
        seen == count
    }

    #[test]
    fn a_dag_is_left_alone() {
        let edges = [(0, 1), (1, 2), (0, 2)];
        assert!(roles(3, &edges).iter().all(|r| *r == Role::Forward));
    }

    #[test]
    fn a_two_cycle_loses_exactly_one_edge_to_reversal() {
        // Two notes linking to each other: the common case.
        let r = roles(2, &[(0, 1), (1, 0)]);
        assert_eq!(r.iter().filter(|r| **r == Role::Reversed).count(), 1);
        assert!(is_acyclic(2, &[(0, 1), (1, 0)], &r));
    }

    #[test]
    fn a_longer_cycle_is_broken_once() {
        let edges = [(0, 1), (1, 2), (2, 3), (3, 0)];
        let r = roles(4, &edges);
        assert_eq!(r.iter().filter(|r| **r == Role::Reversed).count(), 1);
        assert!(is_acyclic(4, &edges, &r));
    }

    #[test]
    fn overlapping_cycles_are_all_broken() {
        // Figure-eight: 0->1->2->0 and 2->3->2.
        let edges = [(0, 1), (1, 2), (2, 0), (2, 3), (3, 2)];
        let r = roles(4, &edges);
        assert!(is_acyclic(4, &edges, &r));
    }

    #[test]
    fn a_self_loop_is_marked_and_never_layered() {
        let r = roles(2, &[(0, 0), (0, 1)]);
        assert_eq!(r[0], Role::SelfLoop);
        assert_eq!(r[1], Role::Forward);
        assert_eq!(oriented(&[(0, 0, 1), (0, 1, 1)], &r), vec![(0, 1, 1)]);
    }

    #[test]
    fn reversal_keeps_the_weight_and_swaps_the_ends() {
        let r = vec![Role::Reversed];
        assert_eq!(oriented(&[(0, 1, 2)], &r), vec![(1, 0, 2)]);
    }

    #[test]
    fn the_same_cycle_is_broken_the_same_way_every_time() {
        let edges = [(0, 1), (1, 2), (2, 0), (1, 3), (3, 1)];
        let first = roles(4, &edges);
        for _ in 0..5 {
            assert_eq!(roles(4, &edges), first);
        }
    }

    #[test]
    fn a_deep_chain_does_not_exhaust_the_stack() {
        let edges: Vec<(usize, usize)> = (0..50_000).map(|i| (i, i + 1)).collect();
        let r = roles(50_001, &edges);
        assert!(r.iter().all(|r| *r == Role::Forward));
    }
}
