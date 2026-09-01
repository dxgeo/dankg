# Graph mod

`dankg graph`'s eight pieces, plus the two re-exports (`Corpus`, and the
`Edge`/`EdgeKind`/`Graph`/`Node`/`NodeId`/`NodeKind` family) that let a
caller elsewhere in the crate write `graph::Corpus` instead of reaching
into `graph::index::Corpus` directly.

```rust name=graph_mod path=graph/mod.rs
pub mod build;
pub mod cache;
pub mod ignore;
pub mod index;
pub mod model;
pub mod resolve;
pub mod slug;
pub mod view;

pub use index::Corpus;
pub use model::{Edge, EdgeKind, Graph, Node, NodeId, NodeKind};
```

## Test helper

Shared, not duplicated: the layout and render test suites need a `Graph`
built straight from in-memory files exactly as much as the graph tests
themselves do, so the one function that does it -- parse each file, build
its nodes and containment/raw-link edges, then resolve the whole set --
lives here rather than in any one of them.

```rust name=graph_of path=graph/mod.rs
/// Parse and resolve a corpus held in memory. Test-only, and shared because
/// the layout and render tests need graphs as much as the graph tests do.
#[cfg(test)]
pub(crate) fn graph_of(files: &[(&str, &str)]) -> Graph {
    use crate::diag::Diags;
    use crate::md::Document;

    let mut diags = Diags::new("test-corpus");
    let parsed: Vec<_> = files
        .iter()
        .map(|(path, source)| {
            let mut file_diags = Diags::new(*path);
            let doc = Document::parse(source, &mut file_diags);
            build::build(path, &doc, source.lines().count() as u32)
        })
        .collect();
    resolve::resolve(&parsed, &mut diags)
}
```
