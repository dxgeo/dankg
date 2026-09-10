# TUI eval

Running a block without leaving the graph view reuses everything
`dankg eval`'s own CLI loop already has. `blocks_in_section` narrows to
one node's own extent, using the same `plan::top_level_blocks` the
planner walks. `run` hands off to
[`eval::session::run_one`](../eval/session.md), the same function the
CLI's multi-target loop calls, so there is exactly one answer to "what
does running one block actually do," not a second, TUI-flavored copy
of it.

```rust name=module_doc path=tui/eval.rs
//! Cycling through a selected node's named blocks and running one without
//! leaving the graph view: `keys.eval` to cycle, `enter` to run, `esc` to
//! cancel. Delegates the actual run to `eval::session::run_one`, the same
//! function the CLI's own multi-target loop uses, so there is exactly one
//! answer to "what does running one block actually do."
//!
//! `scan_protocol_lines`, below, is the other half of `[tui] commands`:
//! a `protocol=lines` block's own captured stdout is not free text but
//! `select:`/`status:`/`tag:` control lines (eval-custom-plan.md). This
//! module only recognizes them; `App` is the one place with a graph to
//! resolve a target against and a tree cursor to move.

use super::input;
use crate::config::Config;
use crate::diag::Diags;
use crate::eval::{plan, session};
use crate::md::Document;
use std::fs;
```

The file is read fresh on every call, rather than cached, since it may
have changed since the graph was last loaded. An unreadable or
unparsable file just yields no blocks. There is nowhere in the grid to
show an error, and `editor.rs`'s own handoff already treats a spawn
failure the same way: best-effort.

```rust name=blocks_in_section path=tui/eval.rs
/// Named top-level blocks whose own line falls within `[start_line,
/// end_line]`: a node's heading section, exactly the extent
/// `graph/build.rs` already computes for it. Read fresh from `path`
/// each time, rather than cached, since the file may have changed since
/// the graph was last loaded. An unreadable or unparsable file yields
/// no blocks rather than an error: there is nowhere in the grid to
/// show one, and the editor handoff (`editor.rs`) already treats a
/// spawn failure the same way, best-effort.
///
/// Each block's position among *all* the file's named top-level blocks
/// is carried alongside its name, not just the name on its own. A
/// section that spans a nested sub-heading can legally contain two
/// blocks sharing a name (decision 22 scopes uniqueness to one
/// heading, not a whole subtree), and that position is what `run`
/// needs to run the right one.
pub fn blocks_in_section(path: &str, start_line: u32, end_line: u32) -> Vec<(usize, String)> {
    let Ok(source) = fs::read_to_string(path) else { return Vec::new() };
    let mut diags = Diags::new(path);
    let doc = Document::parse(&source, &mut diags);
    plan::top_level_blocks(&doc, path)
        .into_iter()
        .enumerate()
        .filter(|(_, b)| b.line >= start_line && b.line <= end_line)
        .map(|(i, b)| (i, b.name.to_string()))
        .collect()
}

/// `Ok`/`Failed` carry whatever the run actually captured, so a caller
/// can show more than just "it worked" -- `App` uses this to put a
/// command's own output on the status line instead of a bare "ok".
/// `Failed`'s text prefers `stderr`, falling back to `stdout` only when
/// `stderr` is empty: a nonzero exit's own diagnostic is usually the
/// more useful of the two. `TimedOut` carries nothing -- whatever was
/// captured before the kill is as likely to be a truncated, misleading
/// fragment as a useful report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok(String),
    Failed(String),
    TimedOut,
    Error(String),
}
```

`keyed_commands` is `[tui] commands`'s own reader: every top-level
block in the named file carrying a `key=` that `input::parse`
recognises, as `(position, key, name, protocol_lines)`. A `key=` that
does not parse -- more than one character, an unrecognised word -- is
skipped here, with nothing surfaced: the same best-effort stance
`blocks_in_section` above already takes for a file it cannot read at
all. A key that parses but collides with a built-in, or with another
command in the same file, is a different kind of problem -- `App::load`
is the one place that also holds `Keymap` and the rest of this file's
own bindings to check against, so refusing those, loudly, is its job,
not this function's.

```rust name=keyed_commands path=tui/eval.rs
/// Every top-level block in `path` carrying a `key=` `input::parse`
/// recognises, as `(position, key, name, protocol_lines)`, in document
/// order. `position` counts among *all* of `path`'s named top-level
/// blocks, exactly as `plan_for_index` expects it -- the same convention
/// `blocks_in_section` already follows. `protocol_lines` is the block's
/// own `protocol=lines` attribute, carried straight through so
/// `App::run_command` knows whether to scan the run's output at all.
/// Read fresh from disk each time, since the file may have changed
/// since the graph was last loaded.
pub fn keyed_commands(path: &str) -> Vec<(usize, input::Key, String, bool)> {
    let Ok(source) = fs::read_to_string(path) else { return Vec::new() };
    let mut diags = Diags::new(path);
    let doc = Document::parse(&source, &mut diags);
    plan::top_level_blocks(&doc, path)
        .into_iter()
        .enumerate()
        .filter_map(|(position, b)| {
            let key = input::parse(b.key?)?;
            Some((position, key, b.name.to_string(), b.protocol_lines))
        })
        .collect()
}
```

No separate confirm step here: cycling to a block with `keys.eval` and
pressing `enter` to run it already *is* the confirmation, the same way
`enter` needs no second prompt before it spawns the configured editor on a
selected node.

```rust name=run path=tui/eval.rs
/// Runs the block at `position` (an index into `path`'s named top-level
/// blocks, as `blocks_in_section` returns it) and writes its result back
/// into `path`. No separate confirm step: cycling to a block with
/// `keys.eval` and pressing `enter` to run it already *is* the
/// confirmation, the same way `enter` needs no second prompt before it
/// spawns the configured editor on a selected node.
pub fn run(path: &str, config: &Config, position: usize) -> Outcome {
    match session::run_one(path, config, position, false) {
        Ok(summary) if summary.timed_out => Outcome::TimedOut,
        Ok(summary) if !summary.success => {
            Outcome::Failed(if summary.stderr.trim().is_empty() { summary.stdout } else { summary.stderr })
        }
        Ok(summary) => Outcome::Ok(summary.stdout),
        Err(e) => Outcome::Error(e),
    }
}
```

## The `protocol=lines` output convention

A `protocol=lines` block's own stdout is not free text: it is a small
set of recognized line prefixes, `select:`/`status:`/`tag:`
(eval-custom-plan.md's design decision on guarding this against
accidental collisions -- the block opted in, so a line here is trusted
to mean what it says). Every other line, and every line in a block
that never opted in at all, is not this module's concern; the caller's
own raw-text fallback (`outcome_status`, `tui/app.rs`) already covers
it.

`select:`/`tag:` carry a *target*, resolved later by the caller
(`depends::resolve_target`, relative to the command's own file) --
this module only extracts the raw text, since it has no graph to
resolve against. `select:` keeps only its *last* occurrence: a script
emitting more than one is presumably narrowing down to a final answer,
not asking for two things at once. `status:` keeps only its *first*:
`outcome_status`'s own existing precedent for "which line wins" when a
run says more than one thing. `tag:` lines all accumulate, in order,
since each one can name a different node.

```rust name=protocol_output path=tui/eval.rs
/// One `target`'s worth of `tag:` attributes, `key=value` pairs in the
/// order they appeared. `App::apply_protocol_output` is the one reader:
/// it updates only the attributes named here on that node's own
/// annotation, leaving any other attribute (set by an earlier `tag:`
/// line for the same node) untouched.
pub type TagAttrs = Vec<(String, String)>;

/// What `scan_protocol_lines` found in a `protocol=lines` block's own
/// stdout. `select`/`status` are raw, unresolved text -- `App` is the
/// one place with a graph to resolve `select`'s target against.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProtocolOutput {
    pub select: Option<String>,
    pub status: Option<String>,
    pub tags: Vec<(String, TagAttrs)>,
}

/// `key=value key=value ...`, space-separated, in order. Not
/// `InfoString`'s own attribute parser: that one also handles a bare
/// language word and quoting an info string never needs. A token with
/// no `=` is skipped rather than rejecting the whole line -- consistent
/// with this convention's own "a block opted in, trust it" stance, but
/// with nothing crucial riding on one malformed token, there is no
/// reason to throw away every other one alongside it.
fn parse_attrs(rest: &str) -> TagAttrs {
    rest.split_whitespace().filter_map(|tok| tok.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// Every `select:`/`status:`/`tag:` line in `text`, in the order they
/// appeared. A line is only recognized by its own exact prefix,
/// followed immediately by `:` -- `select:`, not `selects:` or
/// `select :`. Everything else is not this convention's concern at
/// all.
pub fn scan_protocol_lines(text: &str) -> ProtocolOutput {
    let mut out = ProtocolOutput::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("select:") {
            out.select = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("status:") {
            if out.status.is_none() {
                out.status = Some(rest.trim().to_string());
            }
        } else if let Some(rest) = line.strip_prefix("tag:") {
            let rest = rest.trim();
            if let Some((target, attrs)) = rest.split_once(char::is_whitespace) {
                out.tags.push((target.to_string(), parse_attrs(attrs)));
            }
        }
    }
    out
}
```

## Tests

```rust name=tests path=tui/eval.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn scratch_file(name: &str, content: &str) -> String {
        let dir = std::env::temp_dir().join(format!("dankg-tui-eval-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn blocks_in_section_only_finds_blocks_within_the_given_line_range() {
        let path = scratch_file(
            "a.md",
            "# One\n\n```sh name=a\n:\n```\n\n# Two\n\n```sh name=b\n:\n```\n",
        );
        // "One" spans lines 1-5 (up to the blank line before "Two"). "a"
        // sits inside it, "b" does not.
        let found = blocks_in_section(&path, 1, 5);
        assert_eq!(found, vec![(0, "a".to_string())]);
    }

    #[test]
    fn blocks_in_section_is_empty_when_the_file_cannot_be_read() {
        assert!(blocks_in_section("/does/not/exist.md", 1, 100).is_empty());
    }

    #[test]
    fn blocks_in_section_carries_the_right_position_when_a_name_is_reused() {
        // "setup" appears twice. The section for "Two" (a nested heading
        // under "One") must report *its own* setup's position, not One's.
        let path = scratch_file(
            "e.md",
            "# One\n\n```sh name=setup\necho one\n```\n\n## Two\n\n```sh name=setup\necho two\n```\n",
        );
        let found = blocks_in_section(&path, 7, 10);
        assert_eq!(found, vec![(1, "setup".to_string())]);
    }

    #[test]
    fn keyed_commands_finds_a_recognised_key_and_skips_an_unrecognised_one() {
        let path = scratch_file(
            "f.md",
            "```sh name=reindex key=g\n:\n```\n\n```sh name=bad key=pageup\n:\n```\n\n```sh name=plain\n:\n```\n",
        );
        let found = keyed_commands(&path);
        assert_eq!(found, vec![(0, input::Key::Char('g'), "reindex".to_string(), false)]);
    }

    #[test]
    fn keyed_commands_recognises_a_ctrl_combination() {
        let path = scratch_file("g.md", "```sh name=go key=ctrl+g\n:\n```\n");
        let found = keyed_commands(&path);
        assert_eq!(found, vec![(0, input::Key::Ctrl('g'), "go".to_string(), false)]);
    }

    #[test]
    fn keyed_commands_carries_protocol_lines_through() {
        let path = scratch_file("p.md", "```sh name=go key=g protocol=lines\n:\n```\n");
        let found = keyed_commands(&path);
        assert_eq!(found, vec![(0, input::Key::Char('g'), "go".to_string(), true)]);
    }

    #[test]
    fn keyed_commands_is_empty_when_the_file_cannot_be_read() {
        assert!(keyed_commands("/does/not/exist.md").is_empty());
    }

    #[test]
    fn run_reports_ok_and_writes_back() {
        let path = scratch_file("b.md", "```sh name=a\necho hi\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        assert_eq!(run(&path, &config, 0), Outcome::Ok("hi\n".to_string()));
        assert!(fs::read_to_string(&path).unwrap().contains("dankg:result name=a"));
    }

    #[test]
    fn run_reports_failed_on_a_nonzero_exit() {
        let path = scratch_file("c.md", "```sh name=a\nexit 1\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        assert_eq!(run(&path, &config, 0), Outcome::Failed(String::new()));
    }

    #[test]
    fn run_reports_failed_prefers_stderr_over_stdout() {
        let path = scratch_file("h.md", "```sh name=a\necho out\necho err >&2\nexit 1\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        assert_eq!(run(&path, &config, 0), Outcome::Failed("err\n".to_string()));
    }

    #[test]
    fn run_reports_an_error_for_a_position_out_of_range() {
        let path = scratch_file("d.md", "```sh name=a\n:\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        assert!(matches!(run(&path, &config, 5), Outcome::Error(_)));
    }

    #[test]
    fn scan_protocol_lines_reads_all_three_prefixes() {
        let out = scan_protocol_lines("select: a.md#one\nstatus: done\ntag: a.md#one kind=task icon=x\nnoise\n");
        assert_eq!(out.select, Some("a.md#one".to_string()));
        assert_eq!(out.status, Some("done".to_string()));
        assert_eq!(out.tags, vec![("a.md#one".to_string(), vec![("kind".to_string(), "task".to_string()), ("icon".to_string(), "x".to_string())])]);
    }

    #[test]
    fn scan_protocol_lines_keeps_the_last_select_and_the_first_status() {
        let out = scan_protocol_lines("select: a.md#one\nstatus: first\nselect: a.md#two\nstatus: second\n");
        assert_eq!(out.select, Some("a.md#two".to_string()), "last select wins");
        assert_eq!(out.status, Some("first".to_string()), "first status wins");
    }

    #[test]
    fn scan_protocol_lines_ignores_unrecognized_text() {
        let out = scan_protocol_lines("just some ordinary output\nstatus:\n");
        assert_eq!(out.select, None);
        assert_eq!(out.status, Some(String::new()));
        assert!(out.tags.is_empty());
    }

    #[test]
    fn scan_protocol_lines_drops_a_tag_line_with_no_attrs() {
        let out = scan_protocol_lines("tag: a.md#one\n");
        assert!(out.tags.is_empty(), "nothing to set, so nothing worth keeping");
    }

    #[test]
    fn scan_protocol_lines_accumulates_every_tag_line() {
        let out = scan_protocol_lines("tag: a.md#one kind=task\ntag: a.md#two kind=doc\n");
        assert_eq!(
            out.tags,
            vec![
                ("a.md#one".to_string(), vec![("kind".to_string(), "task".to_string())]),
                ("a.md#two".to_string(), vec![("kind".to_string(), "doc".to_string())]),
            ]
        );
    }
}
```
