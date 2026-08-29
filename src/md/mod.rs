//! Markdown parsing.
//!
//! DanKG implements a documented subset of CommonMark rather than the whole
//! spec; see architecture.org. Constructs outside the subset are preserved as
//! `Block::Passthrough` so that nothing in a source file is ever silently lost.
//!
//! Parsing runs in three passes: frontmatter, then block structure, then
//! inlines within each block that can contain them.

pub mod block;
pub mod fmt;
pub mod frontmatter;
pub mod inline;

pub use frontmatter::{Frontmatter, Value};

use crate::diag::Diags;

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub frontmatter: Frontmatter,
    pub blocks: Vec<Block>,
}

impl Document {
    pub fn parse(source: &str, diags: &mut Diags) -> Document {
        let source = normalize(source);
        let (frontmatter, rest, first_line) = frontmatter::split(&source, diags);
        let blocks = block::parse(rest, first_line, diags);
        Document { frontmatter, blocks }
    }

    /// Every heading in document order. The graph layer builds nodes from this.
    pub fn headings(&self) -> Vec<(u8, &[Inline], u32)> {
        let mut out = Vec::new();
        collect_headings(&self.blocks, &mut out);
        out
    }

    /// Every named code block in document order.
    pub fn named_blocks(&self) -> Vec<(&InfoString, &str, u32)> {
        let mut out = Vec::new();
        collect_blocks(&self.blocks, &mut out);
        out.retain(|(info, _, _)| info.name().is_some());
        out
    }
}

fn collect_headings<'a>(blocks: &'a [Block], out: &mut Vec<(u8, &'a [Inline], u32)>) {
    for b in blocks {
        match b {
            Block::Heading { level, inlines, line } => out.push((*level, inlines, *line)),
            Block::List(list) => {
                for item in &list.items {
                    collect_headings(&item.blocks, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_blocks<'a>(blocks: &'a [Block], out: &mut Vec<(&'a InfoString, &'a str, u32)>) {
    for b in blocks {
        match b {
            Block::Code { info, text, line, .. } => out.push((info, text.as_str(), *line)),
            Block::List(list) => {
                for item in &list.items {
                    collect_blocks(&item.blocks, out);
                }
            }
            _ => {}
        }
    }
}

/// Normalize line endings. CRLF and lone CR both become LF so that every
/// downstream byte offset and line number means the same thing on every
/// platform.
fn normalize(source: &str) -> String {
    if !source.contains('\r') {
        return source.to_string();
    }
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading { level: u8, inlines: Vec<Inline>, line: u32 },
    /// `fence` is the `` ` `` or `~` the author opened with. The *length* is
    /// not recorded: it is normalized to the shortest run that clears the
    /// body, so anything stored would only be thrown away again.
    Code { info: InfoString, text: String, fence: char, line: u32 },
    Paragraph { inlines: Vec<Inline>, line: u32 },
    List(List),
    ThematicBreak { line: u32 },
    /// A construct outside the implemented subset, kept verbatim.
    Passthrough { text: String, line: u32 },
}

impl Block {
    pub fn line(&self) -> u32 {
        match self {
            Block::Heading { line, .. }
            | Block::Code { line, .. }
            | Block::Paragraph { line, .. }
            | Block::ThematicBreak { line }
            | Block::Passthrough { line, .. } => *line,
            Block::List(l) => l.line,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub ordered: bool,
    pub start: u64,
    pub tight: bool,
    /// The bullet character for an unordered list, or the delimiter (`.` or
    /// `)`) for an ordered one. Authorial, and a change of it starts a new
    /// list, so the parser records it rather than the formatter guessing.
    pub marker: char,
    pub items: Vec<ListItem>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub blocks: Vec<Block>,
}

/// A fenced code block's info string.
///
/// The first word is the language; everything after it is `key=value` DanKG
/// metadata. Other markdown renderers ignore everything past the language, so
/// files carrying DanKG attributes stay portable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InfoString {
    pub lang: Option<String>,
    pub attrs: Vec<(String, String)>,
    /// Words the parser did not recognise. Warned about and ignored for every
    /// other purpose, but kept so that `dankg fmt` never deletes them.
    ///
    /// Between `lang`, `attrs` and this, the info string can be rebuilt without
    /// loss -- which is why the raw text is not also stored.
    pub unknown: Vec<String>,
}

/// Attribute keys DanKG understands. Anything else warns and is ignored.
pub const KNOWN_ATTRS: &[&str] = &["name", "deps", "timeout"];

impl InfoString {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn name(&self) -> Option<&str> {
        self.get("name")
    }

    /// Dependency names, in declaration order. Empty when `deps` is absent.
    pub fn deps(&self) -> Vec<&str> {
        match self.get("deps") {
            None => Vec::new(),
            Some(v) => v.split(',').map(str::trim).filter(|s| !s.is_empty()).collect(),
        }
    }

    pub fn timeout(&self) -> Option<u64> {
        self.get("timeout").and_then(|v| v.parse().ok())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    /// `delim` is the `*` or `_` the author wrote. Both render identically, so
    /// only the formatter cares -- but it cares enough that dropping it would
    /// rewrite every emphasis in a corpus on first run.
    Emph { delim: char, inner: Vec<Inline> },
    Strong { delim: char, inner: Vec<Inline> },
    Link { dest: String, title: Option<String>, text: Vec<Inline> },
    /// `[[target]]` or `[[target|label]]`. Not standard markdown; resolved
    /// against the root rather than as a path.
    WikiLink { target: String, label: Option<String> },
    SoftBreak,
    HardBreak,
}

impl Inline {
    /// Plain-text rendering, used for heading titles and slug generation.
    pub fn plain(inlines: &[Inline]) -> String {
        let mut s = String::new();
        Self::write_plain(inlines, &mut s);
        s
    }

    fn write_plain(inlines: &[Inline], out: &mut String) {
        for i in inlines {
            match i {
                Inline::Text(t) | Inline::Code(t) => out.push_str(t),
                Inline::Emph { inner, .. } | Inline::Strong { inner, .. } => {
                    Self::write_plain(inner, out)
                }
                Inline::Link { text, .. } => Self::write_plain(text, out),
                Inline::WikiLink { target, label } => {
                    out.push_str(label.as_deref().unwrap_or(target))
                }
                Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_crlf_and_lone_cr() {
        assert_eq!(normalize("a\r\nb\rc\nd"), "a\nb\nc\nd");
    }

    #[test]
    fn normalize_is_a_noop_without_cr() {
        assert_eq!(normalize("a\nb\n"), "a\nb\n");
    }

    #[test]
    fn info_string_deps_split_and_trim() {
        let info = InfoString {
            lang: Some("python".into()),
            attrs: vec![("deps".into(), "setup, fetch ,".into())],
            ..Default::default()
        };
        assert_eq!(info.deps(), vec!["setup", "fetch"]);
    }

    #[test]
    fn plain_text_flattens_markup() {
        let inlines = vec![
            Inline::Text("a ".into()),
            Inline::Strong { delim: '*', inner: vec![Inline::Text("b".into())] },
            Inline::Code(" c".into()),
        ];
        assert_eq!(Inline::plain(&inlines), "a b c");
    }
}
