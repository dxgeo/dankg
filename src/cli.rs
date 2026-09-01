//! Argument parsing.
//!
//! Hand-rolled, like everything else. The surface is small enough that a parser
//! crate would cost more than it saves.

use crate::eval::EvalTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Html,
    Dot,
    Mermaid,
}

impl Format {
    fn parse(value: &str) -> Result<Format, String> {
        match value {
            "json" => Ok(Format::Json),
            "html" => Ok(Format::Html),
            "dot" => Ok(Format::Dot),
            "mermaid" => Ok(Format::Mermaid),
            other => Err(format!("unknown format `{other}` (expected json, html, dot, or mermaid)")),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Html => "html",
            Format::Dot => "dot",
            Format::Mermaid => "mermaid",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Graph {
        paths: Vec<String>,
        format: Format,
        output: Option<String>,
        cache: bool,
        /// Hops from the entry. `None` falls back to `[graph] depth`.
        depth: Option<u32>,
        /// Skip view selection and draw the whole index.
        all: bool,
    },
    Index { paths: Vec<String>, cache: bool },
    Fmt { paths: Vec<String>, check: bool },
    Tui { paths: Vec<String>, cache: bool, depth: Option<u32>, all: bool },
    /// `paths` holds exactly one entry for `Block`/`All` -- `deps=` only
    /// resolves within one file (decision 19) -- but any number for `List`,
    /// which walks a corpus the way `graph`/`index`/`check` do and has no
    /// execution to scope.
    Eval { paths: Vec<String>, target: EvalTarget, yes: bool, no_write: bool, cache: bool },
    Check { paths: Vec<String>, cache: bool },
    Help,
    Version,
}

pub const USAGE: &str = "\
dankg -- a plaintext knowledge grapher

usage:
  dankg graph <path>... [--format <fmt>] [--depth N | --all] [-o <file>]
  dankg index [<path>]  [--no-cache]
  dankg fmt   <path>... [--check]
  dankg tui   <path>... [--depth N | --all] [--no-cache]
  dankg eval  <path> [--block <name> | --all] [--yes] [--no-write]
  dankg eval  [<path>...] --list [--no-cache]
  dankg check [<path>...] [--no-cache]
  dankg --help
  dankg --version

options:
  --format <fmt>   json (default), html, dot, mermaid
  --depth <n>      hops from the entry to draw; default from [graph] depth
  --all            draw the whole index (graph/tui), or every named block (eval)
  -o, --output     write to a file instead of stdout
  --no-cache       ignore .dankg/cache/ and write nothing back to it
  --check          report files not in normal form; write nothing
  --block <name>   the named block eval should run, with its dependencies
  --list           list every named block instead of running one -- a file
                   lists just its own, a directory (or several paths, the
                   default being \".\") walks the whole corpus
  --yes            skip eval's \"proceed?\" prompt
  --no-write       run and print output, but do not write results back

`tui` needs a real terminal and draws the same view `graph` would, with the
selected node's source line handed to `[editor] command` on enter (arrows or
hjkl to move, tab to reveal a node's hidden neighbours, enter to open, e to
cycle a node's named blocks and enter to run the cycled one in place, p to
toggle panning the viewport instead of the selection, r to collapse back to
the entry view, q to quit, ? for a full-screen keybinding reference). The
letter keys -- everything but the arrows, enter, tab, esc, and `?` -- are
remappable in `[keys]`.

`graph`, `index`, and `tui` discover the root by walking up for a `.dankg/` directory,
falling back to the directory the named paths share, and then index every
markdown file under it. The named paths set the view; the index is always the
whole root, because backlinks are only honest when every file has been seen.

`--format json` always emits the whole index, which is what makes it the
scriptable surface; `--depth` and `--all` shape the drawn formats. Naming a
directory rather than a file draws everything, since the corpus is the entry.

`--format html` writes one self-contained page: nothing to fetch, nothing to
serve. It carries the whole index, so a node the view left out can be expanded
in the browser without running dankg again. Its source links are root-relative,
so write it to the root -- `-o graph.html` from there is the intended use.

`index` reports what the walk found and what state the cache is in. The cache
is strictly an optimisation: `--no-cache` must produce identical output.

`fmt` rewrites files in place. It refuses to write any file whose formatted
form does not re-parse to the same document, so a formatter bug cannot quietly
destroy a note.

`eval` never runs anything automatically: it prints what it would run (a
named block plus its transitive `deps=`, in order) and asks before running,
unless `--yes`. A block's language must have a configured `[lang.*] command`
or nothing runs. Results are written back into the source, hash-tagged;
`--no-write` prints the captured output instead of writing it. `deps=` only
resolves within the one named file eval was given, so `--block`/`--all`
take exactly one path. `--list` shows every named block -- name, language,
source line, containing heading, and whether its language is configured --
without running anything, which is how to find a block's name in the first
place before naming it to `--block`. Unlike `--block`/`--all`, `--list` has
no execution to scope: naming a file lists just that file's blocks, naming
a directory (or several paths, or nothing -- defaulting to `.`) walks the
whole corpus and lists every file's.

`check` is the CI gate: exits non-zero when the corpus has an unresolved
link or a written eval result whose hash no longer matches its current
source, dependencies, or configured command. Deliberately separate from
`graph`, so drafting a half-written note never fails a build.

Diagnostics go to stderr, so stdout stays pipeable.
";

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut args = args.into_iter().peekable();

    let Some(first) = args.next() else {
        return Ok(Command::Help);
    };

    match first.as_str() {
        "-h" | "--help" | "help" => return Ok(Command::Help),
        "-V" | "--version" | "version" => return Ok(Command::Version),
        "graph" => {}
        "index" => return index(args),
        "fmt" => return fmt(args),
        "tui" => return tui(args),
        "eval" => return eval(args),
        "check" => return check(args),
        other if other.starts_with('-') => {
            return Err(format!("unknown option `{other}`"));
        }
        other => {
            return Err(format!(
                "unknown command `{other}` (expected `graph`, `index`, `fmt`, `tui`, `eval`, or `check`)"
            ));
        }
    }

    let mut paths: Vec<String> = Vec::new();
    let mut format = Format::Json;
    let mut output = None;
    let mut cache = true;
    let mut depth = None;
    let mut all = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" | "-f" => {
                let value = args.next().ok_or("`--format` needs a value")?;
                format = Format::parse(&value)?;
            }
            "-o" | "--output" => {
                output = Some(args.next().ok_or("`--output` needs a value")?);
            }
            "--no-cache" => cache = false,
            "--all" | "-a" => all = true,
            "--depth" | "-d" => {
                let value = args.next().ok_or("`--depth` needs a value")?;
                depth = Some(parse_depth(&value)?);
            }
            "-h" | "--help" => return Ok(Command::Help),
            other if other.starts_with("--format=") => {
                format = Format::parse(&other["--format=".len()..])?;
            }
            other if other.starts_with("--depth=") => {
                depth = Some(parse_depth(&other["--depth=".len()..])?);
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            other => paths.push(other.to_string()),
        }
    }

    if paths.is_empty() {
        return Err("`graph` needs at least one path".to_string());
    }

    if all && depth.is_some() {
        return Err("`--all` and `--depth` ask for different things".to_string());
    }
    Ok(Command::Graph { paths, format, output, cache, depth, all })
}

fn parse_depth(value: &str) -> Result<u32, String> {
    value.parse().map_err(|_| format!("`--depth` wants a whole number, not `{value}`"))
}

/// `index` takes an optional path because "tell me about the corpus I am
/// standing in" is the common case.
fn index<I: Iterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut paths: Vec<String> = Vec::new();
    let mut cache = true;

    for arg in args {
        match arg.as_str() {
            "--no-cache" => cache = false,
            "-h" | "--help" => return Ok(Command::Help),
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            other => paths.push(other.to_string()),
        }
    }

    if paths.is_empty() {
        paths.push(".".to_string());
    }
    Ok(Command::Index { paths, cache })
}

fn fmt<I: Iterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut paths: Vec<String> = Vec::new();
    let mut check = false;

    for arg in args {
        match arg.as_str() {
            "--check" | "-n" => check = true,
            "-h" | "--help" => return Ok(Command::Help),
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            other => paths.push(other.to_string()),
        }
    }

    if paths.is_empty() {
        return Err("`fmt` needs at least one path".to_string());
    }
    Ok(Command::Fmt { paths, check })
}

fn tui<I: Iterator<Item = String>>(mut args: I) -> Result<Command, String> {
    let mut paths: Vec<String> = Vec::new();
    let mut cache = true;
    let mut depth = None;
    let mut all = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--no-cache" => cache = false,
            "--all" | "-a" => all = true,
            "--depth" | "-d" => {
                let value = args.next().ok_or("`--depth` needs a value")?;
                depth = Some(parse_depth(&value)?);
            }
            "-h" | "--help" => return Ok(Command::Help),
            other if other.starts_with("--depth=") => {
                depth = Some(parse_depth(&other["--depth=".len()..])?);
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            other => paths.push(other.to_string()),
        }
    }

    if paths.is_empty() {
        return Err("`tui` needs at least one path".to_string());
    }
    if all && depth.is_some() {
        return Err("`--all` and `--depth` ask for different things".to_string());
    }
    Ok(Command::Tui { paths, cache, depth, all })
}

/// `--block`/`--all` take exactly one path: `deps=` only resolves within
/// one file (`eval::plan`), so there is no meaning to naming a second.
/// `--list` explores rather than runs, so it walks a corpus the way
/// `graph`/`index`/`check` do -- any number of paths, defaulting to `.`.
fn eval<I: Iterator<Item = String>>(mut args: I) -> Result<Command, String> {
    let mut paths: Vec<String> = Vec::new();
    let mut block: Option<String> = None;
    let mut all = false;
    let mut list = false;
    let mut yes = false;
    let mut no_write = false;
    let mut cache = true;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--block" | "-b" => {
                block = Some(args.next().ok_or("`--block` needs a value")?);
            }
            "--all" | "-a" => all = true,
            "--list" | "-l" => list = true,
            "--yes" | "-y" => yes = true,
            "--no-write" => no_write = true,
            "--no-cache" => cache = false,
            "-h" | "--help" => return Ok(Command::Help),
            other if other.starts_with("--block=") => {
                block = Some(other["--block=".len()..].to_string());
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            other => paths.push(other.to_string()),
        }
    }

    let target = match (block, all, list) {
        (Some(name), false, false) => EvalTarget::Block(name),
        (None, true, false) => EvalTarget::All,
        (None, false, true) => EvalTarget::List,
        (None, false, false) => return Err("`eval` needs `--block <name>`, `--all`, or `--list`".to_string()),
        _ => return Err("`--block`, `--all`, and `--list` ask for different things".to_string()),
    };

    if matches!(target, EvalTarget::List) {
        if paths.is_empty() {
            paths.push(".".to_string());
        }
    } else {
        if paths.is_empty() {
            return Err("`eval` needs a path".to_string());
        }
        if paths.len() > 1 {
            return Err("`eval` takes exactly one path; `deps=` only resolves within one file".to_string());
        }
    }
    Ok(Command::Eval { paths, target, yes, no_write, cache })
}

/// `check` takes any number of paths, defaulting to `.` -- the corpus you are
/// standing in is the common case, same as `index`.
fn check<I: Iterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut paths: Vec<String> = Vec::new();
    let mut cache = true;

    for arg in args {
        match arg.as_str() {
            "--no-cache" => cache = false,
            "-h" | "--help" => return Ok(Command::Help),
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            other => paths.push(other.to_string()),
        }
    }

    if paths.is_empty() {
        paths.push(".".to_string());
    }
    Ok(Command::Check { paths, cache })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_to_json_on_stdout() {
        let cmd = parse(args(&["graph", "a.md"])).unwrap();
        assert_eq!(
            cmd,
            Command::Graph {
                paths: vec!["a.md".into()],
                format: Format::Json,
                output: None,
                cache: true,
                depth: None,
                all: false,
            }
        );
    }

    #[test]
    fn accepts_both_format_spellings() {
        let a = parse(args(&["graph", "a.md", "--format", "dot"])).unwrap();
        let b = parse(args(&["graph", "a.md", "--format=dot"])).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn collects_multiple_paths_and_output() {
        let cmd = parse(args(&["graph", "a.md", "b.md", "-o", "g.json"])).unwrap();
        let Command::Graph { paths, output, .. } = cmd else { panic!() };
        assert_eq!(paths, vec!["a.md".to_string(), "b.md".to_string()]);
        assert_eq!(output.as_deref(), Some("g.json"));
    }

    #[test]
    fn no_args_is_help() {
        assert_eq!(parse(args(&[])).unwrap(), Command::Help);
    }

    #[test]
    fn fmt_collects_paths_and_check() {
        let cmd = parse(args(&["fmt", "a.md", "b.md", "--check"])).unwrap();
        assert_eq!(
            cmd,
            Command::Fmt { paths: vec!["a.md".into(), "b.md".into()], check: true }
        );
        let cmd = parse(args(&["fmt", "a.md"])).unwrap();
        assert_eq!(cmd, Command::Fmt { paths: vec!["a.md".into()], check: false });
    }

    #[test]
    fn depth_accepts_both_spellings_and_conflicts_with_all() {
        let a = parse(args(&["graph", "a.md", "--depth", "3"])).unwrap();
        let b = parse(args(&["graph", "a.md", "--depth=3"])).unwrap();
        assert_eq!(a, b);
        let Command::Graph { depth, all, .. } = a else { panic!() };
        assert_eq!(depth, Some(3));
        assert!(!all);

        let Command::Graph { depth, all, .. } = parse(args(&["graph", "a.md", "--all"])).unwrap()
        else {
            panic!()
        };
        assert_eq!(depth, None);
        assert!(all);

        assert!(parse(args(&["graph", "a.md", "--all", "--depth", "1"]))
            .unwrap_err()
            .contains("different things"));
        assert!(parse(args(&["graph", "a.md", "--depth", "lots"]))
            .unwrap_err()
            .contains("whole number"));
    }

    #[test]
    fn index_defaults_to_the_working_directory() {
        assert_eq!(
            parse(args(&["index"])).unwrap(),
            Command::Index { paths: vec![".".into()], cache: true }
        );
        assert_eq!(
            parse(args(&["index", "notes", "--no-cache"])).unwrap(),
            Command::Index { paths: vec!["notes".into()], cache: false }
        );
    }

    #[test]
    fn the_cache_is_on_unless_turned_off() {
        let Command::Graph { cache, .. } = parse(args(&["graph", "a.md"])).unwrap() else {
            panic!()
        };
        assert!(cache);
        let Command::Graph { cache, .. } =
            parse(args(&["graph", "a.md", "--no-cache"])).unwrap()
        else {
            panic!()
        };
        assert!(!cache);
    }

    #[test]
    fn errors_are_specific() {
        assert!(parse(args(&["graph"])).unwrap_err().contains("at least one path"));
        assert!(parse(args(&["graph", "a.md", "--format"])).unwrap_err().contains("needs a value"));
        assert!(parse(args(&["graph", "a.md", "-f", "yaml"])).unwrap_err().contains("unknown format"));
        assert!(parse(args(&["wat"])).unwrap_err().contains("unknown command"));
        assert!(parse(args(&["index", "--wat"])).unwrap_err().contains("unknown option"));
        assert!(parse(args(&["graph", "a.md", "--wat"])).unwrap_err().contains("unknown option"));
        assert!(parse(args(&["fmt"])).unwrap_err().contains("at least one path"));
        assert!(parse(args(&["fmt", "a.md", "--wat"])).unwrap_err().contains("unknown option"));
    }

    #[test]
    fn tui_collects_paths_depth_and_cache() {
        assert_eq!(
            parse(args(&["tui", "a.md"])).unwrap(),
            Command::Tui { paths: vec!["a.md".into()], cache: true, depth: None, all: false }
        );
        assert_eq!(
            parse(args(&["tui", "a.md", "--depth", "2", "--no-cache"])).unwrap(),
            Command::Tui { paths: vec!["a.md".into()], cache: false, depth: Some(2), all: false }
        );
        assert_eq!(
            parse(args(&["tui", "a.md", "--all"])).unwrap(),
            Command::Tui { paths: vec!["a.md".into()], cache: true, depth: None, all: true }
        );
    }

    #[test]
    fn tui_errors_match_graphs() {
        assert!(parse(args(&["tui"])).unwrap_err().contains("at least one path"));
        assert!(parse(args(&["tui", "a.md", "--wat"])).unwrap_err().contains("unknown option"));
        assert!(parse(args(&["tui", "a.md", "--depth"])).unwrap_err().contains("needs a value"));
        assert!(parse(args(&["tui", "a.md", "--all", "--depth", "1"]))
            .unwrap_err()
            .contains("different things"));
    }

    #[test]
    fn eval_collects_a_block_target() {
        assert_eq!(
            parse(args(&["eval", "a.md", "--block", "index"])).unwrap(),
            Command::Eval {
                paths: vec!["a.md".into()],
                target: EvalTarget::Block("index".into()),
                yes: false,
                no_write: false,
                cache: true,
            }
        );
        assert_eq!(
            parse(args(&["eval", "a.md", "--block=index"])).unwrap(),
            Command::Eval {
                paths: vec!["a.md".into()],
                target: EvalTarget::Block("index".into()),
                yes: false,
                no_write: false,
                cache: true,
            }
        );
    }

    #[test]
    fn eval_collects_all_yes_and_no_write() {
        assert_eq!(
            parse(args(&["eval", "a.md", "--all", "--yes", "--no-write"])).unwrap(),
            Command::Eval {
                paths: vec!["a.md".into()],
                target: EvalTarget::All,
                yes: true,
                no_write: true,
                cache: true,
            }
        );
    }

    #[test]
    fn eval_needs_a_target() {
        assert!(parse(args(&["eval", "a.md"]))
            .unwrap_err()
            .contains("`--block <name>`, `--all`, or `--list`"));
    }

    #[test]
    fn eval_list_needs_no_other_target() {
        assert_eq!(
            parse(args(&["eval", "a.md", "--list"])).unwrap(),
            Command::Eval {
                paths: vec!["a.md".into()],
                target: EvalTarget::List,
                yes: false,
                no_write: false,
                cache: true,
            }
        );
    }

    #[test]
    fn eval_list_defaults_to_the_working_directory() {
        assert_eq!(
            parse(args(&["eval", "--list"])).unwrap(),
            Command::Eval { paths: vec![".".into()], target: EvalTarget::List, yes: false, no_write: false, cache: true }
        );
    }

    #[test]
    fn eval_list_accepts_a_directory_or_several_paths() {
        assert_eq!(
            parse(args(&["eval", "notes", "--list"])).unwrap(),
            Command::Eval {
                paths: vec!["notes".into()],
                target: EvalTarget::List,
                yes: false,
                no_write: false,
                cache: true,
            }
        );
        assert_eq!(
            parse(args(&["eval", "a.md", "b.md", "--list", "--no-cache"])).unwrap(),
            Command::Eval {
                paths: vec!["a.md".into(), "b.md".into()],
                target: EvalTarget::List,
                yes: false,
                no_write: false,
                cache: false,
            }
        );
    }

    #[test]
    fn eval_list_conflicts_with_block_and_all() {
        assert!(parse(args(&["eval", "a.md", "--list", "--all"]))
            .unwrap_err()
            .contains("different things"));
        assert!(parse(args(&["eval", "a.md", "--list", "--block", "x"]))
            .unwrap_err()
            .contains("different things"));
    }

    #[test]
    fn eval_block_and_all_conflict() {
        assert!(parse(args(&["eval", "a.md", "--block", "x", "--all"]))
            .unwrap_err()
            .contains("different things"));
    }

    #[test]
    fn eval_refuses_more_than_one_path() {
        assert!(parse(args(&["eval", "a.md", "b.md", "--all"]))
            .unwrap_err()
            .contains("exactly one path"));
    }

    #[test]
    fn eval_needs_a_path() {
        assert!(parse(args(&["eval", "--all"])).unwrap_err().contains("needs a path"));
    }

    #[test]
    fn check_defaults_to_the_working_directory() {
        assert_eq!(parse(args(&["check"])).unwrap(), Command::Check { paths: vec![".".into()], cache: true });
        assert_eq!(
            parse(args(&["check", "notes", "--no-cache"])).unwrap(),
            Command::Check { paths: vec!["notes".into()], cache: false }
        );
    }
}
