# Graph query

Read-only lookups over an already-built `Graph`. A distinct concern from
`model`'s data shapes, `build`'s per-file construction, and `resolve`'s
link resolution: nothing here mutates a `Graph` or discovers a new one, it
only answers a question about one already assembled.

```rust name=module_doc path=graph/query.rs
//! Read-only lookups over an already-built `Graph`.

use super::build::relation_node;
use super::model::{EdgeKind, Graph, Node, NodeId, NodeKind};
```

`find_producer` is decision 35's own resolution rule, exactly as written:
*"Exactly one block may claim to have produced `NAME`. Two blocks
producing a same-named relation in different databases, or a relation a
later block drops and recreates, both leave more than one candidate, and
DanKG refuses rather than guess which one a reader meant."* A relation
name is looked up by its bare slug alone, across every `db:*` namespace at
once -- `xdeps=table:NAME` never names which database, on purpose, so the
search cannot narrow by one either.

```rust name=find_producer path=graph/query.rs
/// Resolves `relation_name` against `graph`'s own `Produces` edges,
/// searching every `Relation` node whose slug matches it regardless of
/// which `db:*` namespace it lives in. Exactly one producing block
/// resolves; zero or more than one refuses, decision 35's own rule.
pub fn find_producer(graph: &Graph, relation_name: &str) -> Result<NodeId, String> {
    let candidates: Vec<&NodeId> = graph
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Relation && n.id.slug == relation_name)
        .flat_map(|relation| {
            graph
                .edges
                .iter()
                .filter(move |e| e.kind == EdgeKind::Produces && e.to == relation.id)
                .map(|e| &e.from)
        })
        .collect();

    match candidates.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(format!("no block produces a relation named `{relation_name}`")),
        many => Err(format!(
            "`{relation_name}` is ambiguous: {} blocks produce a relation by that name ({})",
            many.len(),
            many.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ")
        )),
    }
}
```

`live_orphans` is decision 37's own filter: `dankg graph --live` spawns `db_name`'s configured `list` and hands every identifier it reported here. Anything `graph` already has a node for -- produced, read, or listed by an earlier database in the same run -- is already explained, so only a name with no node at all comes back. This never mutates `graph` itself: nothing from `--live` is cached or written back, so building the new nodes is the caller's decision, not this function's.

```rust name=live_orphans path=graph/query.rs
/// Relation names `db_name`'s configured `list` command reported that
/// `graph` has no node for at all -- a table created by hand, by a tool
/// `dankg` never touched, or documented once by a section since deleted.
/// Duplicates within `relations` collapse to one node, the same as two
/// files naming the same relation already do at build time.
pub fn live_orphans(graph: &Graph, db_name: &str, relations: &[String]) -> Vec<Node> {
    let db_ns = format!("db:{db_name}");
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for name in relations {
        let id = NodeId::new(&db_ns, name.as_str());
        if graph.contains(&id) || !seen.insert(id.clone()) {
            continue;
        }
        out.push(relation_node(&id, name));
    }
    out
}
```

## Tests

```rust name=tests path=graph/query.rs
#[cfg(test)]
mod tests {
    use super::*;
    use super::super::model::{Edge, Node};

    fn relation(db: &str, name: &str) -> Node {
        Node {
            id: NodeId::new(format!("db:{db}"), name),
            title: name.to_string(),
            file: format!("db:{db}"),
            line: 0,
            end_line: 0,
            level: 7,
            parent: None,
            tags: Vec::new(),
            external: Vec::new(),
            resolved: true,
            kind: NodeKind::Relation,
        }
    }

    fn produces(block: (&str, &str), relation: &NodeId) -> Edge {
        Edge {
            from: NodeId::new(block.0, block.1),
            to: relation.clone(),
            kind: EdgeKind::Produces,
            line: 0,
            reciprocated: false,
        }
    }

    #[test]
    fn exactly_one_producer_resolves() {
        let rel = relation("warehouse", "orders");
        let g = Graph { nodes: vec![rel.clone()], edges: vec![produces(("a", "setup"), &rel.id)] };
        assert_eq!(find_producer(&g, "orders").unwrap(), NodeId::new("a", "setup"));
    }

    #[test]
    fn no_producer_refuses() {
        let g = Graph { nodes: vec![relation("warehouse", "orders")], edges: vec![] };
        let err = find_producer(&g, "orders").unwrap_err();
        assert!(err.contains("no block produces"), "{err:?}");
    }

    #[test]
    fn an_unknown_relation_name_refuses_the_same_way() {
        let g = Graph { nodes: vec![], edges: vec![] };
        let err = find_producer(&g, "ghost").unwrap_err();
        assert!(err.contains("no block produces"), "{err:?}");
    }

    #[test]
    fn two_producers_in_different_databases_refuses_as_ambiguous() {
        let rel_a = relation("warehouse", "orders");
        let rel_b = relation("staging", "orders");
        let g = Graph {
            nodes: vec![rel_a.clone(), rel_b.clone()],
            edges: vec![produces(("a", "setup"), &rel_a.id), produces(("b", "other"), &rel_b.id)],
        };
        let err = find_producer(&g, "orders").unwrap_err();
        assert!(err.contains("ambiguous"), "{err:?}");
    }

    #[test]
    fn two_blocks_producing_the_same_relation_node_refuses_as_ambiguous() {
        // A relation a later block drops and recreates, decision 35's own
        // second example of the ambiguous case -- still just "more than
        // one candidate", not a different kind of error.
        let rel = relation("warehouse", "orders");
        let g = Graph {
            nodes: vec![rel.clone()],
            edges: vec![produces(("a", "setup"), &rel.id), produces(("a", "rebuild"), &rel.id)],
        };
        let err = find_producer(&g, "orders").unwrap_err();
        assert!(err.contains("ambiguous"), "{err:?}");
    }

    #[test]
    fn live_orphans_returns_a_node_for_a_name_the_graph_does_not_have() {
        let g = Graph { nodes: vec![], edges: vec![] };
        let orphans = live_orphans(&g, "warehouse", &["ghost_table".to_string()]);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].id, NodeId::new("db:warehouse", "ghost_table"));
        assert_eq!(orphans[0].kind, NodeKind::Relation);
        assert_eq!(orphans[0].title, "ghost_table");
    }

    #[test]
    fn live_orphans_skips_a_name_the_graph_already_explains() {
        let rel = relation("warehouse", "orders");
        let g = Graph { nodes: vec![rel.clone()], edges: vec![produces(("a", "setup"), &rel.id)] };
        assert!(live_orphans(&g, "warehouse", &["orders".to_string()]).is_empty());
    }

    #[test]
    fn live_orphans_dedupes_a_name_listed_twice() {
        let g = Graph { nodes: vec![], edges: vec![] };
        let orphans = live_orphans(&g, "warehouse", &["ghost_table".to_string(), "ghost_table".to_string()]);
        assert_eq!(orphans.len(), 1);
    }

    #[test]
    fn live_orphans_is_scoped_to_its_own_database() {
        // A relation this graph already explains in `staging` is still an
        // orphan when `--live` lists `warehouse`: a `db:NAME` namespace
        // belongs to one database (`NodeId`'s own doc comment), so the same
        // bare name in a different database is a different node.
        let rel = relation("staging", "orders");
        let g = Graph { nodes: vec![rel.clone()], edges: vec![produces(("a", "setup"), &rel.id)] };
        let orphans = live_orphans(&g, "warehouse", &["orders".to_string()]);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].id, NodeId::new("db:warehouse", "orders"));
    }
}
```
