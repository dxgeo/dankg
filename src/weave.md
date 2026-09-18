# Weave

`dankg weave`: plan-weave.md's own decisions 41-45. Turns one markdown
file into a readable document, HTML or PDF (via Typst), never a whole
corpus (decision 41 -- confirmed single-file only for now, unlike
`tangle`). Much smaller than `tangle.rs`: there is no corpus-walk
branch to have, only the single-file path `tangle::run` and `eval`'s
own single-file path already take for the identical reason (decision
19\): nothing here needs to know about any file but the one asked for.

```rust name=module_doc path=weave.rs
//! `dankg weave`: turns one markdown file into a readable document, HTML
//! or PDF via Typst. Single-file only (plan-weave.md decision 41) --
//! there is no corpus-walk branch, the same single-file path
//! `tangle::run`/`eval` already take for the identical reason.

use crate::cmd;
use crate::config::Config;
use crate::diag::Diags;
use crate::eval::files::Files;
use crate::eval::session::corpus_graph_if_needed;
use crate::eval::{plan, result};
use crate::graph::build::{file_stem, strip_extension};
use crate::graph::index;
use crate::md::Document;
use crate::render::{typst, weave_html};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Html,
    Pdf,
}

pub struct Report {
    /// Where the rendered page went: a file path, or `None` for stdout
    /// (HTML's own default with no `-o`).
    pub output: Option<PathBuf>,
    /// PDF only: the `.typ` source's own path. Always written, whether or
    /// not a PDF actually got compiled from it.
    pub typ_path: Option<PathBuf>,
    /// PDF only: whether `[weave.pdf] command` actually ran and produced
    /// one.
    pub pdf_written: bool,
}
```

Root discovery walks up from `path`'s own absolute form for `.dankg/`,
falling back to the named file's own parent directory, then loads
whatever config that root has. `path` is absolutized first, the same
shape `session::locate` already established, rather than handed to
`discover_root` exactly as the reader typed it.

A bare relative filename with no directory component --
`dankg weave demo.md`, run from inside its own directory, the ordinary
case -- has an empty `Path::parent()`. Walking up from that instead of
the file's real absolute parent stops one step too early, silently
landing on the wrong root. Nothing noticed before decision 50: every
earlier use of `root` only ever fed `Path::join`, which happens to
still land in the right place when the wrong root is merely empty.
`resolve_artifact` does real root-relative string arithmetic instead,
where that same wrong root produces a real wrong path.
`tangle::run`'s own single-file branch has this identical gap today.
It is not fixed here, since nothing in `tangle` yet depends on a
correct root-relative path the way weave now does.

The title falls back the same way `graph/build.rs`'s own `file_node`
does for a headingless file: frontmatter's `title` first, the bare
file name otherwise.

`name` is `file_stem`, not `strip_extension` alone. `path` is whatever
the reader typed -- root-relative, relative to the working directory,
or absolute -- and `render_pdf` below joins it onto `.dankg/build/weave`
to place the `.typ`/`.pdf`. `Path::join` silently *replaces* its base
entirely when the joined piece is itself absolute, so joining onto
`strip_extension(path)`'s full path unstripped of its directory would
silently drop the whole `.dankg/build/weave` prefix the moment a
reader passed an absolute path. `file_stem` never has that problem: it
is never anything but a bare name.

```rust name=run path=weave.rs
pub fn run(path: &str, format: Format, output: Option<&str>, toc: bool, figures_outside: bool) -> Result<Report, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut diags = Diags::new(path);
    let doc = Document::parse(&source, &mut diags);

    let abs_path = index::absolute(Path::new(path));
    let root = index::discover_root(&abs_path)
        .unwrap_or_else(|| abs_path.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")));
    let mut cfg_diags = Diags::new(".dankg/config");
    let config = Config::load(&root, &mut cfg_diags);
    diags.absorb(cfg_diags);

    let name = file_stem(&strip_extension(path)).to_string();
    let title = doc.frontmatter.title().map(str::to_string).unwrap_or_else(|| name.clone());

    let entry_rel = abs_path.strip_prefix(&root).map(index::to_slash).unwrap_or_else(|_| path.to_string());
    warn_stale_pairs(&doc, path, &entry_rel, &root, &config, &mut diags);
    let (tables, images) = produced_artifacts(&doc, &entry_rel, &root, &mut diags);

    let report = match format {
        Format::Html => {
            render_html(&doc, &title, &config, &root, &tables, &images, output, figures_outside, &mut diags)?
        }
        Format::Pdf => render_pdf(
            &doc, &title, &config, &root, &name, &tables, &images, output, toc, figures_outside, &mut diags,
        )?,
    };

    diags.sort();
    diags.emit();
    Ok(report)
}
```

Every recognized pair (decision 46) gets one freshness check: the
same comparison `dankg check` and `dankg eval --if-stale` already
make, `is_stale`'s `expected_hash` against the marker's own stored
`hash`. `corpus_graph_if_needed` is `session::run_single`'s own
`--if-stale` precheck, reused rather than reimplemented -- the same
lazy shape decision 19 already established: a `Graph` is only ever
built if some block's own `xdeps=` actually carries a `table:` entry.
Weave still never executes anything; this only ever compares hashes
already on disk against a fresh recomputation.

A stale result changes nothing about what gets rendered. `warn_stale_pairs`
writes to `diags` alone -- a stderr warning, by the time `run` calls
`diags.emit()` -- and returns nothing for either renderer to consume.
The woven document stays a pure function of the one file's own
content. Two runs against the same file produce byte-identical
output, regardless of whatever state a `deps=`/`xdeps=` chain happens
to be in elsewhere. A reader learns about a stale result the same way
they learn about a missing CSS file or an unclosed fence: a warning
next to the output, never a silent change to it.

<!-- dankg:depends target=../architecture.md#decision-47-weave-staleness-is-reported-never-rendered quote="A confirmed match warns about nothing at all." -->

```rust name=warn_stale_pairs path=weave.rs
fn warn_stale_pairs(doc: &Document, path: &str, entry_rel: &str, root: &Path, config: &Config, diags: &mut Diags) {
    let mut discover_diags = Diags::new(entry_rel);
    let mut files = Files::new(root.to_path_buf());
    if files.discover(entry_rel, &mut discover_diags).is_err() {
        return;
    }
    let (files, graph) = match corpus_graph_if_needed(path, files) {
        Ok(v) => v,
        Err(e) => {
            diags.warn(0, format!("could not check eval result staleness: {e}"));
            return;
        }
    };
    let all_blocks = files.all_blocks();
    let mut xdep_cache = std::collections::HashMap::new();

    for b in plan::top_level_blocks(doc, entry_rel) {
        let Some(stored) = result::recorded_hash(doc, b.index, b.name) else { continue };
        let outcome = (|| {
            let chain = plan::plan_for(&all_blocks, entry_rel, b.name).map_err(|e| e.to_string())?;
            let hash_template = result::hash_template_for(config, &chain)?;
            result::is_stale(&files, config, &all_blocks, graph.as_ref(), &chain, &hash_template, stored, &mut xdep_cache)
        })();
        match outcome {
            Ok(false) => {}
            Ok(true) => diags.warn(b.line, format!("`{}` recorded result looks stale; rerun `dankg eval`", b.name)),
            Err(e) => diags.warn(b.line, format!("`{}` staleness could not be verified: {e}", b.name)),
        }
    }
}
```

A recognized pair (decision 46) whose source block also declares
`produces=file:PATH` (decision 33) may have a real table or a real
image sitting on disk, not just captured stdout -- `df.to_csv(...)`
or `plt.savefig(...)`, say, printing nothing themselves.
`produced_artifacts` resolves that path the same way `dankg check`
already verifies one (`plan::parse_artifact`/`resolve_artifact`,
decision 33's own logic, reused rather than reimplemented) and reads
it, once its extension says which of the two it is. Anything else --
an unrecognized extension, a missing or unreadable file, an artifact
path that escapes the root -- is silently absent from both returned
maps; only a real read failure gets a stderr warning, matching
decision 47's own "misconfigured is reported, not fatal" stance.

The table map holds a lang tag and the file's own raw content, keyed
by the source block's index -- not a parsed `TableData`, since a
`.json` file that turns out not to be table-shaped still needs
`code_or_data_table`'s own existing fallback to an ordinary code
block (decision 45). That dispatch already lives in both renderers.
Handing over raw text and a lang tag reuses it exactly, rather than a
third copy of the same csv/tsv/json-or-fallback logic. The image map
holds the raw bytes and the artifact's own root-relative path --
unlike a table, an image is never fed through a shared parser either
backend already has. Each renders it its own way (decision 51).

<!-- dankg:depends target=../architecture.md#decision-50-a-producesfile-csvtsvjson-artifact-renders-as-a-table quote="never a parsed `TableData`" -->

```rust name=produced_artifacts path=weave.rs
fn produced_artifacts(
    doc: &Document,
    entry_rel: &str,
    root: &Path,
    diags: &mut Diags,
) -> (HashMap<usize, (String, String)>, HashMap<usize, (Vec<u8>, String)>) {
    let mut tables = HashMap::new();
    let mut images = HashMap::new();
    for b in plan::top_level_blocks(doc, entry_rel) {
        if result::recorded_hash(doc, b.index, b.name).is_none() {
            continue; // no recognized pair, nothing to attach an artifact to
        }
        let Some(raw) = b.produces else { continue };
        let Some(rel_path) = plan::parse_artifact(raw) else { continue };
        let Some(resolved) = plan::resolve_artifact(entry_rel, rel_path) else {
            diags.warn(b.line, format!("`{}` produces={raw} escapes the root; not rendered", b.name));
            continue;
        };
        if let Some(lang) = table_lang(rel_path) {
            match fs::read_to_string(root.join(&resolved)) {
                Ok(content) => {
                    tables.insert(b.index, (lang.to_string(), content));
                }
                Err(e) => diags.warn(b.line, format!("`{}` produces={raw} could not be read ({e}); not rendered", b.name)),
            }
        } else if image_ext(rel_path).is_some() {
            match fs::read(root.join(&resolved)) {
                Ok(bytes) => {
                    images.insert(b.index, (bytes, resolved));
                }
                Err(e) => diags.warn(b.line, format!("`{}` produces={raw} could not be read ({e}); not rendered", b.name)),
            }
        }
    }
    (tables, images)
}

/// The three extensions `code_or_data_table` already renders as a table
/// (decision 45); anything else is not a table shape this feature
/// knows how to show, so `produced_artifacts` leaves it out silently
/// rather than guessing.
fn table_lang(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "csv" => Some("csv"),
        "tsv" => Some("tsv"),
        "json" => Some("json"),
        _ => None,
    }
}

/// The image extensions decision 51 knows how to render. `jpg`/`jpeg`
/// canonicalize to one tag: both mean the same MIME type, and both
/// renderers only ever need to know "this is a jpeg."
fn image_ext(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "gif" => Some("gif"),
        "svg" => Some("svg"),
        "webp" => Some("webp"),
        _ => None,
    }
}
```

`[weave.html] css` and `[weave.pdf] template` are both a path to a
local file, read once and handed to the renderer -- CSS as a parameter
`weave_html::render` appends after `WEAVE_CSS`, a Typst template as a
plain-text preamble concatenated in front of the emitted body before
the `.typ` is ever written (`render::typst` itself never learns a
template exists). Either path missing or unreadable warns and is
skipped, rather than failing the whole run: the woven document is
still good without a reader's own styling or preamble. This is the
same "misconfigured is reported, not fatal" shape `Config::load`
itself already takes for an unreadable `.dankg/config`.

```rust name=asset_and_html path=weave.rs
fn read_asset(root: &Path, rel: &str, key: &str, diags: &mut Diags) -> Option<String> {
    let full = root.join(rel);
    match fs::read_to_string(&full) {
        Ok(text) => Some(text),
        Err(e) => {
            diags.warn(0, format!("[weave.*] {key} = {rel} could not be read ({e}); ignored"));
            None
        }
    }
}

fn render_html(
    doc: &Document,
    title: &str,
    config: &Config,
    root: &Path,
    tables: &HashMap<usize, (String, String)>,
    images: &HashMap<usize, (Vec<u8>, String)>,
    output: Option<&str>,
    figures_outside: bool,
    diags: &mut Diags,
) -> Result<Report, String> {
    let extra_css = config.weave("html").and_then(|w| w.css).and_then(|rel| read_asset(root, &rel, "css", diags));
    let rendered = weave_html::render(doc, title, extra_css.as_deref(), tables, images, figures_outside, diags);

    let written = match output {
        Some(p) => {
            let file = PathBuf::from(p);
            fs::write(&file, &rendered).map_err(|e| format!("{}: {e}", file.display()))?;
            Some(file)
        }
        None => {
            use std::io::Write;
            std::io::stdout().write_all(rendered.as_bytes()).map_err(|e| e.to_string())?;
            None
        }
    };
    Ok(Report { output: written, typ_path: None, pdf_written: false })
}
```

`weave.pdf`'s own `command` and `template` are independent, the same
way `tangle.*`'s `command` and `glue` already are: either, both, or
neither may be configured. The `.typ` is always written, whether or
not `command` exists to compile it -- decision 44's own graceful
degradation.

An image (decision 51) has to actually exist somewhere Typst can find
it by a relative path -- `render::typst` only ever emits markup, never
touches a filesystem. `render_pdf` copies each one into
`build_dir/assets/`, at the same root-relative path `produced_artifacts`
already resolved it to, before compiling. `#image(...)`'s own
reference and the copy's own destination are computed from that
identical string. The two can never name different files. A copy
that fails is dropped from the map `typst::render` sees and warned
about on stderr, the same "misconfigured is reported, not fatal"
shape as everything else weave reads off disk -- never a reference to
a file that was never actually written.

```rust name=render_pdf path=weave.rs
fn render_pdf(
    doc: &Document,
    title: &str,
    config: &Config,
    root: &Path,
    name: &str,
    tables: &HashMap<usize, (String, String)>,
    images: &HashMap<usize, (Vec<u8>, String)>,
    output: Option<&str>,
    toc: bool,
    figures_outside: bool,
    diags: &mut Diags,
) -> Result<Report, String> {
    let build_dir = root.join(".dankg").join("build").join("weave");
    let assets_dir = build_dir.join("assets");
    let mut copied_images = images.clone();
    for (index, (bytes, resolved)) in images {
        let dest = assets_dir.join(resolved);
        let result = dest
            .parent()
            .map(fs::create_dir_all)
            .unwrap_or(Ok(()))
            .and_then(|()| fs::write(&dest, bytes));
        if let Err(e) = result {
            diags.warn(0, format!("could not copy `{resolved}` into the build directory ({e}); not rendered"));
            copied_images.remove(index);
        }
    }

    let body = typst::render(doc, title, toc, tables, &copied_images, figures_outside, diags);
    let weave_cfg = config.weave("pdf");
    let preamble =
        weave_cfg.as_ref().and_then(|w| w.template.clone()).and_then(|rel| read_asset(root, &rel, "template", diags));
    let content = match preamble {
        Some(p) => format!("{p}\n\n{body}"),
        None => body,
    };

    let typ_path = build_dir.join(format!("{name}.typ"));
    if let Some(parent) = typ_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(&typ_path, &content).map_err(|e| format!("{}: {e}", typ_path.display()))?;

    let pdf_path = match output {
        Some(p) => PathBuf::from(p),
        None => build_dir.join(format!("{name}.pdf")),
    };
    if let Some(parent) = pdf_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }

    let mut pdf_written = false;
    if let Some(template) = weave_cfg.as_ref().and_then(|c| c.command.as_ref()) {
        spawn_pdf(template, &typ_path, &pdf_path)?;
        pdf_written = true;
    }

    Ok(Report {
        output: if pdf_written { Some(pdf_path) } else { None },
        typ_path: Some(typ_path),
        pdf_written,
    })
}

fn spawn_pdf(template: &str, typ_path: &Path, pdf_path: &Path) -> Result<(), String> {
    let typ = typ_path.to_string_lossy();
    let pdf = pdf_path.to_string_lossy();
    let Some(argv) = cmd::build(template, &[("typ", &typ), ("pdf", &pdf)]) else {
        return Err("`[weave.pdf] command` is empty once substituted".to_string());
    };
    let status = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .status()
        .map_err(|e| format!("could not run `{}`: {e}", argv[0]))?;
    if !status.success() {
        return Err("`[weave.pdf] command` exited with a failure".to_string());
    }
    Ok(())
}
```

## Tests

```rust name=tests path=weave.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch(files: &[(&str, &str)]) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("dankg-weave-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for (name, content) in files {
            let path = dir.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut f = fs::File::create(&path).unwrap();
            f.write_all(content.as_bytes()).unwrap();
        }
        dir
    }

    #[test]
    fn html_with_no_output_path_still_reports_none() {
        let dir = scratch(&[("a.md", "# H\n\ntext\n")]);
        let report = run(dir.join("a.md").to_str().unwrap(), Format::Html, None, true, false).unwrap();
        assert!(report.output.is_none());
        assert!(report.typ_path.is_none());
    }

    #[test]
    fn html_with_an_output_path_writes_a_file() {
        let dir = scratch(&[("a.md", "# H\n\ntext\n")]);
        let out = dir.join("out.html");
        let report = run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        assert_eq!(report.output, Some(out.clone()));
        let content = fs::read_to_string(&out).unwrap();
        // The page's own title heading (from the file name, "a" -- no
        // frontmatter here) is distinct from the document's own "H"
        // heading, which keeps its slug-derived id.
        assert!(content.contains("<h1>a</h1>"), "{content}");
        assert!(content.contains("<h1 id=\"h\">H</h1>"), "{content}");
    }

    #[test]
    fn title_falls_back_to_the_file_name_with_no_frontmatter() {
        let dir = scratch(&[("notes.md", "text\n")]);
        let out = dir.join("out.html");
        run(dir.join("notes.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("<title>notes</title>"), "{content}");
    }

    #[test]
    fn frontmatter_title_wins_over_the_file_name() {
        let dir = scratch(&[("notes.md", "---\ntitle: Real Title\n---\ntext\n")]);
        let out = dir.join("out.html");
        run(dir.join("notes.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("<title>Real Title</title>"), "{content}");
    }

    #[test]
    fn configured_css_is_read_and_appended() {
        let dir = scratch(&[
            (".dankg/config", "[weave.html]\ncss = theme.css\n"),
            ("theme.css", "body { color: red; }"),
            ("a.md", "# H\n"),
        ]);
        let out = dir.join("out.html");
        run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("color: red"), "{content}");
    }

    #[test]
    fn a_missing_css_file_warns_and_is_skipped_not_fatal() {
        let dir = scratch(&[(".dankg/config", "[weave.html]\ncss = nope.css\n"), ("a.md", "# H\n")]);
        let out = dir.join("out.html");
        let report = run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false);
        assert!(report.is_ok());
    }

    #[test]
    fn pdf_always_writes_the_typ_even_unconfigured() {
        let dir = scratch(&[("a.md", "# H\n\ntext\n")]);
        let report = run(dir.join("a.md").to_str().unwrap(), Format::Pdf, None, true, false).unwrap();
        assert!(report.typ_path.as_ref().unwrap().exists());
        assert!(!report.pdf_written);
        assert!(report.output.is_none());
    }

    #[test]
    fn pdf_typ_defaults_under_dankg_build_weave() {
        let dir = scratch(&[("a.md", "# H\n")]);
        let report = run(dir.join("a.md").to_str().unwrap(), Format::Pdf, None, true, false).unwrap();
        assert_eq!(report.typ_path, Some(dir.join(".dankg/build/weave/a.typ")));
    }

    #[test]
    fn a_configured_command_runs_and_reports_the_pdf_path() {
        let dir = scratch(&[
            (".dankg/config", "[weave.pdf]\ncommand = sh -c 'touch {pdf}'\n"),
            ("a.md", "# H\n"),
        ]);
        let out = dir.join("out.pdf");
        let report = run(dir.join("a.md").to_str().unwrap(), Format::Pdf, Some(out.to_str().unwrap()), true, false).unwrap();
        assert!(report.pdf_written);
        assert_eq!(report.output, Some(out.clone()));
        assert!(out.exists());
    }

    #[test]
    fn a_configured_template_is_prepended_to_the_typ() {
        let dir = scratch(&[
            (".dankg/config", "[weave.pdf]\ntemplate = template.typ\n"),
            ("template.typ", "#set page(margin: 2cm)"),
            ("a.md", "# H\n"),
        ]);
        let report = run(dir.join("a.md").to_str().unwrap(), Format::Pdf, None, true, false).unwrap();
        let content = fs::read_to_string(report.typ_path.unwrap()).unwrap();
        let template_pos = content.find("#set page(margin: 2cm)").unwrap();
        let title_pos = content.find("= H").unwrap();
        assert!(template_pos < title_pos, "{content}");
    }

    #[test]
    fn a_missing_template_file_warns_and_the_typ_is_still_written() {
        let dir = scratch(&[(".dankg/config", "[weave.pdf]\ntemplate = nope.typ\n"), ("a.md", "# H\n")]);
        let report = run(dir.join("a.md").to_str().unwrap(), Format::Pdf, None, true, false).unwrap();
        assert!(report.typ_path.unwrap().exists());
    }

    fn stale_scratch(hash_hex: &str) -> (PathBuf, Config) {
        let dir = scratch(&[
            (".dankg/config", "[lang.sh]\ncommand = sh {file}\n"),
            ("a.md", &format!("```sh name=a\necho hi\n```\n\n<!-- dankg:result name=a hash={hash_hex} -->\n\n```\nhi\n```\n")),
        ]);
        let config = Config::load(&dir, &mut Diags::new(".dankg/config"));
        (dir, config)
    }

    #[test]
    fn warn_stale_pairs_is_silent_when_the_recorded_result_is_current() {
        let (dir, config) = stale_scratch("0000000000000000");
        let source = fs::read_to_string(dir.join("a.md")).unwrap();
        let doc = Document::parse(&source, &mut Diags::new("a.md"));
        let blocks = plan::top_level_blocks(&doc, "a.md");
        let chain = plan::plan_for(&blocks, "a.md", "a").unwrap();
        let template = result::hash_template_for(&config, &chain).unwrap();
        let real_hash = result::expected_hash(&chain, &template, &[]);

        let source = fs::read_to_string(dir.join("a.md")).unwrap().replace("0000000000000000", &crate::hash::hex(real_hash));
        fs::write(dir.join("a.md"), &source).unwrap();
        let doc = Document::parse(&source, &mut Diags::new("a.md"));

        let mut diags = Diags::new("a.md");
        warn_stale_pairs(&doc, dir.join("a.md").to_str().unwrap(), "a.md", &dir, &config, &mut diags);
        assert!(diags.items().is_empty(), "{:?}", diags.items());
    }

    #[test]
    fn warn_stale_pairs_warns_when_the_source_changed_since_the_result_was_recorded() {
        let (dir, config) = stale_scratch("0000000000000000");
        let source = fs::read_to_string(dir.join("a.md")).unwrap();
        let doc = Document::parse(&source, &mut Diags::new("a.md"));

        let mut diags = Diags::new("a.md");
        warn_stale_pairs(&doc, dir.join("a.md").to_str().unwrap(), "a.md", &dir, &config, &mut diags);
        assert_eq!(diags.items().len(), 1, "{:?}", diags.items());
        assert!(diags.items()[0].message.contains("looks stale"), "{:?}", diags.items());
    }

    #[test]
    fn warn_stale_pairs_is_silent_for_a_named_block_with_no_recorded_result() {
        let dir = scratch(&[
            (".dankg/config", "[lang.sh]\ncommand = sh {file}\n"),
            ("a.md", "```sh name=a\necho hi\n```\n"),
        ]);
        let config = Config::load(&dir, &mut Diags::new(".dankg/config"));
        let source = fs::read_to_string(dir.join("a.md")).unwrap();
        let doc = Document::parse(&source, &mut Diags::new("a.md"));

        let mut diags = Diags::new("a.md");
        warn_stale_pairs(&doc, dir.join("a.md").to_str().unwrap(), "a.md", &dir, &config, &mut diags);
        assert!(diags.items().is_empty(), "{:?}", diags.items());
    }

    #[test]
    fn table_lang_recognizes_csv_tsv_json_case_insensitively() {
        assert_eq!(table_lang("data.csv"), Some("csv"));
        assert_eq!(table_lang("data.CSV"), Some("csv"));
        assert_eq!(table_lang("data.tsv"), Some("tsv"));
        assert_eq!(table_lang("data.json"), Some("json"));
        assert_eq!(table_lang("chart.png"), None);
        assert_eq!(table_lang("no_extension"), None);
    }

    #[test]
    fn produced_artifacts_reads_a_csv_artifact_for_a_recognized_pair() {
        let dir = scratch(&[
            ("a.md", "```python name=a produces=file:data.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n"),
            ("data.csv", "x,y\n1,2\n"),
        ]);
        let source = fs::read_to_string(dir.join("a.md")).unwrap();
        let doc = Document::parse(&source, &mut Diags::new("a.md"));

        let mut diags = Diags::new("a.md");
        let (tables, images) = produced_artifacts(&doc, "a.md", &dir, &mut diags);
        assert_eq!(tables.get(&0), Some(&("csv".to_string(), "x,y\n1,2\n".to_string())));
        assert!(images.is_empty());
        assert!(diags.items().is_empty(), "{:?}", diags.items());
    }

    #[test]
    fn produced_artifacts_reads_a_png_artifact_for_a_recognized_pair() {
        let dir = scratch(&[(
            "a.md",
            "```python name=a produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n",
        )]);
        fs::write(dir.join("chart.png"), [0xffu8, 0xd8, 0xff, 0xe0]).unwrap();
        let source = fs::read_to_string(dir.join("a.md")).unwrap();
        let doc = Document::parse(&source, &mut Diags::new("a.md"));

        let mut diags = Diags::new("a.md");
        let (tables, images) = produced_artifacts(&doc, "a.md", &dir, &mut diags);
        assert!(tables.is_empty());
        assert_eq!(images.get(&0), Some(&(vec![0xff, 0xd8, 0xff, 0xe0], "chart.png".to_string())));
        assert!(diags.items().is_empty(), "{:?}", diags.items());
    }

    #[test]
    fn produced_artifacts_is_silent_for_a_named_block_with_no_recorded_result() {
        let dir = scratch(&[("a.md", "```python name=a produces=file:data.csv\nwrite_csv()\n```\n"), ("data.csv", "x,y\n1,2\n")]);
        let source = fs::read_to_string(dir.join("a.md")).unwrap();
        let doc = Document::parse(&source, &mut Diags::new("a.md"));

        let mut diags = Diags::new("a.md");
        let (tables, images) = produced_artifacts(&doc, "a.md", &dir, &mut diags);
        assert!(tables.is_empty());
        assert!(images.is_empty());
        assert!(diags.items().is_empty(), "{:?}", diags.items());
    }

    #[test]
    fn produced_artifacts_warns_on_a_missing_artifact() {
        let dir = scratch(&[(
            "a.md",
            "```python name=a produces=file:missing.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\ndone\n```\n",
        )]);
        let source = fs::read_to_string(dir.join("a.md")).unwrap();
        let doc = Document::parse(&source, &mut Diags::new("a.md"));

        let mut diags = Diags::new("a.md");
        let (tables, images) = produced_artifacts(&doc, "a.md", &dir, &mut diags);
        assert!(tables.is_empty());
        assert!(images.is_empty());
        assert_eq!(diags.items().len(), 1, "{:?}", diags.items());
        assert!(diags.items()[0].message.contains("could not be read"), "{:?}", diags.items());
    }

    #[test]
    fn produced_artifacts_is_silent_for_an_unrecognized_extension() {
        let dir = scratch(&[(
            "a.md",
            "```python name=a produces=file:notes.txt\nwrite_notes()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\ndone\n```\n",
        )]);
        let source = fs::read_to_string(dir.join("a.md")).unwrap();
        let doc = Document::parse(&source, &mut Diags::new("a.md"));

        let mut diags = Diags::new("a.md");
        let (tables, images) = produced_artifacts(&doc, "a.md", &dir, &mut diags);
        assert!(tables.is_empty());
        assert!(images.is_empty());
        assert!(diags.items().is_empty(), "{:?}", diags.items());
    }

    #[test]
    fn image_ext_recognizes_common_extensions_case_insensitively_and_canonicalizes_jpeg() {
        assert_eq!(image_ext("chart.png"), Some("png"));
        assert_eq!(image_ext("chart.PNG"), Some("png"));
        assert_eq!(image_ext("chart.jpg"), Some("jpg"));
        assert_eq!(image_ext("chart.jpeg"), Some("jpg"));
        assert_eq!(image_ext("chart.svg"), Some("svg"));
        assert_eq!(image_ext("data.csv"), None);
    }
}
```
