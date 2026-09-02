# TUI editor

This is the one place `dankg tui` shells out to something the reader
chose, rather than something this crate rendered itself. `[editor]
command` (decision 17) goes through [`crate::cmd`](../cmd.md) exactly
like any other configured template. What is specific to this module is
the fallback chain when nothing is configured, and the fact that it
never touches raw-mode terminal state on its own. Suspending and
resuming that around the editor's own full-screen use of the terminal
is `app.rs`'s job, since opening someone's editor is the one case in
this whole crate where another program, not `dankg`, owns the screen
for a while.

```rust name=module_doc path=tui/editor.rs
//! Hands a node's source location to the reader's editor.
//!
//! Deliberately does not know about `term::RawMode`: suspending and
//! resuming raw mode around this call is the event loop's job (`app.rs`,
//! not built yet), since this module has no business owning terminal state
//! it did not set up.

use crate::cmd;
use crate::config::Config;
use std::io;
use std::process::{Command, ExitStatus};
```

```rust name=open path=tui/editor.rs
/// Spawns the configured `[editor] command` (decision 17) at
/// `file:line`, inheriting this process's stdio so the editor draws
/// directly to the terminal, and blocks until it exits. Falls back to
/// `$EDITOR`/`$VISUAL` when nothing is configured, opening the bare
/// file with no line number. Flag syntax for "open at a line" is not
/// standard across editors the way `[editor] command`'s explicit
/// `{file}`/`{line}` template lets a reader state it, so the fallback
/// is a documented gap, not a silent one. `None` when neither source
/// names an editor.
pub fn open(config: &Config, file: &str, line: u32) -> io::Result<Option<ExitStatus>> {
    let env_editor = std::env::var("EDITOR").or_else(|_| std::env::var("VISUAL")).ok();
    let Some(argv) = resolve(config.editor(), env_editor.as_deref(), file, line) else {
        return Ok(None);
    };
    let status = Command::new(&argv[0]).args(&argv[1..]).status()?;
    Ok(Some(status))
}
```

`open` itself is the thin, untested wrapper that reads the real
environment and spawns a real process. Every case below is exercised
through `resolve` instead, which takes both inputs as plain arguments.

```rust name=resolve path=tui/editor.rs
/// The argv `open` would spawn, given what `config.editor()` and the
/// environment resolved to. Pure and separately testable from `open`:
/// the thin, untested wrapper that reads the real environment and
/// spawns a real process.
fn resolve(
    configured: Option<&str>,
    env_editor: Option<&str>,
    file: &str,
    line: u32,
) -> Option<Vec<String>> {
    if let Some(template) = configured {
        let line = line.to_string();
        return cmd::build(template, &[("file", file), ("line", &line)]);
    }
    let mut argv = cmd::split(env_editor?);
    if argv.is_empty() {
        return None;
    }
    argv.push(file.to_string());
    Some(argv)
}
```

## Tests

```rust name=tests path=tui/editor.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_template_wins_over_the_environment() {
        assert_eq!(
            resolve(Some("code -g {file}:{line}"), Some("vim"), "a.md", 10),
            Some(vec!["code".to_string(), "-g".to_string(), "a.md:10".to_string()])
        );
    }

    #[test]
    fn falls_back_to_the_environment_editor_with_no_line_number() {
        assert_eq!(
            resolve(None, Some("vim"), "a.md", 10),
            Some(vec!["vim".to_string(), "a.md".to_string()])
        );
    }

    #[test]
    fn an_environment_editor_can_carry_its_own_flags() {
        assert_eq!(
            resolve(None, Some("code -w"), "a.md", 3),
            Some(vec!["code".to_string(), "-w".to_string(), "a.md".to_string()])
        );
    }

    #[test]
    fn nothing_configured_and_no_environment_editor_resolves_to_nothing() {
        assert_eq!(resolve(None, None, "a.md", 10), None);
    }
}
```
