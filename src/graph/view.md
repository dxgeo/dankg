# Graph view

This module is step 7 of the pipeline that architecture.md's *Pipeline*
section lays out. It is the first step that depends on the entry at all.
Steps 1-6 build the index over the whole root regardless of what a
reader named, since backlinks are only honest once every file has been
seen. This module turns "the whole index" into "the subgraph a reader
can actually take in." That subgraph is the entry's own nodes, plus
everything within a chosen number of hops, counted in both directions.
A link that points *at* the entry is as much a neighbour as one it
points *away* to. That is the entire reason the index is corpus-wide
rather than per-file in the first place.

```rust name=module_doc path=graph/view.rs
//! View selection: the entry, plus everything within N hops of it.
//!
//! Steps 1-6 of the pipeline build the index and never depend on the entry.
//! This is step 7, the first thing that does. The index is always the whole
//! root, because backlinks are only honest when every file has been seen.
//! The view is what a reader can actually take in.
//!
//! Hops are counted in both directions. A link that points at the entry is as
//! much a neighbour as one the entry points at. That is the whole reason the
//! index is built over the corpus rather than over the file. Containment
//! counts too. This is the open question in architecture.md. It can make
//! depth 2 feel shallow in a deeply nested file. `--all` is the answer
//! until something better is decided.
//!
//! One exception (decision 38): entering a `Relation` node over a
//! `Produces`/`Reads` edge, from either direction, costs nothing against
//! `depth`. Leaving one, onto a block, costs the ordinary hop. A relation
//! renders alongside every block already in view that touches it, at any
//! depth including 0, without shortening the distance between two
//! otherwise-unrelated blocks that happen to share it.

use super::build::strip_extension;
use super::model::{EdgeKind, Graph, NodeId, NodeKind};
```

```rust name=entry_nodes path=graph/view.rs
/// Every node belonging to one of the named files. A file names all of its
/// headings, not just the first. "graph this file and what it touches" is the
/// question being asked.
pub fn entry_nodes(graph: &Graph, files: &[String]) -> Vec<NodeId> {
    let keys: Vec<String> = files.iter().map(|f| strip_extension(f)).collect();
    graph
        .nodes
        .iter()
        .filter(|n| keys.contains(&n.id.file))
        .map(|n| n.id.clone())
        .collect()
}
```

`select` is induced, not spanning. An edge between two nodes that both
made it into the view is kept even when it was not the specific edge that
brought either endpoint in. Dropping it would render a graph missing
structure the reader can plainly see is there.

```rust name=select path=graph/view.rs
/// The subgraph induced by the entry and everything within `depth` hops.
///
/// Induced, not spanning. An edge between two nodes that both made it in is
/// kept even when it was not the edge that brought either of them there.
/// Dropping it would render a graph that is missing structure it can see.
pub fn select(graph: &Graph, entries: &[NodeId], depth: u32) -> Graph {
    let mut frontier: Vec<NodeId> = Vec::new();
    let mut chosen: Vec<NodeId> = Vec::new();

    for id in entries {
        if graph.contains(id) && !chosen.contains(id) {
            chosen.push(id.clone());
            frontier.push(id.clone());
        }
    }
    // Decision 38: a relation any already-chosen block touches renders for
    // free, at depth 0 same as any other. Run once for the entries
    // themselves, before the first real hop ever spends anything.
    zero_cost_relations(graph, &mut chosen, &mut frontier);

    for _ in 0..depth {
        let mut next: Vec<NodeId> = Vec::new();
        for edge in &graph.edges {
            for (near, far) in [(&edge.from, &edge.to), (&edge.to, &edge.from)] {
                if frontier.contains(near) && !chosen.contains(far) && !next.contains(far) {
                    next.push(far.clone());
                }
            }
        }
        if next.is_empty() {
            break;
        }
        chosen.extend(next.iter().cloned());
        frontier = next;
        // A block this real hop just reached may touch a relation of its
        // own -- free the same way the entries' did, not a second hop.
        zero_cost_relations(graph, &mut chosen, &mut frontier);
    }

    Graph {
        nodes: graph.nodes.iter().filter(|n| chosen.contains(&n.id)).cloned().collect(),
        edges: graph
            .edges
            .iter()
            .filter(|e| chosen.contains(&e.from) && chosen.contains(&e.to))
            .cloned()
            .collect(),
    }
}
```

`zero_cost_relations` is decision 38's own exception to the ordinary hop above: it runs *outside* the `depth` loop entirely, so nothing it finds ever consumes a hop. It only ever adds a `Relation` -- stepping further, off a relation onto a block, is the ordinary edge scan already above, once the relation sits in `frontier` for that scan to reach. This is a single pass, not a fixed point, because a relation never edges to another relation (`EdgeKind::Produces`/`Reads` only ever connect a block to a relation), so nothing reachable this way ever needs a second round to settle.

```rust name=zero_cost_relations path=graph/view.rs
fn zero_cost_relations(graph: &Graph, chosen: &mut Vec<NodeId>, frontier: &mut Vec<NodeId>) {
    let mut free: Vec<NodeId> = Vec::new();
    for edge in &graph.edges {
        if !matches!(edge.kind, EdgeKind::Produces | EdgeKind::Reads) {
            continue;
        }
        for (near, far) in [(&edge.from, &edge.to), (&edge.to, &edge.from)] {
            let Some(far_node) = graph.node(far) else { continue };
            if far_node.kind == NodeKind::Relation
                && frontier.contains(near)
                && !chosen.contains(far)
                && !free.contains(far)
            {
                free.push(far.clone());
            }
        }
    }
    chosen.extend(free.iter().cloned());
    frontier.extend(free);
}
```

`select_view` is the entry point both `dankg graph` and the TUI actually
call. It gives one answer to "which view," shared rather than each
command re-deriving it. `default_depth` arrives as a plain argument,
rather than this function reaching into a loaded `Config` itself, so it
stays a pure function of its inputs. Every case below can be driven
straight from values, with no config file involved.

```rust name=select_view path=graph/view.rs
/// The subgraph to render: the named entry files' nodes plus `depth` hops, or
/// the whole index under `all`. Also the whole index when there is no entry,
/// because a bare directory was named and the only sensible reading of
/// "graph this corpus" is all of it. Shared by `dankg graph` and the TUI
/// (milestone 7), since both need the same answer to "which view."
///
/// `default_depth` is the caller's job to resolve (typically `[graph]
/// depth` off the loaded config), so this stays a pure function of its
/// arguments. It does not reach into a `Corpus` or raise diagnostics
/// itself.
pub fn select_view(index: &Graph, entry_files: &[String], depth: Option<u32>, all: bool, default_depth: u32) -> Graph {
    if all || entry_files.is_empty() {
        return index.clone();
    }
    let hops = depth.unwrap_or(default_depth);
    let entries = entry_nodes(index, entry_files);
    select(index, &entries, hops)
}
```

## Tests

```rust name=tests path=graph/view.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{graph_of, EdgeKind};

    /// a -> b -> c -> d, one link per hop, each in its own file.
    fn chain() -> Graph {
        graph_of(&[
            ("a.md", "# A\n\n[to b](b.md#b)\n"),
            ("b.md", "# B\n\n[to c](c.md#c)\n"),
            ("c.md", "# C\n\n[to d](d.md#d)\n"),
            ("d.md", "# D\n"),
        ])
    }

    fn ids(graph: &Graph) -> Vec<String> {
        let mut out: Vec<String> = graph.nodes.iter().map(|n| n.id.to_string()).collect();
        out.sort();
        out
    }

    #[test]
    fn depth_counts_hops_from_the_entry() {
        let g = chain();
        let entry = vec![NodeId::new("a", "a")];
        assert_eq!(ids(&select(&g, &entry, 0)), vec!["a#a"]);
        assert_eq!(ids(&select(&g, &entry, 1)), vec!["a#a", "b#b"]);
        assert_eq!(ids(&select(&g, &entry, 2)), vec!["a#a", "b#b", "c#c"]);
    }

    #[test]
    fn hops_are_counted_in_both_directions() {
        let g = chain();
        // `d` links to nothing. Everything reaches it. One hop back finds `c`.
        let selected = select(&g, &[NodeId::new("d", "d")], 1);
        assert_eq!(ids(&selected), vec!["c#c", "d#d"]);
    }

    #[test]
    fn naming_a_file_selects_every_heading_in_it() {
        let g = graph_of(&[("a.md", "# One\n\n## Two\n\n### Three\n")]);
        let entries = entry_nodes(&g, &["a.md".to_string()]);
        assert_eq!(entries.len(), 3);
        assert_eq!(ids(&select(&g, &entries, 0)), vec!["a#one", "a#three", "a#two"]);
    }

    #[test]
    fn the_subgraph_is_induced_not_spanning() {
        // b and c both arrive at depth 1, from a. The b--c edge is between two
        // selected nodes and has to survive.
        let g = graph_of(&[
            ("a.md", "# A\n\n[b](b.md#b) and [c](c.md#c)\n"),
            ("b.md", "# B\n\n[c](c.md#c)\n"),
            ("c.md", "# C\n"),
        ]);
        let selected = select(&g, &[NodeId::new("a", "a")], 1);
        assert!(
            selected
                .edges
                .iter()
                .any(|e| e.from.to_string() == "b#b" && e.to.to_string() == "c#c"),
            "an edge between two selected nodes is kept"
        );
    }

    #[test]
    fn containment_is_a_hop_like_any_other() {
        let g = graph_of(&[("a.md", "# One\n\n## Two\n")]);
        let selected = select(&g, &[NodeId::new("a", "one")], 1);
        assert_eq!(ids(&selected), vec!["a#one", "a#two"]);
        assert_eq!(selected.edges[0].kind, EdgeKind::Contains);
    }

    // Decision 38: a relation costs nothing to enter over a Produces/Reads
    // edge, from either direction; leaving one, onto a block, costs the
    // ordinary hop. `graph_of` has no way to author a `db=` block's own
    // provenance without a real eval run, so these build the graph by
    // hand, the same way `graph::model` and `graph::query`'s own tests do.
    use super::super::model::{Edge, Node};

    fn relation(db: &str, name: &str) -> Node {
        crate::graph::build::relation_node(&NodeId::new(format!("db:{db}"), name), name)
    }

    fn edge(kind: EdgeKind, from: (&str, &str), to: (&str, &str)) -> Edge {
        Edge { from: NodeId::new(from.0, from.1), to: NodeId::new(to.0, to.1), kind, line: 0, reciprocated: false }
    }

    fn block(file: &str, slug: &str) -> Node {
        Node {
            id: NodeId::new(file, slug),
            title: slug.to_string(),
            file: format!("{file}.md"),
            line: 1,
            end_line: 1,
            level: 7,
            parent: None,
            tags: Vec::new(),
            external: Vec::new(),
            resolved: true,
            kind: NodeKind::Block,
        }
    }

    #[test]
    fn a_produced_relation_is_free_at_depth_zero() {
        // a --Produces--> db:t#r
        let g = Graph {
            nodes: vec![block("a", "setup"), relation("t", "r")],
            edges: vec![edge(EdgeKind::Produces, ("a", "setup"), ("db:t", "r"))],
        };
        assert_eq!(ids(&select(&g, &[NodeId::new("a", "setup")], 0)), vec!["a#setup", "db:t#r"]);
    }

    #[test]
    fn a_read_relation_is_free_from_either_direction() {
        // db:t#r --Reads--> a; entering `r` from `a` means walking this
        // edge backward, the "either direction" decision 38 names.
        let g = Graph {
            nodes: vec![block("a", "setup"), relation("t", "r")],
            edges: vec![edge(EdgeKind::Reads, ("db:t", "r"), ("a", "setup"))],
        };
        assert_eq!(ids(&select(&g, &[NodeId::new("a", "setup")], 0)), vec!["a#setup", "db:t#r"]);
    }

    #[test]
    fn leaving_a_relation_onto_a_block_costs_the_ordinary_hop() {
        // a --Produces--> db:t#r --Reads--> b
        let g = Graph {
            nodes: vec![block("a", "setup"), relation("t", "r"), block("b", "load")],
            edges: vec![
                edge(EdgeKind::Produces, ("a", "setup"), ("db:t", "r")),
                edge(EdgeKind::Reads, ("db:t", "r"), ("b", "load")),
            ],
        };
        // The relation is free at depth 0; `b` is not reachable yet.
        assert_eq!(ids(&select(&g, &[NodeId::new("a", "setup")], 0)), vec!["a#setup", "db:t#r"]);
        // One real hop off the relation reaches `b`, the ordinary cost.
        assert_eq!(ids(&select(&g, &[NodeId::new("a", "setup")], 1)), vec!["a#setup", "b#load", "db:t#r"]);
    }

    #[test]
    fn a_relation_reveals_every_other_block_touching_it_one_hop_away() {
        // Two producers into the same relation: a sibling reveal, exactly
        // as much a real hop buys as a downstream reader would be.
        let g = Graph {
            nodes: vec![block("a", "setup"), block("c", "rebuild"), relation("t", "r")],
            edges: vec![
                edge(EdgeKind::Produces, ("a", "setup"), ("db:t", "r")),
                edge(EdgeKind::Produces, ("c", "rebuild"), ("db:t", "r")),
            ],
        };
        assert_eq!(ids(&select(&g, &[NodeId::new("a", "setup")], 0)), vec!["a#setup", "db:t#r"]);
        assert_eq!(ids(&select(&g, &[NodeId::new("a", "setup")], 1)), vec!["a#setup", "c#rebuild", "db:t#r"]);
    }

    #[test]
    fn an_unreachable_island_is_left_out() {
        let g = graph_of(&[("a.md", "# A\n"), ("z.md", "# Z\n")]);
        assert_eq!(ids(&select(&g, &[NodeId::new("a", "a")], 9)), vec!["a#a"]);
    }

    #[test]
    fn an_entry_that_is_not_in_the_graph_selects_nothing() {
        let g = chain();
        assert!(select(&g, &[NodeId::new("gone", "gone")], 2).nodes.is_empty());
        assert!(select(&g, &[], 2).nodes.is_empty());
    }

    #[test]
    fn selection_preserves_the_index_order() {
        let g = chain();
        let selected = select(&g, &[NodeId::new("a", "a")], 3);
        assert_eq!(selected.nodes.len(), g.nodes.len());
        assert_eq!(selected, g, "selecting everything is the identity");
    }

    #[test]
    fn select_view_uses_the_default_depth_when_none_is_given() {
        let g = chain();
        let files = vec!["a.md".to_string()];
        let default = select_view(&g, &files, None, false, 1);
        assert_eq!(ids(&default), vec!["a#a".to_string(), "b#b".to_string()]);
        let explicit = select_view(&g, &files, Some(1), false, 99);
        assert_eq!(default, explicit, "an explicit depth overrides the default");
    }

    #[test]
    fn select_view_falls_back_to_the_whole_index() {
        let g = chain();
        assert_eq!(select_view(&g, &["a.md".to_string()], None, true, 1), g, "--all");
        assert_eq!(select_view(&g, &[], None, false, 1), g, "no entry: nothing else to draw");
    }
}
```
