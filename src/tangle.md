# Tangle

`dankg tangle`: decisions 22-28. Assembles named, top-level blocks in one
target language into a source tree, grouped by *containment* rather than
`deps=` -- tangle never reads that attribute at all -- and hands the
result to a configured build command if one exists. This very file is
tangled by exactly the mechanism it implements: `src/tangle.rs` is
generated from this literate source the same way `src/hash.rs` is from
`src/hash.md`.

Scoped to one file or a whole corpus (decision 26), the same file-or-
directory choice `--list` already offers: naming one file tangles just
it, exactly as it always has; naming a directory (or several paths) walks
the corpus the way `graph`/`index`/`check` do. A single named file skips
`index::load` entirely and never triggers the whole-root walk or graph
build `index::load` does -- the same reason `eval`'s own single-file path
avoids it (decision 19): nothing here needs to know about any file but
the one asked for.

```rust name=module_doc path=tangle.rs
//! `dankg tangle`: decisions 22-28. Assembles named, top-level blocks in one
//! target language into a source tree, grouped by containment rather than
//! `deps=` -- tangle never reads that attribute at all -- and hands the
//! result to a configured build command if one exists.
//!
//! Scoped to one file or a whole corpus (decision 26), the same file-or-
//! directory choice `--list` already offers: naming one file tangles just
//! it, exactly as before; naming a directory (or several paths) walks the
//! corpus the way `graph`/`index`/`check` do. A single named file skips
//! `index::load` entirely and never triggers the whole-root walk or graph
//! build `index::load` does -- the same reason `eval`'s own single-file
//! path avoids it (decision 19): nothing here needs to know about any file
//! but the one asked for.

use crate::cmd;
use crate::config::Config;
use crate::diag::Diags;
use crate::eval::plan::{self, BlockRef};
use crate::graph::build::strip_extension;
use crate::graph::index;
use crate::graph::slug::Slugger;
use crate::md::{Document, Inline};
use crate::render::json::string as json_string;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Report {
    pub dir: PathBuf,
    pub files: Vec<PathBuf>,
    pub ran_glue: bool,
    pub ran_command: bool,
}

/// One file already read and parsed, kept alive for the rest of `run` since
/// every `BlockRef` borrows from its `Document`.
struct Source {
    /// Shown in banners and the manifest: the exact path for a single named
    /// file, root-relative for a corpus walk.
    display: String,
    doc: Document,
}
```

`run` opens with the same single-file-versus-corpus branch `--list`
already draws, then narrows to exactly the blocks `dankg eval` could run
\-- `plan::top_level_blocks`, never `Document::named_blocks`'s
list-descending walk, for the identical reason eval itself does not use
it. Nesting only ever appears once more than one file actually
contributes to the same target language's tree; naming a single file,
even out of a much larger corpus with nothing else in that language,
tangles exactly as it always did, with no extra subdirectory to explain.

```rust name=run path=tangle.rs
/// `dankg tangle <path>... --lang <lang> [-o <dir>]`.
pub fn run(paths: &[String], lang: &str, output: Option<&str>, cache: bool) -> Result<Report, String> {
    let (root, config, sources) = match paths {
        [only] if !Path::new(only).is_dir() => {
            let source = fs::read_to_string(only).map_err(|e| format!("{only}: {e}"))?;
            let mut diags = Diags::new(only);
            let doc = Document::parse(&source, &mut diags);
            let root = index::discover_root(Path::new(only))
                .unwrap_or_else(|| Path::new(only).parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")));
            let mut cfg_diags = Diags::new(".dankg/config");
            let config = Config::load(&root, &mut cfg_diags);
            diags.absorb(cfg_diags);
            diags.sort();
            diags.emit();
            (root, config, vec![Source { display: only.clone(), doc }])
        }
        _ => {
            let mut diags = Diags::new("dankg");
            let corpus = index::load(paths, cache, &mut diags)?;
            diags.sort();
            diags.emit();

            // A directory names a corpus; several explicit files name just
            // themselves -- the same distinction `session::list_corpus_text`
            // already draws for `eval --list`.
            let only_files = paths.iter().all(|p| !Path::new(p).is_dir());
            let targets: Vec<String> = if only_files { corpus.entries } else { corpus.paths };

            let mut sources = Vec::with_capacity(targets.len());
            for rel_path in targets {
                let full = corpus.root.join(&rel_path);
                let Ok(source) = fs::read_to_string(&full) else { continue };
                let mut file_diags = Diags::new(rel_path.as_str());
                let doc = Document::parse(&source, &mut file_diags);
                sources.push(Source { display: rel_path, doc });
            }
            (corpus.root, corpus.config, sources)
        }
    };

    // Decision 23: exactly the blocks eval could run, further narrowed to
    // one language -- never `Document::named_blocks`'s list-descending walk,
    // for the same reason eval does not use it either.
    let mut wanted: Vec<(&Source, Vec<&BlockRef>, Vec<plan::Heading>)> = Vec::new();
    let all_blocks: Vec<Vec<BlockRef>> = sources.iter().map(|s| plan::top_level_blocks(&s.doc, &s.display)).collect();
    for (source, blocks) in sources.iter().zip(&all_blocks) {
        let matching: Vec<&BlockRef> = blocks.iter().filter(|b| b.lang == Some(lang)).collect();
        if !matching.is_empty() {
            wanted.push((source, matching, source.doc.headings()));
        }
    }

    let tangle_cfg = config.tangle(lang);
    let ext = tangle_cfg.as_ref().and_then(|t| t.ext.clone()).unwrap_or_else(|| lang.to_string());
    let out_dir = match output {
        Some(dir) => PathBuf::from(dir),
        None => root.join(".dankg").join("build").join(lang),
    };

    if wanted.is_empty() {
        return Ok(Report { dir: out_dir, files: Vec::new(), ran_glue: false, ran_command: false });
    }

    // A file's own subdirectory only appears once more than one file is
    // actually contributing to this tree -- naming one file, even out of a
    // larger corpus with nothing else in this language, tangles exactly as
    // it always has, with no extra nesting to explain.
    let multi = wanted.len() > 1;

    let mut written = Vec::new();
    let mut manifest: Vec<ManifestEntry> = Vec::new();
    for (source, blocks, headings) in &wanted {
        let file_prefix = if multi { strip_extension(&source.display) } else { String::new() };
        let public = source.doc.frontmatter.tangle_public();
        let groups = group_by_file(blocks, headings, &ext, &file_prefix);
        for (rel_path, heading_title, group_blocks) in &groups {
            let content = render_file(&source.display, heading_title.as_deref(), rel_path, group_blocks);
            let file_path = out_dir.join(rel_path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            fs::write(&file_path, content).map_err(|e| format!("{}: {e}", file_path.display()))?;
            written.push(file_path);
            let entry_blocks = group_blocks
                .iter()
                .map(|b| BlockEntry { name: b.name.to_string(), line: b.line, end_line: b.end_line })
                .collect();
            manifest.push(ManifestEntry {
                path: rel_path.clone(),
                source: source.display.clone(),
                public,
                blocks: entry_blocks,
            });
        }
    }

    let mut ran_glue = false;
    let mut ran_command = false;
    if let Some(cfg) = &tangle_cfg {
        if cfg.glue.is_some() {
            write_manifest(&out_dir, &manifest)?;
        }
        // Glue first, build second: a build command needs the structural
        // connective tissue glue adds (decision 27) already in place.
        if let Some(template) = &cfg.glue {
            spawn_against_dir(template, &out_dir, "glue")?;
            ran_glue = true;
        }
        if let Some(template) = &cfg.command {
            spawn_against_dir(template, &out_dir, "command")?;
            ran_command = true;
        }
    }

    Ok(Report { dir: out_dir, files: written, ran_glue, ran_command })
}

fn spawn_against_dir(template: &str, out_dir: &Path, key: &str) -> Result<(), String> {
    let dir = out_dir.to_string_lossy();
    let Some(argv) = cmd::build(template, &[("dir", &dir)]) else {
        return Err(format!("`[tangle.*] {key}` is empty once substituted"));
    };
    let status = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .status()
        .map_err(|e| format!("could not run `{}`: {e}", argv[0]))?;
    if !status.success() {
        return Err(format!("`[tangle.*] {key}` exited with a failure"));
    }
    Ok(())
}
```

The manifest is never read by DanKG's own code -- only ever produced, for
an external `glue` program that chooses to consume it (decision 28). Its
`blocks` field carries only line ranges, straight off the same
`BlockRef` fields `eval::result` already uses for write-back positioning
\-- never the prose those lines sit near, which stays a glue script's own
judgment to make (decision 27), never DanKG's.

```rust name=manifest_types_and_write path=tangle.rs
/// One tangled file, for the sidecar manifest a `glue` command may read
/// (decision 28). Never read by DanKG's own code -- only ever produced, for
/// an external program that chooses to consume it.
struct ManifestEntry {
    /// Relative to the tangle output directory.
    path: String,
    /// Root-relative (or exactly as named) source `.md` path.
    source: String,
    /// `dankg.tangle.public` from that source file's frontmatter.
    public: bool,
    /// This file's own contributing blocks, in document order (version 2).
    /// Line ranges only, straight off the same `BlockRef` fields
    /// `result.rs` already uses for write-back positioning -- not the
    /// prose those lines sit near, which stays a glue script's own
    /// judgment to make (decision 27), never DanKG's.
    blocks: Vec<BlockEntry>,
}

/// One block's position in its *source* file -- `line`/`end_line` are the
/// opening and closing fence lines, exactly as `plan::BlockRef` carries
/// them, unchanged by wherever tangle placed the block's own output.
struct BlockEntry {
    name: String,
    line: u32,
    end_line: u32,
}

const MANIFEST_NAME: &str = ".dankg-tangle-manifest.json";

/// Written only when a `glue` command is configured: a language with no
/// glue step never gets an extra file cluttering its output, and nothing
/// in DanKG itself ever reads this back.
fn write_manifest(out_dir: &Path, entries: &[ManifestEntry]) -> Result<(), String> {
    let mut out = String::from("{\n  \"version\": 2,\n  \"files\": [\n");
    for (i, e) in entries.iter().enumerate() {
        out.push_str("    {\"path\": ");
        out.push_str(&json_string(&e.path));
        out.push_str(", \"source\": ");
        out.push_str(&json_string(&e.source));
        out.push_str(&format!(", \"public\": {}, \"blocks\": [", e.public));
        for (j, b) in e.blocks.iter().enumerate() {
            out.push_str("{\"name\": ");
            out.push_str(&json_string(&b.name));
            out.push_str(&format!(", \"line\": {}, \"end_line\": {}}}", b.line, b.end_line));
            if j + 1 < e.blocks.len() {
                out.push_str(", ");
            }
        }
        out.push_str("]}");
        out.push_str(if i + 1 < entries.len() { ",\n" } else { "\n" });
    }
    out.push_str("  ]\n}\n");
    let path = out_dir.join(MANIFEST_NAME);
    fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))
}
```

```rust name=comment_and_extension path=tangle.rs
/// A file-extension comment marker, for the "generated, do not edit" banner
/// every tangled file opens with. Keyed by the *output file's* extension,
/// not `--lang`: a `path=` block can redirect anywhere -- a `rust`-fenced
/// block naming `path=Cargo.toml` still has to open with a `#`, since the
/// destination is TOML, whatever fence produced it. A short,
/// hand-maintained table rather than a guess: an unlisted extension falls
/// back to `#`, which is at least a comment in more languages than any
/// other single choice.
fn comment_prefix(ext: &str) -> &'static str {
    match ext {
        "rs" | "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "java" | "go" | "js" | "jsx" | "ts" | "tsx" => "//",
        _ => "#",
    }
}

/// The part of `rel_path` after its last `.`, or "" for an extensionless
/// path -- which just means [`comment_prefix`] falls back to `#`.
fn extension(rel_path: &str) -> &str {
    rel_path.rsplit('/').next().unwrap_or(rel_path).rsplit_once('.').map(|(_, ext)| ext).unwrap_or("")
}

/// Every heading's assigned slug, by line -- the same per-file `Slugger`
/// `graph/build.rs` runs, so a tangled file's name matches the graph node's
/// own anchor for that heading, and two same-titled headings still tangle
/// to two distinct files instead of silently merging.
fn heading_slugs(headings: &[plan::Heading]) -> HashMap<u32, String> {
    let mut slugger = Slugger::new();
    headings.iter().map(|(_, inlines, line)| (*line, slugger.assign(&Inline::plain(inlines)))).collect()
}

fn heading_title(headings: &[plan::Heading], line: u32) -> String {
    headings
        .iter()
        .find(|(_, _, l)| *l == line)
        .map(|(_, inlines, _)| Inline::plain(inlines))
        .unwrap_or_default()
}
```

`placement` is decision 24 made concrete: containment decides where a
block lands, `deps=` is never even consulted. An explicit `path=` always
lands directly under the output directory, ignoring `file_prefix`
entirely -- an escape hatch that only escaped *partway* (still nested
under this file's own subdirectory) would not be much of one for
something like a corpus-wide `Cargo.toml`, which needs to sit at the true
root of the assembled tree regardless of how many other files are
tangling alongside it.

```rust name=placement path=tangle.rs
/// `b`'s output path, relative to the tangle output directory, and the
/// title of the heading it came from (`None` for the file-level scope, or
/// for a block naming its own `path=`, since neither has one heading to
/// credit). Decision 24: containment decides this, never `deps=`. An
/// explicit `path=` always lands directly under the output directory,
/// ignoring `file_prefix` entirely -- an escape hatch that only escaped
/// partway (still nested under this file's own subdirectory) would not be
/// much of one for something like a corpus-wide `Cargo.toml`.
fn placement(
    b: &BlockRef,
    headings: &[plan::Heading],
    slugs: &HashMap<u32, String>,
    ext: &str,
    file_prefix: &str,
) -> (String, Option<String>) {
    if let Some(p) = b.path {
        return (p.to_string(), None);
    }
    let heading = plan::containing_heading(headings, b.line);
    let (rel, title) = match plan::root_heading(headings, heading) {
        None => (format!("main.{ext}"), None),
        Some(line) => {
            let slug = slugs.get(&line).cloned().unwrap_or_default();
            let ident = slug.replace('-', "_");
            (format!("{ident}.{ext}"), Some(heading_title(headings, line)))
        }
    };
    let full = if file_prefix.is_empty() { rel } else { format!("{file_prefix}/{rel}") };
    (full, title)
}

/// Groups `blocks` by [`placement`], preserving the order each distinct
/// path was first seen in -- document order, since `blocks` is already in
/// document order and this only ever appends to an existing group.
fn group_by_file<'a>(
    blocks: &[&'a BlockRef<'a>],
    headings: &[plan::Heading],
    ext: &str,
    file_prefix: &str,
) -> Vec<(String, Option<String>, Vec<&'a BlockRef<'a>>)> {
    let slugs = heading_slugs(headings);
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, (Option<String>, Vec<&BlockRef>)> = HashMap::new();

    for &b in blocks {
        let (rel_path, title) = placement(b, headings, &slugs, ext, file_prefix);
        let entry = groups.entry(rel_path.clone()).or_insert_with(|| {
            order.push(rel_path.clone());
            (title, Vec::new())
        });
        entry.1.push(b);
    }

    order.into_iter().map(|p| {
        let (title, blocks) = groups.remove(&p).expect("just inserted");
        (p, title, blocks)
    }).collect()
}
```

```rust name=render_file path=tangle.rs
/// A block's own `source` (`plan::block_ref`) already ends in exactly one
/// `\n` -- `block.rs` pushes one after every content line, including the
/// last -- so joining consecutive blocks with one more reproduces the same
/// "exactly one blank line" shape `result.rs`'s write-back already commits
/// to, rather than butting two blocks' code directly against each other
/// with nothing to mark where one substitution ends and the next begins.
fn render_file(source_path: &str, heading_title: Option<&str>, rel_path: &str, blocks: &[&BlockRef]) -> String {
    let comment = comment_prefix(extension(rel_path));
    let mut out = String::new();
    match heading_title {
        Some(title) => {
            out.push_str(&format!("{comment} generated by `dankg tangle` from {source_path} (\"{title}\") -- do not edit\n\n"));
        }
        None => out.push_str(&format!("{comment} generated by `dankg tangle` from {source_path} -- do not edit\n\n")),
    }
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(b.source);
    }
    out
}
```

## Tests

```rust name=tests path=tangle.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch(files: &[(&str, &str)]) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("dankg-tangle-test-{}-{n}", std::process::id()));
        for (name, content) in files {
            let path = dir.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut f = fs::File::create(&path).unwrap();
            f.write_all(content.as_bytes()).unwrap();
        }
        dir
    }

    fn one(path: &str) -> Vec<String> {
        vec![path.to_string()]
    }

    #[test]
    fn a_level_one_heading_becomes_one_file() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\n"),
            (
                "a.md",
                "# Parsing\n\n```rust name=lex\nfn lex() {}\n```\n\n```rust name=parse\nfn parse() {}\n```\n",
            ),
        ]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        assert_eq!(report.files, vec![out.join("parsing.rs")]);
        let content = fs::read_to_string(out.join("parsing.rs")).unwrap();
        assert!(content.contains("fn lex"), "{content}");
        assert!(content.contains("fn parse"), "{content}");
        assert!(content.contains("generated by `dankg tangle`"), "{content}");
        assert!(content.contains("\"Parsing\""), "{content}");
    }

    #[test]
    fn consecutive_blocks_in_one_file_get_a_blank_line_between_them() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\n"),
            (
                "a.md",
                "# Parsing\n\n```rust name=lex\nfn lex() {}\n```\n\n```rust name=parse\nfn parse() {}\n```\n",
            ),
        ]);
        let out = dir.join("out");
        run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        let content = fs::read_to_string(out.join("parsing.rs")).unwrap();
        assert!(content.contains("fn lex() {}\n\nfn parse() {}\n"), "{content}");
    }

    #[test]
    fn two_headings_become_two_files() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\n"),
            (
                "a.md",
                "# One\n\n```rust name=a\nfn a() {}\n```\n\n# Two\n\n```rust name=b\nfn b() {}\n```\n",
            ),
        ]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        let mut names: Vec<_> = report.files.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, vec!["one.rs".to_string(), "two.rs".to_string()]);
    }

    #[test]
    fn a_nested_heading_folds_into_its_top_level_ancestor() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\n"),
            ("a.md", "# One\n\n## Sub\n\n```rust name=a\nfn a() {}\n```\n"),
        ]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        assert_eq!(report.files, vec![out.join("one.rs")]);
    }

    #[test]
    fn a_block_before_any_heading_goes_to_main() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\n"),
            ("a.md", "```rust name=a\nfn a() {}\n```\n\n# One\n"),
        ]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        assert_eq!(report.files, vec![out.join("main.rs")]);
    }

    #[test]
    fn path_overrides_the_heading_derived_default() {
        let dir = scratch(&[(
            "a.md",
            "# One\n\n```toml name=manifest path=Cargo.toml\n[package]\nname = \"x\"\n```\n",
        )]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "toml", Some(out.to_str().unwrap()), true).unwrap();
        assert_eq!(report.files, vec![out.join("Cargo.toml")]);
    }

    #[test]
    fn the_banner_comment_follows_the_destination_extension_not_lang() {
        // A `rust`-fenced block naming `path=Cargo.toml` still lands in a
        // TOML file: the banner has to open with `#`, not `//`, regardless
        // of which fence produced it.
        let dir = scratch(&[(
            "a.md",
            "```rust name=manifest path=Cargo.toml\n[package]\nname = \"x\"\n```\n",
        )]);
        let out = dir.join("out");
        run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        let content = fs::read_to_string(out.join("Cargo.toml")).unwrap();
        assert!(content.starts_with("# generated"), "{content:?}");
    }

    #[test]
    fn a_block_nested_in_a_list_is_not_tangled() {
        let dir = scratch(&[("a.md", "# One\n\n- ```rust name=hidden\n  fn hidden() {}\n  ```\n")]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        assert!(report.files.is_empty(), "{:?}", report.files);
    }

    #[test]
    fn a_different_language_block_is_skipped() {
        let dir = scratch(&[("a.md", "# One\n\n```python name=a\nx = 1\n```\n")]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        assert!(report.files.is_empty());
    }

    #[test]
    fn defaults_to_a_dankg_build_directory_under_the_root() {
        let dir = scratch(&[(".dankg/config", ""), ("a.md", "```rust name=a\nfn a() {}\n```\n")]);
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "rust", None, true).unwrap();
        assert_eq!(report.dir, dir.join(".dankg").join("build").join("rust"));
    }

    #[test]
    fn ext_falls_back_to_the_lang_name_when_unconfigured() {
        let dir = scratch(&[("a.md", "```ocaml name=a\nlet a = 1\n```\n")]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "ocaml", Some(out.to_str().unwrap()), true).unwrap();
        assert_eq!(report.files, vec![out.join("main.ocaml")]);
    }

    #[test]
    fn a_configured_ext_is_used_instead() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\n"),
            ("a.md", "```rust name=a\nfn a() {}\n```\n"),
        ]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        assert_eq!(report.files, vec![out.join("main.rs")]);
    }

    #[test]
    fn a_configured_command_runs_against_the_assembled_directory() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.sh]\ncommand = sh -c 'touch {dir}/built'\next = sh\n"),
            ("a.md", "```sh name=a\necho hi\n```\n"),
        ]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "sh", Some(out.to_str().unwrap()), true).unwrap();
        assert!(report.ran_command);
        assert!(out.join("built").exists());
    }

    #[test]
    fn no_command_configured_stops_at_materializing_files() {
        let dir = scratch(&[("a.md", "```python name=a\nx = 1\n```\n")]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "python", Some(out.to_str().unwrap()), true).unwrap();
        assert!(!report.ran_command);
        assert!(!report.files.is_empty());
    }

    #[test]
    fn a_directory_walks_the_whole_corpus() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\n"),
            ("a.md", "# One\n\n```rust name=a\nfn a() {}\n```\n"),
            ("sub/b.md", "# Two\n\n```rust name=b\nfn b() {}\n```\n"),
        ]);
        let out = dir.join("out");
        let report = run(&[dir.to_string_lossy().into_owned()], "rust", Some(out.to_str().unwrap()), true).unwrap();
        let mut rel: Vec<String> = report
            .files
            .iter()
            .map(|p| p.strip_prefix(&out).unwrap().to_string_lossy().into_owned())
            .collect();
        rel.sort();
        assert_eq!(rel, vec!["a/one.rs".to_string(), "sub/b/two.rs".to_string()]);
    }

    #[test]
    fn naming_one_file_out_of_a_larger_corpus_does_not_nest_it() {
        // Only `a.md` has rust blocks -- tangling it directly, even though
        // `sub/b.md` exists alongside it, should look exactly like tangling
        // it alone: no per-file subdirectory for a single contributor.
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\n"),
            ("a.md", "# One\n\n```rust name=a\nfn a() {}\n```\n"),
            ("sub/b.md", "# Two\n\n```python name=b\nx = 1\n```\n"),
        ]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        assert_eq!(report.files, vec![out.join("one.rs")]);
    }

    #[test]
    fn several_named_files_tangle_only_themselves_not_the_whole_corpus() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\n"),
            ("a.md", "# One\n\n```rust name=a\nfn a() {}\n```\n"),
            ("b.md", "# Two\n\n```rust name=b\nfn b() {}\n```\n"),
            ("c.md", "# Three\n\n```rust name=c\nfn c() {}\n```\n"),
        ]);
        let out = dir.join("out");
        let a = dir.join("a.md").to_string_lossy().into_owned();
        let b = dir.join("b.md").to_string_lossy().into_owned();
        let report = run(&[a, b], "rust", Some(out.to_str().unwrap()), true).unwrap();
        let mut rel: Vec<String> = report
            .files
            .iter()
            .map(|p| p.strip_prefix(&out).unwrap().to_string_lossy().into_owned())
            .collect();
        rel.sort();
        assert_eq!(rel, vec!["a/one.rs".to_string(), "b/two.rs".to_string()], "c.md was never named");
    }

    #[test]
    fn glue_runs_before_command_and_the_manifest_is_written_only_when_glue_is_configured() {
        let dir = scratch(&[
            (
                ".dankg/config",
                "[tangle.rust]\next = rs\nglue = sh -c 'touch {dir}/glued'\ncommand = sh -c 'test -f {dir}/glued && touch {dir}/built'\n",
            ),
            ("a.md", "```rust name=a\nfn a() {}\n```\n"),
        ]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        assert!(report.ran_glue);
        assert!(report.ran_command);
        assert!(out.join("glued").exists());
        assert!(out.join("built").exists(), "command should see glue's output already in place");
        assert!(out.join(MANIFEST_NAME).exists());
    }

    #[test]
    fn no_manifest_when_no_glue_is_configured() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\ncommand = sh -c 'touch {dir}/built'\n"),
            ("a.md", "```rust name=a\nfn a() {}\n```\n"),
        ]);
        let out = dir.join("out");
        let report = run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        assert!(!report.ran_glue);
        assert!(!out.join(MANIFEST_NAME).exists());
    }

    #[test]
    fn manifest_carries_the_source_files_public_hint() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\nglue = sh -c 'true'\n"),
            (
                "a.md",
                "---\ndankg.tangle.public: true\n---\n```rust name=a\nfn a() {}\n```\n",
            ),
        ]);
        let out = dir.join("out");
        run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        let manifest = fs::read_to_string(out.join(MANIFEST_NAME)).unwrap();
        assert!(manifest.contains("\"public\": true"), "{manifest}");
    }

    #[test]
    fn manifest_blocks_carry_exact_line_ranges_in_document_order() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\nglue = sh -c 'true'\n"),
            (
                "a.md",
                "# One\n\n```rust name=a\nfn a() {}\n```\n\n```rust name=b\nfn b() {}\n```\n",
            ),
        ]);
        let out = dir.join("out");
        run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        let manifest = fs::read_to_string(out.join(MANIFEST_NAME)).unwrap();
        assert!(manifest.contains("\"version\": 2"), "{manifest}");
        // `a`'s fence opens at line 3 (after the heading and a blank line)
        // and closes at line 5; `b`'s opens at 7, closes at 9. Wrong here
        // means glue would window against the wrong prose entirely.
        assert!(manifest.contains("\"name\": \"a\", \"line\": 3, \"end_line\": 5"), "{manifest}");
        assert!(manifest.contains("\"name\": \"b\", \"line\": 7, \"end_line\": 9"), "{manifest}");
        // Both appearing is not enough -- `a` must be listed before `b`
        // (document order), not whatever order a HashMap happened to keep.
        let pos_a = manifest.find("\"name\": \"a\"").unwrap();
        let pos_b = manifest.find("\"name\": \"b\"").unwrap();
        assert!(pos_a < pos_b, "{manifest}");
    }

    #[test]
    fn a_path_override_block_still_gets_its_own_manifest_block_entry() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\nglue = sh -c 'true'\n"),
            (
                "a.md",
                "# One\n\n```rust name=manifest path=Cargo.toml\n[package]\n```\n",
            ),
        ]);
        let out = dir.join("out");
        run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        let manifest = fs::read_to_string(out.join(MANIFEST_NAME)).unwrap();
        assert!(manifest.contains("\"path\": \"Cargo.toml\""), "{manifest}");
        assert!(manifest.contains("\"name\": \"manifest\", \"line\": 3, \"end_line\": 5"), "{manifest}");
    }

    #[test]
    fn corpus_wide_manifest_keeps_each_files_blocks_scoped_to_its_own_source() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\nglue = sh -c 'true'\n"),
            ("a.md", "# A\n\n```rust name=one\nfn one() {}\n```\n"),
            ("b.md", "# B\n\n\n```rust name=two\nfn two() {}\n```\n"),
        ]);
        let out = dir.join("out");
        run(&[dir.to_string_lossy().into_owned()], "rust", Some(out.to_str().unwrap()), true).unwrap();
        let manifest = fs::read_to_string(out.join(MANIFEST_NAME)).unwrap();
        assert!(manifest.contains("\"source\": \"a.md\""), "{manifest}");
        assert!(manifest.contains("\"source\": \"b.md\""), "{manifest}");
        // `b.md` has one extra blank line before its fence than `a.md`
        // does -- if per-file line tracking leaked across files (e.g. a
        // shared counter, or an offset from the other file's length),
        // this is exactly where it would show up.
        assert!(manifest.contains("\"name\": \"one\", \"line\": 3, \"end_line\": 5"), "{manifest}");
        assert!(manifest.contains("\"name\": \"two\", \"line\": 4, \"end_line\": 6"), "{manifest}");
    }

    #[test]
    fn public_defaults_to_false_with_no_frontmatter() {
        let dir = scratch(&[
            (".dankg/config", "[tangle.rust]\next = rs\nglue = sh -c 'true'\n"),
            ("a.md", "```rust name=a\nfn a() {}\n```\n"),
        ]);
        let out = dir.join("out");
        run(&one(dir.join("a.md").to_str().unwrap()), "rust", Some(out.to_str().unwrap()), true).unwrap();
        let manifest = fs::read_to_string(out.join(MANIFEST_NAME)).unwrap();
        assert!(manifest.contains("\"public\": false"), "{manifest}");
    }
}
```
