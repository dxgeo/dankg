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

use super::build::strip_extension;
use super::model::{Graph, NodeId};
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

`select_view` is the entry point both `dankg graph` and the TUI actually
call -- one answer to "which view," shared rather than each command
re-deriving it. `default_depth` arrives as a plain argument rather than
this function reaching into a loaded `Config` itself, so it stays a pure
function of its inputs and every case below can be driven straight from
values, with no config file involved.

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
        // `d` links to nothing; everything reaches it. One hop back finds `c`.
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
