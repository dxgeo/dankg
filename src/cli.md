# CLI

Argument parsing is hand-rolled, like everything else in this crate
([decision 1](../architecture.md#decision-1-dependency-policy)). The
surface is small enough that a parser crate would cost more than it
saves.

```rust name=module_doc path=cli.rs
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
```

```rust name=command path=cli.rs
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Graph {
        paths: Vec<String>,
        format: Format,
        output: Option<String>,
        cache: bool,
        /// Hops from the entry. `None` falls back to `[graph] depth`.
        depth: Option<u32>,
        /// Skip view selection. Render the whole index.
        all: bool,
        /// Spawn every `[db.*]`'s own `list` and add a node for whatever
        /// it reports that the index does not already explain (decision
        /// 37). Never automatic: this is the explicit ask decision 9
        /// requires, and nothing it finds is cached or written back.
        live: bool,
    },
    Index { paths: Vec<String>, cache: bool },
    Fmt { paths: Vec<String>, check: bool },
    Tui { paths: Vec<String>, cache: bool, depth: Option<u32>, all: bool },
    /// `paths` holds exactly one entry for `Block`/`All`/`Each`. `deps=`
    /// only resolves within one file (decision 19). `List` takes any
    /// number of paths instead. It walks a corpus the way
    /// `graph`/`index`/`check` do, with no execution to scope. `if_stale`
    /// refuses to pair with `List`, which has no execution to skip.
    Eval { paths: Vec<String>, target: EvalTarget, yes: bool, no_write: bool, if_stale: bool, cache: bool },
    Check { paths: Vec<String>, cache: bool },
    /// `dankg tangle <path>... --lang LANG [-o DIR]`: assemble named blocks
    /// into a source tree (decisions 23-28). One file tangles just it,
    /// unchanged since decisions 23-25. A directory (or several paths)
    /// walks the corpus (decision 26), the same file-or-directory choice
    /// `--list` already offers.
    Tangle { paths: Vec<String>, lang: String, output: Option<String>, cache: bool },
    /// `dankg init [<path>]`: scaffolds a brand-new corpus at `path`
    /// (default `.`), creating it first if it does not exist. Exactly
    /// one path, never a corpus-wide walk -- unlike every
    /// `paths: Vec<String>` variant above, `init` never reads an
    /// existing corpus, only ever writes into one place.
    Init { path: String },
    Help,
    Version,
}
```

`USAGE` is both `--help`'s own output and the reference this project's
`README.md` deliberately does *not* duplicate. The README itself puts it
this way: "kept in sync with the binary, not duplicated here." This
string is the one canonical description of every command's behavior.

```rust name=usage path=cli.rs
pub const USAGE: &str = "\
dankg -- a plaintext knowledge grapher

usage:
  dankg graph <path>... [--format <fmt>] [--depth N | --all] [--live] [-o <file>]
  dankg index [<path>]  [--no-cache]
  dankg fmt   <path>... [--check]
  dankg tui   <path>... [--depth N | --all] [--no-cache]
  dankg eval  <path> [--block <name> | --all | --each] [--yes] [--no-write] [--if-stale]
  dankg eval  [<path>...] --list [--no-cache]
  dankg check [<path>...] [--no-cache]
  dankg tangle <path>... --lang <lang> [-o <dir>] [--no-cache]
  dankg init  [<path>]
  dankg --help
  dankg --version

options:
  --format <fmt>   json (default), html, dot, mermaid
  --depth <n>      hops from the entry to draw; default from [graph] depth
  --all            draw the whole index (graph/tui), or every eval DAG leaf
  --live           spawn every [db.*]'s own list command (graph only) and add
                   a node for whatever it reports that the corpus does not
                   already explain
  -o, --output     write to a file instead of stdout, or a directory (tangle)
  --no-cache       ignore .dankg/cache/ and write nothing back to it
  --check          report files not in normal form; write nothing
  --block <name>   the named block eval should run, with its dependencies
  --each           run every named block, dependency or not, each on its own
  --list           list every named block instead of running one -- a file
                   lists just its own, a directory (or several paths, the
                   default being \".\") walks the whole corpus
  --yes            skip eval's \"proceed?\" prompt
  --no-write       run and print output, but do not write results back
  --if-stale       skip a target already up to date; exits 0 before any
                   plan is printed or anything is asked or run
  --lang <lang>    which fence language tangle assembles

`tui` needs a real terminal and draws the whole corpus as a collapsible
tree, plus a cross-reference panel for the selected node's own links,
backlinks, and relations. The named file(s) start expanded to `[tui]
depth`/`--depth`/`--all` (unset falls back to a smaller default than
`[graph] depth`'s own, since a character grid has no zoom to fall back
on); every other file in the corpus still appears, collapsed to one
line. The selected node's source line is handed to `[editor] command`
on enter (arrows or hjkl to move; left/right collapse/expand a node or
step to its parent/first child; tab toggles focus between the tree and
the panel, where up/down move its own cursor, the tree previews
whichever link is under the cursor before you commit to it, and enter
makes that jump permanent; / to jump to a node by title anywhere in the
corpus, enter to confirm, esc to cancel; n/p jump to the next/previous
match of the last search; e to cycle a node's named blocks and enter to
run the cycled one in place; r to collapse back to the entry view; q to
quit; ? for a full-screen keybinding reference). The letter keys --
everything but the arrows, enter, tab, esc, `/`, `n`, `p`, and `?` --
are remappable in `[keys]`.

`graph`, `index`, and `tui` discover the root by walking up for a `.dankg/` directory,
falling back to the directory the named paths share, and then index every
markdown file under it. The named paths set the view; the index is always the
whole root, because backlinks are only honest when every file has been seen.

`--format json` always emits the whole index, which is what makes it the
scriptable surface; `--depth` and `--all` shape the drawn formats. Naming a
directory rather than a file draws everything, since the corpus is the entry.

`--live` (decision 37) is the one door `graph` opens to a database. Every
other run only ever displays what an earlier `dankg eval` already wrote
back. With `--live`, `graph` spawns each `[db.*]`'s own `list` command,
read-only, and adds a node for every relation it names that the corpus
does not already explain -- a table created by hand, by a tool `dankg`
never touched, or documented once by a section since deleted. A `[db.*]`
with no `list` configured is reported and skipped, the same allowlist
rule an unconfigured `[lang.*]` already gets. Nothing `--live` finds is
cached or written back: a second run may turn up a different orphan, or
none at all.

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
`--no-write` prints the captured output instead of writing it.
`--if-stale` checks a target's recorded hash against a fresh recomputation
first, the same comparison `check` already makes; already fresh, it prints as
much and exits 0, before the plan is printed, before the prompt, before
anything spawns. A target never run before has nothing recorded to compare
against, so it always counts as needing to run. `--each`/`--all` filter this
way per target, running whichever ones are left; naming every one fresh prints
as much and exits 0 too, with nothing left to run. A block's
name is unique across its whole file, and `deps=` resolves flat against
that same file -- any block, under any heading, can depend on any other.
`deps=other.md#name` reaches a block in another file, resolved relative to
the file that wrote it, the same way a written link's `other.md#heading`
resolves; a plain `deps=name` always stays local to whichever file declared
it. `--block`/`--all`/`--each` still take exactly one path -- only a
target's own name is looked up there, though its chain may then reach
outside it. `--all` runs only the blocks nothing else in the file depends
on; `--each` runs every named block, dependency or not, each with its own
recorded result -- the two differ only in which blocks run, never in how.
`--list` shows every named block -- name, language, source line, containing
heading, and whether its language is configured -- without running
anything, which is how to find a block's name in the first place before
naming it to `--block`. Unlike `--block`/`--all`/`--each`, `--list` has no
execution to scope: naming a file lists just that file's blocks, naming a
directory (or several paths, or nothing -- defaulting to \".\") walks the
whole corpus and lists every file's.

`check` is the CI gate: exits non-zero when the corpus has an unresolved
link or a written eval result whose hash no longer matches its current
source, dependencies, or configured command. Deliberately separate from
`graph`, so drafting a half-written note never fails a build.

`tangle` assembles named, top-level blocks -- exactly the ones eval can
run -- into a source tree, grouped by their containing top-level heading
(each becomes one file) rather than by `deps=`, which tangle never
consults. One file tangles just it; a directory (or several paths) walks
the corpus, nesting each contributing file under its own subdirectory once
more than one is involved. A block wanting a path outside that
heading-derived layout (a `Cargo.toml`) names one explicitly with `path=`.
`-o` names the output directory, defaulting to `.dankg/build/<lang>/`. Two
independent commands may run against the assembled tree, both substituting
`{dir}`: a configured `[tangle.<lang>] glue` first, an external,
per-language extension point for structural connective tissue (Rust's
`mod` declarations, say) that DanKG's own code deliberately never
generates itself; then `[tangle.<lang>] command`, for a language with a
real build step. Either, both, or neither may be configured -- omit both
and tangle stops at materializing the files, which is the whole operation
for a language with no separate compile step. Never automatic, the same as
`eval`, but with no confirm prompt: tangle does not run the reader's
program, only assembles and optionally builds it.

`init` scaffolds a brand-new corpus at `<path>` (default `.`),
creating the directory itself first if it does not exist: a `.dankg/`
directory with a commented starter `config`, a `.dankgignore`, and an
`index.md`. It refuses outright, writing nothing, when `<path>`
already has its own `.dankg/`. A `.dankg/` somewhere *above* `<path>`
is not this command's concern, though -- running `init` inside an
existing corpus on purpose makes a nested one, the same way
`example/example_1` nests inside DanKG's own repository. `index.md`
and `.dankgignore` are only ever written when not already there;
existing content elsewhere in `<path>` is never touched.

Diagnostics go to stderr, so stdout stays pipeable.
";
```

`parse` dispatches on the first argument. Then it hands the rest to each
subcommand's own parser. `graph`'s own flags stay inline here. `graph`
has no separate function of its own. It is both the default and the
most-used command.

```rust name=parse path=cli.rs
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
        "tangle" => return tangle(args),
        "init" => return init(args),
        other if other.starts_with('-') => {
            return Err(format!("unknown option `{other}`"));
        }
        other => {
            return Err(format!(
                "unknown command `{other}` (expected `graph`, `index`, `fmt`, `tui`, `eval`, `check`, `tangle`, or `init`)"
            ));
        }
    }

    let mut paths: Vec<String> = Vec::new();
    let mut format = Format::Json;
    let mut output = None;
    let mut cache = true;
    let mut depth = None;
    let mut all = false;
    let mut live = false;

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
            "--live" => live = true,
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
    Ok(Command::Graph { paths, format, output, cache, depth, all, live })
}

fn parse_depth(value: &str) -> Result<u32, String> {
    value.parse().map_err(|_| format!("`--depth` wants a whole number, not `{value}`"))
}
```

`index`, `fmt`, and `tui` each get their own small parser. Each follows
the same shape: collect known flags, collect everything else as a path,
refuse anything starting with `-` that was not recognised.

```rust name=index_fmt_tui path=cli.rs
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
```

`eval`'s own target logic is the one genuinely branchy parser here. Four
mutually exclusive modes collapse into a single `match` on which flags
were actually set. `--list` alone defaults its own path list to `.` and
accepts more than one path. It explores rather than evaluates.

```rust name=eval_parser path=cli.rs
/// `--block`/`--all` take exactly one path. `deps=` only resolves within
/// one file (`eval::plan`). Naming a second path has no meaning.
/// `--list` explores rather than evaluates. It walks a corpus the way
/// `graph`/`index`/`check` do, with any number of paths, defaulting to `.`.
fn eval<I: Iterator<Item = String>>(mut args: I) -> Result<Command, String> {
    let mut paths: Vec<String> = Vec::new();
    let mut block: Option<String> = None;
    let mut all = false;
    let mut each = false;
    let mut list = false;
    let mut yes = false;
    let mut no_write = false;
    let mut if_stale = false;
    let mut cache = true;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--block" | "-b" => {
                block = Some(args.next().ok_or("`--block` needs a value")?);
            }
            "--all" | "-a" => all = true,
            "--each" => each = true,
            "--list" | "-l" => list = true,
            "--yes" | "-y" => yes = true,
            "--no-write" => no_write = true,
            "--if-stale" => if_stale = true,
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

    let target = match (block, all, each, list) {
        (Some(name), false, false, false) => EvalTarget::Block(name),
        (None, true, false, false) => EvalTarget::All,
        (None, false, true, false) => EvalTarget::Each,
        (None, false, false, true) => EvalTarget::List,
        (None, false, false, false) => {
            return Err("`eval` needs `--block <name>`, `--all`, `--each`, or `--list`".to_string())
        }
        _ => return Err("`--block`, `--all`, `--each`, and `--list` ask for different things".to_string()),
    };

    if matches!(target, EvalTarget::List) {
        if paths.is_empty() {
            paths.push(".".to_string());
        }
        if if_stale {
            return Err("`--if-stale` and `--list` ask for different things".to_string());
        }
    } else {
        if paths.is_empty() {
            return Err("`eval` needs a path".to_string());
        }
        if paths.len() > 1 {
            return Err("`eval` takes exactly one path; `deps=` only resolves within one file".to_string());
        }
    }
    Ok(Command::Eval { paths, target, yes, no_write, if_stale, cache })
}
```

`tangle` and `check` both accept any number of paths or a bare directory.
The reasons are the opposite of why `eval`'s `--block`/`--all`/`--each`
are pinned to one path. Neither `tangle` nor `check` reads `deps=` at all
(decision 24). `check` walks the whole corpus regardless of what was
named (decision 6). Neither case has a single-file DAG forcing a
narrower shape.

```rust name=tangle_and_check path=cli.rs
/// One file tangles just it. A directory (or several paths) walks the
/// corpus (decision 26), the same choice `--list` already offers. Unlike
/// `--block`/`--all`/`--each`, tangle never reads `deps=` (decision 24).
/// There is no single-file DAG forcing this to stop at one path.
fn tangle<I: Iterator<Item = String>>(mut args: I) -> Result<Command, String> {
    let mut paths: Vec<String> = Vec::new();
    let mut lang: Option<String> = None;
    let mut output = None;
    let mut cache = true;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lang" => {
                lang = Some(args.next().ok_or("`--lang` needs a value")?);
            }
            "-o" | "--output" => {
                output = Some(args.next().ok_or("`--output` needs a value")?);
            }
            "--no-cache" => cache = false,
            "-h" | "--help" => return Ok(Command::Help),
            other if other.starts_with("--lang=") => {
                lang = Some(other["--lang=".len()..].to_string());
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            other => paths.push(other.to_string()),
        }
    }

    if paths.is_empty() {
        return Err("`tangle` needs a path".to_string());
    }
    let lang = lang.ok_or("`tangle` needs `--lang <lang>`")?;
    Ok(Command::Tangle { paths, lang, output, cache })
}

/// `check` takes any number of paths, defaulting to `.`. The corpus you
/// are standing in is the common case, same as `index`.
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

/// `init` takes an optional single path, defaulting to `.` -- the
/// same "the common case is right here" reasoning `index`/`check`
/// already give. Unlike either, at most one: `init` never walks a
/// corpus, only ever writes into the one place named.
fn init<I: Iterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut path: Option<String> = None;

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            _ if path.is_some() => {
                return Err(format!("`init` takes at most one path (already have `{}`)", path.unwrap()));
            }
            other => path = Some(other.to_string()),
        }
    }

    Ok(Command::Init { path: path.unwrap_or_else(|| ".".to_string()) })
}
```

## Tests

```rust name=tests path=cli.rs
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
                live: false,
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
    fn live_defaults_off_and_combines_with_depth_and_all() {
        let Command::Graph { live, .. } = parse(args(&["graph", "a.md"])).unwrap() else { panic!() };
        assert!(!live);

        let Command::Graph { live, depth, .. } = parse(args(&["graph", "a.md", "--live", "--depth", "2"])).unwrap()
        else {
            panic!()
        };
        assert!(live);
        assert_eq!(depth, Some(2));

        let Command::Graph { live, all, .. } = parse(args(&["graph", "a.md", "--all", "--live"])).unwrap() else {
            panic!()
        };
        assert!(live);
        assert!(all);
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
                if_stale: false,
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
                if_stale: false,
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
                if_stale: false,
                cache: true,
            }
        );
    }

    #[test]
    fn eval_needs_a_target() {
        assert!(parse(args(&["eval", "a.md"]))
            .unwrap_err()
            .contains("`--block <name>`, `--all`, `--each`, or `--list`"));
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
                if_stale: false,
                cache: true,
            }
        );
    }

    #[test]
    fn eval_list_defaults_to_the_working_directory() {
        assert_eq!(
            parse(args(&["eval", "--list"])).unwrap(),
            Command::Eval {
                paths: vec![".".into()],
                target: EvalTarget::List,
                yes: false,
                no_write: false,
                if_stale: false,
                cache: true,
            }
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
                if_stale: false,
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
                if_stale: false,
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
    fn eval_collects_each() {
        assert_eq!(
            parse(args(&["eval", "a.md", "--each"])).unwrap(),
            Command::Eval {
                paths: vec!["a.md".into()],
                target: EvalTarget::Each,
                yes: false,
                no_write: false,
                if_stale: false,
                cache: true,
            }
        );
    }

    #[test]
    fn eval_each_conflicts_with_all_and_block_and_list() {
        assert!(parse(args(&["eval", "a.md", "--each", "--all"]))
            .unwrap_err()
            .contains("different things"));
        assert!(parse(args(&["eval", "a.md", "--each", "--block", "x"]))
            .unwrap_err()
            .contains("different things"));
        assert!(parse(args(&["eval", "a.md", "--each", "--list"]))
            .unwrap_err()
            .contains("different things"));
    }

    #[test]
    fn eval_collects_if_stale() {
        assert_eq!(
            parse(args(&["eval", "a.md", "--block", "index", "--if-stale"])).unwrap(),
            Command::Eval {
                paths: vec!["a.md".into()],
                target: EvalTarget::Block("index".into()),
                yes: false,
                no_write: false,
                if_stale: true,
                cache: true,
            }
        );
    }

    #[test]
    fn eval_if_stale_conflicts_with_list() {
        assert!(parse(args(&["eval", "a.md", "--list", "--if-stale"]))
            .unwrap_err()
            .contains("different things"));
        assert!(parse(args(&["eval", "--if-stale", "--list"]))
            .unwrap_err()
            .contains("different things"));
    }

    #[test]
    fn tangle_collects_path_lang_and_output() {
        assert_eq!(
            parse(args(&["tangle", "a.md", "--lang", "rust"])).unwrap(),
            Command::Tangle { paths: vec!["a.md".into()], lang: "rust".into(), output: None, cache: true }
        );
        assert_eq!(
            parse(args(&["tangle", "a.md", "--lang=rust", "-o", "build"])).unwrap(),
            Command::Tangle {
                paths: vec!["a.md".into()],
                lang: "rust".into(),
                output: Some("build".into()),
                cache: true,
            }
        );
    }

    #[test]
    fn tangle_accepts_a_directory_or_several_paths() {
        assert_eq!(
            parse(args(&["tangle", "notes", "--lang", "rust"])).unwrap(),
            Command::Tangle { paths: vec!["notes".into()], lang: "rust".into(), output: None, cache: true }
        );
        assert_eq!(
            parse(args(&["tangle", "a.md", "b.md", "--lang", "rust", "--no-cache"])).unwrap(),
            Command::Tangle {
                paths: vec!["a.md".into(), "b.md".into()],
                lang: "rust".into(),
                output: None,
                cache: false,
            }
        );
    }

    #[test]
    fn tangle_needs_a_path_and_a_lang() {
        assert!(parse(args(&["tangle", "--lang", "rust"])).unwrap_err().contains("needs a path"));
        assert!(parse(args(&["tangle", "a.md"])).unwrap_err().contains("needs `--lang"));
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
```
