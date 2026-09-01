# Types

This is pilot 2 of "can dankg build itself, dankg-style" -- pilot 1
(`../hash.md`) proved the tangle-and-build loop on one flat file. This one
answers a question pilot 1 could not: does `[tangle.rust] glue`'s real
mod.rs-*generating* code path work, not just its "a mod.rs is already
there, leave it alone" backoff?

Pilot 1 never exercised it, and neither would a straight port of any real
multi-file directory in this crate's own `src/` -- because hand-written
Rust already hand-writes its own `mod.rs`. `glue/rust.py`'s generator only
runs when a directory ends up with `.rs` files in it and *no* `mod.rs`
among them, which real, hand-authored source never has cause to leave out.
So this pilot deliberately withholds one thing from the literate source
that the real `src/layout/mod.rs` has and this one does not: nothing here
declares `mod acyclic; mod rank; mod order; mod coord;` anywhere. Four of
this file's five sections below are the real `src/layout/{acyclic,rank,
order,coord}.rs`, byte-for-byte except for one import path each (see each
section's own note) -- Sugiyama's four layout phases, unmodified. What's
missing is `mod.rs`'s own driver function and its graph-backed test suite
(`layout()`, `assemble()`, and everything depending on `crate::graph` and
`graph_of`'s markdown-to-graph pipeline) -- pulling that in would mean
also tangling `graph::model`, `graph::build`, `graph::resolve` and `md::`
just to prove a mod.rs gets written, which is a much bigger pilot for no
extra evidence about the thing being tested here.

What *is* new, not lifted from the real source at all: the four phases
share three small types -- `Dag`, `Segment`, `Route` -- that live at the
top of the real `mod.rs`, above its own `mod` declarations. Since nothing
here may write a `mod.rs` by hand, those types need a file of their own;
`types.rs` is that file, and this section is it. Everything in the block
below is still a verbatim copy of the real definitions (`src/layout/
mod.rs` lines 126–167), just gathered into their own module instead of
sitting above four `mod` lines that no longer exist in this source.

```rust name=types
//! Shared layout types. Extracted from the real `mod.rs`'s own top, into a
//! file of its own -- see `layout.md#types` for why.

use super::acyclic::Role;

pub const NODE_SEP: i32 = 28;
pub const MARGIN: i32 = 24;

/// The working graph: real nodes `0..real`, virtual bend points after them.
///
/// Phases mutate this in place. It is deliberately a plain struct of parallel
/// vectors: index-addressed, so nothing depends on a hash map's order.
#[derive(Debug, Clone, Default)]
pub(crate) struct Dag {
    pub real: usize,
    pub rank: Vec<u32>,
    pub width: Vec<i32>,
    /// A stable tie-break key per node. Real nodes use their `NodeId`; virtual
    /// ones use the edge they belong to, so both are unique and reproducible.
    pub key: Vec<String>,
    /// Edges between adjacent ranks only, after splitting.
    pub segments: Vec<Segment>,
    /// Node indices per rank, left to right.
    pub layers: Vec<Vec<usize>>,
    /// Each node's index within its layer.
    pub pos: Vec<u32>,
    pub x: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Segment {
    pub from: usize,
    pub to: usize,
    pub weight: i32,
}

impl Dag {
    pub fn count(&self) -> usize {
        self.rank.len()
    }

    pub fn is_virtual(&self, node: usize) -> bool {
        node >= self.real
    }
}

/// How one input edge was routed: which nodes it passes through, and whether
/// it had to be turned around to get there.
#[derive(Debug, Clone)]
pub(crate) struct Route {
    pub role: Role,
    /// Node indices from the layout's source to its target, inclusive. A
    /// self-loop holds its single node; nothing else is ever shorter than two.
    pub path: Vec<usize>,
}
```

# Acyclic

Phase 1. Layering needs a DAG, and a knowledge graph is full of cycles --
two notes that link to each other are the normal case, not the exception.
A depth-first search finds the back edges and the layout runs them the
other way. Nothing is deleted: each edge remembers that it was turned
around, so the renderer draws the arrowhead the way the author wrote it
and only the geometry runs backwards.

Byte-for-byte the real `src/layout/acyclic.rs`, including its own tests
-- this file has no `super::` dependency on the shared types at all, so
nothing about it needed to change.

```rust name=acyclic
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
```

# Rank

Phase 2: assign each node to a layer, and split the edges that skip one.
Longest-path layering -- a node sits one layer below the lowest of
everything that points at it, which is what makes a heading appear
directly under its parent. An edge spanning several layers is split into a
chain of one-layer segments joined by virtual nodes, which become the bend
points of the drawn polyline.

Real `src/layout/rank.rs`, with two changes, both disclosed rather than
silent: its import reaches `Dag`/`Route`/`Segment` through `types` now,
since nothing here writes the `mod.rs` that used to hold them directly
above it, and its leading `//!` -- which duplicated this section's own
prose word for word, caught by `rust-doclint.py` once it could actually
resolve this pilot's source files (see *Pilot notes*) -- is trimmed to a
pointer under the same convention `hash.md` already established.

```rust name=rank
//! Longest-path layering, phase 2 of 4. See `layout.md#rank` for why.

use super::acyclic::{self, Role};
use super::types::{Dag, Route, Segment};

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
```

# Order

Phase 3: order the nodes within each layer, to reduce edge crossings.
Minimising crossings exactly is NP-hard, so this is the standard median
heuristic: sweep down the layers putting each node at the median position
of its neighbours above, then sweep up doing the same with the neighbours
below, four times, keeping the best ordering seen. The starting order
comes from a depth-first walk in index order, and every tie is broken by
the node's stable key, so the result is the same on every run.

Real `src/layout/order.rs`, with the same two changes as `rank.rs` above
-- the import path, and the leading `//!` trimmed for the same reason,
caught the same way. Its own test module's `crate::layout::acyclic`/`rank`
references are crate-absolute already and need no change at all.

```rust name=order
//! Median-heuristic crossing reduction, phase 3 of 4. See `layout.md#order` for why.

use super::types::{Dag, Segment};

const SWEEPS: usize = 4;

pub(crate) fn minimize_crossings(dag: &mut Dag) {
    dag.layers = initial_order(dag);
    if dag.layers.len() < 2 {
        renumber(dag);
        return;
    }

    let mut best = dag.layers.clone();
    let mut fewest = crossings(dag, &best);

    for sweep in 0..SWEEPS {
        renumber(dag);
        if sweep % 2 == 0 {
            for rank in 1..dag.layers.len() {
                reorder(dag, rank, Direction::Down);
            }
        } else {
            for rank in (0..dag.layers.len() - 1).rev() {
                reorder(dag, rank, Direction::Up);
            }
        }
        let count = crossings(dag, &dag.layers);
        if count < fewest {
            fewest = count;
            best = dag.layers.clone();
        }
    }

    dag.layers = best;
    renumber(dag);
}

#[derive(Clone, Copy, PartialEq)]
enum Direction {
    /// Order this layer by where its predecessors sit.
    Down,
    /// Order this layer by where its successors sit.
    Up,
}

/// Depth-first from every node in index order, appending each node to its
/// layer the first time it is reached. Nodes connected to each other therefore
/// start out near each other, which is most of the work.
fn initial_order(dag: &Dag) -> Vec<Vec<usize>> {
    let ranks = dag.rank.iter().copied().max().unwrap_or(0) as usize + 1;
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); ranks];
    let mut seen = vec![false; dag.count()];

    let mut out: Vec<Vec<usize>> = vec![Vec::new(); dag.count()];
    for segment in &dag.segments {
        out[segment.from].push(segment.to);
    }

    for root in 0..dag.count() {
        if seen[root] {
            continue;
        }
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if seen[node] {
                continue;
            }
            seen[node] = true;
            layers[dag.rank[node] as usize].push(node);
            // Reversed, so the lowest-numbered successor comes off the stack
            // first and the walk reads left to right.
            for &next in out[node].iter().rev() {
                if !seen[next] {
                    stack.push(next);
                }
            }
        }
    }
    layers
}

fn renumber(dag: &mut Dag) {
    dag.pos = vec![0; dag.count()];
    for layer in &dag.layers {
        for (i, &node) in layer.iter().enumerate() {
            dag.pos[node] = i as u32;
        }
    }
}

fn reorder(dag: &mut Dag, rank: usize, direction: Direction) {
    let layer = dag.layers[rank].clone();
    let medians: Vec<i64> = layer.iter().map(|&n| median(dag, n, direction)).collect();

    let mut ranked: Vec<(usize, usize)> = (0..layer.len()).map(|i| (i, i)).collect();
    ranked.sort_by(|a, b| {
        let (ma, mb) = (medians[a.0], medians[b.0]);
        // A node with nothing in the reference layer has no opinion, so it
        // keeps the position it already had.
        match (ma < 0, mb < 0) {
            (true, true) | (false, false) => (ma, &dag.key[layer[a.0]]).cmp(&(mb, &dag.key[layer[b.0]])),
            (true, false) => (a.1 as i64).cmp(&mb),
            (false, true) => ma.cmp(&(b.1 as i64)),
        }
    });

    dag.layers[rank] = ranked.iter().map(|(i, _)| layer[*i]).collect();
}

/// The weighted median position of a node's neighbours in the adjacent layer,
/// or `-1` when it has none.
///
/// A containment segment counts twice, which is what pulls a heading into line
/// with its children rather than with whatever else happens to link to it.
fn median(dag: &Dag, node: usize, direction: Direction) -> i64 {
    let mut positions: Vec<u32> = Vec::new();
    for segment in &dag.segments {
        let neighbour = match direction {
            Direction::Down if segment.to == node => segment.from,
            Direction::Up if segment.from == node => segment.to,
            _ => continue,
        };
        for _ in 0..segment.weight.max(1) {
            positions.push(dag.pos[neighbour]);
        }
    }
    if positions.is_empty() {
        return -1;
    }
    positions.sort_unstable();
    // Scaled by two so an even-sized list keeps the half-step between its two
    // middle values instead of rounding it away.
    let mid = positions.len() / 2;
    if positions.len() % 2 == 1 {
        positions[mid] as i64 * 2
    } else {
        positions[mid - 1] as i64 + positions[mid] as i64
    }
}

/// Total crossings over every adjacent pair of layers.
fn crossings(dag: &Dag, layers: &[Vec<usize>]) -> usize {
    let mut pos = vec![0u32; dag.count()];
    for layer in layers {
        for (i, &node) in layer.iter().enumerate() {
            pos[node] = i as u32;
        }
    }

    // Two segments between the same pair of layers cross when their endpoints
    // are in the opposite order at the top and at the bottom.
    let mut total = 0;
    for rank in 0..layers.len().saturating_sub(1) {
        let between: Vec<&Segment> = dag
            .segments
            .iter()
            .filter(|s| dag.rank[s.from] as usize == rank)
            .collect();
        for (i, a) in between.iter().enumerate() {
            for b in &between[i + 1..] {
                let (top, bottom) = (
                    pos[a.from].cmp(&pos[b.from]),
                    pos[a.to].cmp(&pos[b.to]),
                );
                if top != std::cmp::Ordering::Equal
                    && bottom != std::cmp::Ordering::Equal
                    && top != bottom
                {
                    total += 1;
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::acyclic;
    use crate::layout::rank;

    fn build(count: usize, edges: &[(usize, usize, i32)]) -> Dag {
        let mut dag = Dag {
            real: count,
            rank: vec![0; count],
            width: vec![80; count],
            key: (0..count).map(|i| format!("n{i:03}")).collect(),
            ..Dag::default()
        };
        let roles = acyclic::break_cycles(count, edges);
        rank::assign(&mut dag, edges, &roles);
        rank::split_long_edges(&mut dag, edges, &roles);
        minimize_crossings(&mut dag);
        dag
    }

    fn e(from: usize, to: usize) -> (usize, usize, i32) {
        (from, to, 1)
    }

    #[test]
    fn every_node_lands_in_the_layer_its_rank_says() {
        let d = build(4, &[e(0, 1), e(0, 2), e(1, 3)]);
        for (rank, layer) in d.layers.iter().enumerate() {
            for &node in layer {
                assert_eq!(d.rank[node] as usize, rank);
            }
        }
    }

    #[test]
    fn positions_are_dense_and_match_the_layers() {
        let d = build(5, &[e(0, 1), e(0, 2), e(0, 3), e(0, 4)]);
        assert_eq!(d.layers[1].len(), 4);
        let mut seen: Vec<u32> = d.layers[1].iter().map(|&n| d.pos[n]).collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1, 2, 3]);
    }

    #[test]
    fn an_obviously_crossed_pairing_is_untangled() {
        // 0 and 1 on top, 2 and 3 below, wired 0->3 and 1->2. Whichever way
        // the layers end up ordered, the two edges must not cross.
        let d = build(4, &[e(0, 3), e(1, 2)]);
        assert_eq!(crossings(&d, &d.layers), 0);
    }

    #[test]
    fn a_worse_sweep_never_replaces_a_better_ordering() {
        // A deliberately awkward bipartite graph; the result only has to be no
        // worse than the depth-first starting point.
        let edges: Vec<(usize, usize, i32)> =
            vec![e(0, 5), e(1, 4), e(2, 6), e(3, 5), e(0, 6), e(1, 7)];
        let d = build(8, &edges);
        let start = {
            let mut fresh = d.clone();
            fresh.layers = initial_order(&fresh);
            crossings(&fresh, &fresh.layers)
        };
        assert!(crossings(&d, &d.layers) <= start);
    }

    #[test]
    fn a_node_with_no_neighbours_above_keeps_its_place() {
        // 3 has nothing pointing at it, so the down sweep has no opinion about
        // it and it must not be shuffled to an end.
        let d = build(4, &[e(0, 1), e(0, 2)]);
        assert!(d.layers.iter().flatten().any(|&n| n == 3));
        assert_eq!(d.layers.iter().flatten().count(), 4, "no node is lost");
    }

    #[test]
    fn bend_points_are_ordered_alongside_real_nodes() {
        let d = build(3, &[e(0, 1), e(1, 2), e(0, 2)]);
        assert_eq!(d.layers[1].len(), 2, "the real node and the bend point share a layer");
        assert!(d.layers[1].iter().any(|&n| d.is_virtual(n)));
    }

    #[test]
    fn a_single_layer_graph_is_ordered_without_sweeping() {
        let d = build(3, &[]);
        assert_eq!(d.layers.len(), 1);
        assert_eq!(d.layers[0], vec![0, 1, 2]);
    }

    #[test]
    fn ordering_is_the_same_on_every_run() {
        let edges = vec![e(0, 2), e(0, 3), e(1, 2), e(1, 4), e(2, 5), e(3, 5), e(4, 5)];
        let first = build(6, &edges).layers;
        for _ in 0..5 {
            assert_eq!(build(6, &edges).layers, first);
        }
    }

    #[test]
    fn crossings_counts_a_known_tangle() {
        // 0 -> 3 and 1 -> 2. With the lower layer as [2, 3] the two edges
        // cross; swapping it to [3, 2] untangles them.
        let mut d = build(4, &[e(0, 3), e(1, 2)]);
        d.layers[0] = vec![0, 1];

        d.layers[1] = vec![2, 3];
        renumber(&mut d);
        assert_eq!(crossings(&d, &d.layers), 1);

        d.layers[1] = vec![3, 2];
        renumber(&mut d);
        assert_eq!(crossings(&d, &d.layers), 0);
    }
}
```

# Coord

Phase 4: turn layers and orders into x coordinates. Sugiyama's priority
method -- each node would like to sit at the average of the nodes it
connects to in the layer above or below, and gets to, as far as its
neighbours allow. A bend point outranks every real node, however well
connected: a long edge that zigzags is far harder to follow than one node
sitting slightly off-centre. Ordering never changes here, only position --
a node may slide within its layer but can never pass a neighbour, so phase
3's crossing count survives.

Real `src/layout/coord.rs`, with the same import-path change as the
previous two sections, the same leading-`//!` trim, and one more: a
`///` on `BEND_PRIORITY` restated this section's own "outranks every real
node" clause almost verbatim, so it is reworded to say something the
prose above does not (why `i64::MAX` specifically, not just "large")
rather than trimmed away entirely -- unlike a `//!`, a `///` on one
specific item is worth keeping when it can carry its own information.

```rust name=coord
//! Sugiyama's priority method, phase 4 of 4. See `layout.md#coord` for why.

use super::types::{Dag, MARGIN, NODE_SEP};

/// Alternating down/up passes. Four each way: enough to settle, few enough
/// that the cost stays linear in practice.
const PASSES: usize = 8;

/// Deliberately `i64::MAX`, not just "bigger than any real priority" --
/// nothing computed from segment weights can ever tie with it by accident.
const BEND_PRIORITY: i64 = i64::MAX;

pub(crate) fn assign(dag: &mut Dag) {
    pack(dag);

    for pass in 0..PASSES {
        let down = pass % 2 == 0;
        let ranks: Vec<usize> = if down {
            (1..dag.layers.len()).collect()
        } else {
            (0..dag.layers.len().saturating_sub(1)).rev().collect()
        };
        for rank in ranks {
            sweep(dag, rank, down);
        }
    }

    normalize(dag);
}

/// Left-to-right packing, which is both the starting point and the guarantee
/// that nothing ever overlaps: every later move is checked against it.
fn pack(dag: &mut Dag) {
    dag.x = vec![0; dag.count()];
    for layer in &dag.layers {
        let mut left = 0;
        for &node in layer {
            dag.x[node] = left + dag.width[node] / 2;
            left += dag.width[node] + NODE_SEP;
        }
    }
}

fn sweep(dag: &mut Dag, rank: usize, down: bool) {
    let layer = dag.layers[rank].clone();
    let wants: Vec<Option<i32>> = layer.iter().map(|&n| desired(dag, n, down)).collect();
    let priorities: Vec<i64> = layer.iter().map(|&n| priority(dag, n, down)).collect();

    // Highest priority first, ties by position so the pass is reproducible.
    let mut order: Vec<usize> = (0..layer.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(priorities[i]), i));

    for i in order {
        let Some(want) = wants[i] else { continue };
        if want > dag.x[layer[i]] {
            push_right(dag, &layer, &priorities, i, want);
        } else if want < dag.x[layer[i]] {
            push_left(dag, &layer, &priorities, i, want);
        }
    }
}

/// Where a node would like to be: the weighted average of what it connects to
/// in the reference layer. `None` when it connects to nothing there, in which
/// case it stays where the packing put it.
fn desired(dag: &Dag, node: usize, down: bool) -> Option<i32> {
    let mut total: i64 = 0;
    let mut weight: i64 = 0;

    for segment in &dag.segments {
        let neighbour = match (down, segment.to == node, segment.from == node) {
            (true, true, _) => segment.from,
            (false, _, true) => segment.to,
            _ => continue,
        };
        let w = segment.weight.max(1) as i64;
        total += dag.x[neighbour] as i64 * w;
        weight += w;
    }
    if weight == 0 {
        return None;
    }
    // Round to nearest; coordinates are non-negative until `normalize`.
    Some(((total + weight / 2) / weight) as i32)
}

fn priority(dag: &Dag, node: usize, down: bool) -> i64 {
    if dag.is_virtual(node) {
        return BEND_PRIORITY;
    }
    dag.segments
        .iter()
        .filter(|s| if down { s.to == node } else { s.from == node })
        .map(|s| s.weight.max(1) as i64)
        .sum()
}

/// Move node `i` right towards `want`, pushing the lower-priority nodes on its
/// right along with it, and stopping short of the first one that outranks it.
fn push_right(dag: &mut Dag, layer: &[usize], priorities: &[i64], i: usize, want: i32) {
    let node = layer[i];
    let mut limit = i32::MAX;
    let mut needed = 0;
    let mut end = layer.len();

    for j in i + 1..layer.len() {
        let other = layer[j];
        if priorities[j] >= priorities[i] {
            limit = dag.x[other] - dag.width[other] / 2 - needed - NODE_SEP - dag.width[node] / 2;
            end = j;
            break;
        }
        needed += NODE_SEP + dag.width[other];
    }

    let target = want.min(limit);
    if target <= dag.x[node] {
        return;
    }
    dag.x[node] = target;

    let mut right = target + dag.width[node] / 2;
    for &other in &layer[i + 1..end] {
        let least = right + NODE_SEP + dag.width[other] / 2;
        dag.x[other] = dag.x[other].max(least);
        right = dag.x[other] + dag.width[other] / 2;
    }
}

fn push_left(dag: &mut Dag, layer: &[usize], priorities: &[i64], i: usize, want: i32) {
    let node = layer[i];
    let mut limit = i32::MIN;
    let mut needed = 0;
    let mut end = 0;

    for j in (0..i).rev() {
        let other = layer[j];
        if priorities[j] >= priorities[i] {
            limit = dag.x[other] + dag.width[other] / 2 + needed + NODE_SEP + dag.width[node] / 2;
            end = j + 1;
            break;
        }
        needed += NODE_SEP + dag.width[other];
    }

    let target = want.max(limit);
    if target >= dag.x[node] {
        return;
    }
    dag.x[node] = target;

    let mut left = target - dag.width[node] / 2;
    for &other in layer[end..i].iter().rev() {
        let most = left - NODE_SEP - dag.width[other] / 2;
        dag.x[other] = dag.x[other].min(most);
        left = dag.x[other] - dag.width[other] / 2;
    }
}

/// Slide everything so the leftmost box starts at the margin. Pushing left is
/// allowed to go negative, and a canvas cannot.
fn normalize(dag: &mut Dag) {
    let Some(leftmost) = (0..dag.count()).map(|n| dag.x[n] - dag.width[n] / 2).min() else {
        return;
    };
    let shift = MARGIN - leftmost;
    for x in &mut dag.x {
        *x += shift;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{acyclic, order, rank};

    fn build(count: usize, widths: &[i32], edges: &[(usize, usize, i32)]) -> Dag {
        let mut dag = Dag {
            real: count,
            rank: vec![0; count],
            width: widths.to_vec(),
            key: (0..count).map(|i| format!("n{i:03}")).collect(),
            ..Dag::default()
        };
        let roles = acyclic::break_cycles(count, edges);
        rank::assign(&mut dag, edges, &roles);
        rank::split_long_edges(&mut dag, edges, &roles);
        // Bend points have no width, and the builder above cannot know how
        // many there will be.
        dag.width.resize(dag.count(), 0);
        order::minimize_crossings(&mut dag);
        assign(&mut dag);
        dag
    }

    fn e(from: usize, to: usize) -> (usize, usize, i32) {
        (from, to, 1)
    }

    fn overlaps(dag: &Dag) -> bool {
        dag.layers.iter().any(|layer| {
            layer.windows(2).any(|pair| {
                let (a, b) = (pair[0], pair[1]);
                dag.x[b] - dag.width[b] / 2 < dag.x[a] + dag.width[a] / 2 + NODE_SEP
            })
        })
    }

    #[test]
    fn nothing_overlaps_and_nothing_starts_before_the_margin() {
        let d = build(5, &[100, 80, 200, 60, 120], &[e(0, 1), e(0, 2), e(0, 3), e(1, 4)]);
        assert!(!overlaps(&d));
        for n in 0..d.count() {
            assert!(d.x[n] - d.width[n] / 2 >= MARGIN, "node {n} starts at {}", d.x[n]);
        }
    }

    #[test]
    fn the_leftmost_box_sits_exactly_on_the_margin() {
        let d = build(3, &[100, 100, 100], &[e(0, 1), e(0, 2)]);
        let leftmost = (0..d.count()).map(|n| d.x[n] - d.width[n] / 2).min().unwrap();
        assert_eq!(leftmost, MARGIN);
    }

    #[test]
    fn a_parent_is_centred_over_two_children() {
        let d = build(3, &[100, 100, 100], &[e(0, 1), e(0, 2)]);
        let middle = (d.x[1] + d.x[2]) / 2;
        assert!((d.x[0] - middle).abs() <= 1, "parent {} vs midpoint {middle}", d.x[0]);
    }

    #[test]
    fn a_lone_child_lines_up_under_its_parent() {
        let d = build(2, &[100, 60], &[e(0, 1)]);
        assert_eq!(d.x[0], d.x[1]);
    }

    #[test]
    fn a_bend_point_never_yields_to_a_real_node() {
        // 0 -> 1 -> 2 with the spanning edge 0 -> 2 bending beside 1, plus a
        // wide sibling crowding the middle layer. The last pass runs upwards,
        // so the bend's one wish is to sit under node 2 -- and it outranks
        // everything in its layer, so it gets it exactly.
        let d = build(4, &[100, 100, 100, 240], &[e(0, 1), e(1, 2), e(0, 2), e(0, 3)]);
        let bend = d.real;
        assert!(d.is_virtual(bend), "the spanning edge produced a bend point");
        assert_eq!(d.x[bend], d.x[2], "the bend lines up with its target");
        assert!(!overlaps(&d), "and the real nodes moved out of its way");
    }

    #[test]
    fn ordering_survives_coordinate_assignment() {
        let d = build(6, &[80, 80, 240, 80, 80, 80], &[e(0, 2), e(0, 3), e(1, 4), e(1, 5)]);
        for layer in &d.layers {
            for pair in layer.windows(2) {
                assert!(d.x[pair[0]] < d.x[pair[1]], "a node overtook its neighbour");
            }
        }
    }

    #[test]
    fn wide_labels_get_the_room_they_need() {
        let d = build(3, &[80, 400, 80], &[e(0, 1), e(0, 2)]);
        let (a, b) = (d.layers[1][0], d.layers[1][1]);
        assert!(d.x[b] - d.width[b] / 2 - (d.x[a] + d.width[a] / 2) >= NODE_SEP);
    }

    #[test]
    fn coordinates_are_the_same_on_every_run() {
        let widths = [100, 80, 160, 60, 120, 90];
        let edges = vec![e(0, 1), e(0, 2), e(1, 3), e(2, 3), e(0, 4), e(4, 5), e(0, 5)];
        let first = build(6, &widths, &edges).x;
        for _ in 0..5 {
            assert_eq!(build(6, &widths, &edges).x, first);
        }
    }
}
```

## Pilot notes

Reproduce with the repo's own `dankg` binary, from the repo root:

```
cargo run --bin dankg -- tangle literate/layout-pilot/scaffold.md literate/layout-pilot/layout.md --lang rust
```

Two files named explicitly (not the directory), so this corpus is exactly
`{scaffold.md, layout.md}` -- naming `literate/layout-pilot` instead would
work identically here, but naming `literate` would not: it would also
pull in `../hash.md`, which has its own root and its own pilot, and this
one should never depend on that coincidence.

**`glue` chains two scripts, and the chaining itself surfaced a real bug.**
`.dankg/config` runs `rust.py` (mod.rs generation) then `rust-doclint.py`
(the duplicate-doc-comment check from `hash.md`'s own pilot). The first
attempt at chaining tangled clean and silent -- which read as a pass until
manual inspection found `rust-doclint.py` had not actually checked
anything: its `Path(entry["source"])` assumed `source` was resolvable from
wherever the script runs, true for `hash.md`'s single-file tangle (where
`source` is exactly the path typed on the command line) but false here,
where a corpus-wide tangle makes `source` root-relative instead (`"layout.md"`,
not `"literate/layout-pilot/layout.md"`) -- and neither `{dir}` nor
anything else `glue` is handed carries that root. Fixed with an optional
second CLI argument (`rust-doclint.py {dir} literate/layout-pilot`, wired
into this file's own `.dankg/config`), backed by a regression test in
`glue/test_glue.py` and, once actually working, a real result: it found
three genuine duplications this file had carried since it was first
written -- `rank`, `order`, and `coord`'s leading `//!` each restated this
document's own prose almost verbatim, and `coord`'s `BEND_PRIORITY` doc
duplicated a clause too. All four are fixed above, following exactly the
convention `hash.md` already established. The lesson worth keeping: a
silent, clean run is not evidence of a passing check -- it is only
evidence until someone verifies the check actually ran at all.

**What this pilot proved that `hash.md` could not.** `hash.md` is one
flat file with no `mod.rs` question to ask at all. This one deliberately
withholds `layout/mod.rs`'s `mod acyclic; mod rank; mod order; mod types;`
declarations from the literate source, so `glue/rust.py` -- the real,
unmodified reference glue script, unchanged from what pilot 1 already
shipped -- has to generate `layout/mod.rs` from nothing for the crate to
compile at all. Verified three ways, not just "it built": (1) the
generated `layout/mod.rs` is exactly `mod acyclic; mod coord; mod order;
mod rank; mod types;`, present nowhere in this source; (2) disabling
`glue` in `.dankg/config` and re-tangling fails the build with rustc's own
`E0583: file not found for module` -- glue is load-bearing here, not
incidentally unnecessary; (3) two independent mutations to the ported
algorithm code (self-loop detection in `acyclic`, and a `.max()` dropped
in `rank`'s longest-path assignment -- the second one, tried first,
turned out to coincide with every existing test's own graph shape and
proved nothing either way, which is its own small lesson about how easy a
mutation test is to get wrong) confirm the transcribed tests still catch
real breakage, not just pass because nothing exercises the code.

**What's a deliberate departure from `src/layout/`, not a copy.**
`types.rs` does not exist in the real crate -- `Dag`/`Segment`/`Route`
live at the top of the real `mod.rs`, above its own `mod` lines, and since
nothing here may write those lines by hand, the shared types needed
somewhere else to live. Extracting them is the smallest change that makes
`layout/`'s own `mod.rs` have *nothing* hand-authored left in it. `mod.rs`'s
driver function (`layout()`, `assemble()`, `points_of()`) and its whole
`graph_of`-backed test suite are out of scope entirely, not trimmed down
-- pulling in `crate::graph` would mean also tangling `graph::model`
(itself dependency-free, so tractable) but `graph_of`'s own test helper
needs `graph::build`, `graph::resolve`, and `md::` too, none of which
bear on the question this pilot exists to answer.

**A finding worth keeping, independent of this specific pilot** (now also
recorded in architecture.org, next to decision 27's own explanation).
Real, hand-written Rust in this repo's own `src/` will never naturally
exercise `glue`'s generative path, because a human author already writes
their own `mod.rs` -- and `glue/rust.py` explicitly never overwrites one
that already exists, so tangling that file verbatim leaves nothing for
the generator to do. A straight *port* of an existing multi-file module
therefore proves nothing about the generator at all; only a module
authored fresh *through* dankg, whose author never writes `mod.rs` in the
first place, actually needs it. That is exactly why this pilot could not
just transcribe `src/layout/` unchanged -- `types.rs`'s extraction above
exists specifically to remove the one hand-written piece that would have
made `glue` a no-op here too.
