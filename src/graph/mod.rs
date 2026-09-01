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
