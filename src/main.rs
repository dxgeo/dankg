use dankg::cli::{self, Command, Format};
use dankg::diag::{Diags, Level};
use dankg::eval::{plan, result, run as eval_run, session};
use dankg::graph::index::{self, Corpus};
use dankg::graph::{resolve, view, EdgeKind, Graph};
use dankg::layout;
use dankg::md::{fmt, Block, Document};
use dankg::render::{dot, html, json, mermaid};
use dankg::tui;
use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let command = match cli::parse(args) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("error: {message}\n");
            eprint!("{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };

    match command {
        Command::Help => {
            print!("{}", cli::USAGE);
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("dankg {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::Graph { paths, format, output, cache, depth, all } => {
            report(graph(&paths, format, output.as_deref(), cache, depth, all))
        }
        Command::Index { paths, cache } => report(index_report(&paths, cache)),
        Command::Tui { paths, cache, depth, all } => report(tui::run(&paths, cache, depth, all)),
        Command::Fmt { paths, check } => match format_files(&paths, check) {
            Ok(true) => ExitCode::SUCCESS,
            // `--check` is a gate: "some file is not in normal form" is a
            // non-zero exit, not an error.
            Ok(false) => ExitCode::from(1),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        },
        Command::Eval { paths, target, yes, no_write, cache } => {
            report(session::run(&paths, &target, yes, no_write, cache))
        }
        Command::Check { paths, cache } => match check_cmd(&paths, cache) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::from(1),
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        },
    }
}

fn report(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Rewrite each file into normal form, or report which ones would change.
///
/// Returns whether every file was already formatted (under `--check`) or was
/// successfully handled (otherwise).
fn format_files(paths: &[String], check: bool) -> Result<bool, String> {
    let mut diags = Diags::new("dankg");
    let mut clean = true;
    let mut changed = 0usize;

    for path in paths {
        let source = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;

        let mut file_diags = Diags::new(path);
        let doc = Document::parse(&source, &mut file_diags);
        let formatted = fmt::format(&doc);

        // Verify before writing, always. Round-tripping is the strongest test
        // this parser has, and a file on disk is not the place to discover a
        // formatter bug.
        if let Err(why) = fmt::verify(&doc, &formatted) {
            diags.warn_in(path, 1, format!("{why}; left unchanged"));
            diags.absorb(file_diags);
            clean = false;
            continue;
        }
        diags.absorb(file_diags);

        if formatted == source {
            continue;
        }
        changed += 1;
        if check {
            // The list of files is this command's output, so it goes to stdout.
            println!("{path}");
            clean = false;
        } else {
            fs::write(path, &formatted).map_err(|e| format!("{path}: {e}"))?;
        }
    }

    diags.sort();
    diags.emit();
    if check {
        eprintln!("{changed} of {} file(s) would be reformatted", paths.len());
    } else {
        eprintln!("{changed} of {} file(s) reformatted", paths.len());
    }
    Ok(clean)
}

/// `dankg check [<path>...]`: the CI gate. Unresolved links come from the
/// same whole-root index `graph`/`index` build; staleness is checked
/// separately per file, since `eval`'s dependency DAG (decision: scoped to
/// one file) has nothing to say across files either.
fn check_cmd(paths: &[String], cache: bool) -> Result<bool, String> {
    let mut diags = Diags::new("dankg");
    let corpus = index::load(paths, cache, &mut diags)?;
    let index_graph = resolve::resolve(&corpus.files, &mut diags);
    let unresolved = index_graph.nodes.iter().filter(|n| !n.resolved).count();

    let mut stale = 0usize;
    let mut checked = 0usize;
    for rel_path in &corpus.paths {
        let full = corpus.root.join(rel_path);
        let Ok(source) = fs::read_to_string(&full) else { continue };
        let mut file_diags = Diags::new(rel_path.as_str());
        let doc = Document::parse(&source, &mut file_diags);
        diags.absorb(file_diags);
        let blocks = plan::top_level_blocks(&doc);

        for b in &blocks {
            let Some((marker_line, _)) = result::locate_existing(&doc, b.index, b.name) else { continue };
            let _ = marker_line;
            let Some(Block::Passthrough { text, .. }) = doc.blocks.get(b.index + 1) else { continue };
            let Some((_, stored_hash, _)) = text.lines().next().and_then(result::parse_marker) else { continue };
            checked += 1;

            let Ok(chain) = plan::plan_for(&blocks, b.name) else {
                stale += 1;
                eprintln!("stale: {rel_path} `{}` (plan changed since this result was written)", b.name);
                continue;
            };
            // A language dropped from config since the result was written
            // cannot be re-verified; that is reported by the missing
            // `[lang.*]` section itself, not double-counted as stale here.
            let Some(lang) = eval_run::command_for(&corpus.config, &chain) else { continue };
            if result::expected_hash(&chain, &lang.command) != stored_hash {
                stale += 1;
                eprintln!("stale: {rel_path} `{}`", b.name);
            }
        }
    }

    diags.sort();
    diags.emit();
    eprintln!("root: {}", corpus.display);
    if unresolved > 0 {
        eprintln!("{unresolved} unresolved of {} node(s)", index_graph.nodes.len());
    }
    eprintln!("{stale} stale of {checked} eval result(s)");
    Ok(unresolved == 0 && stale == 0)
}

fn graph(
    paths: &[String],
    format: Format,
    output: Option<&str>,
    cache: bool,
    depth: Option<u32>,
    all: bool,
) -> Result<(), String> {
    let mut diags = Diags::new("dankg");
    let corpus = index::load(paths, cache, &mut diags)?;
    let index = resolve::resolve(&corpus.files, &mut diags);

    // JSON is the index, not a view of it: it is the scriptable surface, and a
    // consumer that asked for the graph should not get a fragment of it.
    let view = if format == Format::Json {
        None
    } else {
        let default_depth = corpus.config.depth(&mut diags);
        Some(view::select_view(&index, &corpus.entries, depth, all, default_depth))
    };
    let drawn = view.as_ref().unwrap_or(&index);

    let rendered = match format {
        Format::Json => {
            if depth.is_some() {
                diags.warn_in("dankg", 0, "`--depth` does not apply to `--format json`, which always emits the whole index");
            }
            json::render(&index)
        }
        Format::Dot => {
            let laid = layout::layout(drawn);
            dot::render(drawn, &laid)
        }
        Format::Mermaid => {
            let laid = layout::layout(drawn);
            mermaid::render(drawn, &laid)
        }
        Format::Html => {
            let laid = layout::layout(drawn);
            // The whole index goes into the page beside the drawn subgraph, so
            // expanding a node in the browser needs no second run and no
            // server. `corpus.entries` are already root-relative.
            let page = html::Page { root: &corpus.display, entries: &corpus.entries };
            html::render(drawn, &laid, &index, &page)
        }
    };

    match output {
        Some(path) => fs::write(path, &rendered).map_err(|e| format!("{path}: {e}"))?,
        None => {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            lock.write_all(rendered.as_bytes()).map_err(|e| e.to_string())?;
        }
    }

    diags.sort();
    diags.emit();
    summarize(&corpus, &index, view.as_ref(), &diags);
    Ok(())
}

/// One line each to stderr, so stdout stays a clean pipe.
fn summarize(corpus: &Corpus, index: &Graph, view: Option<&Graph>, diags: &Diags) {
    let unresolved = index.nodes.iter().filter(|n| !n.resolved).count();
    eprintln!("root: {}", corpus.display);
    eprintln!(
        "indexed: {} file(s), {} nodes, {} edges",
        corpus.stats.indexed,
        index.nodes.len(),
        index.edges.len()
    );
    if let Some(view) = view {
        if view.nodes.len() < index.nodes.len() {
            eprintln!("drawn: {} nodes, {} edges", view.nodes.len(), view.edges.len());
        }
    }
    if unresolved > 0 {
        eprintln!("{unresolved} unresolved of {} nodes", index.nodes.len());
    }
    let warnings = diags.count(Level::Warn);
    if warnings > 0 {
        eprintln!("{warnings} warning(s)");
    }
}

/// `dankg index`: what the walk found and what state the cache is in.
///
/// This is the command you run when the graph is not what you expected, so it
/// answers the two questions that produces -- which root am I in, and which
/// files did it decide were mine.
fn index_report(paths: &[String], cache: bool) -> Result<(), String> {
    let mut diags = Diags::new("dankg");
    let corpus = index::load(paths, cache, &mut diags)?;
    let graph = resolve::resolve(&corpus.files, &mut diags);

    let mut out = String::new();
    let _ = writeln!(
        out,
        "root:    {} ({})",
        corpus.display,
        if corpus.declared { "declared by .dankg/" } else { "derived from the given paths" }
    );
    let _ = writeln!(
        out,
        "config:  {}",
        corpus.config.source.as_deref().unwrap_or("none, defaults in use")
    );
    let _ = writeln!(
        out,
        "ignore:  {}",
        corpus.ignore.source.as_deref().unwrap_or("none, nothing excluded")
    );

    let stats = &corpus.stats;
    let _ = writeln!(
        out,
        "files:   {} indexed, {} ignored, {} skipped",
        stats.indexed, stats.ignored, stats.skipped
    );

    let unresolved = graph.nodes.iter().filter(|n| !n.resolved).count();
    let links = graph.edges.iter().filter(|e| e.kind == EdgeKind::Link).count();
    let reciprocated = graph.edges.iter().filter(|e| e.reciprocated).count();
    let _ = writeln!(out, "nodes:   {} ({unresolved} unresolved)", graph.nodes.len());
    let _ = writeln!(
        out,
        "edges:   {links} link ({reciprocated} reciprocated), {} contains",
        graph.edges.len() - links
    );

    match &corpus.cache_dir {
        Some(dir) => {
            let c = &stats.cache;
            let _ = writeln!(
                out,
                "cache:   {} -- {} reused, {} reparsed, {} orphaned, {} error(s)",
                index::to_slash(dir.strip_prefix(&corpus.root).unwrap_or(dir)),
                c.hits,
                c.misses,
                corpus.cache_orphans,
                c.errors
            );
        }
        None if cache => {
            let _ = writeln!(out, "cache:   off, this root has no .dankg/ to hold one");
        }
        None => {
            let _ = writeln!(out, "cache:   off, --no-cache");
        }
    }

    for entry in &corpus.entries {
        let _ = writeln!(out, "entry:   {entry}");
    }

    print!("{out}");
    diags.sort();
    diags.emit();
    Ok(())
}
