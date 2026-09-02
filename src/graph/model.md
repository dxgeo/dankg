# Graph model

The graph itself: `Node`, `Edge`, `NodeId`, and the two kinds each of the
first two come in. Edges are stored directed. `reciprocated` is computed
only once the whole corpus is known. When `a -> b` and `b -> a` both
exist, both get marked. Every renderer renders one undirected edge
instead of two separate arrows. That single boolean is the entire
mechanism behind "the link goes both ways, but renders as unidirectional
unless it is linked back."

```rust name=module_doc path=graph/model.rs
//! The graph itself.
//!
//! Edges are stored directed. `reciprocated` is computed once the whole corpus
//! is known. When `a -> b` and `b -> a` both exist, both are marked. The
//! renderer renders one undirected edge instead of two arrows. That is the
//! mechanism behind "the link goes both ways, but renders as unidirectional
//! unless it is linked back".

use std::fmt;

/// A node's address: the linking path with its extension removed, then the
/// heading slug. Stable across runs, so it is safe to put in committed output.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId {
    pub file: String,
    pub slug: String,
}

impl NodeId {
    pub fn new(file: impl Into<String>, slug: impl Into<String>) -> Self {
        NodeId { file: file.into(), slug: slug.into() }
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.file, self.slug)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EdgeKind {
    /// Parent heading to child heading. Derived from document structure.
    Contains,
    /// An explicit reference written by the author.
    Link,
}

impl EdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Contains => "contains",
            EdgeKind::Link => "link",
        }
    }
}

/// What a node stands for. A heading is a section of prose. A block is a
/// named, top-level, evaluable code block (decision 19's exact scope --
/// the same one `eval::plan` and `dankg eval --list` use, so "this is a
/// node you can navigate to" and "this is a node `dankg eval` can
/// evaluate" never disagree). A block is always a leaf. Nothing nests
/// inside one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Heading,
    Block,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Heading => "heading",
            NodeKind::Block => "block",
        }
    }

    pub fn parse(text: &str) -> Option<NodeKind> {
        match text {
            "heading" => Some(NodeKind::Heading),
            "block" => Some(NodeKind::Block),
            _ => None,
        }
    }
}
```

```rust name=node_and_edge path=graph/model.rs
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub title: String,
    /// Path as it appears on disk, extension included.
    pub file: String,
    /// Line the heading (or, for a block node, the code fence) is on.
    pub line: u32,
    /// Last line belonging to this node: for a heading, before the next
    /// heading of the same or higher level; for a block, its own closing
    /// fence (`md/block.rs`'s `Block::Code::end_line`), never recomputed
    /// from sibling structure the way a heading's is. Equal to `line` for
    /// an empty heading section.
    pub end_line: u32,
    /// Heading depth (1..=6), or 0 for the synthetic file-level node.
    /// Meaningless for a block node beyond staying above every real
    /// heading level, which `graph/build.rs`'s extent computation relies
    /// on to never mistake a block for a heading's next sibling.
    pub level: u8,
    pub parent: Option<NodeId>,
    /// File-level frontmatter tags, carried on every node in the file.
    pub tags: Vec<String>,
    /// Absolute URLs referenced from this node. Recorded, never graphed.
    pub external: Vec<String>,
    /// False for placeholder nodes invented to receive a dangling link.
    /// Always true for a block node. Nothing ever links to one by name
    /// today, so there is nothing for it to be unresolved against.
    pub resolved: bool,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    /// Line the link was written on. Zero for containment, which is structural
    /// rather than written.
    pub line: u32,
    pub reciprocated: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}
```

Containment is explicitly excluded from `reciprocate`. A parent containing
a child is not the child linking back. Treating it as mutual would
erase direction from the document's own skeleton. `sort` exists for the
same reason `dankg fmt`'s output is meant to be committed and diffed.
Identical input has to produce identical output, every time.

```rust name=graph_impl path=graph/model.rs
impl Graph {
    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| &n.id == id)
    }

    pub fn contains(&self, id: &NodeId) -> bool {
        self.nodes.iter().any(|n| &n.id == id)
    }

    /// Mark every pair of link edges that point at each other.
    ///
    /// Containment is excluded. A parent containing a child is not the child
    /// linking back. Treating it as mutual would erase direction from the
    /// document skeleton.
    pub fn reciprocate(&mut self) {
        let links: Vec<(NodeId, NodeId)> = self
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Link)
            .map(|e| (e.from.clone(), e.to.clone()))
            .collect();

        for edge in &mut self.edges {
            if edge.kind != EdgeKind::Link {
                continue;
            }
            edge.reciprocated = links
                .iter()
                .any(|(from, to)| from == &edge.to && to == &edge.from);
        }
    }

    /// Impose a total order, so that identical input always produces identical
    /// output and a rendered graph can be committed and diffed.
    pub fn sort(&mut self) {
        self.nodes.sort_by(|a, b| {
            (&a.file, a.line, &a.id).cmp(&(&b.file, b.line, &b.id))
        });
        self.edges.sort_by(|a, b| {
            (&a.from, &a.to, a.kind, a.line).cmp(&(&b.from, &b.to, b.kind, b.line))
        });
        self.edges.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.kind == b.kind && a.line == b.line);
    }
}
```

## Tests

```rust name=tests path=graph/model.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn node(file: &str, slug: &str) -> Node {
        Node {
            id: NodeId::new(file, slug),
            title: slug.to_string(),
            file: format!("{file}.md"),
            line: 1,
            end_line: 1,
            level: 1,
            parent: None,
            tags: Vec::new(),
            external: Vec::new(),
            resolved: true,
            kind: NodeKind::Heading,
        }
    }

    fn link(from: (&str, &str), to: (&str, &str)) -> Edge {
        Edge {
            from: NodeId::new(from.0, from.1),
            to: NodeId::new(to.0, to.1),
            kind: EdgeKind::Link,
            line: 1,
            reciprocated: false,
        }
    }

    #[test]
    fn node_id_displays_as_file_hash_slug() {
        assert_eq!(NodeId::new("notes/project", "overview").to_string(), "notes/project#overview");
    }

    #[test]
    fn node_kind_round_trips_through_text() {
        for kind in [NodeKind::Heading, NodeKind::Block] {
            assert_eq!(NodeKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(NodeKind::parse("nope"), None);
    }

    #[test]
    fn mutual_links_are_marked_both_ways() {
        let mut g = Graph {
            nodes: vec![node("a", "x"), node("b", "y")],
            edges: vec![link(("a", "x"), ("b", "y")), link(("b", "y"), ("a", "x"))],
        };
        g.reciprocate();
        assert!(g.edges.iter().all(|e| e.reciprocated));
    }

    #[test]
    fn one_way_links_are_not_marked() {
        let mut g = Graph {
            nodes: vec![node("a", "x"), node("b", "y")],
            edges: vec![link(("a", "x"), ("b", "y"))],
        };
        g.reciprocate();
        assert!(!g.edges[0].reciprocated);
    }

    #[test]
    fn containment_never_counts_as_reciprocation() {
        let mut g = Graph {
            nodes: vec![node("a", "x"), node("a", "y")],
            edges: vec![
                Edge {
                    from: NodeId::new("a", "x"),
                    to: NodeId::new("a", "y"),
                    kind: EdgeKind::Contains,
                    line: 0,
                    reciprocated: false,
                },
                link(("a", "y"), ("a", "x")),
            ],
        };
        g.reciprocate();
        assert!(!g.edges[0].reciprocated, "containment is structural, not mutual");
        assert!(!g.edges[1].reciprocated, "no link edge points back");
    }

    #[test]
    fn sort_is_deterministic_and_dedupes() {
        let mut a = Graph {
            nodes: vec![node("b", "y"), node("a", "x")],
            edges: vec![link(("a", "x"), ("b", "y")), link(("a", "x"), ("b", "y"))],
        };
        let mut b = Graph {
            nodes: vec![node("a", "x"), node("b", "y")],
            edges: vec![link(("a", "x"), ("b", "y"))],
        };
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }
}
```
