# TUI eval

Running a block without leaving the graph view reuses everything
`dankg eval`'s own CLI loop already has. `blocks_in_section` narrows to
one node's own extent, using the same `plan::top_level_blocks` the
planner walks. `run` hands off to `eval::session::run_one` (not yet
converted to its own literate source), the same function the CLI's
multi-target loop calls, so there is exactly one answer to "what does
running one block actually do," not a second, TUI-flavored copy of it.

```rust name=module_doc path=tui/eval.rs
//! Cycling through a selected node's named blocks and running one without
//! leaving the graph view: `keys.eval` to cycle, `enter` to run, `esc` to
//! cancel. Delegates the actual run to `eval::session::run_one`, the same
//! function the CLI's own multi-target loop uses, so there is exactly one
//! answer to "what does running one block actually do."

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Failed,
    TimedOut,
    Error(String),
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
        Ok(summary) if !summary.success => Outcome::Failed,
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e),
    }
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
    fn run_reports_ok_and_writes_back() {
        let path = scratch_file("b.md", "```sh name=a\necho hi\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        assert_eq!(run(&path, &config, 0), Outcome::Ok);
        assert!(fs::read_to_string(&path).unwrap().contains("dankg:result name=a"));
    }

    #[test]
    fn run_reports_failed_on_a_nonzero_exit() {
        let path = scratch_file("c.md", "```sh name=a\nexit 1\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        assert_eq!(run(&path, &config, 0), Outcome::Failed);
    }

    #[test]
    fn run_reports_an_error_for_a_position_out_of_range() {
        let path = scratch_file("d.md", "```sh name=a\n:\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        assert!(matches!(run(&path, &config, 5), Outcome::Error(_)));
    }
}
```
