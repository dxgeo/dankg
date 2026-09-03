# Eval session

The interactive `dankg eval` flow: print the plan, confirm, run, write
back. It lives in the library crate rather than `main.rs`, unlike
`graph`/`fmt`'s own orchestration. `run_one`, running one already-named
block end to end, is shared with `tui::eval`'s in-grid cycle-and-run,
and the TUI cannot depend on the `main` binary the other way around.
This module is the one place that sequence is implemented at all, so
the CLI's own multi-target loop and the TUI's single-block cycle agree
by construction about what "run this block" means.

```rust name=module_doc path=eval/session.rs
//! The interactive `dankg eval` flow: print the plan, confirm, run, write
//! back. It lives in the library, not `main.rs` (unlike `graph`/`fmt`'s
//! own orchestration), because `run_one` -- running one already-named
//! block end to end -- is shared with `tui::eval`'s in-grid
//! cycle-and-run. The TUI cannot depend on the `main` binary the other
//! way around.

use super::files::Files;
use super::plan::{self, BlockRef};
use super::result;
use super::run as eval_run;
use crate::config::Config;
use crate::diag::Diags;
use crate::graph::index::{self, Corpus};
use crate::md::{Document, Inline};
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalTarget {
    Block(String),
    /// Decision 21: the DAG's leaves only -- a pure dependency gets no
    /// redundant standalone spawn just for being named.
    All,
    /// Decision 21: every named block, dependency or not, each with its
    /// own recorded result -- the pre-decision-21 meaning of `--all`.
    Each,
    /// Explore, don't run: name, language, line, containing heading and
    /// configured-or-not for every top-level named block in the file.
    List,
}

/// What one call to [`run_one`] produced. Presentation (printing to
/// stderr, showing a status line, deciding an exit code) is entirely
/// the caller's job. This is just the data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub timed_out: bool,
}

/// `dankg eval <path> [--block NAME | --all | --list] [--yes] [--no-write]`.
/// `--list` walks whatever was named as a corpus (`list`, below). The
/// other two targets need exactly one path in `paths` (the CLI
/// guarantees this) and run through `run_single`, reading and
/// re-parsing just that file. This is not the whole corpus the way
/// `graph`/`index`/`tui` do, since `deps=` only resolves within one
/// file (decision 19).
pub fn run(paths: &[String], target: &EvalTarget, yes: bool, no_write: bool, cache: bool) -> Result<(), String> {
    if matches!(target, EvalTarget::List) {
        return list(paths, cache);
    }
    let path = paths.first().ok_or("`eval` needs a path")?;
    run_single(path, target, yes, no_write)
}

/// The root and `path`'s own root-relative form. This is what `Files`
/// keys everything on and what a block's `deps=` resolves cross-file
/// references relative to (the same root-relative shape a written link
/// already resolves against). Falls back to `path` itself, unchanged,
/// when it cannot be expressed relative to the discovered root at all.
/// A cross-file `deps=` would then simply fail to resolve anything,
/// exactly as if it had named a file that does not exist. This is no
/// worse than today's behaviour, since nothing before this feature ever
/// needed `path` in root-relative form.
fn locate(path: &str) -> (PathBuf, String) {
    let abs = index::absolute(Path::new(path));
    let root = index::discover_root(&abs)
        .unwrap_or_else(|| abs.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")));
    let rel = abs.strip_prefix(&root).map(index::to_slash).unwrap_or_else(|_| path.to_string());
    (root, rel)
}
```

`run_single` never partially acts on a plan. Every chain's interpreter
is resolved up front, and if even one target's language turns out to
be unconfigured, nothing runs at all, rather than running everything
except that one. The plan the reader confirmed is exactly what
happens, or nothing does. Targets are identified by *position* among
the entry file's own named top-level blocks, never by name. Decision 22
scoped name uniqueness to a heading, so two different targets in the
same loop can legally share a literal name. That position stays valid
across every write-back in the loop, since a result marker is never
itself a named block.

```rust name=run_single path=eval/session.rs
fn run_single(path: &str, target: &EvalTarget, yes: bool, no_write: bool) -> Result<(), String> {
    let (root, entry_file) = locate(path);
    let mut diags = Diags::new("dankg");

    let mut cfg_diags = Diags::new(".dankg/config");
    let config = Config::load(&root, &mut cfg_diags);
    diags.absorb(cfg_diags);

    let mut files = Files::new(root);
    files.discover(&entry_file, &mut diags)?;
    diags.sort();
    diags.emit();

    let blocks = files.all_blocks();
    let chains: Vec<Vec<BlockRef>> = match target {
        EvalTarget::List => unreachable!("dispatched to list() in run()"),
        EvalTarget::Block(name) => vec![plan::plan_for(&blocks, &entry_file, name).map_err(|e| e.to_string())?],
        EvalTarget::All => plan::plan_all(&blocks, &entry_file).map_err(|e| e.to_string())?,
        EvalTarget::Each => plan::plan_each(&blocks, &entry_file).map_err(|e| e.to_string())?,
    };
    if chains.is_empty() {
        eprintln!("nothing to run: {path} has no named top-level blocks");
        return Ok(());
    }

    // The plan is always printed before anything runs (decision 9), and
    // every chain's interpreter is resolved up front too. Eval either
    // runs everything it printed or, on any block with no configured
    // language, runs nothing at all, rather than partially acting on a
    // plan the reader already approved.
    let mut commands = Vec::with_capacity(chains.len());
    for chain in &chains {
        let target_name = chain.last().expect("plan_for/plan_all never return an empty chain").name;
        match eval_run::command_for(&config, chain) {
            Some(lang) => commands.push(lang),
            None => {
                let lang_name = chain.last().and_then(|b| b.lang).unwrap_or("(none)");
                return Err(format!(
                    "`{target_name}` is `{lang_name}`, which has no configured `[lang.{lang_name}] command`; refusing to run anything"
                ));
            }
        }
    }

    let total: usize = chains.iter().map(Vec::len).sum();
    eprintln!("will run ({total} block(s), in order):");
    for (chain, lang) in chains.iter().zip(&commands) {
        for b in chain {
            eprintln!("  {} [{}] {}:{}   via: {}", b.name, b.lang.unwrap_or("?"), b.file, b.line, lang.command);
        }
    }

    if !yes && !confirm()? {
        eprintln!("aborted; nothing run");
        return Ok(());
    }

    // `run_one` re-reads `path` from disk on every call and writes its
    // result back before returning. An earlier target's write-back can
    // grow or shrink the file, shifting every line number below it. That
    // shift is already reflected by the time the next target's own plan
    // is rebuilt, so no in-memory copy of the source needs to be threaded
    // through this loop for that to be correct.
    //
    // Targets are identified by their position among *`entry_file`'s own*
    // named top-level blocks, in document order, not by name. Decision 22
    // scoped name uniqueness to a heading, so two different targets in
    // this very loop can share a literal name, and `run_one` re-resolving
    // by name alone could not tell them apart, or could even run the
    // wrong one. That position stays valid across every write-back in
    // this loop, since a result marker is never itself a named block, so
    // it never changes how many named top-level blocks exist or their
    // relative order. Only their line numbers change, and `run_one`
    // re-derives those fresh from each re-parse anyway. A target is
    // always one of `entry_file`'s own blocks (`plan_for`/`plan_all`/
    // `plan_each` all scope target selection to it), never a cross-file
    // dependency pulled into its chain. Filtering to `entry_file` before
    // counting position is therefore exactly `plan_for_index`'s own
    // contract.
    let targets: Vec<(usize, String)> = chains
        .iter()
        .map(|c| {
            let target = c.last().unwrap();
            let position = blocks
                .iter()
                .filter(|b| b.file == entry_file)
                .position(|b| b.index == target.index)
                .expect("target came from entry_file's own blocks");
            (position, target.name.to_string())
        })
        .collect();
    let mut any_failed = false;

    for (position, name) in &targets {
        let summary = run_one(path, &config, *position, no_write)?;
        // stderr is shown but never stored (architecture.md, Execution).
        if !summary.stderr.is_empty() {
            eprint!("{}", summary.stderr);
        }
        if summary.timed_out {
            eprintln!("`{name}` timed out");
        } else if !summary.success {
            eprintln!("`{name}` exited with a failure");
        }
        any_failed |= !summary.success;
        if no_write {
            print!("{}", summary.stdout);
        }
    }

    if any_failed {
        Err("one or more blocks failed; the result(s) above record what happened".to_string())
    } else {
        Ok(())
    }
}
```

`--list` is the one target that runs nothing. This is exactly why it
alone is allowed to cover more than a single file. `--block`/`--all`/
`--each` stay scoped to one file because `deps=` only ever resolves
within it (decision 19). Listing has no such reason to hold back, so a
named directory walks the whole corpus the same way `graph`/`index`/
`check` already do.

```rust name=list_and_corpus_text path=eval/session.rs
/// `dankg eval <path>... --list`: lists every top-level named block
/// found under whatever was named. A single named file lists just its
/// own blocks. A directory (or several paths) walks the whole corpus
/// the way `graph`/`index`/`check` already do (decision 6) and lists
/// every file's own blocks, each block's line still prefixed by its own
/// file so multiple files stay distinguishable. `--block`/`--all` are
/// scoped to exactly one file, because `deps=` only resolves within one
/// (decision 19). Listing runs nothing, so nothing stops it from
/// covering everything the reader named.
fn list(paths: &[String], cache: bool) -> Result<(), String> {
    let mut diags = Diags::new("dankg");
    let corpus = index::load(paths, cache, &mut diags)?;
    diags.sort();
    diags.emit();

    match list_corpus_text(paths, &corpus) {
        Some(out) => print!("{out}"),
        None => eprintln!("no named top-level blocks found under {}", corpus.display),
    }
    Ok(())
}

/// `None` when nothing in the targeted set has a named top-level block at
/// all, so `list` can tell "found nothing" from "found it, it was empty"
/// without the caller re-deriving that from an empty string.
fn list_corpus_text(paths: &[String], corpus: &Corpus) -> Option<String> {
    // A directory names a corpus, not a file, so the only sensible
    // reading of "list here" is everything under it. This is the same
    // call `graph`/`tui` make for a named directory (architecture.md,
    // View Selection).
    let only_files = paths.iter().all(|p| !Path::new(p).is_dir());
    let targets: &[String] = if only_files { &corpus.entries } else { &corpus.paths };

    let mut out = String::new();
    let mut any = false;
    for rel_path in targets {
        let full = corpus.root.join(rel_path);
        let Ok(source) = fs::read_to_string(&full) else { continue };
        let mut file_diags = Diags::new(rel_path.as_str());
        let doc = Document::parse(&source, &mut file_diags);
        let blocks = plan::top_level_blocks(&doc, rel_path);
        if blocks.is_empty() {
            continue;
        }
        any = true;
        out.push_str(&list_blocks(rel_path, &blocks, &doc, &corpus.config));
    }
    any.then_some(out)
}
```

```rust name=list_blocks path=eval/session.rs
/// `dankg eval <path> --list`: every top-level named block, its
/// language, where it lives, whether that language is configured to run
/// at all, and which heading it falls under. This is the nearest
/// heading at or above the block's own line, since a block's containing
/// node is exactly the range `graph/build.rs` already computes a
/// heading's own extent to be. It is a cheap, self-contained
/// approximation of the same thing, without needing the whole graph
/// pipeline (root discovery, the corpus walk) just to answer "what can
/// I run here."
fn list_blocks(path: &str, blocks: &[BlockRef], doc: &Document, config: &Config) -> String {
    let mut out = String::new();
    if blocks.is_empty() {
        let _ = writeln!(out, "no named top-level blocks in {path}");
        return out;
    }
    let headings = doc.headings();
    for b in blocks {
        let heading = headings.iter().rfind(|(_, _, line)| *line <= b.line).map(|(_, inlines, _)| Inline::plain(inlines));
        let configured = match b.lang.and_then(|l| config.lang(l)) {
            Some(lang) => format!("configured: {}", lang.command),
            None => "unconfigured".to_string(),
        };
        let _ = write!(out, "  {} [{}] {path}:{}", b.name, b.lang.unwrap_or("?"), b.line);
        if let Some(h) = heading {
            let _ = write!(out, "   under \"{h}\"");
        }
        let _ = writeln!(out, "   {configured}");
    }
    out
}
```

`run_one` identifies its target purely by *position*, never by name,
for the same reason `run_single`'s own loop does above. Decision 22
scoped name uniqueness to a heading, so a caller re-resolving by name
alone could not always tell two same-named blocks apart, or worse,
could silently run the wrong one. The caller already knows exactly
which block it means: the one it just cycled to in the TUI, or just
planned in the CLI loop. That identity should not have to survive a
round trip through a string that might not be unique.

```rust name=run_one path=eval/session.rs
/// Runs `position` (an index into `path`'s named top-level blocks, in
/// document order, *not* a name) end to end: rebuilds its plan, resolves
/// its language, spawns its whole chain once, and (unless `no_write`)
/// writes the result back into `path`. This is the one place this
/// sequence is implemented, so `run`'s own multi-target loop and
/// `tui::eval`'s single-block cycle-and-run agree by construction about
/// what "run this block" means.
///
/// It uses position rather than name because decision 22 scoped name
/// uniqueness to a heading. Two different blocks in the same file can
/// legally share a literal name, and re-resolving one by name alone,
/// from no heading of its own to search from, is exactly `plan_for`'s
/// `AmbiguousTarget` case. The caller already knows which block it
/// means (the one it just cycled to, or just planned), and that
/// identity should not have to survive a round trip through a string
/// that might not be unique. The position stays valid across repeated
/// calls against the same file, even as earlier calls write results
/// back, because a result marker is never itself a named block, so it
/// never changes how many named top-level blocks exist or their
/// relative order.
pub fn run_one(path: &str, config: &Config, position: usize, no_write: bool) -> Result<RunSummary, String> {
    let (root, entry_file) = locate(path);
    let mut diags = Diags::new("dankg");
    let mut files = Files::new(root);
    files.discover(&entry_file, &mut diags)?;

    let blocks = files.all_blocks();
    let chain = plan::plan_for_index(&blocks, &entry_file, position).map_err(|e| e.to_string())?;
    let target = chain.last().expect("plan_for_index never returns an empty chain");
    let name = target.name;
    // Resolved before anything runs, the same "plan fully or not at all"
    // discipline `run_single` already applies to language configuration:
    // an unresolvable `xdeps` target should fail fast, not after paying
    // for a spawn that was going to be thrown away anyway.
    let xdep_hashes = result::xdep_hashes(&files, config, &blocks, &chain, &mut std::collections::HashMap::new())?;
    let lang = eval_run::command_for(config, &chain)
        .ok_or_else(|| format!("`{name}` has no configured language"))?;
    let timeout = Duration::from_secs(target.timeout.unwrap_or(eval_run::DEFAULT_TIMEOUT_SECS));
    let output = eval_run::run(&lang, &eval_run::concatenated_source(&chain), timeout)?;

    if !no_write {
        let (source, doc) = files.get(&entry_file).expect("entry_file was just discovered above");
        let result_hash = result::expected_hash(&chain, &lang.command, &xdep_hashes);
        let existing = result::locate_existing(doc, target.index, name);
        let updated =
            result::write_back(source, target.end_line, existing, name, result_hash, !output.success, &output.stdout);
        fs::write(path, &updated).map_err(|e| format!("{path}: {e}"))?;
    }

    Ok(RunSummary { stdout: output.stdout, stderr: output.stderr, success: output.success, timed_out: output.timed_out })
}

fn confirm() -> Result<bool, String> {
    eprint!("proceed? [y/N] ");
    io::stderr().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "Yes" | "YES"))
}
```

## Tests

```rust name=tests path=eval/session.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_file(name: &str, content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dankg-session-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    /// A fresh, isolated directory per call (a counter, not just the pid).
    /// `list_corpus_text` walks the whole directory it is given, so two
    /// tests sharing one scratch directory would see each other's files.
    fn scratch_dir(files: &[(&str, &str)]) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("dankg-session-list-test-{}-{n}", std::process::id()));
        for (name, content) in files {
            let path = dir.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
        }
        dir
    }

    #[test]
    fn list_blocks_reports_name_lang_heading_and_configuration() {
        let doc = Document::parse(
            "# Setup\n\n```sh name=a\n:\n```\n\n# Data\n\n```python name=b\n:\n```\n",
            &mut Diags::new("t"),
        );
        let blocks = plan::top_level_blocks(&doc, "t.md");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        let out = list_blocks("t.md", &blocks, &doc, &config);
        assert!(out.contains("a [sh] t.md:3   under \"Setup\"   configured: sh {file}"), "{out:?}");
        assert!(out.contains("b [python] t.md:9   under \"Data\"   unconfigured"), "{out:?}");
    }

    #[test]
    fn list_blocks_says_so_when_there_are_none() {
        let doc = Document::parse("# Empty\n", &mut Diags::new("t"));
        let blocks = plan::top_level_blocks(&doc, "t.md");
        let config = Config::none();
        let out = list_blocks("t.md", &blocks, &doc, &config);
        assert!(out.contains("no named top-level blocks"));
    }

    #[test]
    fn list_corpus_text_for_a_single_named_file_lists_only_that_file() {
        let dir = scratch_dir(&[
            ("a.md", "```sh name=x\n:\n```\n"),
            ("b.md", "```sh name=y\n:\n```\n"),
        ]);
        let paths = vec![dir.join("a.md").to_string_lossy().into_owned()];
        let mut diags = Diags::new("t");
        let corpus = index::load(&paths, true, &mut diags).unwrap();
        let out = list_corpus_text(&paths, &corpus).expect("a.md has a block");
        assert!(out.contains(" x "), "{out:?}");
        assert!(!out.contains(" y "), "naming one file should not list another's blocks: {out:?}");
    }

    #[test]
    fn list_corpus_text_for_a_directory_lists_every_file() {
        let dir = scratch_dir(&[
            ("a.md", "```sh name=x\n:\n```\n"),
            ("sub/b.md", "```sh name=y\n:\n```\n"),
        ]);
        let paths = vec![dir.to_string_lossy().into_owned()];
        let mut diags = Diags::new("t");
        let corpus = index::load(&paths, true, &mut diags).unwrap();
        let out = list_corpus_text(&paths, &corpus).expect("the corpus has blocks");
        assert!(out.contains(" x "), "{out:?}");
        assert!(out.contains(" y "), "a directory should list every file's blocks: {out:?}");
    }

    #[test]
    fn list_corpus_text_is_none_when_nothing_is_found() {
        let dir = scratch_dir(&[("a.md", "# Empty\n\nnothing to run\n")]);
        let paths = vec![dir.to_string_lossy().into_owned()];
        let mut diags = Diags::new("t");
        let corpus = index::load(&paths, true, &mut diags).unwrap();
        assert!(list_corpus_text(&paths, &corpus).is_none());
    }

    #[test]
    fn run_one_writes_the_result_back_to_disk() {
        let path = scratch_file("a.md", "```sh name=a\necho hi\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        let summary = run_one(path.to_str().unwrap(), &config, 0, false).unwrap();
        assert!(summary.success);
        assert_eq!(summary.stdout, "hi\n");
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("dankg:result name=a"), "{written:?}");
        assert!(written.contains("hi\n"), "{written:?}");
    }

    #[test]
    fn run_one_leaves_the_file_untouched_under_no_write() {
        let path = scratch_file("b.md", "```sh name=a\necho hi\n```\n");
        let before = fs::read_to_string(&path).unwrap();
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        run_one(path.to_str().unwrap(), &config, 0, true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn run_one_refuses_an_xdep_with_no_recorded_result_yet() {
        let path =
            scratch_file("xdep_missing.md", "```sh name=setup\necho hi\n```\n\n```sh name=top xdeps=setup\necho top\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        let err = run_one(path.to_str().unwrap(), &config, 1, false).unwrap_err();
        assert!(err.contains("setup"), "{err:?}");
        assert!(err.contains("run it first"), "{err:?}");
    }

    #[test]
    fn run_one_runs_an_xdeps_target_standalone_once_its_dependency_has_a_recorded_result() {
        let path =
            scratch_file("xdep_ok.md", "```sh name=setup\necho hi\n```\n\n```sh name=top xdeps=setup\necho top\n```\n");
        let config = Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        run_one(path.to_str().unwrap(), &config, 0, false).unwrap();
        let summary = run_one(path.to_str().unwrap(), &config, 1, false).unwrap();
        assert!(summary.success);
        assert_eq!(summary.stdout, "top\n", "setup's echo must never run as part of top's own process");
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("dankg:result name=top"), "{written:?}");
    }

    #[test]
    fn run_one_reports_an_unconfigured_language() {
        let path = scratch_file("c.md", "```python name=a\n:\n```\n");
        let config = Config::none();
        assert!(run_one(path.to_str().unwrap(), &config, 0, false).unwrap_err().contains("no configured language"));
    }
}
```
