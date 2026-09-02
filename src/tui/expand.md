# TUI expand

Tab-to-expand reveals a selected node's hidden neighbours onto the
existing grid, per architecture.md's "Expansion is a placement, not a
second layout." `render/assets.rs`'s HTML renderer script (not yet
converted) solves the identical problem, and this module mirrors its
own `around`/`freeSlot`/`expand` functions. It cannot reuse them
directly: one runs in the browser against a DOM it edits incrementally,
and this rebuilds a `Layout` from scratch every time, because that is
what `tui/draw.rs` (not yet converted) already does every frame anyway,
with no diffing at all.

Rebuilding from scratch, rather than mutating in place, also buys
collapse for free. An anchor no longer reachable, because whatever
revealed *it* was just collapsed, is silently skipped, so whatever it
had revealed folds away too. No separate ownership bookkeeping has to
stay in sync, unlike the HTML renderer's own `collapse`, which has to
reassign or recursively remove nodes another expansion might still
reach.

```rust name=module_doc path=tui/expand.rs
//! Tab-to-expand reveals a selected node's hidden neighbours onto the
//! existing grid, per architecture.md's "Expansion is a placement, not
//! a second layout". The HTML renderer's script (`render/assets.rs`)
//! solves the same problem: a revealed node dropped into the nearest
//! free slot on the rank its edge puts it on, not a second Sugiyama
//! pass. This module mirrors its `around`/`freeSlot`/`expand`
//! functions. It cannot reuse them directly: one runs in the browser
//! against a DOM it edits incrementally, and this rebuilds a `Layout`
//! from scratch every time, because that is what `draw.rs` already does
//! every frame anyway (no diffing; see its own header comment).
//!
//! Rebuilding from scratch, rather than mutating in place, also buys
//! collapse for free. An anchor no longer reachable (because whatever
//! revealed *it* was just collapsed) is silently skipped, so whatever
//! it had revealed folds away too. No separate ownership bookkeeping
//! has to stay in sync, unlike the HTML renderer's `collapse`, which
//! has to reassign or recursively remove nodes another expansion might
//! still reach.

use crate::graph::{Edge, Graph, NodeId};
use crate::layout::{label_width, LaidEdge, LaidNode, Layout, MARGIN, NODE_HEIGHT, NODE_SEP, RANK_SEP};
```

```rust name=hidden_neighbours_and_free_slot path=tui/expand.rs
/// Neighbours of `id` in `index` that are not in `visible`, paired
/// with which way the edge runs: `1` when `id` is the edge's source
/// (the neighbour belongs one rank below, the way a normal child
/// does), `-1` when `id` is the target (one rank above). A self-loop
/// has nothing to reveal: its only neighbour is `id` itself, already
/// visible. Dedup is against `visible` plus what this call has already
/// found, so a pair joined by both a containment and a link edge is
/// only placed once, at whichever direction its first edge (in the
/// index's sorted order) gives.
fn hidden_neighbours(index: &Graph, visible: &[NodeId], id: &NodeId) -> Vec<(NodeId, i64)> {
    let mut found: Vec<(NodeId, i64)> = Vec::new();
    for edge in &index.edges {
        if edge.from == edge.to {
            continue;
        }
        let (other, delta) = if &edge.from == id {
            (&edge.to, 1)
        } else if &edge.to == id {
            (&edge.from, -1)
        } else {
            continue;
        };
        if visible.contains(other) || found.iter().any(|(n, _)| n == other) {
            continue;
        }
        found.push((other.clone(), delta));
    }
    found
}

/// The x nearest `prefer` where a box of `width` clears every one of
/// `boxes` (each an existing box's `(left, right)` extent) by at least
/// `NODE_SEP`. Tries snapping against every existing box's edge before
/// falling back to going past all of them. This is the same algorithm
/// as the HTML renderer's `freeSlot`, since both are answering the same
/// question against the same kind of grid.
fn free_slot(boxes: &[(i32, i32)], width: i32, prefer: i32) -> i32 {
    let fits = |x: i32| boxes.iter().all(|&(l, r)| x + width / 2 + NODE_SEP <= l || x - width / 2 - NODE_SEP >= r);

    let mut tries = vec![prefer];
    for &(l, r) in boxes {
        tries.push(r + NODE_SEP + width / 2);
        tries.push(l - NODE_SEP - width / 2);
    }
    tries.sort_by_key(|&t| (t - prefer).abs());

    if let Some(&t) = tries.iter().find(|&&t| fits(t)) {
        return t;
    }
    let right = boxes.iter().map(|&(_, r)| r).max().unwrap_or(MARGIN);
    right + NODE_SEP + width / 2
}
```

`expand_view` processes anchors in the order the reader opened them.
This is necessarily an order where each anchor is already visible by
the time it is processed, since a reader can only select and expand a
node already on screen. `Placed.rank` is signed for exactly one reason:
an expansion can reach *above* the base view's rank 0 (an anchor's
incoming edge from something outside the view) as easily as below it.
`LaidNode.rank` never needs to represent that in the real,
already-normalized layout.

```rust name=placed_and_expand_view path=tui/expand.rs
/// One node placed so far, tracked with a signed rank because an
/// expansion can reach above the base view's rank 0 (an anchor's
/// incoming edge from something outside the view) as easily as below
/// it. `LaidNode.rank` is unsigned because the real layout never needs
/// to represent that.
struct Placed {
    id: NodeId,
    rank: i64,
    x: i32,
    width: i32,
}

/// Rebuilds the drawn graph and layout from `base` plus every anchor
/// in `expanded`, processed in the order the reader opened them. This
/// is also, necessarily, an order where each anchor is already visible
/// by the time it is processed, since a reader can only select and
/// expand a node that is already on screen.
pub fn expand_view(index: &Graph, base_graph: &Graph, base_layout: &Layout, expanded: &[NodeId]) -> (Graph, Layout) {
    let mut placed: Vec<Placed> = base_layout
        .nodes
        .iter()
        .map(|n| Placed { id: n.id.clone(), rank: n.rank as i64, x: n.x, width: n.width })
        .collect();
    let mut nodes = base_graph.nodes.clone();
    let mut visible: Vec<NodeId> = placed.iter().map(|p| p.id.clone()).collect();

    for anchor in expanded {
        let Some(found) = placed.iter().find(|p| &p.id == anchor) else {
            continue; // whatever revealed it was collapsed. Nothing to expand from
        };
        let (anchor_rank, anchor_x) = (found.rank, found.x);

        for (neighbour, delta) in hidden_neighbours(index, &visible, anchor) {
            let Some(node) = index.node(&neighbour) else { continue };
            let rank = anchor_rank + delta;
            let width = label_width(&node.title);
            let boxes: Vec<(i32, i32)> =
                placed.iter().filter(|p| p.rank == rank).map(|p| (p.x - p.width / 2, p.x + p.width / 2)).collect();
            let x = free_slot(&boxes, width, anchor_x);

            placed.push(Placed { id: neighbour.clone(), rank, x, width });
            nodes.push(node.clone());
            visible.push(neighbour);
        }
    }

    let edges = induced_edges(index, &visible);
    let layout = assemble(placed, laid_edges(base_layout, &edges));
    (Graph { nodes, edges }, layout)
}

/// The edges `index` carries between two nodes that are both visible.
/// This is exactly `graph::view::select`'s definition of an induced
/// subgraph, applied to the visible set expansion just grew, rather
/// than to a hop count.
fn induced_edges(index: &Graph, visible: &[NodeId]) -> Vec<Edge> {
    index.edges.iter().filter(|e| visible.contains(&e.from) && visible.contains(&e.to)).cloned().collect()
}
```

`assemble` does three things at once, each independently necessary.
It normalizes signed ranks back to `LaidNode`'s unsigned ones, shifting
everything down uniformly if expansion reached above rank 0, so the
base nodes' relative positions never change, only their absolute row.
It shifts x right if `free_slot` walked a placement left of the canvas
origin. `free_slot` has no floor to stop it from doing that, and
`tui/draw.rs`'s own `put` silently drops anything at a negative column
rather than erroring, so a crowded expansion could previously make a
revealed node disappear without a trace. And it reassigns `order`
within each rank by `x`. That is what crossing minimization would
already have produced for the base nodes (a no-op for them here), and
the only sane answer for a freshly slotted-in one, since `h`/`l`
navigation walks a rank in screen order.

```rust name=assemble path=tui/expand.rs
/// Normalizes signed ranks to `LaidNode`'s unsigned ones, shifting
/// everything down uniformly if expansion reached above rank 0, so the
/// base nodes' relative positions never change, only their absolute
/// row. Shifts x right if `free_slot` walked a placement left of the
/// canvas origin. `free_slot` has no floor to stop it: the base layout
/// guarantees every left edge is `>= 0`, per its own tests, but nothing
/// here re-enforces that until now, and `draw.rs`'s `put` silently
/// drops anything at a negative column rather than erroring, so a
/// crowded expansion could previously make a revealed node disappear
/// without a trace. And reassigns `order` within each rank by `x`.
/// That is what crossing minimization would already have produced for
/// the base nodes (left untouched here, so a no-op for them), and the
/// only sane answer for a slotted-in one, since `h`/`l` navigation
/// walks a rank in screen order.
fn assemble(mut placed: Vec<Placed>, edges: Vec<LaidEdge>) -> Layout {
    let Some(min_rank) = placed.iter().map(|p| p.rank).min() else {
        return Layout::default();
    };
    let ranks = (placed.iter().map(|p| p.rank).max().unwrap_or(min_rank) - min_rank + 1) as u32;

    let min_left = placed.iter().map(|p| p.x - p.width / 2).min().unwrap_or(0);
    if min_left < 0 {
        for p in &mut placed {
            p.x -= min_left;
        }
    }

    let mut by_rank: Vec<Vec<Placed>> = (0..ranks as usize).map(|_| Vec::new()).collect();
    for p in placed {
        by_rank[(p.rank - min_rank) as usize].push(p);
    }

    let mut nodes = Vec::new();
    for (rank, row) in by_rank.iter_mut().enumerate() {
        row.sort_by(|a, b| a.x.cmp(&b.x).then_with(|| a.id.cmp(&b.id)));
        for (order, p) in row.iter().enumerate() {
            nodes.push(LaidNode {
                id: p.id.clone(),
                rank: rank as u32,
                order: order as u32,
                x: p.x,
                y: MARGIN + rank as i32 * (NODE_HEIGHT + RANK_SEP) + NODE_HEIGHT / 2,
                width: p.width,
                height: NODE_HEIGHT,
            });
        }
    }

    let width = nodes.iter().map(|n| n.x + n.width / 2).max().unwrap_or(0) + MARGIN;
    let height = nodes.iter().map(|n| n.y + n.height / 2).max().unwrap_or(0) + MARGIN;
    Layout { nodes, edges, width, height, ranks }
}

/// Every real, Sugiyama-routed base edge kept exactly as laid out, plus a
/// synthesized entry for each new one expansion exposed.
fn laid_edges(base_layout: &Layout, edges: &[Edge]) -> Vec<LaidEdge> {
    let mut laid = base_layout.edges.clone();
    for edge in edges {
        if laid.iter().any(|e| e.from == edge.from && e.to == edge.to && e.kind == edge.kind) {
            continue;
        }
        // Not run through cycle-breaking or bend-point routing:
        // `draw.rs` reads neither field, deriving its own up/down from
        // the two nodes' actual ranks instead of trusting a precomputed
        // direction (see its `to_is_top`). Only a real, Sugiyama-laid
        // edge carries that information meaningfully.
        laid.push(LaidEdge {
            from: edge.from.clone(),
            to: edge.to.clone(),
            kind: edge.kind,
            reciprocated: edge.reciprocated,
            reversed: false,
            points: Vec::new(),
        });
    }
    laid
}
```

## Tests

```rust name=tests path=tui/expand.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{graph_of, view};
    use crate::layout::layout as run_layout;

    /// entry(rank0) contains two children(rank1). One child links out to
    /// a fourth node the depth-0 view never reaches. depth 0 selects
    /// only the entry, so both children and the linked node start hidden.
    fn corpus() -> Graph {
        graph_of(&[(
            "a.md",
            "# Entry\n\n## Left\n\nsee [out](out.md#out)\n\n## Right\n",
        ), ("out.md", "# Out\n")])
    }

    fn entry_only_view(index: &Graph) -> (Graph, Layout) {
        let entries = view::entry_nodes(index, &["a.md".to_string()]);
        let base = view::select(index, &entries[..1], 0); // just Entry
        let laid = run_layout(&base);
        (base, laid)
    }

    #[test]
    fn expanding_with_nothing_expanded_is_the_base_view_unchanged() {
        let index = corpus();
        let (base, laid) = entry_only_view(&index);
        let (graph, layout) = expand_view(&index, &base, &laid, &[]);
        assert_eq!(graph, base);
        assert_eq!(layout, laid);
    }

    #[test]
    fn tab_on_the_entry_reveals_its_direct_children_one_rank_down() {
        let index = corpus();
        let (base, laid) = entry_only_view(&index);
        let entry = laid.nodes[0].id.clone();
        let (graph, layout) = expand_view(&index, &base, &laid, &[entry]);

        assert_eq!(graph.nodes.len(), 3, "entry, left, right");
        for slug in ["left", "right"] {
            let n = layout.nodes.iter().find(|n| n.id.slug == slug).unwrap_or_else(|| panic!("{slug} revealed"));
            assert_eq!(n.rank, 1, "{slug} sits one rank below the entry");
        }
        // out.md is two hops away (through Left), so it stays hidden.
        assert!(!graph.nodes.iter().any(|n| n.id.slug == "out"));
    }

    #[test]
    fn a_chained_expansion_can_reach_above_rank_zero() {
        // Select Out as the sole entry (rank 0), then expand it. Left
        // reaches Out via an edge Out is the *target* of, so Left
        // belongs one rank *above* Out: rank -1 before normalization.
        let index = corpus();
        let entries = view::entry_nodes(&index, &["out.md".to_string()]);
        let base = view::select(&index, &entries, 0);
        let laid = run_layout(&base);
        let out = laid.nodes[0].id.clone();

        let (graph, layout) = expand_view(&index, &base, &laid, std::slice::from_ref(&out));
        let left = layout.nodes.iter().find(|n| n.id.slug == "left").expect("left revealed");
        let out_laid = layout.nodes.iter().find(|n| n.id == out).unwrap();
        assert_eq!(out_laid.rank, left.rank + 1, "normalization keeps Out one rank below Left");
        assert!(graph.nodes.iter().any(|n| n.id.slug == "left"));
    }

    #[test]
    fn collapsing_an_anchor_also_hides_what_only_it_revealed() {
        let index = corpus();
        let (base, laid) = entry_only_view(&index);
        let entry = laid.nodes[0].id.clone();

        let (with_left, layout_with_left) = expand_view(&index, &base, &laid, std::slice::from_ref(&entry));
        let left = layout_with_left.nodes.iter().find(|n| n.id.slug == "left").unwrap().id.clone();
        assert!(with_left.nodes.iter().any(|n| n.id.slug == "left"));

        // Expand Left too, reaching Out.
        let (fully_expanded, _) = expand_view(&index, &base, &laid, &[entry.clone(), left.clone()]);
        assert!(fully_expanded.nodes.iter().any(|n| n.id.slug == "out"), "out reachable via the now-expanded left");

        // Simulate collapsing Entry: it drops out of `expanded`,
        // leaving `left` in the list but with nothing left to reach it
        // from. `expand_view` should treat it as unreachable, rather
        // than as still anchoring Out.
        let (after_collapsing_entry, _) = expand_view(&index, &base, &laid, &[left]);
        assert_eq!(after_collapsing_entry, base, "left and out both fold away with entry");
    }

    #[test]
    fn a_crowded_expansion_never_places_a_node_off_the_left_of_the_canvas() {
        // Five files all link into a lone entry. Expanding it piles
        // five incoming neighbours onto rank -1, preferring the entry's
        // x, which sits close to the canvas origin. `free_slot` alone
        // will walk some of them left of 0 (reproduced without this
        // test's fix by asserting on the pre-shift `x` field directly).
        let index = graph_of(&[
            ("entry.md", "# Entry\n"),
            ("a.md", "# A\n\n[e](entry.md#entry)\n"),
            ("b.md", "# B\n\n[e](entry.md#entry)\n"),
            ("c.md", "# C\n\n[e](entry.md#entry)\n"),
            ("d.md", "# D\n\n[e](entry.md#entry)\n"),
            ("f.md", "# F\n\n[e](entry.md#entry)\n"),
        ]);
        let entries = view::entry_nodes(&index, &["entry.md".to_string()]);
        let base = view::select(&index, &entries, 0);
        let laid = run_layout(&base);
        let entry = laid.nodes[0].id.clone();

        let (graph, layout) = expand_view(&index, &base, &laid, std::slice::from_ref(&entry));
        assert_eq!(graph.nodes.len(), 6, "entry plus all five linkers");
        for n in &layout.nodes {
            assert!(n.x - n.width / 2 >= 0, "{} has a negative left edge -- draw.rs would drop it silently", n.id);
        }
    }

    #[test]
    fn a_revealed_node_avoids_overlapping_its_new_rank_mates() {
        let index = corpus();
        let (base, laid) = entry_only_view(&index);
        let entry = laid.nodes[0].id.clone();
        let (_, layout) = expand_view(&index, &base, &laid, &[entry]);

        let mut row: Vec<&LaidNode> = layout.nodes.iter().filter(|n| n.rank == 1).collect();
        row.sort_by_key(|n| n.order);
        assert_eq!(row.len(), 2);
        let gap = (row[1].x - row[1].width / 2) - (row[0].x + row[0].width / 2);
        assert!(gap >= NODE_SEP, "revealed siblings overlap: gap {gap}");
    }

    #[test]
    fn free_slot_prefers_the_anchors_x_when_nothing_is_there_yet() {
        assert_eq!(free_slot(&[], 40, 100), 100);
    }

    #[test]
    fn free_slot_snaps_to_the_nearer_side_of_a_blocking_box() {
        // An existing box occupies [80, 120]. A new, width-40 box
        // preferring 90 cannot fit there (it overlaps). The left side
        // (candidate 32) is strictly nearer 90 than the right side
        // (candidate 168), so that is the one it should return.
        let x = free_slot(&[(80, 120)], 40, 90);
        assert!(x + 20 <= 80, "expected the nearer, left-hand slot: got {x}");
    }
}
```
