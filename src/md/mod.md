# Markdown

DanKG implements a documented subset of CommonMark, not the whole spec
(architecture.md, *Markdown subset*). A construct outside that subset is
never silently dropped. It survives as `Block::Passthrough`, verbatim. A
file DanKG cannot fully understand is still a file DanKG never corrupts.
Parsing runs in three passes: frontmatter first, then block structure,
then inlines within whichever blocks can contain them. That's
`md/frontmatter.rs`, `md/block.rs`, and `md/inline.rs`, in that order,
every time. None of the three is yet converted to its own literate
source.

```rust name=module_doc path=md/mod.rs
//! Markdown parsing.
//!
//! DanKG implements a documented subset of CommonMark rather than the whole
//! spec. See architecture.md. Constructs outside the subset are preserved as
//! `Block::Passthrough`. Nothing in a source file is ever silently lost.
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
```

Both walks descend into list items. A heading or a named block can sit
inside a list the same way it can sit anywhere else in the document tree.
This is exactly why `tangle.rs` does *not* reuse `named_blocks` for its
own selection. `plan::top_level_blocks` walks only the top level on
purpose. A block nested in a list is data, not a module.

```rust name=collect_and_normalize path=md/mod.rs
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

/// Normalize line endings. CRLF and lone CR both become LF. This way,
/// every downstream byte offset and line number means the same thing on
/// every platform.
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
```

`Block::Code`'s own `end_line` exists purely for `eval`'s write-back. It
needs to know exactly where a block ends in the *source* without
re-deriving fence-matching a second time. This is also why the fence
character is kept but its length is not. `dankg fmt` always normalizes
to the shortest run that clears the body. A stored length would only
ever be thrown away again.

```rust name=block_enum path=md/mod.rs
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading { level: u8, inlines: Vec<Inline>, line: u32 },
    /// `fence` is the `` ` `` or `~` the author opened with. The *length* is
    /// not recorded. It is normalized to the shortest run that clears the
    /// body. Anything stored would only be thrown away again. `end_line`
    /// is the closing fence's own line, or the last line of the file when
    /// the parser never found a close for one. `eval`'s write-back needs to
    /// know exactly where a block ends in the source without re-deriving
    /// fence-matching a second time.
    Code { info: InfoString, text: String, fence: char, line: u32, end_line: u32 },
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
    /// `)`) for an ordered one. This is authorial. A change of it starts a
    /// new list. The parser records it rather than the formatter guessing.
    pub marker: char,
    pub items: Vec<ListItem>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub blocks: Vec<Block>,
}
```

`InfoString` has three fields: `lang`, the recognised `attrs`, and
`unknown`. They are chosen so the original text can always be rebuilt
without loss. This is exactly why the raw info string itself is never
separately stored. It would just be a fourth, redundant copy of what
these three already reconstruct.

```rust name=info_string path=md/mod.rs
/// A fenced code block's info string.
///
/// The first word is the language. Everything after it is `key=value` DanKG
/// metadata. Other markdown renderers ignore everything past the language.
/// This way, files carrying DanKG attributes stay portable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InfoString {
    pub lang: Option<String>,
    pub attrs: Vec<(String, String)>,
    /// Words the parser did not recognise. Warned about and ignored for every
    /// other purpose. Kept anyway. `dankg fmt` must never delete them.
    ///
    /// Between `lang`, `attrs`, and this, the info string can be rebuilt
    /// without loss. This is why the raw text is not also stored.
    pub unknown: Vec<String>,
}

/// Attribute keys DanKG understands. Anything else warns and is ignored.
pub const KNOWN_ATTRS: &[&str] = &["name", "deps", "xdeps", "produces", "reads", "timeout", "path"];

impl InfoString {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn name(&self) -> Option<&str> {
        self.get("name")
    }

    /// `tangle`'s escape hatch: an explicit output path overriding the
    /// heading-derived default, for a block (a `Cargo.toml`, a
    /// `pyproject.toml`) that does not belong under any heading's module.
    pub fn path(&self) -> Option<&str> {
        self.get("path")
    }

    /// Dependency names, in declaration order. Empty when `deps` is absent.
    pub fn deps(&self) -> Vec<&str> {
        match self.get("deps") {
            None => Vec::new(),
            Some(v) => v.split(',').map(str::trim).filter(|s| !s.is_empty()).collect(),
        }
    }

    /// Cross-language dependency names, in declaration order. Resolved the
    /// same way `deps=` is, but never concatenated into a chain: the
    /// block always runs alone, and the target's own last recorded hash
    /// is folded in by reference instead (`eval::result::xdep_hashes`).
    /// Empty when `xdeps` is absent.
    pub fn xdeps(&self) -> Vec<&str> {
        match self.get("xdeps") {
            None => Vec::new(),
            Some(v) => v.split(',').map(str::trim).filter(|s| !s.is_empty()).collect(),
        }
    }

    pub fn timeout(&self) -> Option<u64> {
        self.get("timeout").and_then(|v| v.parse().ok())
    }

    /// The artifact this block writes (`produces=file:PATH`, decision 33),
    /// as the raw `kind:value` string. Interpreting the `file:` prefix is
    /// `eval::plan::check_file_deps`'s job, not this accessor's: the info
    /// string only ever hands back what was written, the same way `deps()`
    /// hands back names rather than resolving them.
    pub fn produces(&self) -> Option<&str> {
        self.get("produces")
    }

    /// The artifact this block reads (`reads=file:PATH`, decision 33).
    /// Checked against a `deps=`/`xdeps=` target's own `produces=`, never
    /// resolved on its own.
    pub fn reads(&self) -> Option<&str> {
        self.get("reads")
    }
}
```

`Inline::plain` is the one place emphasis, links, and wikilinks all
collapse to bare text. Heading titles and slug generation both need
"what does this heading actually say," not its markup. This is the
single function both go through. That way they never disagree.

```rust name=inline_enum path=md/mod.rs
#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    /// `delim` is the `*` or `_` the author wrote. Both render identically.
    /// Only the formatter cares. It cares enough that dropping it would
    /// rewrite every emphasis in a corpus on first run.
    Emph { delim: char, inner: Vec<Inline> },
    Strong { delim: char, inner: Vec<Inline> },
    Link { dest: String, title: Option<String>, text: Vec<Inline> },
    /// `[[target]]` or `[[target|label]]`. Not standard markdown. Resolved
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
```

## Tests

```rust name=tests path=md/mod.rs
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
    fn info_string_xdeps_split_and_trim() {
        let info = InfoString {
            lang: Some("python".into()),
            attrs: vec![("xdeps".into(), "raw.md#load , clean.md#norm".into())],
            ..Default::default()
        };
        assert_eq!(info.xdeps(), vec!["raw.md#load", "clean.md#norm"]);
    }

    #[test]
    fn xdeps_is_empty_when_absent() {
        let info = InfoString { lang: Some("sh".into()), ..Default::default() };
        assert!(info.xdeps().is_empty());
    }

    #[test]
    fn info_string_produces_and_reads_are_read_raw() {
        let info = InfoString {
            lang: Some("sh".into()),
            attrs: vec![("produces".into(), "file:out.csv".into()), ("reads".into(), "file:in.csv".into())],
            ..Default::default()
        };
        assert_eq!(info.produces(), Some("file:out.csv"));
        assert_eq!(info.reads(), Some("file:in.csv"));
    }

    #[test]
    fn produces_and_reads_are_none_when_absent() {
        let info = InfoString { lang: Some("sh".into()), ..Default::default() };
        assert_eq!(info.produces(), None);
        assert_eq!(info.reads(), None);
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
```
