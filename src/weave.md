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
use crate::diag::{Diags, Level};
use crate::eval::files::Files;
use crate::eval::session::corpus_graph_if_needed;
use crate::eval::{plan, result};
use crate::graph::build::{self, file_stem, strip_extension};
use crate::graph::NodeKind;
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

Diagnostics are emitted from one place, on every exit path. They used
to be emitted from two: once before the unresolved-reference bail, and
once after a successful render. A backend returning `Err` propagated
through `?` and reached neither. Every warning the run had collected was
thrown away at exactly the moment an author needed it most.

The bug was found by forcing an artifact copy to fail. A reference to
that figure survived into the `.typ` with no `<fig:...>` label left to
match. Typst refused to compile it. All the author saw was
`label <fig:chart> does not exist in the document`, pointing into
generated `.typ`. The warning that explained the whole thing -- the
`could not copy` one, naming the real file -- had been collected and
dropped. Two errors reached the author. The one naming the real cause
was not among them.

The fix is shape, not a third `emit` call. `run` computes a
`Result<Report, String>` rather than unwrapping one with `?`, emits,
and returns it. There is now no path out of this function that skips
the emit, which is a property the old shape could not have: every new
`?` added to the middle of it would have reintroduced the same bug.

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
    // One slug source for the whole document (decision 77), built after
    // `drop_repeated_title_heading` so it sees the document weave
    // actually renders.
    let maps = slug_maps(&entry_rel, &doc, source.lines().count() as u32, &mut diags);
    let (labels, numbers) = figures(&doc, &tables, &images, &maps.artifacts);
    let slugs = maps.headings;
    let pairs = pair_anchors(&doc, &maps.blocks);
    let (typst_refs, html_refs) = references(&doc, &tables, &labels, &numbers, &slugs, &pairs);
    resolve_references(&doc, &typst_refs, &labels, &numbers, format, &mut diags);
    let bib = bibliography(&doc, &entry_rel, &root, &mut diags);

    // Every unresolved reference (decision 64) and every unresolved
    // citation key (decision 66) is reported before anything is rendered,
    // so one run shows an author every bad line rather than the first.
    // Both passes have run by now, and both raise a real error, so the
    // count in `diags` is the one thing to check.
    let failed = diags.count(Level::Error);
    let report = if failed > 0 {
        Err(format!("{failed} unresolved reference(s) or citation key(s); nothing rendered"))
    } else {
        match format {
            Format::Html => render_html(
                &doc, &title, &config, &root, &tables, &images, &labels, &numbers, &slugs, &html_refs,
                &pairs, bib.as_ref(), output, figures_outside, &mut diags,
            ),
            Format::Pdf => render_pdf(
                &doc, &title, &config, &root, &name, &tables, &images, &labels, &slugs, &typst_refs,
                &pairs, bib.as_ref(), output, toc, figures_outside, &mut diags,
            ),
        }
    };

    diags.sort();
    diags.emit();
    report
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

Every figure a reference can point at needs a slug (decisions 63 and
70\). The slug is the artifact's own path stem: a block writing
`produces=file:data/quarterly.csv` gets `quarterly`. A reference names
the thing on the page. The thing on the page is the artifact. A
reader-written `artifact=` overrides it for one block. It exists
because a path is a filesystem detail. No prose should have to quote
one to point at a figure.

The stem is slugified rather than checked, the same way a heading's
own title is. A derived slug is not something the author typed, so
warning about it would name a mistake nobody made.
`produces=file:Q3 revenue.csv` therefore gets `q3-revenue`.

A derived slug that collides is suffixed for the same reason, by the
same `Slugger` a repeated heading already goes through. Two artifacts
sharing one stem is ordinary: `a.csv` and `a.png` both stem to `a`,
and neither block did anything wrong. They get `a` and `a-1`.

A *declared* `artifact=` is checked and never repaired, for decision
63's own reason: a silently repaired slug is a reference that silently
points at the wrong figure. An unusable one warns and is dropped. One
colliding with a slug already taken warns and is dropped too.

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

A declared slug has to be an anchor both backends can carry. The rule
is a fixed point: `slugify` has to leave it alone. That keeps a figure
slug and a heading slug interchangeable as a fragment. One failing it
warns at its own line and is dropped, the same as a collision.

Stating it as a fixed point rather than as a charset is a bug fix.
`slugify` lowercases, and decision 70 routes a declared slug through
`Slugger` to reserve it against a later derived one. `artifact=Chart`
passed the old charset check, came back from `Slugger` as `chart`, and
left `[[#Chart]]` looking up a fragment nothing carried --
`resolve_references` compares a fragment verbatim. That is a silently
repaired slug pointing at the wrong figure, which decision 63 refuses
outright. Refusing the input is the fix. The warning names the slug
`slugify` would have produced, so the author can paste it. A value of
pure punctuation slugifies to nothing, and that one warns with no
suggestion rather than an empty one.

The rule is Typst's, tightened by CSS. Typst parses a label holding a
letter, a digit, `-`, `_`, `.` or `:` and refuses anything else:
a declared slug of `a+b` reached `typst compile` as `<fig:a+b>` and
failed with `unclosed label`, pointing into generated `.typ` --
exactly the error decision 64 exists to keep an author from ever
seeing. `.` and `:`
are dropped from the set on top of that, because each one changes
what a CSS selector means. `#fig-a.b` selects an id and a class, not
one id. Decision 63 already chose `-` over `:` in an HTML id for that
same reason.

The same walk numbers every figure it finds (decision 65). HTML has no
way to number one for itself. CSS cannot put a counter value from one
element into an `<a>` elsewhere in the document. The one feature that
does exactly that is `target-counter()`. No browser implements it.
So dankg counts. HTML writes the number as literal text. Typst still
counts for itself off the `#figure` it was handed. Both backends walk
the same figure sequence, which is what lands them on the same number.

The count is per kind, never sequential. Typst numbers a table and an
image on two separate counters, confirmed by a real compile. A single
counter would disagree with the PDF on every document holding both.
`kind: table` and `kind: image` are declared rather than inferred for
exactly this reason. Both backends count off one declaration.

A figure neither backend renders takes no number. `weave=hidden` drops
the whole pair, and `weave=output-hidden` drops the artifact half with
it (decision 53). Neither leaves a figure on the page to count. Such a
figure still keeps its label, which is how a reference to a hidden
figure stays distinguishable from a reference to nothing.
`weave=source-hidden` is the other way round. It keeps the artifact
(decision 60). Its own figure renders and counts like any other.

The two maps come back from one walk rather than two. Each renderer
then receives only what it reads -- Typst the labels alone, HTML both
\-- and neither can disagree with the other about which blocks were
figures in the first place.

<!-- dankg:depends target=../src/graph/slug.md#slugify quote="c.is_alphanumeric() || c == '-' || c == '_'" -->
<!-- dankg:depends target=../architecture.md#decision-63-a-figures-label-comes-from-its-blocks-own-name-or-from-label quote="A label holds letters, digits, `-` and `_`, and nothing else." -->
<!-- dankg:depends target=../architecture.md#decision-65-one-numbering-pre-pass-feeds-html-and-typst-still-counts-for-itself quote="The count is per kind, never sequential." -->

```rust name=figures path=weave.rs
/// Every figure's own label and number, by the block index its pair
/// starts at. The label is `artifact=` when the reader wrote one,
/// otherwise the slugified stem of the artifact's own path
/// (decisions 63 and 70). The number is this figure's position among
/// its own kind, counted in document order (decision 65). A block
/// with no artifact in `tables` or `images` is not a figure
/// (decision 54) and is in neither map. A figure neither backend
/// renders is in `labels` and not in `numbers`: it keeps its label
/// and takes no number.
fn figures(
    doc: &Document,
    tables: &HashMap<usize, (String, String)>,
    images: &HashMap<usize, (Vec<u8>, String)>,
    slugs: &HashMap<usize, String>,
) -> (HashMap<usize, String>, HashMap<usize, u32>) {
    let mut labels = HashMap::new();
    let mut numbers = HashMap::new();
    let (mut tables_seen, mut images_seen) = (0u32, 0u32);
    for (index, b) in doc.blocks.iter().enumerate() {
        let Block::Code { info, .. } = b else { continue };
        let is_table = tables.contains_key(&index);
        if !is_table && !images.contains_key(&index) {
            continue;
        }
        if !info.weave_hidden() && !info.weave_output_hidden() {
            let counter = if is_table { &mut tables_seen } else { &mut images_seen };
            *counter += 1;
            numbers.insert(index, *counter);
        }
        // The name is `graph::build`'s, never derived again here
        // (decision 77). A figure with no slug at all is a figure whose
        // artifact `build` refused outright, which it has already warned
        // about at the block's own line.
        if let Some(slug) = slugs.get(&index) {
            labels.insert(index, slug.clone());
        }
    }
    (labels, numbers)
}

```

Every slug this document needs comes from `graph::build`, through one
call (decision 77). Nothing here derives a name a second time.

Three separate derivations lived here before. A heading walk owned
heading slugs, `figures` owned figure labels, and `pair_anchors` owned a
block's own anchor. Each started its own `Slugger`, and none could see
the other two, so weave and the graph could disagree about one document.
A block named `chart` ahead of a heading "Chart" gave the heading
`chart` here and `chart-1` in the graph, which left `[[#chart]]`
resolving to two different things depending on which tool read it.

Both backends read these maps, which is why they are built here rather
than inside either renderer: HTML puts a heading's slug in its own `id`
and in every table-of-contents link, and Typst puts it in a
`<sec:SLUG>` label a reference can reach (decision 67). Two walks were
two chances to disagree. One source cannot disagree with itself.

<!-- dankg:depends target=../architecture.md#decision-67-heading-numbering-belongs-to-the-template-not-to-dankg quote="Every heading carries a `<sec:SLUG>` label" -->

```rust name=slug_maps path=weave.rs
/// Every slug a woven document needs, taken straight from
/// `graph::build` rather than derived a second time (decision 77).
///
/// `headings` is by heading line, `artifacts` and `blocks` by the block
/// index their node came from. One `Slugger` inside `build` assigns all
/// three, in document order, so a block name, a heading title and an
/// artifact stem that collide are suffixed the same way for both tools.
/// Three separate derivations lived here before, and none of them could
/// see the other two.
struct SlugMaps {
    headings: HashMap<u32, String>,
    artifacts: HashMap<usize, String>,
    blocks: HashMap<usize, String>,
}

fn slug_maps(entry_rel: &str, doc: &Document, source_lines: u32, diags: &mut Diags) -> SlugMaps {
    let built = build::build(entry_rel, doc, source_lines, diags);

    // A node carries the line it came from, never the block index weave
    // keys its own maps by, so this is the bridge between the two.
    let mut index_of_line: HashMap<u32, usize> = HashMap::new();
    for (index, b) in doc.blocks.iter().enumerate() {
        if let Block::Code { line, .. } = b {
            index_of_line.insert(*line, index);
        }
    }

    let mut maps =
        SlugMaps { headings: HashMap::new(), artifacts: HashMap::new(), blocks: HashMap::new() };
    for node in &built.nodes {
        match node.kind {
            // `level` 0 is the synthetic file node, which is not a
            // heading anything in the document wrote.
            NodeKind::Heading if node.level > 0 => {
                maps.headings.insert(node.line, node.id.slug.clone());
            }
            NodeKind::Artifact => {
                if let Some(&index) = index_of_line.get(&node.line) {
                    maps.artifacts.insert(index, node.id.slug.clone());
                }
            }
            NodeKind::Block => {
                if let Some(&index) = index_of_line.get(&node.line) {
                    maps.blocks.insert(index, node.id.slug.clone());
                }
            }
            NodeKind::Heading | NodeKind::Relation => {}
        }
    }

    // A leading heading the declared title absorbed has no heading node
    // of its own (decision 62). The file node *is* its node, so the file
    // node's slug is what that heading's own anchor resolves to. Without
    // this the heading would render with no `id` at all, and only when
    // `cover` is not set -- `build` absorbs whenever a `title` is
    // declared, while weave drops the heading only for a cover page.
    if let Some(title) = doc.frontmatter.title() {
        if let Some(line) = build::repeated_title_heading(doc, title) {
            if let Some(entry) = built.entry_node() {
                maps.headings.insert(line, entry.id.slug.clone());
            }
        }
    }
    maps
}

/// Which recognized pairs carry an anchor, and under what slug
/// (decision 71). The slug is `build`'s own for that block.
///
/// An *unpaired* block is absent. Decision 46's box is what a pair
/// renders, and a block with no recorded result renders no box, so there
/// is nothing for an anchor to sit on.
///
/// A block that renders no box is absent for the same reason, and there
/// are two ways to be one. `weave=hidden` drops the pair outright
/// (decision 48). `weave=source-hidden` together with
/// `weave=output-hidden` drops both halves, which leaves the box empty,
/// and an empty box is dropped rather than emitted (decision 53). An
/// anchor on either would resolve to an id the output does not carry,
/// which is worse than not resolving: the author gets a link that goes
/// nowhere instead of a diagnostic naming the line.
fn pair_anchors(doc: &Document, slugs: &HashMap<usize, String>) -> HashMap<usize, String> {
    let mut out = HashMap::new();
    for index in 0..doc.blocks.len() {
        let Some(Block::Code { info, .. }) = doc.blocks.get(index) else { continue };
        if info.weave_hidden() || (info.weave_source_hidden() && info.weave_output_hidden()) {
            continue;
        }
        if result::recognize_pair(&doc.blocks, index).is_some() {
            if let Some(slug) = slugs.get(&index) {
                out.insert(index, slug.clone());
            }
        }
    }
    out
}
```

## Resolving a reference

A wikilink whose name half is empty is a same-file fragment, and weave
resolves it (decision 64). Decision 41's own single-file scope is the
reason a wikilink naming another file still renders as its own plain
text: there is no corpus to resolve that against. A fragment needs no
corpus at all.

`references` builds what a fragment resolves to, once, for both
backends. Each one gets only what it reads. Typst gets a fragment's own
label, `fig:NAME` or `sec:SLUG`, because that is all it needs to write
`@fig:NAME` or `#link(<sec:SLUG>)[text]`. HTML gets an anchor and the
text a bare reference shows -- `fig-chart` and `Table 1` for a figure,
the heading's own slug and its own title for a heading (decisions 65
and 68). Both come out of one walk, so neither backend can disagree
with the other about what resolved.

Figures go in before headings. A figure slug therefore wins a clash
with a heading slug. Decision 63 justified that by declared beating
derived.
Decision 70 takes the justification away: a figure slug is derived
from the artifact's own path unless `artifact=` says otherwise.

The precedence stands on narrower ground. A figure is one element
where a heading is a whole section. The narrower target is the safer
reading of one ambiguous word. An author who meant the heading writes
`artifact=` on the figure to move it out of the way.

A figure neither backend renders is not a target. It has no number
(decision 65), and pointing at markup that is not on the page is not a
reference. It is still in `labels`, which is what lets the resolver say
*why* rather than just "no such thing".

<!-- dankg:depends target=../architecture.md#decision-64-a-same-file-wikilink-resolves-against-whatever-the-document-holds quote="Figures go in before headings, then pairs." -->

```rust name=references path=weave.rs
/// What each same-file fragment resolves to, for both backends
/// (decision 64). The first map is Typst's: a fragment's own label,
/// `fig:NAME` or `sec:SLUG`. The second is HTML's: a fragment's own
/// anchor id, and the text a bare reference to it shows. A figure with
/// no number is absent from both -- neither backend put it on the page.
fn references(
    doc: &Document,
    tables: &HashMap<usize, (String, String)>,
    labels: &HashMap<usize, String>,
    numbers: &HashMap<usize, u32>,
    slugs: &HashMap<u32, String>,
    pairs: &HashMap<usize, String>,
) -> (HashMap<String, String>, HashMap<String, (String, String)>) {
    let mut typst = HashMap::new();
    let mut html = HashMap::new();
    for (index, label) in labels {
        let Some(number) = numbers.get(index) else { continue };
        let kind = if tables.contains_key(index) { "Table" } else { "Figure" };
        typst.insert(label.clone(), format!("fig:{label}"));
        html.insert(label.clone(), (format!("fig-{label}"), format!("{kind} {number}")));
    }
    // A pair's own anchor goes in after a figure's and before a
    // heading's, so the existing precedence is untouched: a figure
    // still wins a clash, a heading still loses one. The display text
    // for a bare reference is the block's own name, which is the
    // nearest thing a pair has to a caption (decision 71).
    for (_, slug) in pairs {
        typst.entry(slug.clone()).or_insert_with(|| format!("blk:{slug}"));
        html.entry(slug.clone()).or_insert_with(|| (format!("blk-{slug}"), slug.clone()));
    }
    for (_, inlines, line) in doc.headings() {
        let Some(slug) = slugs.get(&line) else { continue };
        typst.entry(slug.clone()).or_insert_with(|| format!("sec:{slug}"));
        html.entry(slug.clone()).or_insert_with(|| (slug.clone(), Inline::plain(inlines).trim().to_string()));
    }
    (typst, html)
}
```

An unresolved reference fails the weave. It does not fall back to its
own literal source text, and it does not render as anything else. That
was the earlier design here and it was wrong. A document that renders
with a broken reference in it is a document an author ships. The
warning scrolls past, the page looks like prose, and the reader is the
one who finds the hole. Failing is what decision 47's own stale badge
already does for a stale result, arrived at from a different direction.

dankg resolves and fails first, before either backend runs. Typst
refuses an unresolvable label too. It says
`label <nope> does not exist in the document` and writes no PDF. That
message is useless to an author though, because it points into
generated `.typ` -- a build artifact nobody wrote by hand.
`resolve_references` reports the markdown line instead.

<!-- dankg:depends target=../architecture.md#decision-64-a-same-file-wikilink-resolves-against-whatever-the-document-holds quote="An unresolved reference fails the weave." -->

The whole document is walked before anything fails. An author who
mistyped three labels wants all three lines from one run, not three
runs. Each one carries its own message, because the next action differs
in each. Nothing in the document carries that name. Or a block carries
it and declares no artifact at all. Or it declares one and has no
recorded result yet, which `dankg eval` writes. Or the artifact is
declared and recorded and could not be read, which the warning above it
already names. Or the figure is real and hidden.

```rust name=resolve_references path=weave.rs
/// Every same-file reference in `doc`, checked against `refs`. Reports
/// each unresolved one at its own line as a real error and returns, so
/// `run` can walk the whole document before it gives up (decision 64).
/// The count lives in `diags` rather than here, because decision 66's
/// own unresolved citation keys land in the same place and `run` gives
/// up on both together.
fn resolve_references(
    doc: &Document,
    refs: &HashMap<String, String>,
    labels: &HashMap<usize, String>,
    numbers: &HashMap<usize, u32>,
    format: Format,
    diags: &mut Diags,
) {
    let mut slices = Vec::new();
    walk_inlines(&doc.blocks, &mut slices);
    let mut found = Vec::new();
    for (line, inlines) in slices {
        collect_fragments(inlines, line, &mut found);
    }
    for (fragment, line, bare) in found {
        let Some(anchor) = refs.get(&fragment) else {
            diags.error(line, unresolved_message(doc, &fragment, labels, numbers));
            continue;
        };
        // Decision 71. A pair's anchor sits on the `#block(stroke: ...)`
        // decision 46 emits. Typst cannot `@`-reference a block at all:
        // `typst compile` answers `cannot reference block`. A bare
        // reference is exactly the form that becomes `@anchor`, so this
        // one is refused here, naming the form that does work, rather
        // than reaching the compiler as generated markup the author
        // never wrote. HTML has no such limit and is left alone.
        //
        // The test is what the fragment *resolved to*, never whether
        // some pair happens to share its name. A block written
        // `name=quarterly produces=file:data/quarterly.csv` gives its
        // own name and its artifact's stem the same slug, and the
        // figure wins that key in `references`. The fragment then
        // resolves to `fig:quarterly`, which is a real Typst figure and
        // references perfectly well. Asking the pair map instead
        // refused `example/weave_example/report.md`, whose whole job is
        // to render.
        if bare && format == Format::Pdf && anchor.starts_with("blk:") {
            diags.error(
                line,
                format!(
                    "a bare reference to the block `{fragment}` cannot render in PDF; give it text, as `[[#{fragment}|...]]`"
                ),
            );
        }
    }
}

/// Every `[[#fragment]]` and `[text](#fragment)` in `inlines`, paired
/// with the enclosing block's own line and whether it was written bare. Recurses through emphasis and
/// through a link's own text, since a reference is legal inside either.
fn collect_fragments(inlines: &[Inline], line: u32, out: &mut Vec<(String, u32, bool)>) {
    for i in inlines {
        match i {
            // The third element is "bare": a wikilink with no text half,
            // the one form that becomes `@anchor` in Typst rather than a
            // `#link`. Decision 71 is the only reader of it.
            Inline::WikiLink { target, label } => {
                if let Some(fragment) = target.strip_prefix('#') {
                    out.push((fragment.to_string(), line, label.is_none()));
                }
            }
            Inline::Link { dest, text, .. } => {
                if let Some(fragment) = dest.strip_prefix('#') {
                    // A markdown link always carries its own text, so it
                    // is never bare.
                    out.push((fragment.to_string(), line, false));
                }
                collect_fragments(text, line, out);
            }
            Inline::Emph { inner, .. } | Inline::Strong { inner, .. } => collect_fragments(inner, line, out),
            Inline::Text(_) | Inline::Code(_) | Inline::Citation { .. } => {}
            Inline::SoftBreak | Inline::HardBreak => {}
        }
    }
}

/// Why one fragment did not resolve. One answer per next action
/// (decision 64): unhide the figure, write a `produces=file:`, run
/// `dankg eval`, fix the file the block writes, or fix the typo.
fn unresolved_message(
    doc: &Document,
    fragment: &str,
    labels: &HashMap<usize, String>,
    numbers: &HashMap<usize, u32>,
) -> String {
    let hidden = labels.iter().find(|(index, label)| label.as_str() == fragment && !numbers.contains_key(index));
    if let Some((index, _)) = hidden {
        let how = match doc.blocks.get(*index) {
            Some(Block::Code { info, .. }) if info.weave_output_hidden() => "weave=output-hidden",
            _ => "weave=hidden",
        };
        return format!("`{fragment}` is a figure hidden by `{how}`; there is nothing on the page to point at");
    }
    let named = doc.blocks.iter().enumerate().find(|(_, b)| match b {
        Block::Code { info, .. } => info.name() == Some(fragment),
        _ => false,
    });
    let Some((index, Block::Code { info, .. })) = named else {
        return format!("nothing in this document is named `{fragment}`");
    };
    let Some(raw) = info.produces() else {
        return format!("`{fragment}` names a block, but it declares no `produces=file:` artifact to be a figure of");
    };
    // Decision 72. The block is real and its artifact resolved; the
    // name asked for just belongs to the block rather than to the
    // figure. Answered before either artifact branch below, both of
    // which would otherwise report a read that actually succeeded.
    if let Some(slug) = labels.get(&index) {
        return format!("`{fragment}` names the block; its figure is `{slug}` -- write `[[#{slug}]]`");
    }
    if result::recorded_hash(doc, index, fragment).is_none() {
        return format!("`{fragment}` produces={raw}, but has no recorded result yet; `dankg eval` writes one");
    }
    format!("`{fragment}` produces={raw}, which could not be read; the warning above says why")
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
                        diags.error(line, format!("citation key `{k}` is not in the bibliography"));
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
    numbers: &HashMap<usize, u32>,
    slugs: &HashMap<u32, String>,
    refs: &HashMap<String, (String, String)>,
    pairs: &HashMap<usize, String>,
    bibliography: Option<&Bibliography>,
    output: Option<&str>,
    figures_outside: bool,
    diags: &mut Diags,
) -> Result<Report, String> {
    let extra_css = config.weave("html").and_then(|w| w.css).and_then(|rel| read_asset(root, &rel, "css", diags));
    let html_bib = bibliography.map(|bib| weave_html::Bibliography { entries: &bib.entries, order: &bib.order });
    let rendered = weave_html::render(
        doc, title, extra_css.as_deref(), tables, images, labels, numbers, slugs, refs, pairs,
        figures_outside, html_bib.as_ref(), diags,
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
identical string. The two can never name different files.

A copy that fails stops the weave, at the block's own line, before
Typst runs. It used to be dropped from the map `typst::render` sees
and warned about instead, on the "misconfigured is reported, not
fatal" shape everything else weave reads off disk follows. Decision 64
broke that shape. `weave::references` has already resolved a reference
to that figure by the time the copy is attempted. The reference went
out as `@fig:chart` with no `<fig:chart>` left in the document to match
it. Typst then refused to compile, naming a label in generated
`.typ` that nobody wrote by hand -- the one error decision 64 exists
to keep an author from seeing. A probe forced it, by writing a regular
file where `build_dir/assets/` has to be a directory.

Dropping the reference instead was the alternative. It keeps the PDF
building, at the cost of a page carrying prose where a reference was,
which is the silent hole decision 64 refuses. The PDF is wrong either
way once an artifact is missing. Failing says so.

Every failed copy is reported before the first one gives up, the same
walk-then-fail shape decisions 64 and 66 already use. `typst::render`
now receives `images` unchanged, since there is no longer a case where
it should see fewer than the document declares.

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
    slugs: &HashMap<u32, String>,
    refs: &HashMap<String, String>,
    pairs: &HashMap<usize, String>,
    bibliography: Option<&Bibliography>,
    output: Option<&str>,
    toc: bool,
    figures_outside: bool,
    diags: &mut Diags,
) -> Result<Report, String> {
    let build_dir = root.join(".dankg").join("build").join("weave");
    let assets_dir = build_dir.join("assets");
    let mut uncopied = 0;
    for (index, (bytes, resolved)) in images {
        let dest = assets_dir.join(resolved);
        let result = dest
            .parent()
            .map(fs::create_dir_all)
            .unwrap_or(Ok(()))
            .and_then(|()| fs::write(&dest, bytes));
        if let Err(e) = result {
            uncopied += 1;
            let line = match doc.blocks.get(*index) {
                Some(Block::Code { line, .. }) => *line,
                _ => 0,
            };
            diags.error(line, format!("could not copy `{resolved}` into the build directory ({e})"));
        }
    }
    // Every failed copy is reported before the first one gives up, the
    // same walk-then-fail shape decisions 64 and 66 use.
    if uncopied > 0 {
        return Err(format!("{uncopied} artifact(s) could not be copied into the build directory; nothing rendered"));
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
        typst::render(doc, title, toc, tables, images, labels, slugs, refs, pairs, figures_outside, bib_summary.as_ref(), diags);
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

    /// Every bad line in one run, and no file of any kind on disk
    /// (decision 64).
    #[test]
    fn three_unresolved_references_fail_the_weave_and_write_nothing() {
        let dir = scratch(&[("a.md", "# Intro\n\nSee [[#one]] and [[#two]] and [x](#three).\n")]);
        let out = dir.join("out.html");
        let result = run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false);
        let err = result.err().expect("an unresolved reference must fail the weave");
        assert!(err.contains("3 unresolved reference(s) or citation key(s)"), "{err}");
        assert!(!out.exists(), "no page is written for a document that failed to resolve");
    }

    #[test]
    fn a_resolving_heading_reference_weaves_normally() {
        let dir = scratch(&[("a.md", "# Intro\n\nSee [[#intro]] and [the start](#intro).\n")]);
        let out = dir.join("out.html");
        run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("<a href=\"#intro\">Intro</a>"), "{content}");
        assert!(content.contains("<a href=\"#intro\">the start</a>"), "{content}");
    }

    /// A reference to a real figure resolves against the label decision 63
    /// gave it, and reads as the number decision 65 counted for it.
    #[test]
    fn a_figure_reference_resolves_end_to_end() {
        let dir = scratch(&[
            ("a.md", "```python name=t produces=file:data.csv caption=\"Revenue\"\nrun()\n```\n\n<!-- dankg:result name=t hash=0000000000000001 -->\n\n```\nok\n```\n\nSee [[#data]].\n"),
            ("data.csv", "x,y\n1,2\n"),
        ]);
        let out = dir.join("out.html");
        run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        // The slug is the artifact's own stem, `data`, never the
        // block's `name=t` (decision 70).
        assert!(content.contains("<figure class=\"table-figure\" id=\"fig-data\">"), "{content}");
        assert!(content.contains("<figcaption>Table 1: Revenue</figcaption>"), "{content}");
        assert!(content.contains("<a href=\"#fig-data\">Table 1</a>"), "{content}");
    }

    /// Decision 71 is what changed this. A reference naming a rendered
    /// pair used to fail the weave, because only the artifact carried a
    /// name. The pair itself is addressable now, so `[[#t]]` resolves to
    /// the whole code-and-output unit and the weave succeeds.
    #[test]
    fn a_reference_naming_a_rendered_pair_resolves_to_the_pair() {
        let dir = scratch(&[
            ("a.md", "```python name=t produces=file:data.csv caption=\"Revenue\"\nrun()\n```\n\n<!-- dankg:result name=t hash=0000000000000001 -->\n\n```\nok\n```\n\nSee [[#t]].\n"),
            ("data.csv", "x,y\n1,2\n"),
        ]);
        let out = dir.join("out.html");
        let result = run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false);
        assert!(result.is_ok(), "{:?}", result.as_ref().err());
        let html = std::fs::read_to_string(&out).unwrap();
        assert!(html.contains("id=\"blk-t\""), "the pair carries its own anchor: {html}");
        assert!(html.contains("href=\"#blk-t\""), "and the reference points at it: {html}");
    }

    /// Decision 72's own message survives, on the one case decision 71
    /// leaves it. A `weave=hidden` pair renders no box, so it carries no
    /// anchor, while `figures` still keeps its artifact's label. A
    /// fragment naming the block therefore still misses, and the message
    /// still names the slug that would have worked.
    #[test]
    fn a_reference_naming_a_hidden_pair_still_says_which_slug_the_figure_has() {
        let dir = scratch(&[
            ("a.md", "```python name=t produces=file:data.csv caption=\"Revenue\" weave=hidden\nrun()\n```\n\n<!-- dankg:result name=t hash=0000000000000001 -->\n\n```\nok\n```\n\nSee [[#t]].\n"),
            ("data.csv", "x,y\n1,2\n"),
        ]);
        let out = dir.join("out.html");
        let result = run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false);
        let err = result.err().expect("a hidden pair carries no anchor, so the reference must still fail");
        assert!(err.contains("unresolved reference"), "{err}");
        assert!(!out.exists(), "nothing is written on an unresolved reference");
    }

    /// Decision 71's own PDF limit. Typst cannot `@`-reference a block at
    /// all, and a bare reference is exactly the form that becomes
    /// `@anchor`. It is refused here, naming the form that works, rather
    /// than reaching `typst compile` as `cannot reference block`.
    #[test]
    fn a_bare_reference_to_a_pair_is_refused_for_pdf_and_allowed_for_html() {
        let src = "```python name=t produces=file:data.csv caption=\"Revenue\"\nrun()\n```\n\n<!-- dankg:result name=t hash=0000000000000001 -->\n\n```\nok\n```\n\nSee [[#t]].\n";
        let dir = scratch(&[("a.md", src), ("data.csv", "x,y\n1,2\n")]);
        let pdf = dir.join("out.pdf");
        let err = run(dir.join("a.md").to_str().unwrap(), Format::Pdf, Some(pdf.to_str().unwrap()), true, false)
            .err()
            .expect("a bare reference to a pair cannot render in PDF");
        assert!(err.contains("unresolved reference"), "{err}");

        // The very same document renders as HTML, which has no such limit.
        let html = dir.join("out.html");
        let ok = run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(html.to_str().unwrap()), true, false);
        assert!(ok.is_ok(), "{:?}", ok.as_ref().err());
    }

    /// The labelled form is what the refusal above names, so it has to
    /// actually work in PDF.
    #[test]
    fn a_labelled_reference_to_a_pair_renders_for_pdf() {
        let src = "```python name=t produces=file:data.csv caption=\"Revenue\"\nrun()\n```\n\n<!-- dankg:result name=t hash=0000000000000001 -->\n\n```\nok\n```\n\nSee [[#t|the run]].\n";
        let dir = scratch(&[("a.md", src), ("data.csv", "x,y\n1,2\n")]);
        let typ = dir.join("out.typ");
        let result = run(dir.join("a.md").to_str().unwrap(), Format::Pdf, Some(typ.to_str().unwrap()), true, false);
        assert!(result.is_ok(), "{:?}", result.as_ref().err());
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

    /// A pair with an artifact and no `artifact=` takes the artifact's
    /// own path stem, never the block's `name=` (decision 70). The two
    /// differ here on purpose: a test where they agree would pass under
    /// either rule.
    #[test]
    fn figures_defaults_to_the_artifacts_own_path_stem() {
        let d = doc("```python name=chart produces=file:data/revenue.png\nsavefig()\n```\n\n<!-- dankg:result name=chart hash=0000000000000001 -->\n\n```\nwrote revenue.png\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "data/revenue.png".to_string()));
        let mut diags = Diags::new("t.md");
        let (labels, _) = figures(&d, &HashMap::new(), &images, &maps_of(&d).artifacts);
        assert_eq!(labels.get(&0), Some(&"revenue".to_string()), "the stem, not `chart`");
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    #[test]
    fn figures_prefers_a_declared_artifact_over_the_path() {
        let d = doc("```python name=chart artifact=revenue produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=chart hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "chart.png".to_string()));
        let mut diags = Diags::new("t.md");
        let (labels, _) = figures(&d, &HashMap::new(), &images, &maps_of(&d).artifacts);
        assert_eq!(labels.get(&0), Some(&"revenue".to_string()));
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    /// A named block with no artifact is not a figure (decision 54), so
    /// there is nothing for a reference to point at and nothing to label.
    #[test]
    fn figures_skips_a_block_with_no_artifact() {
        let d = doc("```sh name=a\necho hi\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nhi\n```\n");
        let mut diags = Diags::new("t.md");
        let (labels, _) = figures(&d, &HashMap::new(), &HashMap::new(), &maps_of(&d).artifacts);
        assert!(labels.is_empty());
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    /// A hidden figure keeps its label, so a later reference to it can be
    /// told apart from a reference to nothing at all.
    #[test]
    fn figures_keeps_a_hidden_figures_own_label() {
        // `name=plot`, not `name=chart`: a block whose name matches its own
        // artifact stem takes the bare slug and leaves the artifact suffixed
        // (decision 77), which is a different test, below.
        let d = doc("```python name=plot weave=hidden produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=plot hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "chart.png".to_string()));
        let mut diags = Diags::new("t.md");
        let (labels, _) = figures(&d, &HashMap::new(), &images, &maps_of(&d).artifacts);
        assert_eq!(labels.get(&0), Some(&"chart".to_string()));
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    /// `artifact=a+b` reached `typst compile` as `<fig:a+b>` and failed
    /// with `unclosed label`, pointing into generated `.typ`. Refusing
    /// it is what keeps that error off an author's screen.
    ///
    /// The refusal moved to `graph::build` with decision 77, and with it
    /// the outcome: the declared slug is refused, and the artifact falls
    /// back to its own path stem rather than going unnamed. That is what
    /// decision 70 always specified. `figures` dropped it entirely
    /// before, which left the two tools disagreeing about one document.
    #[test]
    fn a_declared_slug_typst_cannot_parse_is_refused_and_falls_back_to_the_path() {
        let d = doc("```python name=a artifact=\"a+b\" produces=file:one.png\nrun()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nok\n```\n");
        let images: HashMap<usize, (Vec<u8>, String)> = [(0, (Vec::new(), "one.png".to_string()))].into_iter().collect();
        let (maps, diags) = maps_and_diags(&d);
        let (labels, numbers) = figures(&d, &HashMap::new(), &images, &maps.artifacts);
        assert_eq!(labels.get(&0), Some(&"one".to_string()), "falls back to the path stem");
        assert_eq!(numbers.get(&0), Some(&1), "the figure itself still renders, so it still counts");
        assert_eq!(diags.items().len(), 1, "{:?}", diags.items());
        assert_eq!(diags.items()[0].line, 1, "{:?}", diags.items());
        assert!(diags.items()[0].message.contains("may hold only lowercase letters"), "{:?}", diags.items());
        assert!(diags.items()[0].message.contains("try `ab`"), "the warning names a usable slug");
    }

    /// The rule does not apply to a derived slug. A path stem is not
    /// something the author typed as an anchor, so it is slugified
    /// rather than checked, and nothing warns (decision 70).
    #[test]
    fn a_derived_slug_is_slugified_rather_than_warned_about() {
        let d = doc("```python name=a produces=file:Q3-Revenue(final).png\nrun()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nok\n```\n");
        let images: HashMap<usize, (Vec<u8>, String)> = [(0, (Vec::new(), "Q3-Revenue(final).png".to_string()))].into_iter().collect();
        let mut diags = Diags::new("t.md");
        let (labels, _) = figures(&d, &HashMap::new(), &images, &maps_of(&d).artifacts);
        assert_eq!(labels.get(&0), Some(&"q3-revenuefinal".to_string()), "lowercased, parens dropped");
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    /// Two artifacts sharing one stem is ordinary, and neither block did
    /// anything wrong. The second is suffixed, never warned about --
    /// the opposite of a declared collision (decision 70).
    #[test]
    fn two_derived_slugs_sharing_a_stem_are_suffixed_silently() {
        let d = doc("```python name=a produces=file:out.csv\nrun()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nok\n```\n\n```python name=b produces=file:out.png\nrun()\n```\n\n<!-- dankg:result name=b hash=0000000000000002 -->\n\n```\nok\n```\n");
        let tables: HashMap<usize, (String, String)> = [(0, ("csv".to_string(), "x\n1\n".to_string()))].into_iter().collect();
        let images: HashMap<usize, (Vec<u8>, String)> = [(3, (Vec::new(), "out.png".to_string()))].into_iter().collect();
        let mut diags = Diags::new("t.md");
        let (labels, _) = figures(&d, &tables, &images, &maps_of(&d).artifacts);
        assert_eq!(labels.get(&0), Some(&"out".to_string()));
        assert_eq!(labels.get(&3), Some(&"out-1".to_string()), "suffixed, not dropped");
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    /// An uppercase declared slug used to pass the charset check, then
    /// come back from `Slugger` lowercased, leaving `[[#Chart]]`
    /// pointing at nothing -- a fragment is compared verbatim. The
    /// symptom is the miss, so that is what this pins.
    #[test]
    fn an_uppercase_declared_slug_is_refused_rather_than_lowercased() {
        let d = doc("```python name=a artifact=Chart produces=file:one.png\nrun()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nok\n```\n");
        let images: HashMap<usize, (Vec<u8>, String)> = [(0, (Vec::new(), "one.png".to_string()))].into_iter().collect();
        let (maps, diags) = maps_and_diags(&d);
        let (labels, _) = figures(&d, &HashMap::new(), &images, &maps.artifacts);
        assert_eq!(labels.get(&0), Some(&"one".to_string()), "never silently lowercased to `chart`");
        assert_eq!(diags.items().len(), 1, "{:?}", diags.items());
        assert!(diags.items()[0].message.contains("try `chart`"), "{:?}", diags.items());
    }

    #[test]
    /// Decision 77. One namespace means the block's own name is reserved
    /// before its artifact's stem, so a block written
    /// `name=chart produces=file:chart.png` keeps `chart` for itself and
    /// leaves the artifact `chart-1`. Decision 20 is what forces the
    /// order: a block `dankg eval` runs as `chart` may not appear in the
    /// graph under another name, so the artifact is the one that moves.
    ///
    /// The practical consequence is worth stating. `[[#chart]]` reaches
    /// the code-and-output unit here, not the figure. An author who wants
    /// the bare name on the figure gives the block a different one.
    #[test]
    fn a_block_name_wins_the_bare_slug_and_its_artifact_takes_the_suffix() {
        let d = doc("```python name=chart produces=file:chart.png\nrun()\n```\n\n<!-- dankg:result name=chart hash=0000000000000001 -->\n\n```\nok\n```\n");
        let maps = maps_of(&d);
        assert_eq!(maps.blocks.get(&0), Some(&"chart".to_string()), "the block keeps its own name");
        assert_eq!(maps.artifacts.get(&0), Some(&"chart-1".to_string()), "the artifact takes the suffix");
    }

    /// The same document, woven: weave agrees with `dankg graph` about
    /// both names, which is the whole point of decision 77.
    #[test]
    fn a_heading_colliding_with_a_block_name_is_suffixed_the_way_the_graph_suffixes_it() {
        let d = doc("```sh name=chart\necho hi\n```\n\n# Chart\n");
        let maps = maps_of(&d);
        assert_eq!(maps.blocks.get(&0), Some(&"chart".to_string()), "the block came first");
        // The heading is on line 5. It used to take `chart` here and
        // `chart-1` in the graph, which left `[[#chart]]` meaning two
        // different things depending on which tool read it.
        assert_eq!(maps.headings.get(&5), Some(&"chart-1".to_string()), "{:?}", maps.headings);
    }

    #[test]
    fn a_colliding_declared_slug_warns_by_line_and_falls_back_to_its_path() {
        let d = doc("```python name=a artifact=chart produces=file:one.png\nsavefig()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote one.png\n```\n\n```python name=b artifact=chart produces=file:two.png\nsavefig()\n```\n\n<!-- dankg:result name=b hash=0000000000000002 -->\n\n```\nwrote two.png\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "one.png".to_string()));
        images.insert(3, (Vec::new(), "two.png".to_string()));
        let (maps, diags) = maps_and_diags(&d);
        let (labels, _) = figures(&d, &HashMap::new(), &images, &maps.artifacts);
        assert_eq!(labels.get(&0), Some(&"chart".to_string()), "the first claim stands");
        assert_eq!(labels.get(&3), Some(&"two".to_string()), "the second falls back, never suffixed");
        assert_eq!(diags.items().len(), 1, "{:?}", diags.items());
        assert_eq!(diags.items()[0].line, 11, "{:?}", diags.items());
        assert!(diags.items()[0].message.contains("already taken"), "{:?}", diags.items());
    }

    /// A declared `artifact=` colliding with an earlier *derived* slug
    /// is dropped just the same. The rule is about the slug that comes
    /// out, not about which side declared it. The derived one keeps its
    /// own claim, since it got there first.
    #[test]
    fn a_declared_slug_colliding_with_a_derived_one_falls_back_too() {
        let d = doc("```python name=a produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n\n```python name=b artifact=chart produces=file:two.png\nsavefig()\n```\n\n<!-- dankg:result name=b hash=0000000000000002 -->\n\n```\nwrote two.png\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "chart.png".to_string()));
        images.insert(3, (Vec::new(), "two.png".to_string()));
        let (maps, diags) = maps_and_diags(&d);
        let (labels, _) = figures(&d, &HashMap::new(), &images, &maps.artifacts);
        assert_eq!(labels.get(&0), Some(&"chart".to_string()), "derived, and first");
        assert_eq!(labels.get(&3), Some(&"two".to_string()), "the declaration falls back to its path");
        assert_eq!(diags.items().len(), 1, "{:?}", diags.items());
    }

    /// Typst counts a table and an image on two separate counters, so
    /// this walk does too (decision 65). A single counter would disagree
    /// with the PDF on exactly this document.
    #[test]
    fn figures_numbers_each_kind_on_its_own_counter() {
        let d = doc("```python name=t1 produces=file:t_one.csv\nrun()\n```\n\n<!-- dankg:result name=t1 hash=0000000000000001 -->\n\n```\nok\n```\n\n```python name=i1 produces=file:i_one.png\nrun()\n```\n\n<!-- dankg:result name=i1 hash=0000000000000002 -->\n\n```\nok\n```\n\n```python name=t2 produces=file:t_two.csv\nrun()\n```\n\n<!-- dankg:result name=t2 hash=0000000000000003 -->\n\n```\nok\n```\n\n```python name=i2 produces=file:i_two.png\nrun()\n```\n\n<!-- dankg:result name=i2 hash=0000000000000004 -->\n\n```\nok\n```\n");
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "x\n1\n".to_string()));
        tables.insert(6, ("csv".to_string(), "x\n2\n".to_string()));
        let mut images = HashMap::new();
        images.insert(3, (Vec::new(), "i_one.png".to_string()));
        images.insert(9, (Vec::new(), "i_two.png".to_string()));
        let mut diags = Diags::new("t.md");
        let (_, numbers) = figures(&d, &tables, &images, &maps_of(&d).artifacts);
        assert_eq!(numbers.get(&0), Some(&1), "first table");
        assert_eq!(numbers.get(&3), Some(&1), "first image, its own counter");
        assert_eq!(numbers.get(&6), Some(&2), "second table");
        assert_eq!(numbers.get(&9), Some(&2), "second image");
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    /// `weave=hidden` leaves no figure on the page, so it takes no number
    /// and never shifts the one after it.
    #[test]
    fn figures_never_numbers_a_hidden_figure() {
        let d = doc("```python name=first weave=hidden produces=file:a.png\nrun()\n```\n\n<!-- dankg:result name=first hash=0000000000000001 -->\n\n```\nok\n```\n\n```python name=second produces=file:b.png\nrun()\n```\n\n<!-- dankg:result name=second hash=0000000000000002 -->\n\n```\nok\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "a.png".to_string()));
        images.insert(3, (Vec::new(), "b.png".to_string()));
        let mut diags = Diags::new("t.md");
        let (labels, numbers) = figures(&d, &HashMap::new(), &images, &maps_of(&d).artifacts);
        assert_eq!(numbers.get(&0), None, "hidden, so not counted");
        assert_eq!(numbers.get(&3), Some(&1), "the visible one is still Figure 1");
        assert_eq!(labels.get(&0), Some(&"a".to_string()), "a hidden figure keeps its label");
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    /// `weave=output-hidden` drops the artifact half with the output
    /// (decision 53), so there is no figure left to count either.
    #[test]
    fn figures_never_numbers_an_output_hidden_figure() {
        let d = doc("```python name=first weave=output-hidden produces=file:a.png\nrun()\n```\n\n<!-- dankg:result name=first hash=0000000000000001 -->\n\n```\nok\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "a.png".to_string()));
        let mut diags = Diags::new("t.md");
        let (labels, numbers) = figures(&d, &HashMap::new(), &images, &maps_of(&d).artifacts);
        assert!(numbers.is_empty());
        assert_eq!(labels.get(&0), Some(&"a".to_string()));
    }

    /// `weave=source-hidden` keeps the artifact (decision 60), so its own
    /// figure renders and counts like any other.
    #[test]
    fn figures_numbers_a_source_hidden_figure() {
        let d = doc("```python name=a weave=source-hidden produces=file:a.png\nrun()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nok\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "a.png".to_string()));
        let mut diags = Diags::new("t.md");
        let (_, numbers) = figures(&d, &HashMap::new(), &images, &maps_of(&d).artifacts);
        assert_eq!(numbers.get(&0), Some(&1));
    }

    /// Numbering and slugging are separate. A figure whose declared
    /// slug was dropped as a collision still renders, so it still
    /// counts.
    #[test]
    fn a_figure_whose_declared_slug_was_refused_is_still_numbered() {
        let d = doc("```python name=a artifact=chart produces=file:one.png\nrun()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nok\n```\n\n```python name=b artifact=chart produces=file:two.png\nrun()\n```\n\n<!-- dankg:result name=b hash=0000000000000002 -->\n\n```\nok\n```\n");
        let mut images = HashMap::new();
        images.insert(0, (Vec::new(), "one.png".to_string()));
        images.insert(3, (Vec::new(), "two.png".to_string()));
        let (maps, diags) = maps_and_diags(&d);
        let (labels, numbers) = figures(&d, &HashMap::new(), &images, &maps.artifacts);
        assert_eq!(labels.get(&3), Some(&"two".to_string()), "refused, so it takes its path");
        assert_eq!(numbers.get(&3), Some(&2), "the figure itself still counts");
        assert_eq!(diags.items().len(), 1, "{:?}", diags.items());
    }

    fn one_image_figure(source: &str) -> (Document, HashMap<usize, (Vec<u8>, String)>) {
        (doc(source), [(0usize, (Vec::new(), "chart.png".to_string()))].into_iter().collect())
    }

    #[test]
    fn references_resolves_a_table_figure_to_both_backends_own_shapes() {
        let d = doc("```python name=t produces=file:a.csv\nrun()\n```\n\n<!-- dankg:result name=t hash=0000000000000001 -->\n\n```\nok\n```\n");
        let tables: HashMap<usize, (String, String)> = [(0, ("csv".to_string(), "x\n1\n".to_string()))].into_iter().collect();
        let labels = [(0usize, "t".to_string())].into_iter().collect();
        let numbers = [(0usize, 1u32)].into_iter().collect();
        let (typst, html) = references(&d, &tables, &labels, &numbers, &HashMap::new(), &HashMap::new());
        assert_eq!(typst.get("t"), Some(&"fig:t".to_string()));
        assert_eq!(html.get("t"), Some(&("fig-t".to_string(), "Table 1".to_string())));
    }

    #[test]
    fn references_calls_an_image_figure_a_figure_not_a_table() {
        let (d, images) = one_image_figure("```python name=c produces=file:chart.png\nrun()\n```\n\n<!-- dankg:result name=c hash=0000000000000001 -->\n\n```\nok\n```\n");
        let _ = &images;
        let labels = [(0usize, "c".to_string())].into_iter().collect();
        let numbers = [(0usize, 2u32)].into_iter().collect();
        let (_, html) = references(&d, &HashMap::new(), &labels, &numbers, &HashMap::new(), &HashMap::new());
        assert_eq!(html.get("c"), Some(&("fig-c".to_string(), "Figure 2".to_string())));
    }

    /// A heading resolves too (decision 64). HTML shows its own title
    /// rather than a number, since HTML numbers no heading (decision 68).
    #[test]
    fn references_resolves_a_heading_to_its_slug_and_its_title() {
        let d = doc("# The Design\n");
        let slugs = maps_of(&d).headings;
        let (typst, html) = references(&d, &HashMap::new(), &HashMap::new(), &HashMap::new(), &slugs, &HashMap::new());
        assert_eq!(typst.get("the-design"), Some(&"sec:the-design".to_string()));
        assert_eq!(html.get("the-design"), Some(&("the-design".to_string(), "The Design".to_string())));
    }

    /// Declared beats derived: a figure label is written, a heading slug is
    /// computed from prose.
    #[test]
    fn a_figure_label_wins_a_clash_with_a_heading_slug() {
        let d = doc("# Chart\n\n```python name=chart produces=file:chart.png\nrun()\n```\n\n<!-- dankg:result name=chart hash=0000000000000001 -->\n\n```\nok\n```\n");
        let slugs = maps_of(&d).headings;
        let labels = [(1usize, "chart".to_string())].into_iter().collect();
        let numbers = [(1usize, 1u32)].into_iter().collect();
        let (typst, _) = references(&d, &HashMap::new(), &labels, &numbers, &slugs, &HashMap::new());
        assert_eq!(typst.get("chart"), Some(&"fig:chart".to_string()));
    }

    #[test]
    fn a_hidden_figure_is_not_a_reference_target() {
        let labels = [(0usize, "c".to_string())].into_iter().collect();
        let d = doc("```python name=c weave=hidden produces=file:chart.png\nrun()\n```\n\n<!-- dankg:result name=c hash=0000000000000001 -->\n\n```\nok\n```\n");
        let (typst, html) = references(&d, &HashMap::new(), &labels, &HashMap::new(), &HashMap::new(), &HashMap::new());
        assert!(typst.is_empty());
        assert!(html.is_empty());
    }

    #[test]
    fn resolve_references_passes_a_document_whose_references_all_resolve() {
        let d = doc("# Intro\n\nSee [[#intro]] and [the start](#intro).\n");
        let slugs = maps_of(&d).headings;
        let (typst, _) = references(&d, &HashMap::new(), &HashMap::new(), &HashMap::new(), &slugs, &HashMap::new());
        let mut diags = Diags::new("t.md");
        resolve_references(&d, &typst, &HashMap::new(), &HashMap::new(), Format::Html, &mut diags);
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    /// One run reports every bad line, never just the first (decision 64).
    #[test]
    fn resolve_references_reports_every_unresolved_reference_in_one_walk() {
        let d = doc("# Intro\n\nSee [[#one]].\n\nAnd [[#two]] and [x](#three).\n");
        let slugs = maps_of(&d).headings;
        let (typst, _) = references(&d, &HashMap::new(), &HashMap::new(), &HashMap::new(), &slugs, &HashMap::new());
        let mut diags = Diags::new("t.md");
        resolve_references(&d, &typst, &HashMap::new(), &HashMap::new(), Format::Html, &mut diags);
        assert_eq!(diags.count(Level::Error), 3, "{:?}", diags.items());
    }

    #[test]
    fn an_unresolved_reference_to_nothing_says_so() {
        let d = doc("Text [[#nope]].\n");
        let mut diags = Diags::new("t.md");
        resolve_references(&d, &HashMap::new(), &HashMap::new(), &HashMap::new(), Format::Html, &mut diags);
        assert!(diags.items()[0].message.contains("nothing in this document is named `nope`"), "{:?}", diags.items());
    }

    #[test]
    fn a_reference_to_a_named_block_that_is_no_figure_says_so() {
        let d = doc("```sh name=a\necho hi\n```\n\nText [[#a]].\n");
        let mut diags = Diags::new("t.md");
        resolve_references(&d, &HashMap::new(), &HashMap::new(), &HashMap::new(), Format::Html, &mut diags);
        assert!(diags.items()[0].message.contains("declares no `produces=file:` artifact"), "{:?}", diags.items());
    }

    /// The artifact is declared and recorded, and `produced_artifacts`
    /// could not read it off disk. That is a figure with a missing file,
    /// not a block that is no figure, and the message has to say which.
    #[test]
    fn a_reference_to_a_figure_whose_artifact_could_not_be_read_says_so() {
        let d = doc("```python name=chart produces=file:missing.png caption=\"C\"\nrun()\n```\n\n<!-- dankg:result name=chart hash=0000000000000001 -->\n\n```\nok\n```\n\nText [[#chart]].\n");
        let mut diags = Diags::new("t.md");
        resolve_references(&d, &HashMap::new(), &HashMap::new(), &HashMap::new(), Format::Html, &mut diags);
        assert!(diags.items()[0].message.contains("could not be read"), "{:?}", diags.items());
    }

    /// No recorded result yet is a third thing again, and `dankg eval` is
    /// the next action rather than fixing a file or a typo.
    #[test]
    fn a_reference_to_a_figure_with_no_recorded_result_points_at_eval() {
        let d = doc("```python name=chart produces=file:chart.png caption=\"C\"\nrun()\n```\n\nText [[#chart]].\n");
        let mut diags = Diags::new("t.md");
        resolve_references(&d, &HashMap::new(), &HashMap::new(), &HashMap::new(), Format::Html, &mut diags);
        assert!(diags.items()[0].message.contains("no recorded result yet"), "{:?}", diags.items());
        assert!(diags.items()[0].message.contains("dankg eval"), "{:?}", diags.items());
    }

    #[test]
    fn a_reference_to_a_hidden_figure_names_the_attribute_that_hid_it() {
        let d = doc("```python name=c weave=hidden produces=file:chart.png\nrun()\n```\n\n<!-- dankg:result name=c hash=0000000000000001 -->\n\n```\nok\n```\n\nText [[#c]].\n");
        let labels = [(0usize, "c".to_string())].into_iter().collect();
        let mut diags = Diags::new("t.md");
        resolve_references(&d, &HashMap::new(), &labels, &HashMap::new(), Format::Html, &mut diags);
        assert!(diags.items()[0].message.contains("hidden by `weave=hidden`"), "{:?}", diags.items());
    }

    #[test]
    fn a_reference_to_an_output_hidden_figure_names_that_attribute_instead() {
        let d = doc("```python name=c weave=output-hidden produces=file:chart.png\nrun()\n```\n\n<!-- dankg:result name=c hash=0000000000000001 -->\n\n```\nok\n```\n\nText [[#c]].\n");
        let labels = [(0usize, "c".to_string())].into_iter().collect();
        let mut diags = Diags::new("t.md");
        resolve_references(&d, &HashMap::new(), &labels, &HashMap::new(), Format::Html, &mut diags);
        assert!(diags.items()[0].message.contains("hidden by `weave=output-hidden`"), "{:?}", diags.items());
    }

    /// Decision 41 is narrowed, not repealed: a wikilink naming another
    /// file is not a same-file fragment and is never resolved here.
    #[test]
    fn a_wikilink_naming_another_file_is_not_checked_at_all() {
        let d = doc("See [[Other]] and [[Other#chart]].\n");
        let mut diags = Diags::new("t.md");
        resolve_references(&d, &HashMap::new(), &HashMap::new(), &HashMap::new(), Format::Html, &mut diags);
        assert!(diags.is_empty(), "{:?}", diags.items());
    }

    /// A reference is legal inside emphasis, and inside a link's own text.
    #[test]
    fn a_reference_nested_in_emphasis_is_still_checked() {
        let d = doc("See *[[#nope]]* here.\n");
        let mut diags = Diags::new("t.md");
        resolve_references(&d, &HashMap::new(), &HashMap::new(), &HashMap::new(), Format::Html, &mut diags);
        assert_eq!(diags.count(Level::Error), 1, "{:?}", diags.items());
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

    /// The slug maps a real weave builds (decision 77), so a unit test
    /// sees the same names the document would actually render with.
    /// `source_lines` only bounds a heading's own extent, which no slug
    /// depends on, so a generous constant is enough here.
    fn maps_of(d: &Document) -> SlugMaps {
        slug_maps("t.md", d, 9999, &mut Diags::new("t.md"))
    }

    /// The same, keeping the diagnostics `graph::build` raised. Decision
    /// 70's own refusals are the only thing that warns, and they warn
    /// from there now rather than from `figures`.
    fn maps_and_diags(d: &Document) -> (SlugMaps, Diags) {
        let mut diags = Diags::new("t.md");
        let maps = slug_maps("t.md", d, 9999, &mut diags);
        (maps, diags)
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
    fn a_cited_key_absent_from_every_entry_fails_the_weave() {
        let d = doc("```hayagriva\na:\n  title: A\n```\n\n[@missing]\n");
        let mut diags = Diags::new("t.md");
        let bib = bibliography(&d, "t.md", Path::new("."), &mut diags).unwrap();
        assert_eq!(diags.count(Level::Error), 1, "{:?}", diags.items());
        assert_eq!(diags.count(Level::Warn), 0, "an error, not a warning (decision 66)");
        assert!(bib.order.contains(&"missing".to_string()));
    }

    /// One run reports every bad line, references and citation keys
    /// alike, and writes nothing (decisions 64 and 66).
    #[test]
    fn an_unresolved_citation_key_fails_the_weave_and_writes_nothing() {
        let dir = scratch(&[("a.md", "```hayagriva\na:\n  title: A\n```\n\nText [@a] and [@missing].\n")]);
        let out = dir.join("out.html");
        let result = run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false);
        let err = result.err().expect("an unresolved citation key must fail the weave");
        assert!(err.contains("1 unresolved reference(s) or citation key(s)"), "{err}");
        assert!(!out.exists(), "no page is written for a document that failed to resolve");
    }

    /// Decision 66 leaves a document configuring no bibliography exactly
    /// as decision 59 wrote it. There is nothing to resolve a key
    /// against, so there is no unresolved key to fail on. Without this
    /// carve-out every document mentioning a handle in prose would fail,
    /// since a bare `@` parses as a citation (decision 57b).
    #[test]
    fn the_same_key_with_no_bibliography_configured_still_builds() {
        let dir = scratch(&[("a.md", "Ping me @dan on the forum, and see [@missing].\n")]);
        let out = dir.join("out.html");
        run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("@dan"), "{content}");
        assert!(content.contains("[@missing]"), "{content}");
    }

    /// The documented way out of decision 66, pinned by a test: an
    /// escaped `\@key` is not a citation at all.
    #[test]
    fn an_escaped_at_key_raises_nothing_and_renders_as_written() {
        let dir = scratch(&[("a.md", "```hayagriva\na:\n  title: A\n```\n\nPing me \\@dan, and see [@a].\n")]);
        let out = dir.join("out.html");
        run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("@dan"), "{content}");
        assert!(!content.contains("\\@dan"), "the backslash is an escape, not text: {content}");
    }

    /// A key the bibliography carries but cannot build `Smith (2020)`
    /// from is not an unresolved key. It still falls back to its own
    /// literal text, which is the one fallback decision 66 leaves alone.
    #[test]
    fn a_resolved_entry_with_no_author_still_falls_back_without_failing() {
        let dir = scratch(&[("a.md", "```hayagriva\na:\n  title: A\n```\n\n@a argues this.\n")]);
        let out = dir.join("out.html");
        run(dir.join("a.md").to_str().unwrap(), Format::Html, Some(out.to_str().unwrap()), true, false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("@a argues"), "{content}");
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
