# Tag

`[tui] commands`'s `protocol=lines` convention (eval-custom-plan.md)
lets a command emit `tag: target kind=value` on its own stdout. The
first cut of this kept what it found in memory, on `App`, for the
running session only -- gone the moment the TUI quit. That contradicts
a principle DanKG already commits to elsewhere: a result is "written
back into the markdown, hash-tagged... file stays the source of
truth" (decision 12). Classification gets the same treatment here:
`tag:`'s own target is resolved once, and what it found is written
into the target's own document as a
`<!-- dankg:tag kind=... target=... -->` marker, exactly the way
`eval::result` already writes a block's own result back rather than
remembering it only in a running process.

**Placement, and why it's *before* the node rather than after.**
`eval::result`'s marker sits immediately after the block it reports
on -- a result is naturally something that follows the code that
produced it. A classification has no such direction: it describes the
node, the way a caption describes a photo rather than following it.
Placing it immediately *before* the node it tags also sidesteps a real
collision a block node could otherwise have: `eval::result` already
owns the line right after a block's own closing fence, and a second
marker competing for that same spot would need its own rule for which
one comes first. Before has no such rival -- nothing else in this
codebase ever writes into the line right above a node's own opening
line.

**Scope.** A marker attaches to whichever real node -- heading or
block -- physically sits right after it, found the same way for
either: `Block::line()` already reports a heading's own single line or
a block's own opening fence line, uniformly. A `Relation` node has
neither -- decision: *Provenance without a driver* gives it a
synthetic `db:NAME` namespace instead of a real file and line -- so
there is nowhere for a marker to physically attach, and `write_back`
below is never asked to try.

**Why `target=`, when position already says which node a marker is
for.** Purely positional attachment has a real failure mode: insert a
new heading between an existing marker and the node it was meant for,
an entirely ordinary edit, and the marker silently reattaches to the
*wrong* node -- nothing notices, because nothing ever recorded what it
was actually supposed to mean. `target=#slug` (`resolve_target`'s own
fragment shape, decision: *Provenance without a driver*'s sibling
convention `dankg:depends` already uses) makes the tie explicit
instead of implicit, so `dankg check` can catch drift the same way it
already catches an unresolved link: by comparing what a marker claims
against what is actually there. Every marker `write_back` writes
carries one; a hand-written marker without it still works exactly as
before -- positional, unverified -- since requiring it would make
hand-authoring a tag needlessly heavier than typing `kind=` alone.

**Why the marker holds only `kind=`, not an icon too.** The first cut
of this had a `tag:` line carry its own `icon=` straight into the
marker, one per node. A reader who tags fifty nodes `kind=task` was
repeating the same icon fifty times, and a later change of mind about
which glyph means "task" meant editing fifty documents instead of one
line of config. `[kind.*]` (`config.rs`) now owns the icon,
keyed by the same name a `tag:` line's `kind=` already names, so a
node's own marker only ever needs to say *what it is*, never *how it
looks*.

```rust name=module_doc path=tag.rs
//! `<!-- dankg:tag kind=... target=... -->`: a node's own durable
//! classification, written immediately before the heading or block it
//! describes -- `eval::result`'s own "write it into the file, don't
//! just remember it" policy applied to `[tui] commands`' `tag:` output
//! line convention (eval-custom-plan.md). The icon a `kind` renders as
//! lives in config (`[kind.*]`, `config.rs`), not in the marker
//! itself -- one name here, one glyph there, never both repeated
//! together on every tagged node. `target` makes the tie to that node
//! explicit rather than merely positional, so `dankg check` can catch
//! an edit that quietly moved a marker onto the wrong one.

use crate::md::{Block, Document};
```

## The marker itself

`target` is optional in the parsed result -- a hand-written marker
without one still works, purely positional, exactly as before this
attribute existed. `write_back`, below, always supplies one: it is
the one caller that always has the node's own resolved slug on hand
already, with nothing left to guess.

```rust name=marker path=tag.rs
/// `<!-- dankg:tag kind=value target=#slug -->`. `target`, when
/// present, is `depends::resolve_target`'s own fragment shape --
/// `#slug` for the common same-file case, `file#slug` for one written
/// by hand about a node in some other file -- ready to hand straight
/// to it without reformatting.
pub fn render_marker(kind: &str, target: &str) -> String {
    format!("<!-- dankg:tag kind={kind} target={target} -->")
}

/// The inverse of [`render_marker`], as `(kind, target)`. Tolerates the
/// exact spacing a hand edit might introduce, but not a comment that
/// merely happens to start the same way, nor one with no `kind=` at
/// all -- that is the one attribute every marker must carry, so
/// nothing else here means anything without it. An unrecognized key is
/// ignored rather than rejecting the whole line, the same warn-nothing,
/// drop-gracefully stance `eval::result::parse_marker` already takes
/// on a key it does not know.
pub fn parse_marker(line: &str) -> Option<(String, Option<String>)> {
    let inner = line.trim().strip_prefix("<!--")?.strip_suffix("-->")?.trim();
    let inner = inner.strip_prefix("dankg:tag")?.trim();
    let mut kind = None;
    let mut target = None;
    for word in inner.split_whitespace() {
        let (k, v) = word.split_once('=')?;
        match k {
            "kind" => kind = Some(v.to_string()),
            "target" => target = Some(v.to_string()),
            _ => {}
        }
    }
    Some((kind?, target))
}
```

## Finding a marker already there

`marker_before` is the one place that actually walks `doc.blocks` --
`locate_existing` and `markers_in` are both just different ways of
calling it. A marker several corpus edits old, sitting above a
heading that has since had other content inserted between them, is
not found: the same "adjacent, and nothing else," lone-line
discipline `eval::result::locate_existing` already holds itself to,
aimed the other direction.

```rust name=marker_before path=tag.rs
/// Whether `doc.blocks[target_index - 1]` is a lone-line `dankg:tag`
/// marker naming `doc.blocks[target_index]`'s own node -- `None` for
/// the first block in the file (nothing precedes it), for anything
/// other than a lone single-line `Passthrough` there, or for one that
/// fails to parse. "Lone-line" excludes a comment `md/block.rs`'s
/// `gather_passthrough` merged with more text underneath it (no blank
/// line separating them): that is not a marker this module wrote, so
/// it is left alone rather than guessed at.
fn marker_before(doc: &Document, target_index: usize) -> Option<(u32, String, Option<String>)> {
    let preceding_index = target_index.checked_sub(1)?;
    let Block::Passthrough { text, line } = doc.blocks.get(preceding_index)? else { return None };
    let mut lines = text.lines();
    let first = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    let (kind, target) = parse_marker(first)?;
    Some((*line, kind, target))
}

/// A node's own current marker, keyed by `anchor_line` -- the exact
/// line `Block::line()` already reports for a heading or a block,
/// `graph::model::Node::line`'s own source. `write_back` uses this to
/// replace a stale marker in place, instead of writing a second one
/// underneath it.
pub fn locate_existing(doc: &Document, anchor_line: u32) -> Option<(u32, String, Option<String>)> {
    let index = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Heading { .. } | Block::Code { .. }) && b.line() == anchor_line)?;
    marker_before(doc, index)
}

/// Every already-written marker in `doc`, as `(anchor_line, kind,
/// target)` triples -- the corpus-wide read side, `tui::app::
/// compute_tags`'s one caller, resolving each `anchor_line` back to a
/// `NodeId` against the graph it already has rather than reparsing
/// one here, and `dankg check`'s own pass validating each `kind`
/// against `[kind.*]` and each `target`, when present, against the
/// node the marker actually sits on.
pub fn markers_in(doc: &Document) -> Vec<(u32, String, Option<String>)> {
    doc.blocks
        .iter()
        .enumerate()
        .filter_map(|(i, block)| {
            if !matches!(block, Block::Heading { .. } | Block::Code { .. }) {
                return None;
            }
            let (_, kind, target) = marker_before(doc, i)?;
            Some((block.line(), kind, target))
        })
        .collect()
}
```

## Writing it back

Blank-line spacing around a marker is always renormalized to exactly
what is written here, the same self-healing `eval::result::write_back`
already relies on: a hand-edited extra blank line drifts back to
normal on the next write rather than accumulating across repeated
ones.

```rust name=write_back path=tag.rs
/// Replaces `source`'s bytes with a freshly rendered marker,
/// immediately before `anchor_line` (1-indexed, `Block::line()`'s own
/// numbering). When `existing` names a prior marker's own line
/// (`locate_existing`), everything from there through `anchor_line`
/// (exclusive) is replaced; otherwise the marker is inserted fresh
/// right there, pushing the node itself down by two lines. `target` is
/// always written -- the one caller, `App::write_tag`, already has the
/// node's own resolved `NodeId` in hand, so there is nothing to omit
/// it for.
pub fn write_back(source: &str, anchor_line: u32, existing: Option<u32>, kind: &str, target: &str) -> String {
    let had_trailing_newline = source.ends_with('\n');
    let mut lines: Vec<String> = source.split('\n').map(str::to_string).collect();
    if had_trailing_newline {
        lines.pop(); // drop the phantom empty element `split` leaves after a trailing `\n`
    }

    let insert = vec![render_marker(kind, target), String::new()];
    let end = (anchor_line - 1) as usize; // 0-indexed: right before the node's own (1-indexed) line
    let start = existing.map_or(end, |marker_line| (marker_line - 1) as usize);
    lines.splice(start..end, insert);

    let mut out = lines.join("\n");
    if had_trailing_newline {
        out.push('\n');
    }
    out
}
```

## Guarding against a wide or colored icon

`dankg check` validates a `[kind.*]`'s own `icon` with this, advisory
only -- it can never fail the exit code the way an unknown `kind=`
does, since getting this wrong costs a reader a misaligned column, not
a broken reference. Worth building anyway: this codebase has already
hit the exact failure once by hand, choosing an hourglass, `⌛`, as the
needs-run badge glyph before finding out it renders wide and in color
by default, breaking column alignment, and settling on `↻` instead
(eval-custom-plan.md's own decisions table). Centralizing icons in
`[kind.*]` is the first point where DanKG can actually check a reader's
own choice before it ships into a hundred badges, rather than leaving
every future kind's author to rediscover the same bug by hand.

This is not the real Unicode `Emoji_Presentation` property table -- a
zero-dependency codebase has no way to carry that wholesale, and
guessing by block range is provably wrong even here: `✗`, already a
shipped badge glyph (`badge_for`), sits in the same Dingbats block
(U+2700-U+27BF) as several genuinely emoji-default characters, yet is
itself text-presentation. Scoped narrower on purpose: more than one
codepoint at all (a joined or modifier sequence, or simply two
characters where one was expected -- reliable, no Unicode table
needed); the exact clock/hourglass cluster responsible for the one
real incident (`U+231A`-`U+231B`, `U+23E9`-`U+23FA`); and the modern
emoji blocks from `U+1F000` up, which are overwhelmingly emoji-only by
Unicode's own design, unlike the older mixed blocks a range guess
already failed on above. Missing some rarer emoji-default character
below `U+1F000` costs nothing this check was ever certain about
anyway; flagging a genuinely safe glyph costs one advisory line, never
a build.

```rust name=icon_width path=tag.rs
/// Best-effort: an icon likely to render wider than one terminal
/// column, or in color, per this module's own doc comment above.
pub fn icon_may_break_alignment(icon: &str) -> bool {
    let mut chars = icon.chars();
    let (Some(only), None) = (chars.next(), chars.next()) else { return true };
    matches!(only as u32, 0x231A..=0x231B | 0x23E9..=0x23FA | 0x1F000..=0x1FFFF)
}
```

## Tests

```rust name=tests path=tag.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Diags;

    fn doc(src: &str) -> Document {
        Document::parse(src, &mut Diags::new("t"))
    }

    #[test]
    fn marker_round_trips_through_render_and_parse() {
        assert_eq!(parse_marker(&render_marker("task", "#two")), Some(("task".to_string(), Some("#two".to_string()))));
    }

    #[test]
    fn parse_marker_ignores_an_unrelated_comment() {
        assert_eq!(parse_marker("<!-- just a note -->"), None);
    }

    #[test]
    fn parse_marker_is_none_with_no_kind_at_all() {
        assert_eq!(parse_marker("<!-- dankg:tag target=#two -->"), None);
    }

    #[test]
    fn parse_marker_allows_a_missing_target_a_hand_written_marker_might_omit() {
        assert_eq!(parse_marker("<!-- dankg:tag kind=task -->"), Some(("task".to_string(), None)));
    }

    #[test]
    fn parse_marker_ignores_an_unrecognized_key_but_still_finds_kind_and_target() {
        assert_eq!(
            parse_marker("<!-- dankg:tag bogus=1 kind=task target=#two -->"),
            Some(("task".to_string(), Some("#two".to_string())))
        );
    }

    #[test]
    fn locate_existing_finds_a_marker_immediately_before_a_heading() {
        let d = doc("# One\n\n<!-- dankg:tag kind=task target=#two -->\n\n## Two\n");
        assert_eq!(locate_existing(&d, 5), Some((3, "task".to_string(), Some("#two".to_string()))));
    }

    #[test]
    fn locate_existing_finds_a_marker_immediately_before_a_block() {
        let d = doc("<!-- dankg:tag kind=job target=#a -->\n\n```sh name=a\n:\n```\n");
        assert_eq!(locate_existing(&d, 3), Some((1, "job".to_string(), Some("#a".to_string()))));
    }

    #[test]
    fn locate_existing_is_none_for_the_first_block_in_the_file() {
        let d = doc("# One\n");
        assert_eq!(locate_existing(&d, 1), None);
    }

    #[test]
    fn locate_existing_ignores_a_comment_merged_with_more_text() {
        // No blank line before "## Two", so `gather_passthrough` folds
        // both lines into one multi-line `Passthrough` -- not a marker
        // this module wrote, so it is left alone.
        let d = doc("<!-- dankg:tag kind=task target=#two -->\nsome other note\n\n## Two\n");
        assert_eq!(locate_existing(&d, 4), None);
    }

    #[test]
    fn markers_in_collects_every_marker_in_document_order() {
        let d = doc("<!-- dankg:tag kind=a target=#one -->\n\n# One\n\n<!-- dankg:tag kind=b target=#two -->\n\n## Two\n");
        assert_eq!(
            markers_in(&d),
            vec![(3, "a".to_string(), Some("#one".to_string())), (7, "b".to_string(), Some("#two".to_string()))]
        );
    }

    #[test]
    fn markers_in_is_empty_with_nothing_written() {
        assert!(markers_in(&doc("# One\n\n## Two\n")).is_empty());
    }

    #[test]
    fn write_back_inserts_fresh_before_a_heading_with_nothing_there_yet() {
        let got = write_back("# One\n\n## Two\n", 3, None, "task", "#two");
        assert_eq!(got, "# One\n\n<!-- dankg:tag kind=task target=#two -->\n\n## Two\n");
    }

    #[test]
    fn write_back_replaces_an_existing_marker_in_place() {
        let src = "# One\n\n<!-- dankg:tag kind=task target=#two -->\n\n## Two\n\n# Next\n";
        let got = write_back(src, 5, Some(3), "done", "#two");
        assert_eq!(got, "# One\n\n<!-- dankg:tag kind=done target=#two -->\n\n## Two\n\n# Next\n");
    }

    #[test]
    fn write_back_renormalizes_extra_blank_lines_around_an_existing_marker() {
        let src = "# One\n\n\n\n<!-- dankg:tag kind=task target=#two -->\n\n\n## Two\n";
        let got = write_back(src, 8, Some(5), "task", "#two");
        assert_eq!(got, "# One\n\n\n\n<!-- dankg:tag kind=task target=#two -->\n\n## Two\n");
    }

    #[test]
    fn write_back_preserves_a_missing_trailing_newline() {
        let got = write_back("# One\n\n## Two", 3, None, "task", "#two");
        assert!(!got.ends_with('\n'));
    }

    #[test]
    fn write_back_does_not_disturb_content_that_follows() {
        let got = write_back("# One\n\n## Two\n\n- kept\n", 3, None, "task", "#two");
        assert!(got.ends_with("## Two\n\n- kept\n"));
    }

    #[test]
    fn icon_may_break_alignment_flags_more_than_one_codepoint() {
        assert!(icon_may_break_alignment("XY"));
        assert!(icon_may_break_alignment("👍🏽")); // thumbs up + skin-tone modifier
    }

    #[test]
    fn icon_may_break_alignment_flags_the_known_incident_and_its_family() {
        assert!(icon_may_break_alignment("⌛")); // U+231B, the one this codebase already got bitten by
        assert!(icon_may_break_alignment("⏰")); // U+23F0, same clock cluster
        assert!(icon_may_break_alignment("🎯")); // U+1F3AF, a modern emoji block
    }

    #[test]
    fn icon_may_break_alignment_leaves_known_safe_glyphs_alone() {
        // Every one of these is already a shipped badge glyph
        // (`badge_for`) or this feature's own example icon -- none may
        // ever start failing this check.
        for safe in ["⇒", "⇐", "✗", "↻", "▤", "⚭", "→", "←", "☐"] {
            assert!(!icon_may_break_alignment(safe), "{safe:?} should not be flagged");
        }
    }
}
```
