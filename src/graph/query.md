# Graph query

Read-only lookups over an already-built `Graph`. A distinct concern from
`model`'s data shapes, `build`'s per-file construction, and `resolve`'s
link resolution: nothing here mutates a `Graph` or discovers a new one, it
only answers a question about one already assembled.

```rust name=module_doc path=graph/query.rs
//! Read-only lookups over an already-built `Graph`.

use super::model::{EdgeKind, Graph, NodeId, NodeKind};
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
}
```
