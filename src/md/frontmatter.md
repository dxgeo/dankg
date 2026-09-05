# Markdown frontmatter

A deliberately flat subset of YAML: `key: value` scalars, optionally
quoted, and inline bracket lists (`tags: [a, b]`). Nested mappings, block
sequences, anchors, aliases, and multiline scalars are all outside the
subset. Unsupported input warns with its line number and is skipped,
never guessed at. This matches the same "half-understood is worse than
refused" bar the rest of DanKG's own markdown subset holds
(`md/mod.md`'s `Block::Passthrough`).

```rust name=module_doc path=md/frontmatter.rs
//! Frontmatter: a deliberately flat subset of YAML.
//!
//! Supported: `key: value` scalars, optionally quoted, and inline bracket lists
//! (`tags: [a, b]`). Not supported: nested mappings, block sequences, anchors
//! and aliases, and multiline scalars. Unsupported input warns with its line
//! number and is skipped, never guessed at.

use crate::diag::Diags;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Scalar(String),
    List(Vec<String>),
}
```

`raw` is deliberately kept alongside the lossy `entries` view. Quoting,
comments, key order, and every unsupported line are all gone from
`entries` by the time it exists. `dankg fmt` re-emits `raw` verbatim
instead of trying to reconstruct any of that. This is the same principle
`Block::Passthrough` already applies to a construct outside the
block-level subset, extended down into frontmatter.

```rust name=frontmatter_struct path=md/frontmatter.rs
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Frontmatter {
    pub entries: Vec<(String, Value)>,
    /// The block exactly as written, fences included, empty when there is none.
    ///
    /// `entries` is a lossy view: quoting, comments, key order and every
    /// unsupported line are gone from it. `dankg fmt` re-emits this instead, on
    /// the same principle as `Block::Passthrough`. The formatter never
    /// rewrites a construct it does not fully model.
    pub raw: String,
}
```

`tangle_public` is decision 27's own frontmatter hint, read here and
consumed only through the tangle sidecar manifest a `glue` command reads.
DanKG's own code never branches on it directly. Anything other than
exactly `true` means private. This is the same conservative default
frontmatter already applies everywhere else in this module. A typo
should never silently widen a module's visibility.

```rust name=frontmatter_impl path=md/frontmatter.rs
impl Frontmatter {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn scalar(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Value::Scalar(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// List access that treats a lone scalar as a one-element list, since
    /// `tags: rust` and `tags: [rust]` mean the same thing to a reader.
    pub fn list(&self, key: &str) -> Vec<&str> {
        match self.get(key) {
            Some(Value::List(v)) => v.iter().map(String::as_str).collect(),
            Some(Value::Scalar(s)) => vec![s.as_str()],
            None => Vec::new(),
        }
    }

    pub fn title(&self) -> Option<&str> {
        self.scalar("title")
    }

    pub fn tags(&self) -> Vec<&str> {
        self.list("tags")
    }

    /// Alternate names this file answers to when resolving `[[wikilinks]]`.
    pub fn aliases(&self) -> Vec<&str> {
        let mut v = self.list("alias");
        v.extend(self.list("aliases"));
        v
    }

    /// `dankg.tangle.public` (decision 27): this file's tangled output
    /// should be declared publicly visible wherever a `[tangle.<lang>]
    /// glue` command's generated module declarations would otherwise
    /// default to private. This is per file, not per heading. Frontmatter
    /// has no finer scope than that. It is consumed only through the
    /// sidecar manifest a glue command reads. DanKG's own code never
    /// branches on it. Unset, or anything other than exactly `true`, means
    /// private. This is the same "half-understood is worse than refused"
    /// default frontmatter already applies elsewhere. A typo here should
    /// never silently make a module more visible than the author checked
    /// for.
    pub fn tangle_public(&self) -> bool {
        self.scalar("dankg.tangle.public") == Some("true")
    }
}
```

`split` is the only entry point block-level parsing ever calls
(`md/mod.md`'s `Document::parse`). Without a closing fence, what looked
like frontmatter was never frontmatter at all. A lone `---` is a
thematic break. The whole file stays content untouched. This matches
CommonMark rather than guessing at authorial intent.

```rust name=split path=md/frontmatter.rs
/// Split leading frontmatter off a document.
///
/// Returns the parsed frontmatter, the remaining content, and the 1-based line
/// number that content starts on, so block parsing reports true source lines.
pub fn split<'a>(source: &'a str, diags: &mut Diags) -> (Frontmatter, &'a str, u32) {
    let Some(rest) = strip_open_fence(source) else {
        return (Frontmatter::default(), source, 1);
    };

    // Find the closing fence. Without one this was never frontmatter. It is
    // a thematic break. The whole file is content.
    let mut offset = 0usize;
    let mut close: Option<(usize, u32)> = None;
    for (i, line) in rest.lines().enumerate() {
        let trimmed = line.trim_end();
        if trimmed == "---" || trimmed == "..." {
            close = Some((offset, i as u32 + 2));
            break;
        }
        offset += line.len() + 1;
    }

    let Some((body_end, close_line)) = close else {
        return (Frontmatter::default(), source, 1);
    };

    let body = &rest[..body_end];
    let mut fm = parse_body(body, diags);

    let after = rest[body_end..].find('\n').map(|n| body_end + n + 1).unwrap_or(rest.len());
    let content = &rest[after..];
    fm.raw = source[..source.len() - content.len()].to_string();
    (fm, content, close_line + 1)
}

/// Consume a `---` line at the very start of the document.
fn strip_open_fence(source: &str) -> Option<&str> {
    let end = source.find('\n').unwrap_or(source.len());
    if source[..end].trim_end() != "---" {
        return None;
    }
    Some(if end == source.len() { "" } else { &source[end + 1..] })
}
```

Each unsupported construct gets its own warning with its own
explanation. A reader who hits one should never have to guess which of
YAML's excluded features they wrote, only what to do instead.

```rust name=parse_body path=md/frontmatter.rs
fn parse_body(body: &str, diags: &mut Diags) -> Frontmatter {
    let mut fm = Frontmatter::default();

    for (i, raw) in body.lines().enumerate() {
        // Line 1 is the opening `---`. Body line 0 is document line 2.
        let line = i as u32 + 2;
        let trimmed = raw.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if raw.starts_with(' ') || raw.starts_with('\t') {
            diags.warn(line, "indented frontmatter (nested mapping or block sequence) is not supported, ignored");
            continue;
        }
        if trimmed.starts_with("- ") || trimmed == "-" {
            diags.warn(line, "block sequence in frontmatter is not supported, ignored");
            continue;
        }

        let Some(colon) = find_key_separator(trimmed) else {
            diags.warn(line, "frontmatter line is not a `key: value` pair, ignored");
            continue;
        };

        let key = trimmed[..colon].trim();
        if key.is_empty() {
            diags.warn(line, "frontmatter key is empty, ignored");
            continue;
        }

        let rest = trimmed[colon + 1..].trim();
        if rest.starts_with('|') || rest.starts_with('>') {
            diags.warn(line, "multiline scalar in frontmatter is not supported, ignored");
            continue;
        }
        if rest.starts_with('&') || rest.starts_with('*') {
            diags.warn(line, "anchors and aliases in frontmatter are not supported, ignored");
            continue;
        }
        if rest.starts_with('{') {
            diags.warn(line, "inline mapping in frontmatter is not supported, ignored");
            continue;
        }
        if rest.is_empty() {
            diags.warn(line, format!("frontmatter key `{key}` has no value (nested blocks are not supported), ignored"));
            continue;
        }

        let value = if rest.starts_with('[') {
            match parse_inline_list(rest) {
                Some(items) => Value::List(items),
                None => {
                    diags.warn(line, "unterminated list in frontmatter, ignored");
                    continue;
                }
            }
        } else {
            Value::Scalar(unquote(strip_trailing_comment(rest)))
        };

        if fm.entries.iter().any(|(k, _)| k == key) {
            diags.warn(line, format!("duplicate frontmatter key `{key}`, last value wins"));
            fm.entries.retain(|(k, _)| k != key);
        }
        fm.entries.push((key.to_string(), value));
    }

    fm
}
```

The rest are small, single-purpose helpers `parse_body` leans on: finding
the real key/value separator without being fooled by a colon inside a
quoted value, stripping a trailing comment without swallowing a hex color
like `#fff`, and unquoting a scalar or splitting an inline list on
commas that are themselves not inside quotes.

```rust name=helpers path=md/frontmatter.rs
/// Locate the `:` separating key from value, ignoring colons inside quotes.
fn find_key_separator(line: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"') | (None, '\'') => quote = Some(c),
            (None, ':') => return Some(i),
            _ => {}
        }
    }
    None
}

/// Strip a ` # comment` tail. YAML requires whitespace before the `#`. This
/// keeps values such as `color: #fff` intact.
fn strip_trailing_comment(value: &str) -> &str {
    if value.starts_with('"') || value.starts_with('\'') {
        return value;
    }
    let bytes = value.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'#' && i > 0 && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
            return value[..i].trim_end();
        }
    }
    value
}

fn parse_inline_list(value: &str) -> Option<Vec<String>> {
    let close = value.rfind(']')?;
    let inner = &value[1..close];
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    Some(split_top_level(inner).into_iter().map(|s| unquote(s.trim())).collect())
}

/// Split on commas that are not inside quotes.
fn split_top_level(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut quote: Option<char> = None;
    let mut start = 0usize;
    for (i, c) in inner.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"') | (None, '\'') => quote = Some(c),
            (None, ',') => {
                out.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out.into_iter().filter(|s| !s.trim().is_empty()).collect()
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}
```

## Tests

```rust name=tests path=md/frontmatter.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> (Frontmatter, String, u32, Diags) {
        let mut d = Diags::new("t.md");
        let (fm, rest, line) = split(src, &mut d);
        (fm, rest.to_string(), line, d)
    }

    #[test]
    fn parses_scalars_and_lists() {
        let (fm, rest, line, d) = parse("---\ntitle: DanKG Design\ntags: [rust, graphs]\n---\n# Body\n");
        assert_eq!(fm.title(), Some("DanKG Design"));
        assert_eq!(fm.tags(), vec!["rust", "graphs"]);
        assert_eq!(rest, "# Body\n");
        assert_eq!(line, 5);
        assert!(d.is_empty());
    }

    #[test]
    fn no_frontmatter_leaves_source_untouched() {
        let (fm, rest, line, d) = parse("# Just a heading\n");
        assert!(fm.is_empty());
        assert_eq!(rest, "# Just a heading\n");
        assert_eq!(line, 1);
        assert!(d.is_empty());
    }

    #[test]
    fn unterminated_fence_is_content_not_frontmatter() {
        // A lone `---` is a thematic break. Nothing is dropped. Nothing warns.
        let (fm, rest, _, d) = parse("---\nsome text\n");
        assert!(fm.is_empty());
        assert_eq!(rest, "---\nsome text\n");
        assert!(d.is_empty());
    }

    #[test]
    fn warns_on_nested_mapping() {
        let (fm, _, _, d) = parse("---\ntitle: ok\nnested:\n  a: 1\n---\n");
        assert_eq!(fm.title(), Some("ok"));
        assert_eq!(d.items().len(), 2, "one for the empty value, one for the indent");
        assert!(d.items()[1].message.contains("nested mapping"));
    }

    #[test]
    fn warns_on_block_sequence() {
        let (_, _, _, d) = parse("---\ntags:\n- rust\n---\n");
        assert!(d.items().iter().any(|x| x.message.contains("block sequence")));
    }

    #[test]
    fn warns_on_multiline_scalar() {
        let (_, _, _, d) = parse("---\nbody: |\n---\n");
        assert!(d.items()[0].message.contains("multiline scalar"));
        assert_eq!(d.items()[0].line, 2);
    }

    #[test]
    fn scalar_treated_as_single_item_list() {
        let (fm, _, _, _) = parse("---\ntags: rust\n---\n");
        assert_eq!(fm.tags(), vec!["rust"]);
    }

    #[test]
    fn quotes_and_colons_survive() {
        let (fm, _, _, _) = parse("---\ntitle: \"Graphs: A Study\"\n---\n");
        assert_eq!(fm.title(), Some("Graphs: A Study"));
    }

    #[test]
    fn strips_comment_but_not_hex_colour() {
        let (fm, _, _, _) = parse("---\na: 1 # note\nb: #fff\n---\n");
        assert_eq!(fm.scalar("a"), Some("1"));
        assert_eq!(fm.scalar("b"), Some("#fff"));
    }

    #[test]
    fn duplicate_key_warns_and_last_wins() {
        let (fm, _, _, d) = parse("---\na: 1\na: 2\n---\n");
        assert_eq!(fm.scalar("a"), Some("2"));
        assert!(d.items()[0].message.contains("duplicate"));
    }

    #[test]
    fn aliases_merge_both_spellings() {
        let (fm, _, _, _) = parse("---\nalias: one\naliases: [two, three]\n---\n");
        assert_eq!(fm.aliases(), vec!["one", "two", "three"]);
    }

    #[test]
    fn tangle_public_defaults_to_false() {
        let (fm, _, _, _) = parse("# No frontmatter\n");
        assert!(!fm.tangle_public());
    }

    #[test]
    fn tangle_public_reads_the_dotted_key() {
        let (fm, _, _, _) = parse("---\ndankg.tangle.public: true\n---\n");
        assert!(fm.tangle_public());
    }

    #[test]
    fn tangle_public_rejects_anything_but_exactly_true() {
        let (fm, _, _, _) = parse("---\ndankg.tangle.public: yes\n---\n");
        assert!(!fm.tangle_public(), "a typo should never silently widen visibility");
    }
}
```
