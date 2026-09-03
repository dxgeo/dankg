# Eval result

Write-back is text splicing over the original source. It is never a
second pass through `md/fmt.rs`. A full AST-to-text rewrite would
reformat every other block in the file the first time `dankg eval`
touched it. That is not eval's job, and it is not what decision 12
("file stays the source of truth") asks for. This module locates the
exact line range to touch. It uses the already-parsed `Document` only
to *find* that range, and it replaces nothing else in the file.

```rust name=module_doc path=eval/result.rs
//! Hashing, the `<!-- dankg:result ... -->` marker, and write-back.
//!
//! Write-back is text splicing over the original source. It is not a
//! second pass through `md/fmt.rs`. A full AST-to-text rewrite would
//! reformat every other block in the file on the first `dankg eval`.
//! That is not eval's job, and it is not what decision 12 ("file stays
//! the source of truth") asks for. Instead this locates the exact line
//! range to touch. It uses the already-parsed `Document` only to *find*
//! that range, and it replaces nothing else.

use super::files::Files;
use super::plan::{self, BlockRef};
use super::run;
use crate::hash;
use crate::md::{Block, Document};
```

`expected_hash` covers the target's whole dependency chain and the
*template* a language resolved to. It deliberately does not cover the
fully substituted argv, which would embed eval's own ephemeral temp
file path and mark every result stale the instant it was checked.
Editing `[lang.*] command` in config is exactly the kind of change
that should invalidate a result. A different temp path on every run is
not a change at all. `xdep_hashes` extends this one input further:
each is a *referenced* block's own last recorded hash, never its
source, folded in by value rather than by concatenation, because the
whole reason an `xdeps` edge exists is to reach a block this target's
own interpreter cannot run.

```rust name=expected_hash path=eval/result.rs
/// Covers the target's transitive dependency sources, in the order they
/// are concatenated, the *template* the language resolved to, and each
/// of the target's own `xdep_hashes`, in declaration order. It does not
/// cover the fully-substituted argv, which would embed eval's own
/// ephemeral temp file path and make every result "stale" the instant it
/// was checked. Editing `[lang.*] command` in config is exactly the kind
/// of change that should mark a result stale. A different temp path on
/// every run is not a change at all. `xdep_hashes` is empty for a target
/// with no `xdeps=`, which is exactly why this never changes the hash
/// of anything that does not use the feature.
pub fn expected_hash(chain: &[BlockRef], command_template: &str, xdep_hashes: &[u64]) -> u64 {
    let mut bytes = run::concatenated_source(chain).into_bytes();
    bytes.push(0);
    bytes.extend_from_slice(command_template.as_bytes());
    for h in xdep_hashes {
        bytes.push(0);
        bytes.extend_from_slice(&h.to_le_bytes());
    }
    hash::fnv1a(&bytes)
}
```

`xdep_hashes` is what actually crosses the language boundary
`plan::resolve_xdeps` only resolves a name for. Reading a referenced
block's last recorded marker, on its own, is not enough: a stale
marker (its own source changed since it last ran) is stale *text*,
sitting there unchanged until someone re-runs it, and folding it in
as though it were trustworthy would silently swallow staleness at
every `xdeps` boundary it crossed. So each reference is *verified*,
not merely read: `verified_hash` recomputes what the referenced
block's own hash would be right now, from its own current chain and
its own `xdeps`, recursively, the same check `dankg check`'s per-block
loop already runs. Only when that recomputation matches what is
actually stored does the stored value get trusted and folded in. A
mismatch propagates outward as an error, exactly like "never run" does
below -- which is what makes staleness cross a language boundary at
all, rather than stopping dead at the first one.

```rust name=xdep_lookup path=eval/result.rs
/// A block's own last recorded result hash, read straight from its
/// `<!-- dankg:result -->` marker. `None` covers both "no result yet"
/// and "a comment sits there that merely looks like one" --
/// `parse_marker` already treats those as indistinguishable.
pub fn recorded_hash(doc: &Document, index: usize, name: &str) -> Option<u64> {
    locate_existing(doc, index, name)?;
    let Block::Passthrough { text, .. } = doc.blocks.get(index + 1)? else { return None };
    let (_, hash, _) = parse_marker(text.lines().next()?)?;
    Some(hash)
}

/// `blocks[index]`'s own hash, verified fresh right now rather than
/// merely read. Recomputes its chain and its own `xdeps` (recursively,
/// through this same function) from current source, exactly what
/// `dankg check`'s per-block loop already does, and only returns the
/// stored value if a fresh recomputation still matches it. A mismatch,
/// a missing recorded result, or an `xdeps` cycle back through
/// `visiting` are all reported by name rather than silently treated as
/// fresh. An unconfigured language cannot be re-verified at all -- the
/// stored value is trusted as-is, the same leniency an unconfigured
/// language already gets from not being double-counted as stale in
/// `main.rs`.
/// Every `xdeps=` entry declared by *any* block in `chain`, resolved and
/// verified fresh, in chain order. Not just `chain`'s last element: a
/// `deps=` member concatenated into the chain can carry its own
/// `xdeps=` that the chain's own target never mentions at all
/// (`dedupe`'s `xdeps=raw.md#load_raw`, concatenated into `normalize`'s
/// run by `deps=dedupe`, say). `run::concatenated_source` already folds
/// every chain member's *source* in for exactly this reason; this folds
/// in every member's `xdeps` the same way. `verified_hash` below calls
/// this same function on *its own* target's chain, so a broken `xdeps`
/// buried behind several `deps=` hops is caught at every level of the
/// recursion, not just the outermost one.
fn chain_xdep_hashes(
    blocks: &[BlockRef],
    files: &Files,
    config: &crate::config::Config,
    chain: &[BlockRef],
    visiting: &mut std::collections::HashSet<usize>,
) -> Result<Vec<u64>, String> {
    let mut hashes = Vec::new();
    for b in chain {
        for i in plan::resolve_xdeps(blocks, b).map_err(|e| e.to_string())? {
            hashes.push(verified_hash(blocks, files, config, i, visiting)?);
        }
    }
    Ok(hashes)
}

fn verified_hash(
    blocks: &[BlockRef],
    files: &Files,
    config: &crate::config::Config,
    index: usize,
    visiting: &mut std::collections::HashSet<usize>,
) -> Result<u64, String> {
    let b = &blocks[index];
    if !visiting.insert(index) {
        return Err(format!("`{}` is part of an xdeps cycle", b.name));
    }
    let chain = plan::plan_for(blocks, b.file, b.name).map_err(|e| e.to_string())?;
    let xdep_hashes = chain_xdep_hashes(blocks, files, config, &chain, visiting)?;
    visiting.remove(&index);

    let (_, doc) = files.get(b.file).ok_or_else(|| format!("{}: not loaded", b.file))?;
    let Some(stored) = recorded_hash(doc, b.index, b.name) else {
        return Err(format!("`{}` has no recorded result yet -- run it first", b.name));
    };
    let Some(lang) = run::command_for(config, &chain) else {
        return Ok(stored); // unconfigured language: cannot re-verify, trust what is stored
    };
    if expected_hash(&chain, &lang.command, &xdep_hashes) == stored {
        Ok(stored)
    } else {
        Err(format!("`{}` is itself stale -- run it first", b.name))
    }
}

/// `chain_xdep_hashes` for a caller outside this module: `dankg check`'s
/// staleness loop and `run_one` both already hold the exact chain
/// they mean, and neither needs to see `verified_hash`'s own recursion.
pub fn xdep_hashes(
    files: &Files,
    config: &crate::config::Config,
    blocks: &[BlockRef],
    chain: &[BlockRef],
) -> Result<Vec<u64>, String> {
    let mut visiting = std::collections::HashSet::new();
    chain_xdep_hashes(blocks, files, config, chain, &mut visiting)
}
```

```rust name=marker path=eval/result.rs
/// `failed` marks a non-zero exit or a timeout. The output is still
/// stored (architecture.md, Execution: "a non-zero exit stores the
/// output and marks the result failed"), just flagged rather than
/// dropped. This way a reader sees what actually happened last time,
/// without `dankg eval` silently discarding a run that went wrong.
pub fn render_marker(name: &str, result_hash: u64, failed: bool) -> String {
    let flag = if failed { " failed" } else { "" };
    format!("<!-- dankg:result name={name} hash={}{flag} -->", hash::hex(result_hash))
}

/// The inverse of [`render_marker`]. It tolerates the exact spacing a
/// hand edit might introduce, but not a comment that merely happens to
/// start the same way. `key=value` order is not fixed, but `name` and
/// `hash` must both be present, matching how `md/block.rs` reads a
/// fence's own info string.
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
```

```rust name=locate_existing path=eval/result.rs
/// Looks immediately after the top-level code block at
/// `doc.blocks[code_index]` for an already-written result marker naming
/// `name`. Returns `(marker_line, result_fence_close_line)` (both
/// 1-indexed, inclusive) when one is found. `write_back` uses this to
/// replace a stale result in place, instead of appending a second copy
/// underneath it.
pub fn locate_existing(doc: &Document, code_index: usize, name: &str) -> Option<(u32, u32)> {
    let Block::Passthrough { text, line } = doc.blocks.get(code_index + 1)? else { return None };
    // Our own marker is always a lone line. A comment merged with more
    // passthrough text by `gather_passthrough` (no blank line separating
    // them) is not one we wrote, so it is left alone rather than guessed
    // at.
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
```

Blank-line spacing around a result is always renormalized to exactly what
`write_back` writes here, so drift from a hand edit self-heals on the next
`dankg eval` rather than slowly accumulating across repeated runs.

```rust name=write_back path=eval/result.rs
/// Replaces `source`'s bytes with a freshly rendered marker and result
/// fence. When there is a prior result for this name, it replaces from
/// just after `code_end_line` through `existing`'s end. When there is
/// not (`existing` is `None`), it inserts fresh right after
/// `code_end_line`. Blank-line spacing around the result is always
/// renormalized to exactly what is written here, so it self-heals any
/// drift rather than accumulating it across repeated evals.
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
```

## Tests

```rust name=tests path=eval/result.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
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
        let h1 = expected_hash(&chain, "python {file}", &[]);

        let d2 = doc("```python name=a\nx=2\n```\n");
        let blocks2 = top_level_blocks(&d2, "t.md");
        let chain2 = plan_for(&blocks2, "t.md", "a").unwrap();
        assert_ne!(h1, expected_hash(&chain2, "python {file}", &[]), "different source, different hash");
        assert_ne!(h1, expected_hash(&chain, "python3 {file}", &[]), "different command, different hash");
    }

    #[test]
    fn expected_hash_is_stable_across_repeated_calls() {
        let d = doc("```sh name=a\necho hi\n```\n");
        let blocks = top_level_blocks(&d, "t.md");
        let chain = plan_for(&blocks, "t.md", "a").unwrap();
        assert_eq!(expected_hash(&chain, "sh {file}", &[]), expected_hash(&chain, "sh {file}", &[]));
    }

    #[test]
    fn expected_hash_changes_with_an_xdep_hash() {
        let d = doc("```sh name=a\necho hi\n```\n");
        let blocks = top_level_blocks(&d, "t.md");
        let chain = plan_for(&blocks, "t.md", "a").unwrap();
        let without = expected_hash(&chain, "sh {file}", &[]);
        let with_one = expected_hash(&chain, "sh {file}", &[42]);
        let with_another = expected_hash(&chain, "sh {file}", &[43]);
        assert_ne!(without, with_one, "an xdep hash must actually be folded in");
        assert_ne!(with_one, with_another, "a different xdep hash must change the result");
    }

    #[test]
    fn recorded_hash_reads_a_written_marker() {
        let d = doc("```sh name=a\necho hi\n```\n\n<!-- dankg:result name=a hash=000000000000002a -->\n\n```\nhi\n```\n");
        assert_eq!(recorded_hash(&d, 0, "a"), Some(0x2a));
    }

    #[test]
    fn recorded_hash_is_none_before_anything_has_run() {
        let d = doc("```sh name=a\necho hi\n```\n");
        assert_eq!(recorded_hash(&d, 0, "a"), None);
    }

    fn scratch_dir(suffix: &str, content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dankg-result-xdep-test-{}-{suffix}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("t.md"), content).unwrap();
        dir
    }

    #[test]
    fn xdep_hashes_reads_a_verified_fresh_targets_recorded_hash() {
        let dir = scratch_dir("ok", "```sh name=setup\necho hi\n```\n");
        let doc = Document::parse(&std::fs::read_to_string(dir.join("t.md")).unwrap(), &mut Diags::new("t"));
        let blocks = top_level_blocks(&doc, "t.md");
        let chain = plan_for(&blocks, "t.md", "setup").unwrap();
        let h = expected_hash(&chain, "sh {file}", &[]);
        // A marker that genuinely matches a fresh recomputation, not just
        // a made-up value -- `verified_hash` recomputes and compares now,
        // so a fixture with an arbitrary stored hash would report stale.
        let content = format!(
            "```sh name=setup\necho hi\n```\n\n<!-- dankg:result name=setup hash={} -->\n\n```\nhi\n```\n\n```sh name=top xdeps=setup\n:\n```\n",
            hash::hex(h)
        );
        std::fs::write(dir.join("t.md"), content).unwrap();

        let mut files = Files::new(dir);
        let mut diags = Diags::new("t");
        files.discover("t.md", &mut diags).unwrap();
        let blocks = files.all_blocks();
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        let top = blocks.iter().find(|b| b.name == "top").unwrap();
        assert_eq!(xdep_hashes(&files, &config, &blocks, std::slice::from_ref(top)).unwrap(), vec![h]);
    }

    #[test]
    fn xdep_hashes_reports_a_target_with_no_recorded_result_yet() {
        let dir = scratch_dir("missing", "```sh name=setup\necho hi\n```\n\n```sh name=top xdeps=setup\n:\n```\n");
        let mut files = Files::new(dir);
        let mut diags = Diags::new("t");
        files.discover("t.md", &mut diags).unwrap();
        let blocks = files.all_blocks();
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        let top = blocks.iter().find(|b| b.name == "top").unwrap();
        let err = xdep_hashes(&files, &config, &blocks, std::slice::from_ref(top)).unwrap_err();
        assert!(err.contains("setup"), "{err:?}");
        assert!(err.contains("run it first"), "{err:?}");
    }

    #[test]
    fn xdep_hashes_reports_when_the_referenced_target_is_itself_stale() {
        // `setup`'s stored marker does not match a fresh recomputation of
        // its current source -- exactly what a plain text edit to an
        // already-run block looks like, before anyone re-runs it. This is
        // the regression this whole function exists to catch: a naive
        // "just read the stored marker" version would report `top` as
        // fresh here, which is wrong.
        let dir = scratch_dir(
            "stale-upstream",
            "```sh name=setup\necho changed\n```\n\n<!-- dankg:result name=setup hash=0000000000000000 -->\n\n```\nold\n```\n\n```sh name=top xdeps=setup\n:\n```\n",
        );
        let mut files = Files::new(dir);
        let mut diags = Diags::new("t");
        files.discover("t.md", &mut diags).unwrap();
        let blocks = files.all_blocks();
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        let top = blocks.iter().find(|b| b.name == "top").unwrap();
        let err = xdep_hashes(&files, &config, &blocks, std::slice::from_ref(top)).unwrap_err();
        assert!(err.contains("setup"), "{err:?}");
        assert!(err.contains("itself stale"), "{err:?}");
    }

    #[test]
    fn xdep_hashes_reports_an_xdeps_cycle() {
        let dir = scratch_dir("cycle", "```sh name=a xdeps=b\n:\n```\n\n```sh name=b xdeps=a\n:\n```\n");
        let mut files = Files::new(dir);
        let mut diags = Diags::new("t");
        files.discover("t.md", &mut diags).unwrap();
        let blocks = files.all_blocks();
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        let a = blocks.iter().find(|b| b.name == "a").unwrap();
        let err = xdep_hashes(&files, &config, &blocks, std::slice::from_ref(a)).unwrap_err();
        assert!(err.contains("cycle"), "{err:?}");
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
```
