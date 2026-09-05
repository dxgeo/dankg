# Depends

A code block's staleness has a clean signal: re-hash the source, compare
it to what last ran. Prose has no such signal. Two sections of prose can
say the same words and mean different things, or say different words and
mean the same thing. Hashing a whole section would flag a typo fix as
loudly as a meaning change. Hashing nothing would flag nothing at all.

`<!-- dankg:depends target=... quote="..." -->` picks a narrower claim
instead: the author names the exact sentence they are relying on, not the
whole section it lives in. `dankg check` only asks whether that sentence
is still there. This is a substring search over whitespace-normalized
text, not a hash, so a reflowed paragraph or a fixed typo elsewhere in
the section changes nothing. Only an edit that actually touches the
pinned words does.

This is advisory. See `check`'s own wiring (`main.md`) for why it never
fails the exit code the way an unresolved link or a stale eval result
does: a substring match is a much weaker signal than a source hash, and a
mechanism that fails the build on a weak signal trains a reader to
silence it rather than read it.

```rust name=module_doc path=depends.rs
//! `<!-- dankg:depends target=... quote="..." -->`: a prose claim pinned
//! against another section, checked by substring rather than by hash.
//!
//! A code block's staleness has a clean signal: re-hash the source,
//! compare it to what last ran. Prose has none. Two sections can say the
//! same words and mean different things, or different words and mean the
//! same thing. Hashing a whole section would flag a typo fix as loudly as
//! a meaning change. This module hashes nothing. It looks for the exact
//! sentence the marker names, in the target section's current text,
//! after whitespace normalization. `dankg check` reports a miss as
//! advisory, never as a build failure: see its own wiring for why.

use crate::graph::build::strip_extension;
use crate::graph::resolve::{dir_of, join_normalize};
use crate::graph::slug::slugify;
use crate::graph::NodeId;
use crate::md::{Block, Document};
```

## The marker

`target` and `quote` are unordered `key=value` words, exactly the
convention `eval::result`'s own `<!-- dankg:result ... -->` marker
already uses. `target` never contains a space, so it is written bare.
`quote` almost always does, so it is written quoted. `cmd::split` already
knows how to tell those two shapes apart -- it is shared here rather than
given a second, narrower implementation, the same reasoning `resolve.rs`
shares `join_normalize` with `eval::plan`'s cross-file `deps=`.

```rust name=marker path=depends.rs
/// One `dankg:depends` marker, as written. `line` is the marker's own
/// 1-indexed source line, for a diagnostic to point at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependsMarker {
    /// `other.md#heading`, or `#heading` for the declaring file itself --
    /// the same shape a written link's own target already has.
    pub target: String,
    /// The exact claim this marker pins. Checked as a substring of the
    /// target section's whitespace-normalized text, never as a hash.
    pub quote: String,
    pub line: u32,
}

/// The inverse of a hand-written marker. Tolerant of `key=value` order
/// and of `cmd::split`'s own quoting rules, but not of a comment that
/// merely happens to start the same way -- both `target` and `quote`
/// must be present, matching how `eval::result::parse_marker` treats its
/// own two required keys.
pub fn parse_marker(line: &str) -> Option<(String, String)> {
    let inner = line.trim().strip_prefix("<!--")?.strip_suffix("-->")?.trim();
    let inner = inner.strip_prefix("dankg:depends")?.trim();

    let mut target = None;
    let mut quote = None;
    for word in crate::cmd::split(inner) {
        let (k, v) = word.split_once('=')?;
        match k {
            "target" => target = Some(v.to_string()),
            "quote" => quote = Some(v.to_string()),
            _ => {}
        }
    }
    Some((target?, quote?))
}
```

## Finding markers in a document

A marker is never write-back generated the way an eval result marker is.
Nothing here ever produces one, so there is no anchor to look "right
after," the way `eval::result::locate_existing` looks right after a code
block. Instead every `Passthrough` block in the document is scanned, one
line at a time, for a line that parses as a marker. A marker mixed into a
larger passthrough chunk (an HTML comment with no blank line separating
it from surrounding prose) is still found, unlike `eval::result`'s own
lone-block restriction: that restriction exists to avoid clobbering
neighbouring text on write-back, and this module never writes anything
back at all.

```rust name=markers_in path=depends.rs
/// Every `dankg:depends` marker in `doc`, in document order.
pub fn markers_in(doc: &Document) -> Vec<DependsMarker> {
    let mut out = Vec::new();
    for block in &doc.blocks {
        let Block::Passthrough { text, line } = block else { continue };
        for (i, l) in text.lines().enumerate() {
            if let Some((target, quote)) = parse_marker(l) {
                out.push(DependsMarker { target, quote, line: *line + i as u32 });
            }
        }
    }
    out
}
```

## Resolving a target

Exactly a written link's own resolution (`graph::resolve::resolve_path`),
narrowed to the one shape a marker's `target` can take: a path, optional,
plus a fragment. There is no dangling-placeholder step here, unlike a
real link. A marker that names nothing is reported by the caller as
"target not found," using the same whole-corpus `Graph` `check_cmd`
already built for the unresolved-link pass, rather than this module
inventing a second lookup.

```rust name=resolve_target path=depends.rs
/// Resolves `target`, written from `declaring_file`, to the `NodeId` it
/// would address as a link. `None` only when the path part would climb
/// above the root -- the same refusal `join_normalize` already gives a
/// written link. A `target` naming no such node is not `None`: it is a
/// `NodeId` the caller's own `Graph` simply does not contain, exactly how
/// `graph::resolve` already tells a resolved link apart from a dangling
/// one.
pub fn resolve_target(declaring_file: &str, target: &str) -> Option<NodeId> {
    let (path_part, fragment) = target.split_once('#').unwrap_or((target, ""));
    let key = if path_part.is_empty() {
        strip_extension(declaring_file)
    } else {
        let joined = join_normalize(dir_of(declaring_file), path_part)?;
        strip_extension(&joined)
    };
    Some(NodeId::new(key, slugify(fragment)))
}
```

## Slicing a section and checking the claim

`section_text` is a pure function of a source string and a line range --
exactly `Node::line`/`Node::end_line`, whatever produced them. It never
touches a filesystem itself. `check_cmd` already has a loaded source for
every file in the corpus (`eval::files::Files`, reused rather than a
second read), so this module has no reason to load one of its own.

```rust name=section_text path=depends.rs
/// The literal text spanning `start_line..=end_line`, both 1-indexed and
/// inclusive -- exactly the range a `Node`'s own `line`/`end_line` already
/// describe. Never normalized here; `verify` is what normalizes, and only
/// for the comparison itself, so a caller wanting the raw section for a
/// diagnostic still gets it unmodified.
pub fn section_text(source: &str, start_line: u32, end_line: u32) -> String {
    source
        .lines()
        .skip(start_line.saturating_sub(1) as usize)
        .take((end_line.saturating_sub(start_line) + 1) as usize)
        .collect::<Vec<_>>()
        .join("\n")
}
```

Whitespace is the one kind of "changed nothing" this module is willing to
see through on its own. Reflowing a paragraph moves words across line
breaks without changing any of them; a hand-edited blank line or a
trailing space carries no claim at all. Neither is `md/fmt.rs`'s full
normal form, which would need a real parse of arbitrary target prose just
to compare two fragments. Collapsing whitespace runs is the smallest
rule that already covers the case that actually shows up: a note
rewrapped by an editor, not rewritten by an author.

```rust name=normalize_and_verify path=depends.rs
/// Collapses every run of whitespace to a single space and trims the
/// ends. This is what keeps a rewrapped paragraph -- a line break moved,
/// no word changed -- from registering as drift.
pub fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The quote is still there, verbatim modulo whitespace.
    Fresh,
    /// It is not. This is advisory, never a hash mismatch: it means "go
    /// look," not "this build is broken."
    Stale,
}

/// Checks `marker.quote` against `section`'s current text, both
/// whitespace-normalized first. A substring match, not a hash comparison
/// -- see this module's own opening paragraph for why hashing the whole
/// section is the wrong tool here.
pub fn verify(marker: &DependsMarker, section: &str) -> Verdict {
    if normalize_ws(section).contains(&normalize_ws(&marker.quote)) {
        Verdict::Fresh
    } else {
        Verdict::Stale
    }
}
```

## Tests

```rust name=tests path=depends.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Diags;

    fn doc(src: &str) -> Document {
        Document::parse(src, &mut Diags::new("t.md"))
    }

    #[test]
    fn marker_round_trips() {
        let m = "<!-- dankg:depends target=other.md#heading quote=\"the walk never follows symlinks\" -->";
        assert_eq!(
            parse_marker(m),
            Some(("other.md#heading".to_string(), "the walk never follows symlinks".to_string()))
        );
    }

    #[test]
    fn marker_parses_regardless_of_key_order() {
        let m = "<!-- dankg:depends quote=\"x\" target=#heading -->";
        assert_eq!(parse_marker(m), Some(("#heading".to_string(), "x".to_string())));
    }

    #[test]
    fn a_marker_missing_quote_does_not_parse() {
        assert_eq!(parse_marker("<!-- dankg:depends target=a.md#h -->"), None);
    }

    #[test]
    fn an_unrelated_comment_does_not_parse_as_a_marker() {
        assert_eq!(parse_marker("<!-- just a note -->"), None);
    }

    #[test]
    fn markers_in_finds_a_lone_marker_and_its_line() {
        let d = doc("# Heading\n\nSome prose.\n\n<!-- dankg:depends target=#other quote=\"x\" -->\n\n# Other\n");
        let found = markers_in(&d);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].target, "#other");
        assert_eq!(found[0].line, 5);
    }

    #[test]
    fn markers_in_finds_one_mixed_into_a_larger_passthrough_block() {
        // No blank line separates the marker from the next line, so
        // `md/block.rs` gathers both into one Passthrough block. Unlike
        // `eval::result::locate_existing`, this still finds it: nothing
        // here ever writes back, so there is no neighbouring text to
        // protect from being clobbered.
        let d = doc("<!-- dankg:depends target=#h quote=\"x\" -->\n<!-- unrelated -->\n\n# H\n");
        let found = markers_in(&d);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
    }

    #[test]
    fn markers_in_finds_none_in_an_ordinary_document() {
        assert!(markers_in(&doc("# Heading\n\nJust prose.\n")).is_empty());
    }

    #[test]
    fn resolve_target_same_file() {
        assert_eq!(resolve_target("notes/a.md", "#some-heading"), Some(NodeId::new("notes/a", "some-heading")));
    }

    #[test]
    fn resolve_target_cross_file_relative_to_the_declaring_file() {
        assert_eq!(
            resolve_target("notes/deep/a.md", "../b.md#two"),
            Some(NodeId::new("notes/b", "two"))
        );
    }

    #[test]
    fn resolve_target_bare_file_with_no_fragment() {
        assert_eq!(resolve_target("a.md", "b.md"), Some(NodeId::new("b", "")));
    }

    #[test]
    fn resolve_target_refuses_a_path_that_escapes_the_root() {
        assert_eq!(resolve_target("a.md", "../../etc/passwd.md#x"), None);
    }

    #[test]
    fn section_text_slices_an_inclusive_one_indexed_range() {
        let src = "one\ntwo\nthree\nfour\n";
        assert_eq!(section_text(src, 2, 3), "two\nthree");
    }

    #[test]
    fn section_text_of_a_single_line_range() {
        assert_eq!(section_text("one\ntwo\nthree\n", 2, 2), "two");
    }

    #[test]
    fn normalize_ws_collapses_runs_and_trims() {
        assert_eq!(normalize_ws("  the   walk\nnever  follows  symlinks  "), "the walk never follows symlinks");
    }

    #[test]
    fn verify_is_fresh_when_the_quote_is_still_present() {
        let m = DependsMarker { target: "#h".into(), quote: "never follows symlinks".into(), line: 1 };
        assert_eq!(verify(&m, "The walk never follows symlinks. It refuses instead."), Verdict::Fresh);
    }

    #[test]
    fn verify_is_fresh_across_a_rewrapped_line_break() {
        // Same words, different line break -- exactly the "reformatting,
        // not a meaning change" case whitespace normalization exists for.
        let m = DependsMarker { target: "#h".into(), quote: "never follows symlinks".into(), line: 1 };
        assert_eq!(verify(&m, "The walk never follows\nsymlinks, on purpose."), Verdict::Fresh);
    }

    #[test]
    fn verify_is_stale_once_the_pinned_words_are_actually_gone() {
        let m = DependsMarker { target: "#h".into(), quote: "never follows symlinks".into(), line: 1 };
        assert_eq!(verify(&m, "The walk always follows symlinks now."), Verdict::Stale);
    }
}
```
