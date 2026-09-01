//! Hashing, the `<!-- dankg:result ... -->` marker, and write-back.
//!
//! Write-back is text splicing over the original source, not a second pass
//! through `md/fmt.rs`: a full AST-to-text rewrite would reformat every
//! other block in the file on the first `dankg eval`, which is not eval's
//! job and not what decision 12 ("file stays the source of truth") asks
//! for. Instead this locates the exact line range to touch -- using the
//! already-parsed `Document` only to *find* that range -- and replaces
//! nothing else.

use super::plan::BlockRef;
use super::run;
use crate::hash;
use crate::md::{Block, Document};

/// Covers the target's transitive dependency sources, in the order they are
/// concatenated, and the *template* the language resolved to -- not the
/// fully-substituted argv, which would embed eval's own ephemeral temp file
/// path and make every result "stale" the instant it was checked. Editing
/// `[lang.*] command` in config is exactly the kind of change that should
/// mark a result stale; a different temp path on every run is not a change
/// at all.
pub fn expected_hash(chain: &[BlockRef], command_template: &str) -> u64 {
    let mut bytes = run::concatenated_source(chain).into_bytes();
    bytes.push(0);
    bytes.extend_from_slice(command_template.as_bytes());
    hash::fnv1a(&bytes)
}

/// `failed` marks a non-zero exit or a timeout: the output is still stored
/// (architecture.md, Execution -- "a non-zero exit stores the output and
/// marks the result failed"), just flagged rather than dropped, so a reader
/// sees what actually happened last time without `dankg eval` silently
/// discarding a run that went wrong.
pub fn render_marker(name: &str, result_hash: u64, failed: bool) -> String {
    let flag = if failed { " failed" } else { "" };
    format!("<!-- dankg:result name={name} hash={}{flag} -->", hash::hex(result_hash))
}

/// The inverse of [`render_marker`], tolerant of the exact spacing a hand
/// edit might introduce but not of a comment that merely happens to start
/// the same way -- `key=value` order is not fixed, but `name` and `hash`
/// must both be present, matching how `md/block.rs` reads a fence's own
/// info string.
pub fn parse_marker(line: &str) -> Option<(String, u64, bool)> {
    let inner = line.trim().strip_prefix("<!--")?.strip_suffix("-->")?.trim();
    let inner = inner.strip_prefix("dankg:result")?.trim();

    let mut name = None;
    let mut result_hash = None;
    let mut failed = false;
    for word in inner.split_whitespace() {
        if word == "failed" {
            failed = true;
            continue;
        }
        let (k, v) = word.split_once('=')?;
        match k {
            "name" => name = Some(v.to_string()),
            "hash" => result_hash = hash::parse_hex(v),
            _ => {}
        }
    }
    Some((name?, result_hash?, failed))
}

/// Looks immediately after the top-level code block at `doc.blocks[code_index]`
/// for an already-written result marker naming `name`, returning
/// `(marker_line, result_fence_close_line)` -- both 1-indexed, inclusive --
/// when one is found. `write_back` uses this to replace a stale result in
/// place instead of appending a second copy underneath it.
pub fn locate_existing(doc: &Document, code_index: usize, name: &str) -> Option<(u32, u32)> {
    let Block::Passthrough { text, line } = doc.blocks.get(code_index + 1)? else { return None };
    // Our own marker is always a lone line: a comment merged with more
    // passthrough text by `gather_passthrough` (no blank line separating
    // them) is not one we wrote, so it is left alone rather than guessed at.
    let mut lines = text.lines();
    let first = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    let (marker_name, _, _) = parse_marker(first)?;
    if marker_name != name {
        return None;
    }
    let Block::Code { end_line, .. } = doc.blocks.get(code_index + 2)? else { return None };
    Some((*line, *end_line))
}

/// Replaces `source`'s bytes from just after `code_end_line` through
/// `existing`'s end (when there is a prior result for this name) -- or
/// inserts fresh right after `code_end_line` (when there is not -- `existing`
/// is `None`) -- with a freshly rendered marker and result fence. Blank-line
/// spacing around the result is always renormalized to exactly what is
/// written here, so it self-heals any drift rather than accumulating it
/// across repeated evals.
pub fn write_back(
    source: &str,
    code_end_line: u32,
    existing: Option<(u32, u32)>,
    name: &str,
    result_hash: u64,
    failed: bool,
    output: &str,
) -> String {
    let had_trailing_newline = source.ends_with('\n');
    let mut lines: Vec<String> = source.split('\n').map(str::to_string).collect();
    if had_trailing_newline {
        lines.pop(); // drop the phantom empty element `split` leaves after a trailing `\n`
    }

    let mut insert: Vec<String> =
        vec![String::new(), render_marker(name, result_hash, failed), String::new(), "```".to_string()];
    insert.extend(output.lines().map(str::to_string));
    insert.push("```".to_string());

    let start = code_end_line as usize; // 0-indexed: right after the code block's last (1-indexed) line
    let end = existing.map_or(start, |(_, result_close_line)| result_close_line as usize);
    lines.splice(start..end, insert);

    let mut out = lines.join("\n");
    if had_trailing_newline {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Diags;
    use crate::eval::plan::{plan_for, top_level_blocks};

    fn doc(src: &str) -> Document {
        Document::parse(src, &mut Diags::new("t.md"))
    }

    #[test]
    fn expected_hash_changes_with_source_or_command() {
        let d = doc("```python name=a\nx=1\n```\n");
        let blocks = top_level_blocks(&d, "t.md");
        let chain = plan_for(&blocks, "t.md", "a").unwrap();
        let h1 = expected_hash(&chain, "python {file}");

        let d2 = doc("```python name=a\nx=2\n```\n");
        let blocks2 = top_level_blocks(&d2, "t.md");
        let chain2 = plan_for(&blocks2, "t.md", "a").unwrap();
        assert_ne!(h1, expected_hash(&chain2, "python {file}"), "different source, different hash");
        assert_ne!(h1, expected_hash(&chain, "python3 {file}"), "different command, different hash");
    }

    #[test]
    fn expected_hash_is_stable_across_repeated_calls() {
        let d = doc("```sh name=a\necho hi\n```\n");
        let blocks = top_level_blocks(&d, "t.md");
        let chain = plan_for(&blocks, "t.md", "a").unwrap();
        assert_eq!(expected_hash(&chain, "sh {file}"), expected_hash(&chain, "sh {file}"));
    }

    #[test]
    fn marker_round_trips() {
        let m = render_marker("index", 0xa3f9c1, false);
        assert_eq!(parse_marker(&m), Some(("index".to_string(), 0xa3f9c1, false)));
    }

    #[test]
    fn a_failed_marker_round_trips_with_the_flag_set() {
        let m = render_marker("index", 0xa3f9c1, true);
        assert_eq!(parse_marker(&m), Some(("index".to_string(), 0xa3f9c1, true)));
    }

    #[test]
    fn marker_parses_regardless_of_key_order() {
        let m = "<!-- dankg:result hash=1 name=foo -->";
        assert_eq!(parse_marker(m), Some(("foo".to_string(), 1, false)));
    }

    #[test]
    fn an_unrelated_comment_does_not_parse_as_a_marker() {
        assert_eq!(parse_marker("<!-- just a note -->"), None);
    }

    #[test]
    fn locate_existing_finds_a_matching_marker_and_fence() {
        let src = "```python name=index\nprint(1)\n```\n\n<!-- dankg:result name=index hash=0000000000000001 -->\n\n```\n1\n```\n";
        let d = doc(src);
        let got = locate_existing(&d, 0, "index").unwrap();
        // marker is line 5, the result fence closes on line 9.
        assert_eq!(got, (5, 9));
    }

    #[test]
    fn locate_existing_ignores_a_marker_for_a_different_name() {
        let src = "```python name=index\nprint(1)\n```\n\n<!-- dankg:result name=other hash=0000000000000001 -->\n\n```\n1\n```\n";
        let d = doc(src);
        assert!(locate_existing(&d, 0, "index").is_none());
    }

    #[test]
    fn locate_existing_is_none_with_nothing_following() {
        let d = doc("```python name=index\nprint(1)\n```\n");
        assert!(locate_existing(&d, 0, "index").is_none());
    }

    #[test]
    fn write_back_inserts_after_a_block_with_nothing_following() {
        let src = "```sh name=a\necho hi\n```\n";
        let got = write_back(src, 3, None, "a", 1, false, "hi\n");
        assert_eq!(got, "```sh name=a\necho hi\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nhi\n```\n");
    }

    #[test]
    fn write_back_marks_a_failed_run() {
        let src = "```sh name=a\nfalse\n```\n";
        let got = write_back(src, 3, None, "a", 1, true, "");
        assert!(got.contains("hash=0000000000000001 failed -->"), "{got:?}");
    }

    #[test]
    fn write_back_preserves_a_missing_trailing_newline() {
        let src = "```sh name=a\necho hi\n```"; // no trailing newline
        let got = write_back(src, 3, None, "a", 1, false, "hi\n");
        assert!(!got.ends_with('\n'));
        assert!(got.ends_with("```"));
    }

    #[test]
    fn write_back_does_not_disturb_content_that_follows() {
        let src = "```sh name=a\necho hi\n```\n\n# Next\n";
        let got = write_back(src, 3, None, "a", 1, false, "hi\n");
        assert!(got.ends_with("\n\n# Next\n"), "{got:?}");
    }

    #[test]
    fn write_back_replaces_an_existing_result_in_place() {
        let src = "```sh name=a\necho hi\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nold\n```\n\n# Next\n";
        let d = doc(src);
        let existing = locate_existing(&d, 0, "a").unwrap();
        let got = write_back(src, 3, Some(existing), "a", 2, false, "new\n");
        assert_eq!(
            got,
            "```sh name=a\necho hi\n```\n\n<!-- dankg:result name=a hash=0000000000000002 -->\n\n```\nnew\n```\n\n# Next\n"
        );
    }

    #[test]
    fn write_back_renormalizes_extra_blank_lines_around_an_existing_result() {
        let src = "```sh name=a\necho hi\n```\n\n\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n\n```\nold\n```\n";
        let d = doc(src);
        let existing = locate_existing(&d, 0, "a").unwrap();
        let got = write_back(src, 3, Some(existing), "a", 1, false, "old\n");
        assert_eq!(got, "```sh name=a\necho hi\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nold\n```\n");
    }

    #[test]
    fn write_back_handles_multi_line_output() {
        let got = write_back("```sh name=a\n:\n```\n", 3, None, "a", 1, false, "one\ntwo\nthree\n");
        assert!(got.contains("```\none\ntwo\nthree\n```\n"), "{got:?}");
    }
}
