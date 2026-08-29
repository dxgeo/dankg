//! Turning parsed documents into nodes and containment edges.
//!
//! Link edges are not resolved here: a link's target may live in a file that
//! has not been parsed yet, so building records raw targets and
//! [`super::resolve`] turns them into edges once the whole corpus is known.

use super::model::{Edge, EdgeKind, Node, NodeId};
use super::slug::Slugger;
use crate::md::{Block, Document, Inline};

/// A link as written, before its target is known to exist.
#[derive(Debug, Clone, PartialEq)]
pub struct RawLink {
    pub from: NodeId,
    pub target: Target,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// `[text](dest)` -- a path, resolved relative to the linking file.
    Path(String),
    /// `[[target]]` -- resolved by searching the corpus.
    Wiki(String),
}

#[derive(Debug, Clone)]
pub struct ParsedFile {
    /// Path as it appears on disk.
    pub path: String,
    /// Path with its extension removed; the `file` half of every `NodeId`.
    pub key: String,
    pub nodes: Vec<Node>,
    pub containment: Vec<Edge>,
    pub links: Vec<RawLink>,
    /// Alternate names this file answers to when resolving wikilinks.
    pub aliases: Vec<String>,
}

impl ParsedFile {
    /// The node `[text](file.md)` with no fragment should land on.
    pub fn entry_node(&self) -> Option<&Node> {
        self.nodes.first()
    }
}

pub fn build(path: &str, doc: &Document, line_count: u32) -> ParsedFile {
    let key = strip_extension(path);
    let tags: Vec<String> = doc.frontmatter.tags().into_iter().map(str::to_string).collect();
    let aliases = doc.frontmatter.aliases().into_iter().map(str::to_string).collect();

    let mut slugger = Slugger::new();
    let mut nodes: Vec<Node> = Vec::new();
    let mut containment: Vec<Edge> = Vec::new();
    let mut links: Vec<RawLink> = Vec::new();
    // Heading levels seen so far, as (level, id), innermost last.
    let mut stack: Vec<(u8, NodeId)> = Vec::new();
    let mut current: Option<NodeId> = None;

    let mut visit = |block: &Block,
                     nodes: &mut Vec<Node>,
                     containment: &mut Vec<Edge>,
                     links: &mut Vec<RawLink>,
                     stack: &mut Vec<(u8, NodeId)>,
                     current: &mut Option<NodeId>| {
        match block {
            Block::Heading { level, inlines, line } => {
                let title = Inline::plain(inlines).trim().to_string();
                let id = NodeId::new(&key, slugger.assign(&title));

                while stack.last().is_some_and(|(l, _)| *l >= *level) {
                    stack.pop();
                }
                let parent = stack.last().map(|(_, id)| id.clone());
                if let Some(parent) = &parent {
                    containment.push(Edge {
                        from: parent.clone(),
                        to: id.clone(),
                        kind: EdgeKind::Contains,
                        line: 0,
                        reciprocated: false,
                    });
                }

                nodes.push(Node {
                    id: id.clone(),
                    title,
                    file: path.to_string(),
                    line: *line,
                    end_line: *line,
                    level: *level,
                    parent,
                    tags: tags.clone(),
                    external: Vec::new(),
                    resolved: true,
                });

                // A link written in a heading is still a link, and a heading is
                // always one line, so no cursor is needed.
                let mut cursor = *line;
                collect_links(inlines, &id, &mut cursor, links, nodes);

                stack.push((*level, id.clone()));
                *current = Some(id);
            }
            Block::Paragraph { inlines, line } => {
                let owner = match current.clone() {
                    Some(id) => id,
                    // Content before the first heading belongs to the file, not
                    // to a heading that happens to come later.
                    None => {
                        let id = file_node(&key, path, doc, nodes, &tags);
                        *current = Some(id.clone());
                        stack.push((0, id.clone()));
                        id
                    }
                };
                // Line breaks advance the cursor, so a link on the third line
                // of a paragraph reports that line rather than the paragraph's.
                let mut cursor = *line;
                collect_links(inlines, &owner, &mut cursor, links, nodes);
            }
            _ => {}
        }
    };

    for block in flatten(&doc.blocks) {
        visit(block, &mut nodes, &mut containment, &mut links, &mut stack, &mut current);
    }

    if nodes.is_empty() {
        // A file with no headings and no links still needs an address.
        file_node(&key, path, doc, &mut nodes, &tags);
    }

    set_extents(&mut nodes, line_count);

    ParsedFile { path: path.to_string(), key, nodes, containment, links, aliases }
}

/// Create the synthetic file-level node, titled from frontmatter or the file
/// name. Returns an existing one rather than duplicating it.
fn file_node(
    key: &str,
    path: &str,
    doc: &Document,
    nodes: &mut Vec<Node>,
    tags: &[String],
) -> NodeId {
    if let Some(existing) = nodes.first() {
        if existing.level == 0 {
            return existing.id.clone();
        }
    }

    let title = doc
        .frontmatter
        .title()
        .map(str::to_string)
        .unwrap_or_else(|| file_stem(key).to_string());
    let id = NodeId::new(key, super::slug::slugify(&title));

    let node = Node {
        id: id.clone(),
        title,
        file: path.to_string(),
        line: 1,
        end_line: 1,
        level: 0,
        parent: None,
        tags: tags.to_vec(),
        external: Vec::new(),
        resolved: true,
    };
    nodes.insert(0, node);
    id
}

/// A heading owns every line up to the next heading of the same or higher
/// level.
fn set_extents(nodes: &mut [Node], line_count: u32) {
    for i in 0..nodes.len() {
        let level = nodes[i].level;
        let end = nodes[i + 1..]
            .iter()
            .find(|n| n.level <= level)
            .map(|n| n.line.saturating_sub(1))
            .unwrap_or(line_count);
        nodes[i].end_line = end.max(nodes[i].line);
    }
}

/// Every block in document order, descending into list items.
fn flatten(blocks: &[Block]) -> Vec<&Block> {
    let mut out = Vec::new();
    push_flat(blocks, &mut out);
    out
}

fn push_flat<'a>(blocks: &'a [Block], out: &mut Vec<&'a Block>) {
    for b in blocks {
        out.push(b);
        if let Block::List(list) = b {
            for item in &list.items {
                push_flat(&item.blocks, out);
            }
        }
    }
}

fn collect_links(
    inlines: &[Inline],
    owner: &NodeId,
    line: &mut u32,
    links: &mut Vec<RawLink>,
    nodes: &mut [Node],
) {
    for inline in inlines {
        match inline {
            Inline::Link { dest, text, .. } => {
                if is_external(dest) {
                    if let Some(node) = nodes.iter_mut().find(|n| &n.id == owner) {
                        if !node.external.contains(dest) {
                            node.external.push(dest.clone());
                        }
                    }
                } else if !dest.is_empty() {
                    links.push(RawLink {
                        from: owner.clone(),
                        target: Target::Path(dest.clone()),
                        line: *line,
                    });
                }
                collect_links(text, owner, line, links, nodes);
            }
            Inline::WikiLink { target, .. } => links.push(RawLink {
                from: owner.clone(),
                target: Target::Wiki(target.clone()),
                line: *line,
            }),
            Inline::Emph { inner: children, .. } | Inline::Strong { inner: children, .. } => {
                collect_links(children, owner, line, links, nodes)
            }
            // The parser joins a paragraph's lines with break markers, so each
            // one is exactly one source line.
            Inline::SoftBreak | Inline::HardBreak => *line += 1,
            _ => {}
        }
    }
}

/// Absolute URLs and mail links are recorded on the node but never graphed.
fn is_external(dest: &str) -> bool {
    if dest.starts_with("mailto:") || dest.starts_with("//") {
        return true;
    }
    match dest.find("://") {
        Some(i) => dest[..i].chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-'),
        None => false,
    }
}

pub fn strip_extension(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    match normalized.rfind('.') {
        Some(dot) if !normalized[dot..].contains('/') => normalized[..dot].to_string(),
        _ => normalized,
    }
}

pub fn file_stem(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Diags;

    fn parse(src: &str) -> Document {
        let mut d = Diags::new("t.md");
        Document::parse(src, &mut d)
    }

    fn build_src(path: &str, src: &str) -> ParsedFile {
        let doc = parse(src);
        let lines = src.lines().count() as u32;
        build(path, &doc, lines)
    }

    #[test]
    fn headings_become_nodes_with_slugs() {
        let f = build_src("notes/project.md", "# Overview\n\n## Key Features\n");
        assert_eq!(f.key, "notes/project");
        let ids: Vec<String> = f.nodes.iter().map(|n| n.id.to_string()).collect();
        assert_eq!(ids, vec!["notes/project#overview", "notes/project#key-features"]);
    }

    #[test]
    fn nesting_produces_containment_edges() {
        let f = build_src("a.md", "# One\n\n## Two\n\n### Three\n\n## Four\n");
        let pairs: Vec<(String, String)> = f
            .containment
            .iter()
            .map(|e| (e.from.slug.clone(), e.to.slug.clone()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("one".to_string(), "two".to_string()),
                ("two".to_string(), "three".to_string()),
                ("one".to_string(), "four".to_string()),
            ]
        );
    }

    #[test]
    fn extents_run_to_the_next_sibling_or_end() {
        let f = build_src("a.md", "# One\n\ntext\n\n## Two\n\nmore\n");
        assert_eq!((f.nodes[0].line, f.nodes[0].end_line), (1, 7));
        assert_eq!((f.nodes[1].line, f.nodes[1].end_line), (5, 7));
    }

    #[test]
    fn link_lines_advance_across_a_multiline_paragraph() {
        let f = build_src("a.md", "# One\n\nfirst [x](b.md)\nsecond [y](c.md)\nthird [z](d.md)\n");
        let lines: Vec<u32> = f.links.iter().map(|l| l.line).collect();
        assert_eq!(lines, vec![3, 4, 5]);
    }

    #[test]
    fn links_written_in_a_heading_are_collected() {
        let f = build_src("a.md", "# See [x](b.md)\n");
        assert_eq!(f.links.len(), 1);
        assert_eq!(f.links[0].from.slug, "see-x");
        assert_eq!(f.links[0].line, 1);
    }

    #[test]
    fn links_attach_to_the_enclosing_heading() {
        let f = build_src("a.md", "# One\n\nsee [x](b.md#y)\n\n# Two\n\nsee [[z]]\n");
        assert_eq!(f.links.len(), 2);
        assert_eq!(f.links[0].from.slug, "one");
        assert_eq!(f.links[0].target, Target::Path("b.md#y".into()));
        assert_eq!(f.links[0].line, 3);
        assert_eq!(f.links[1].from.slug, "two");
        assert_eq!(f.links[1].target, Target::Wiki("z".into()));
    }

    #[test]
    fn links_inside_emphasis_and_lists_are_found() {
        let f = build_src("a.md", "# One\n\n- **[x](b.md)**\n");
        assert_eq!(f.links.len(), 1);
        assert_eq!(f.links[0].target, Target::Path("b.md".into()));
    }

    #[test]
    fn external_urls_are_recorded_not_linked() {
        let f = build_src("a.md", "# One\n\n[site](https://example.com) [m](mailto:a@b.c)\n");
        assert!(f.links.is_empty());
        assert_eq!(
            f.nodes[0].external,
            vec!["https://example.com".to_string(), "mailto:a@b.c".to_string()]
        );
    }

    #[test]
    fn file_without_headings_gets_a_file_level_node() {
        let f = build_src("notes/ideas.md", "just text\n");
        assert_eq!(f.nodes.len(), 1);
        assert_eq!(f.nodes[0].level, 0);
        assert_eq!(f.nodes[0].id.to_string(), "notes/ideas#ideas");
    }

    #[test]
    fn frontmatter_title_names_the_file_level_node() {
        let f = build_src("a.md", "---\ntitle: My Notes\n---\ntext\n");
        assert_eq!(f.nodes[0].title, "My Notes");
        assert_eq!(f.nodes[0].id.slug, "my-notes");
    }

    #[test]
    fn preheading_links_belong_to_the_file_not_a_later_heading() {
        let f = build_src("a.md", "see [x](b.md)\n\n# Later\n");
        assert_eq!(f.links[0].from.slug, "a", "owner is the file node");
        assert_eq!(f.nodes[0].level, 0);
        assert_eq!(f.nodes[1].title, "Later");
    }

    #[test]
    fn tags_are_carried_onto_every_node() {
        let f = build_src("a.md", "---\ntags: [rust]\n---\n# One\n\n## Two\n");
        assert!(f.nodes.iter().all(|n| n.tags == vec!["rust".to_string()]));
    }

    #[test]
    fn duplicate_headings_get_distinct_slugs() {
        let f = build_src("a.md", "# Notes\n\n# Notes\n");
        assert_eq!(f.nodes[0].id.slug, "notes");
        assert_eq!(f.nodes[1].id.slug, "notes-1");
    }
}
