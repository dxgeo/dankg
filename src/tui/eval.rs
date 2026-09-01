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

/// Named top-level blocks whose own line falls within `[start_line,
/// end_line]` -- a node's heading section, exactly the extent
/// `graph/build.rs` already computes for it -- read fresh from `path` each
/// time rather than cached, since the file may have changed since the graph
/// was last loaded. An unreadable or unparsable file yields no blocks
/// rather than an error: there is nowhere in the grid to show one, and the
/// editor handoff (`editor.rs`) already treats a spawn failure the same
/// way, best-effort.
pub fn blocks_in_section(path: &str, start_line: u32, end_line: u32) -> Vec<String> {
    let Ok(source) = fs::read_to_string(path) else { return Vec::new() };
    let mut diags = Diags::new(path);
    let doc = Document::parse(&source, &mut diags);
    plan::top_level_blocks(&doc)
        .into_iter()
        .filter(|b| b.line >= start_line && b.line <= end_line)
        .map(|b| b.name.to_string())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Failed,
    TimedOut,
    Error(String),
}

/// Runs `name` and writes its result back into `path`. No separate confirm
/// step: cycling to a block with `keys.eval` and pressing `enter` to run it
/// already *is* the confirmation, the same way `enter` needs no second
/// prompt before it spawns the configured editor on a selected node.
pub fn run(path: &str, config: &Config, name: &str) -> Outcome {
    match session::run_one(path, config, name, false) {
        Ok(summary) if summary.timed_out => Outcome::TimedOut,
        Ok(summary) if !summary.success => Outcome::Failed,
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e),
    }
}

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
        // "One" spans lines 1-5 (up to the blank line before "Two"); "a" sits
        // inside it, "b" does not.
        let names = blocks_in_section(&path, 1, 5);
        assert_eq!(names, vec!["a".to_string()]);
    }

    #[test]
    fn blocks_in_section_is_empty_when_the_file_cannot_be_read() {
        assert!(blocks_in_section("/does/not/exist.md", 1, 100).is_empty());
    }

    #[test]
    fn run_reports_ok_and_writes_back() {
        let path = scratch_file("b.md", "```sh name=a\necho hi\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        assert_eq!(run(&path, &config, "a"), Outcome::Ok);
        assert!(fs::read_to_string(&path).unwrap().contains("dankg:result name=a"));
    }

    #[test]
    fn run_reports_failed_on_a_nonzero_exit() {
        let path = scratch_file("c.md", "```sh name=a\nexit 1\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        assert_eq!(run(&path, &config, "a"), Outcome::Failed);
    }

    #[test]
    fn run_reports_an_error_for_an_unknown_block() {
        let path = scratch_file("d.md", "```sh name=a\n:\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        assert!(matches!(run(&path, &config, "ghost"), Outcome::Error(_)));
    }
}
