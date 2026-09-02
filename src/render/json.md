# Render json

`--format json` is the one output every other consumer of a DanKG graph
goes through. Even `dankg` itself goes through it: the graph logic's own
test suite renders through this module and asserts on the result, rather
than probing `Graph` internals directly. So a shape change here is a shape
change every test sees.

The output is pretty-printed, not minified, because it is meant to be
committed and diffed. It is hand-rolled, not pulled from a JSON crate.
[Decision 1](../../architecture.md#decision-1-dependency-policy) rules
that out. The shape needed here is also small enough to keep fully under
this crate's own control.

```rust name=module_doc path=render/json.rs
//! Canonical JSON graph dump.
//!
//! This is the surface that makes DanKG scriptable. It is also the one the
//! graph logic is tested through. It is pretty-printed rather than minified
//! because the output is meant to be committed and diffed.
//!
//! It is written by hand. DanKG takes no crates. The shape here is small and
//! fully under our control.

use crate::graph::{Graph, Node};
use std::fmt::Write as _;

/// Bumped whenever the shape changes in a way a consumer would notice.
/// 2: a node now carries `kind` (`"heading"` or `"block"`). A named
/// top-level code block is now its own node, not invisible content inside
/// its heading's line range.
pub const SCHEMA_VERSION: u32 = 2;
```

```rust name=render path=render/json.rs
pub fn render(graph: &Graph) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    let _ = writeln!(out, "  \"version\": {SCHEMA_VERSION},");

    out.push_str("  \"nodes\": [\n");
    for (i, node) in graph.nodes.iter().enumerate() {
        node_json(node, &mut out);
        out.push_str(if i + 1 < graph.nodes.len() { ",\n" } else { "\n" });
    }
    out.push_str("  ],\n");

    out.push_str("  \"edges\": [\n");
    for (i, edge) in graph.edges.iter().enumerate() {
        let _ = write!(
            out,
            "    {{\"from\": {}, \"to\": {}, \"kind\": {}, \"line\": {}, \"reciprocated\": {}}}",
            string(&edge.from.to_string()),
            string(&edge.to.to_string()),
            string(edge.kind.as_str()),
            edge.line,
            edge.reciprocated
        );
        out.push_str(if i + 1 < graph.edges.len() { ",\n" } else { "\n" });
    }
    out.push_str("  ]\n}\n");
    out
}

fn node_json(node: &Node, out: &mut String) {
    let _ = write!(
        out,
        "    {{\"id\": {}, \"title\": {}, \"file\": {}, \"line\": {}, \"end_line\": {}, \
         \"level\": {}, \"parent\": {}, \"tags\": {}, \"external\": {}, \"resolved\": {}, \"kind\": {}}}",
        string(&node.id.to_string()),
        string(&node.title),
        string(&node.file),
        node.line,
        node.end_line,
        node.level,
        match &node.parent {
            Some(p) => string(&p.to_string()),
            None => "null".to_string(),
        },
        array(&node.tags),
        array(&node.external),
        node.resolved,
        string(node.kind.as_str())
    );
}

fn array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&string(item));
    }
    out.push(']');
    out
}
```

`string` is the one escaper every hand-rolled JSON writer in this crate
shares, including `tangle`'s own sidecar manifest. That avoids each one
writing its own escaper and risking the two disagreeing on what needs
escaping.

```rust name=string_escaper path=render/json.rs
/// A JSON string literal, with the escapes the spec requires. `pub(crate)`
/// so `tangle`'s sidecar manifest (also hand-rolled JSON, also small and
/// fully under our control) has one escaper to agree with instead of two.
pub(crate) fn string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
```

## Tests

```rust name=tests path=render/json.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, EdgeKind, NodeId, NodeKind};

    fn sample() -> Graph {
        Graph {
            nodes: vec![Node {
                id: NodeId::new("a", "one"),
                title: "One \"quoted\"".into(),
                file: "a.md".into(),
                line: 1,
                end_line: 3,
                level: 1,
                parent: None,
                tags: vec!["rust".into()],
                external: vec![],
                resolved: true,
                kind: NodeKind::Heading,
            }],
            edges: vec![Edge {
                from: NodeId::new("a", "one"),
                to: NodeId::new("b", "two"),
                kind: EdgeKind::Link,
                line: 3,
                reciprocated: true,
            }],
        }
    }

    #[test]
    fn escapes_quotes_and_controls() {
        assert_eq!(string("a\"b"), "\"a\\\"b\"");
        assert_eq!(string("a\nb"), "\"a\\nb\"");
        assert_eq!(string("\u{1}"), "\"\\u0001\"");
    }

    #[test]
    fn renders_expected_shape() {
        let out = render(&sample());
        assert!(out.contains("\"version\": 2"));
        assert!(out.contains("\"id\": \"a#one\""));
        assert!(out.contains("\"title\": \"One \\\"quoted\\\"\""));
        assert!(out.contains("\"parent\": null"));
        assert!(out.contains("\"tags\": [\"rust\"]"));
        assert!(out.contains("\"kind\": \"heading\""), "the node's own kind: {out:?}");
        assert!(out.contains("\"kind\": \"link\""), "the edge's kind: {out:?}");
        assert!(out.contains("\"reciprocated\": true"));
        assert!(out.ends_with("]\n}\n"));
    }

    #[test]
    fn empty_graph_is_still_valid_json() {
        let out = render(&Graph::default());
        assert_eq!(out, "{\n  \"version\": 2,\n  \"nodes\": [\n  ],\n  \"edges\": [\n  ]\n}\n");
    }
}
```
