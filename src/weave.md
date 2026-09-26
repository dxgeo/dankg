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
use crate::data::bib::{self, BibEntry};
use crate::diag::Diags;
use crate::eval::files::Files;
use crate::eval::session::corpus_graph_if_needed;
use crate::eval::{plan, result};
use crate::graph::build::{file_stem, strip_extension};
use crate::graph::index;
use crate::md::{Block, Document, Inline};
use crate::render::typst::BibliographySummary;
use crate::render::{typst, weave_html};
use std::collections::{HashMap, HashSet};
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
    let mut doc = Document::parse(&source, &mut diags);

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
    if doc.frontmatter.cover() == Some(true) {
        drop_repeated_title_heading(&mut doc, &title);
    }
    let (tables, images) = produced_artifacts(&doc, &entry_rel, &root, &mut diags);
    let labels = figure_labels(&doc, &tables, &images, &mut diags);
    let bib = bibliography(&doc, &entry_rel, &root, &mut diags);

    let report = match format {
        Format::Html => {
            render_html(
                &doc, &title, &config, &root, &tables, &images, &labels, bib.as_ref(), output, figures_outside,
                &mut diags,
            )?
        }
        Format::Pdf => render_pdf(
            &doc, &title, &config, &root, &name, &tables, &images, &labels, bib.as_ref(), output, toc,
            figures_outside, &mut diags,
        )?,
    };

    diags.sort();
    diags.emit();
    Ok(report)
}
```

## The repeated title

`cover: true` says the document's own title block -- the PDF cover
page, the HTML `<h1>` and byline -- is the one place the title
belongs. The document's own leading heading, when it repeats that
title, is the repeated heading. `run` drops it before either backend
sees the document.

It is dropped here rather than in each renderer, for two reasons.
Both backends want the identical document. And HTML's own table of
contents is built from the same blocks its body is, so a heading
dropped here leaves the outline too -- Typst's `#outline()` and
HTML's `<nav>` alike -- without either module knowing the rule
exists.

Where this sits inside `run` is not free either. Dropping a block
shifts every later block's own index, and `tables`/`images` are keyed
by that index. So it runs after `warn_stale_pairs`, which compares
indices against the file still on disk, and before
`produced_artifacts`, which computes the keys both renderers then
read.

Only the document's very first block qualifies, and only on an exact
match of the title once both sides are trimmed. A heading further
down is the author's own structure, and weave never guesses at which.
`Inline::plain` is the same flattening `graph/build.rs` already
derives a heading node's own title with, so a heading written
`# *Weave*` matches a frontmatter `title: Weave` here exactly as it
would there.

<!-- dankg:depends target=../architecture.md#decision-61-a-frontmatter-cover-switch-for-the-woven-title quote="Only the document's very first block qualifies, and only on an exact match" -->

```rust name=drop_repeated_title_heading path=weave.rs
fn drop_repeated_title_heading(doc: &mut Document, title: &str) {
    let repeats = matches!(
        doc.blocks.first(),
        Some(Block::Heading { inlines, .. }) if Inline::plain(inlines).trim() == title.trim()
    );
    if repeats {
        doc.blocks.remove(0);
    }
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

Every figure a reference can point at needs a name (decision 63). The
label is the block's own `name=`. That name is already unique within
its file, and already that block's own graph node slug. A
reader-written `label=` overrides it for one block. It exists because
`name` is also the block's eval identity and its tangle identity.
Renaming a block for a code reason should not break every reference
the prose already wrote.

`figure_labels` is the walk that collects them, keyed by the block
index each pair starts at -- the same key `produced_artifacts` already
hands both renderers its own artifacts under. A block with no artifact
in either map is not a figure. It gets no label. Decision 54 is what
makes that the right test. A produced artifact always carries a
caption: its own `produces=file:PATH` echo, when the reader wrote no
`caption=` of their own. So every entry in those two maps already
renders as a real figure.

A figure hidden by `weave=hidden` or `weave=output-hidden`
(decision 53) still takes its label here. Each backend applies its own
hiding. The block is therefore still in `doc.blocks` when this walk
reaches it. Keeping it means a reference to a hidden figure can be
told apart from a reference to nothing at all.

Two figures claiming one label warns at the second one's own line.
The second one is dropped, matching the second-fence rule decision 58
already set. Suffixing the collision the way `Slugger` suffixes a
duplicate heading would be wrong here. A silently suffixed label is a
reference that silently points at the wrong figure.

```rust name=figure_labels path=weave.rs
/// Every figure's own label, by the block index its pair starts at --
/// `label=` when the reader wrote one, otherwise the block's own
/// `name=` (decision 63). A block with no artifact in `tables` or
/// `images` is not a figure (decision 54) and is absent here. Each
/// backend turns one label into its own identifier: `<fig:NAME>` in
/// Typst, `id="fig-NAME"` in HTML.
fn figure_labels(
    doc: &Document,
    tables: &HashMap<usize, (String, String)>,
    images: &HashMap<usize, (Vec<u8>, String)>,
    diags: &mut Diags,
) -> HashMap<usize, String> {
    let mut labels = HashMap::new();
    let mut claimed: HashMap<&str, u32> = HashMap::new();
    for (index, b) in doc.blocks.iter().enumerate() {
        let Block::Code { info, line, .. } = b else { continue };
        if !tables.contains_key(&index) && !images.contains_key(&index) {
            continue;
        }
        let Some(label) = info.label().or_else(|| info.name()) else { continue };
        if let Some(&first) = claimed.get(label) {
            diags.warn(
                *line,
                format!("figure label `{label}` is already used by the figure on line {first}; this one is dropped"),
            );
            continue;
        }
        claimed.insert(label, *line);
        labels.insert(index, label.to_string());
    }
    labels
}
```

A document's bibliography (decision 58/59, `plan-weave-citations.md`)
comes from at most one `hayagriva`-tagged fence, a frontmatter
`bibliography:` file, or both together. `bibliography` is the pre-pass
that locates and merges them, then walks every inline in the whole
document -- headings, paragraphs, list items, table cells, the same
full-document reach decision 41 already gives weave -- collecting each
`Inline::Citation`'s own keys in first-appearance order. That order
*is* the numbering both renderers use; nothing counts it a second
time. `None` when neither source is configured, in which case a
`[@key]`/bare `@key` in the source falls back to its own literal text
in both renderers, never speculative citation markup with nothing to
resolve it against.

The external file's entries load first; the fence's own entries load
second and win on a duplicate key -- the same "last wins" convention
`md/frontmatter.rs`'s own duplicate-key handling already established.
A key cited in the document but absent from every parsed entry warns
at that citation's own block -- a heading or a paragraph's own `line`,
the finest position the AST tracks for an inline. A second
`hayagriva` fence warns and is ignored; nothing here has a use case
yet for more than one anchor point.

The merged raw Hayagriva text -- simple concatenation, external file
first -- is handed back for `render_pdf` to write to disk unchanged.
Typst reads that file itself and does its own full-schema formatting.
A real YAML reader treats a later duplicate top-level key as the one
that wins -- the same "fence wins" outcome dankg's own parsed
`entries` map already computes. The raw text needs no byte-level
merge of its own to agree with it.

<!-- dankg:depends target=../architecture.md#decision-41-weave-scope quote="Weave never executes anything." -->
<!-- dankg:depends target=../src/md/frontmatter.md#markdown-frontmatter quote="Unsupported input warns with its line number and is skipped, never guessed at." -->

```rust name=bibliography path=weave.rs
pub struct Bibliography {
    /// Every parsed entry, external file first, fence second and winning a
    /// duplicate key -- keyed by its own citation key.
    pub entries: HashMap<String, BibEntry>,
    /// Cited keys in first-appearance order across the whole document.
    /// This *is* the numbering a rendered references list uses.
    pub order: Vec<String>,
    /// The merged raw Hayagriva text, written to disk for Typst to read
    /// directly -- `render::typst` never touches an entry's own fields.
    pub raw: String,
}

fn bibliography(doc: &Document, entry_rel: &str, root: &Path, diags: &mut Diags) -> Option<Bibliography> {
    let mut fences = Vec::new();
    find_hayagriva_fences(&doc.blocks, &mut fences);
    for &(_, line) in fences.iter().skip(1) {
        diags.warn(line, "more than one `hayagriva` fence in one document; only the first is used");
    }
    let fence = fences.into_iter().next();

    let external = doc.frontmatter.bibliography().and_then(|path| {
        let resolved = plan::resolve_artifact(entry_rel, path)?;
        match fs::read_to_string(root.join(&resolved)) {
            Ok(content) => Some((resolved, content)),
            Err(e) => {
                diags.warn(0, format!("bibliography={path} could not be read ({e}); ignored"));
                None
            }
        }
    });

    if fence.is_none() && external.is_none() {
        return None;
    }

    let mut entries: HashMap<String, BibEntry> = HashMap::new();
    let mut raw = String::new();

    if let Some((resolved, content)) = &external {
        let mut ext_diags = Diags::new(resolved.as_str());
        for e in bib::parse(content, &mut ext_diags) {
            entries.insert(e.key.clone(), e);
        }
        diags.absorb(ext_diags);
        raw.push_str(content);
    }

    if let Some((text, fence_line)) = fence {
        let mut fence_diags = Diags::new(entry_rel);
        let fence_entries = bib::parse(text, &mut fence_diags);
        // Relative to the fence's own content (decision 58's own module
        // boundary: a reusable parser knows nothing about its caller's
        // position) -- remapped here, the one place that position is
        // actually known.
        for d in fence_diags.items() {
            let mut remapped = d.clone();
            remapped.line += fence_line;
            diags.add(remapped);
        }
        for e in fence_entries {
            if entries.contains_key(&e.key) {
                diags.warn(
                    fence_line,
                    format!(
                        "bibliography key `{}` is defined in both the external file and the fence; the fence wins",
                        e.key
                    ),
                );
            }
            entries.insert(e.key.clone(), e);
        }
        if !raw.is_empty() {
            raw.push('\n');
        }
        raw.push_str(text);
    }

    let mut seen = HashSet::new();
    let mut order = Vec::new();
    let mut slices = Vec::new();
    walk_inlines(&doc.blocks, &mut slices);
    for (line, inlines) in slices {
        collect_citation_keys(inlines, line, &entries, &mut seen, &mut order, diags);
    }

    Some(Bibliography { entries, order, raw })
}

/// At most one is used; a second warns at its own caller. Recurses into a
/// list item's own blocks, the same full-document reach every walk here
/// gives weave -- a fence has no reason to live only at the top level.
fn find_hayagriva_fences<'a>(blocks: &'a [Block], out: &mut Vec<(&'a str, u32)>) {
    for b in blocks {
        match b {
            Block::Code { info, text, line, .. } if info.lang.as_deref() == Some("hayagriva") => {
                out.push((text.as_str(), *line));
            }
            Block::List(l) => {
                for item in &l.items {
                    find_hayagriva_fences(&item.blocks, out);
                }
            }
            _ => {}
        }
    }
}

/// Every inline-bearing block, paired with the enclosing block's own
/// `line` -- the finest position the AST tracks for one of its inlines,
/// used to attribute an unresolved citation to at least the right block
/// rather than nowhere at all.
fn walk_inlines<'a>(blocks: &'a [Block], out: &mut Vec<(u32, &'a [Inline])>) {
    for b in blocks {
        match b {
            Block::Heading { inlines, line, .. } | Block::Paragraph { inlines, line, .. } => {
                out.push((*line, inlines));
            }
            Block::Table { header, rows, line, .. } => {
                for cell in header {
                    out.push((*line, cell));
                }
                for row in rows {
                    for cell in row {
                        out.push((*line, cell));
                    }
                }
            }
            Block::List(l) => {
                for item in &l.items {
                    walk_inlines(&item.blocks, out);
                }
            }
            Block::Code { .. } | Block::ThematicBreak { .. } | Block::Passthrough { .. } => {}
        }
    }
}

fn collect_citation_keys(
    inlines: &[Inline],
    line: u32,
    entries: &HashMap<String, BibEntry>,
    seen: &mut HashSet<String>,
    order: &mut Vec<String>,
    diags: &mut Diags,
) {
    for i in inlines {
        match i {
            Inline::Citation { keys, .. } => {
                for k in keys {
                    if !entries.contains_key(k) {
                        diags.warn(line, format!("citation key `{k}` is not in the bibliography"));
                    }
                    if seen.insert(k.clone()) {
                        order.push(k.clone());
                    }
                }
            }
            Inline::Emph { inner, .. } | Inline::Strong { inner, .. } => {
                collect_citation_keys(inner, line, entries, seen, order, diags)
            }
            Inline::Link { text, .. } => collect_citation_keys(text, line, entries, seen, order, diags),
            Inline::Text(_) | Inline::Code(_) | Inline::WikiLink { .. } | Inline::SoftBreak | Inline::HardBreak => {}
        }
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
    labels: &HashMap<usize, String>,
    bibliography: Option<&Bibliography>,
    output: Option<&str>,
    figures_outside: bool,
    diags: &mut Diags,
) -> Result<Report, String> {
    let extra_css = config.weave("html").and_then(|w| w.css).and_then(|rel| read_asset(root, &rel, "css", diags));
    let html_bib = bibliography.map(|bib| weave_html::Bibliography { entries: &bib.entries, order: &bib.order });
    let rendered = weave_html::render(
        doc, title, extra_css.as_deref(), tables, images, labels, figures_outside, html_bib.as_ref(), diags,
    );

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
    labels: &HashMap<usize, String>,
    bibliography: Option<&Bibliography>,
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

    // The identical copy-before-compile shape `images` already gets
    // (decision 51): `render::typst` never touches a filesystem. The
    // merged raw text goes to disk here, before `typst::render` is asked
    // to emit a `#bibliography(...)` call pointing at it.
    let bib_summary = bibliography.and_then(|bib| {
        let dest = build_dir.join("bibliography.yml");
        match fs::create_dir_all(&build_dir).and_then(|()| fs::write(&dest, &bib.raw)) {
            Ok(()) => {
                Some(BibliographySummary { valid_keys: bib.entries.keys().cloned().collect(), asset_path: "bibliography.yml".to_string() })
            }
            Err(e) => {
                diags.warn(0, format!("could not write bibliography.yml into the build directory ({e}); not rendered"));
                None
            }
        }
    });

    let body =
        typst::render(doc, title, toc, tables, &copied_images, labels, figures_outside, bib_summary.as_ref(), diags);
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
    fn cover_true_drops_a_leading_heading_that_repeats_the_title() {
        let dir = scratch(&[("notes.md", "---\ntitle: Real Title\ncover: true\n---\n# Real Title\n\ntext\n\n## Later\n")]);
        let out = dir.join("out.html");
        run(dir.join("notes.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("<h1>Real Title</h1>"), "the title block itself stays: {content}");
        assert!(!content.contains("<h1 id=\"real-title\">"), "the body's own repeat is gone: {content}");
        // Gone from the table of contents with it -- the TOC is built
        // from the same blocks the body is.
        assert!(!content.contains("#real-title"), "{content}");
        assert!(content.contains("#later"), "a heading further down is untouched: {content}");
    }

    #[test]
    fn cover_true_leaves_a_leading_heading_that_says_something_else() {
        let dir = scratch(&[("notes.md", "---\ntitle: Real Title\ncover: true\n---\n# Something Else\n")]);
        let out = dir.join("out.html");
        run(dir.join("notes.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("<h1 id=\"something-else\">Something Else</h1>"), "{content}");
    }

    #[test]
    fn no_cover_key_repeats_the_title_the_way_it_always_did() {
        let dir = scratch(&[("notes.md", "---\ntitle: Real Title\n---\n# Real Title\n")]);
        let out = dir.join("out.html");
        run(dir.join("notes.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("<h1>Real Title</h1>"), "{content}");
        assert!(content.contains("<h1 id=\"real-title\">Real Title</h1>"), "{content}");
    }

    #[test]
    fn cover_false_drops_the_title_block_and_keeps_the_document_heading() {
        let dir = scratch(&[("notes.md", "---\ntitle: Real Title\nauthor: Jane Doe\ncover: false\n---\n# Real Title\n")]);
        let out = dir.join("out.html");
        run(dir.join("notes.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(!content.contains("<h1>Real Title</h1>"), "the generated title block is gone: {content}");
        assert!(!content.contains("Jane Doe"), "its byline goes with it: {content}");
        assert!(content.contains("<h1 id=\"real-title\">Real Title</h1>"), "the document's own heading carries it: {content}");
        assert!(content.contains("<title>Real Title</title>"), "the tab still needs a name: {content}");
    }

    #[test]
    fn cover_false_drops_the_typst_cover_page() {
        let dir = scratch(&[("notes.md", "---\ntitle: Real Title\ncover: false\n---\n# Real Title\n")]);
        let report = run(dir.join("notes.md").to_str().unwrap(), Format::Pdf, None, true, false).unwrap();
        let typ = fs::read_to_string(report.typ_path.unwrap()).unwrap();
        assert!(!typ.contains("#pagebreak()"), "no cover page, so nothing to break after: {typ}");
        assert!(typ.starts_with("#outline()"), "{typ}");
        assert!(typ.contains("= Real Title"), "the document's own heading carries the title: {typ}");
    }

    #[test]
    fn cover_true_keeps_the_typst_cover_page_and_drops_the_repeat() {
        let dir = scratch(&[("notes.md", "---\ntitle: Real Title\ncover: true\n---\n# Real Title\n\ntext\n")]);
        let report = run(dir.join("notes.md").to_str().unwrap(), Format::Pdf, None, true, false).unwrap();
        let typ = fs::read_to_string(report.typ_path.unwrap()).unwrap();
        assert!(typ.contains("#text(size: 28pt, weight: \"bold\")[Real Title]"), "{typ}");
        assert!(!typ.contains("= Real Title"), "the body's own repeat is gone: {typ}");
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

    /// A pair with an artifact and no `label=` takes its own `name=`
    /// (decision 63).
    #[test]
    fn figure_labels_defaults_to_the_blocks_own_name() {
        let d = doc("```python name=chart produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=chart hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "chart.png".to_string()));
        let mut diags = Diags::new("t.md");
        let labels = figure_labels(&d, &HashMap::new(), &images, &mut diags);
        assert_eq!(labels.get(&0), Some(&"chart".to_string()));
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    #[test]
    fn figure_labels_prefers_a_reader_written_label_over_the_name() {
        let d = doc("```python name=chart label=revenue produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=chart hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "chart.png".to_string()));
        let mut diags = Diags::new("t.md");
        let labels = figure_labels(&d, &HashMap::new(), &images, &mut diags);
        assert_eq!(labels.get(&0), Some(&"revenue".to_string()));
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    /// A named block with no artifact is not a figure (decision 54), so
    /// there is nothing for a reference to point at and nothing to label.
    #[test]
    fn figure_labels_skips_a_block_with_no_artifact() {
        let d = doc("```sh name=a\necho hi\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nhi\n```\n");
        let mut diags = Diags::new("t.md");
        let labels = figure_labels(&d, &HashMap::new(), &HashMap::new(), &mut diags);
        assert!(labels.is_empty());
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    /// A hidden figure keeps its label, so a later reference to it can be
    /// told apart from a reference to nothing at all.
    #[test]
    fn figure_labels_keeps_a_hidden_figures_own_label() {
        let d = doc("```python name=chart weave=hidden produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=chart hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "chart.png".to_string()));
        let mut diags = Diags::new("t.md");
        let labels = figure_labels(&d, &HashMap::new(), &images, &mut diags);
        assert_eq!(labels.get(&0), Some(&"chart".to_string()));
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    #[test]
    fn a_colliding_label_warns_by_line_and_the_second_one_is_dropped() {
        let d = doc("```python name=a label=chart produces=file:one.png\nsavefig()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote one.png\n```\n\n```python name=b label=chart produces=file:two.png\nsavefig()\n```\n\n<!-- dankg:result name=b hash=0000000000000002 -->\n\n```\nwrote two.png\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "one.png".to_string()));
        images.insert(3, (Vec::new(), "two.png".to_string()));
        let mut diags = Diags::new("t.md");
        let labels = figure_labels(&d, &HashMap::new(), &images, &mut diags);
        assert_eq!(labels.get(&0), Some(&"chart".to_string()));
        assert_eq!(labels.get(&3), None, "the second claim is dropped, not suffixed");
        assert_eq!(diags.items().len(), 1, "{:?}", diags.items());
        assert_eq!(diags.items()[0].line, 11, "{:?}", diags.items());
        assert!(diags.items()[0].message.contains("already used by the figure on line 1"), "{:?}", diags.items());
    }

    /// A `label=` colliding with another block's own defaulted `name=`
    /// collides just the same. The rule is about the label that comes
    /// out, not about which attribute it came from.
    #[test]
    fn a_label_colliding_with_a_defaulted_name_collides_too() {
        let d = doc("```python name=chart produces=file:one.png\nsavefig()\n```\n\n<!-- dankg:result name=chart hash=0000000000000001 -->\n\n```\nwrote one.png\n```\n\n```python name=b label=chart produces=file:two.png\nsavefig()\n```\n\n<!-- dankg:result name=b hash=0000000000000002 -->\n\n```\nwrote two.png\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "one.png".to_string()));
        images.insert(3, (Vec::new(), "two.png".to_string()));
        let mut diags = Diags::new("t.md");
        let labels = figure_labels(&d, &HashMap::new(), &images, &mut diags);
        assert_eq!(labels.get(&0), Some(&"chart".to_string()));
        assert_eq!(labels.get(&3), None);
        assert_eq!(diags.items().len(), 1, "{:?}", diags.items());
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

    fn doc(source: &str) -> Document {
        Document::parse(source, &mut Diags::new("t.md"))
    }

    #[test]
    fn bibliography_is_none_when_neither_source_is_configured() {
        let d = doc("# H\n\n[@key]\n");
        let mut diags = Diags::new("t.md");
        assert!(bibliography(&d, "t.md", Path::new("."), &mut diags).is_none());
    }

    #[test]
    fn bibliography_from_a_hayagriva_fence_alone() {
        let d = doc("```hayagriva\nnetwok2019:\n  title: A Paper\n```\n\n[@netwok2019]\n");
        let mut diags = Diags::new("t.md");
        let bib = bibliography(&d, "t.md", Path::new("."), &mut diags).unwrap();
        assert!(bib.entries.contains_key("netwok2019"));
        assert_eq!(bib.order, vec!["netwok2019".to_string()]);
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    #[test]
    fn bibliography_from_a_frontmatter_file_alone() {
        let dir = scratch(&[
            ("refs.yml", "netwok2019:\n  title: A Paper\n"),
            ("a.md", "---\nbibliography: refs.yml\n---\n\n[@netwok2019]\n"),
        ]);
        let d = doc(&fs::read_to_string(dir.join("a.md")).unwrap());
        let mut diags = Diags::new("a.md");
        let bib = bibliography(&d, "a.md", &dir, &mut diags).unwrap();
        assert!(bib.entries.contains_key("netwok2019"));
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    #[test]
    fn bibliography_merges_both_sources_fence_wins_a_duplicate_key() {
        let dir = scratch(&[
            ("refs.yml", "a:\n  title: External\n"),
            ("doc.md", "---\nbibliography: refs.yml\n---\n\n```hayagriva\na:\n  title: Fence\n```\n\n[@a]\n"),
        ]);
        let d = doc(&fs::read_to_string(dir.join("doc.md")).unwrap());
        let mut diags = Diags::new("doc.md");
        let bib = bibliography(&d, "doc.md", &dir, &mut diags).unwrap();
        assert_eq!(bib.entries["a"].title.as_deref(), Some("Fence"));
        assert_eq!(diags.count(crate::diag::Level::Warn), 1);
    }

    #[test]
    fn citation_order_spans_headings_and_list_items() {
        let d = doc("```hayagriva\na:\n  title: A\nb:\n  title: B\n```\n\n## [@b]\n\n- [@a]\n");
        let mut diags = Diags::new("t.md");
        let bib = bibliography(&d, "t.md", Path::new("."), &mut diags).unwrap();
        assert_eq!(bib.order, vec!["b".to_string(), "a".to_string()]);
    }

    #[test]
    fn a_cited_key_absent_from_every_entry_warns() {
        let d = doc("```hayagriva\na:\n  title: A\n```\n\n[@missing]\n");
        let mut diags = Diags::new("t.md");
        let bib = bibliography(&d, "t.md", Path::new("."), &mut diags).unwrap();
        assert_eq!(diags.count(crate::diag::Level::Warn), 1);
        assert!(bib.order.contains(&"missing".to_string()));
    }

    #[test]
    fn an_uncited_entry_never_appears_in_the_ordered_list() {
        let d = doc("```hayagriva\na:\n  title: A\nb:\n  title: B\n```\n\n[@a]\n");
        let mut diags = Diags::new("t.md");
        let bib = bibliography(&d, "t.md", Path::new("."), &mut diags).unwrap();
        assert_eq!(bib.order, vec!["a".to_string()]);
        assert!(bib.entries.contains_key("b"));
    }

    #[test]
    fn a_second_hayagriva_fence_warns_and_is_ignored() {
        let d = doc("```hayagriva\na:\n  title: A\n```\n\n```hayagriva\nb:\n  title: B\n```\n\n[@a]\n");
        let mut diags = Diags::new("t.md");
        let bib = bibliography(&d, "t.md", Path::new("."), &mut diags).unwrap();
        assert!(bib.entries.contains_key("a"));
        assert!(!bib.entries.contains_key("b"));
        assert_eq!(diags.count(crate::diag::Level::Warn), 1);
    }

    #[test]
    fn render_pdf_writes_bibliography_yml_matching_the_merged_raw_text() {
        let dir = scratch(&[("a.md", "```hayagriva\na:\n  title: A\n```\n\n[@a]\n")]);
        run(dir.join("a.md").to_str().unwrap(), Format::Pdf, None, false, false).unwrap();
        let written = fs::read_to_string(dir.join(".dankg/build/weave/bibliography.yml")).unwrap();
        assert_eq!(written, "a:\n  title: A\n");
    }

    #[test]
    fn render_pdf_typ_emits_a_bibliography_call_and_a_real_citation() {
        let dir = scratch(&[("a.md", "```hayagriva\na:\n  title: A\n```\n\n[@a]\n")]);
        let report = run(dir.join("a.md").to_str().unwrap(), Format::Pdf, None, false, false).unwrap();
        let typ = fs::read_to_string(report.typ_path.unwrap()).unwrap();
        assert!(typ.contains("#bibliography(\"bibliography.yml\")"), "{typ}");
        assert!(typ.contains("@a"), "{typ}");
    }

    #[test]
    fn render_pdf_with_no_bibliography_emits_no_bibliography_call() {
        let dir = scratch(&[("a.md", "# H\n\n[@a]\n")]);
        let report = run(dir.join("a.md").to_str().unwrap(), Format::Pdf, None, false, false).unwrap();
        let typ = fs::read_to_string(report.typ_path.unwrap()).unwrap();
        assert!(!typ.contains("#bibliography("), "{typ}");
        assert!(!dir.join(".dankg/build/weave/bibliography.yml").exists());
    }
}
```
