//! Argument parsing.
//!
//! Hand-rolled, like everything else. The surface is small enough that a parser
//! crate would cost more than it saves.

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
  dankg --help
  dankg --version

options:
  --format <fmt>   json (default), html, dot, mermaid
  --depth <n>      hops from the entry to draw; default from [graph] depth
  --all            draw the whole index, not a view of it
  -o, --output     write to a file instead of stdout
  --no-cache       ignore .dankg/cache/ and write nothing back to it
  --check          report files not in normal form; write nothing

`tui` needs a real terminal and draws the same view `graph` would, with the
selected node's source line handed to `[editor] command` on enter (arrows or
hjkl to move, tab to reveal a node's hidden neighbours, enter to open, r to
collapse back to the entry view, q to quit).

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
        other if other.starts_with('-') => {
            return Err(format!("unknown option `{other}`"));
        }
        other => {
            return Err(format!(
                "unknown command `{other}` (expected `graph`, `index`, `fmt`, or `tui`)"
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
}
