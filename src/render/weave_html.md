# Render weave html

A single self-contained HTML page for `dankg weave --format html`, the
same one-file convention `render::html` already follows: `WEAVE_CSS`
inlined from `render::assets`, no network requests. It is a fresh
implementation, not a reuse of `render::html` or of
`tests/support/html.rs`'s own CommonMark oracle. `render::html` draws a
graph, an SVG canvas with a script behind it -- nothing here in common.
`tests/support/html.rs` exists to be scored against the CommonMark
spec, not to be a product renderer, and `src/` cannot depend on
`tests/` regardless. Its block/inline walk is the shape this module
follows, not code it calls.

```rust name=module_doc path=render/weave_html.rs
//! A single self-contained HTML page for `dankg weave --format html`.
//!
//! `WEAVE_CSS` is inlined from `render::assets`, the same one-file
//! convention `render::html` already follows. A fresh implementation,
//! not a reuse of `render::html` (a graph canvas, nothing in common) or
//! of `tests/support/html.rs`'s own CommonMark conformance oracle --
//! `src/` cannot depend on `tests/` regardless of what it draws.
//!
//! The table of contents toggle is pure CSS: a hidden checkbox, a
//! `<label>`, and a `:checked` sibling selector. No JavaScript exists
//! for it to misfire.

use crate::data::bib::{BibEntry, Person, SerialNumber};
use crate::data::table::{self, TableData};
use crate::diag::Diags;
use crate::eval::result::{self, Pair};
use crate::md::{Align, Block, Document, Frontmatter, InfoString, Inline, List, Value};
use crate::render::assets;
use std::collections::HashMap;
use std::fmt::Write as _;

/// What HTML needs to render both citations and a real reference list --
/// unlike `render::typst::BibliographySummary`, this keeps every entry's
/// own fields, since there is no external compiler here to hand a real
/// bibliography file to and defer formatting to (decision 59). Built from
/// `weave::Bibliography`'s own two relevant fields at the call site, not a
/// dependency on `weave` itself -- the same layering `data::bib`'s shared
/// types already give both this module and `weave.rs` independently.
pub struct Bibliography<'a> {
    pub entries: &'a HashMap<String, BibEntry>,
    /// Every cited key, resolved or not, in first-appearance order.
    /// Numbering (both the in-text link and the reference list's own
    /// `<li>`) counts only the resolved subset -- an unresolved key never
    /// consumes a number. Decision 66 makes an unresolved key fail the
    /// weave outright, so a rendered document has none to skip.
    pub order: &'a [String],
}

/// `extra_css`, from `[weave.html] css` (decision 43), is appended after
/// `WEAVE_CSS` inside the same `<style>` tag rather than replacing it.
/// Appended, not swapped in: the cascade lets a reader override one rule
/// -- a font, a color, the reading column's width -- without having to
/// reimplement the CSS-only table-of-contents toggle or the table
/// borders just to get back what they did not mean to lose.
///
/// `author`/`date` never render as raw frontmatter text beneath the
/// `<h1>`, the same non-literal treatment `render::typst`'s own cover
/// page gives them: `author` as an unlabeled byline, `date` as a real
/// formatted date -- "September 18, 2026", not "2026-09-18" -- built by
/// this module's own hand-rolled month-name table (decision 1: no
/// date-handling crate here either). Every other frontmatter key is
/// left alone; this page has never dumped the rest of a document's
/// frontmatter the way the PDF cover page does, and still does not.
///
/// That whole title block -- `<h1>`, byline, date -- is this backend's
/// own cover. Frontmatter's own `cover: false` (decision 61) drops it,
/// and the document's own first heading carries the title alone. The
/// `<title>` in `<head>` is never dropped with it: a browser tab still
/// needs a name, and nothing about it repeats on the page.
pub fn render(
    doc: &Document,
    title: &str,
    extra_css: Option<&str>,
    tables: &HashMap<usize, (String, String)>,
    images: &HashMap<usize, (Vec<u8>, String)>,
    labels: &HashMap<usize, String>,
    numbers: &HashMap<usize, u32>,
    slugs: &HashMap<u32, String>,
    refs: &HashMap<String, (String, String)>,
    figures_outside: bool,
    bibliography: Option<&Bibliography>,
    diags: &mut Diags,
) -> String {

    let mut out = String::with_capacity(assets::WEAVE_CSS.len() + 4 * 1024);
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    let _ = writeln!(out, "<title>{}</title>", escape(title));
    out.push_str("<style>\n");
    out.push_str(assets::WEAVE_CSS);
    if let Some(css) = extra_css {
        out.push('\n');
        out.push_str(css);
    }
    out.push_str("\n</style>\n");
    out.push_str("</head>\n<body>\n");

    out.push_str("<input type=\"checkbox\" id=\"toc-toggle\" class=\"toc-toggle\">\n");
    toc(&mut out, doc, slugs);

    out.push_str("<main>\n");
    if doc.frontmatter.cover() != Some(false) {
        let _ = writeln!(out, "<h1>{}</h1>", escape(title));
        if let Some(author) = author_line(&doc.frontmatter) {
            let _ = writeln!(out, "<p class=\"byline\">{author}</p>");
        }
        if let Some(date) = date_line(&doc.frontmatter) {
            let _ = writeln!(out, "<p class=\"byline-date\">{date}</p>");
        }
    }
    blocks(&mut out, &doc.blocks, slugs, tables, images, labels, numbers, refs, figures_outside, bibliography, diags);
    // Unconditionally at the end, never at the `hayagriva` fence's own
    // position (decision 59) -- the same fixed structural placement
    // `render::typst`'s own `#bibliography(...)` call gets.
    if let Some(bib) = bibliography {
        reference_list_html(&mut out, bib);
    }
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

/// `author`'s own byline, joined comma-separated for a list of several --
/// the identical join `render::typst`'s own `author_line` uses, so a
/// reader sees the same text in either format.
fn author_line(frontmatter: &Frontmatter) -> Option<String> {
    frontmatter.entries.iter().find(|(k, _)| k == "author").map(|(_, v)| frontmatter_value_html(v))
}

/// `date`'s own line: "September 18, 2026" when `Frontmatter::date_parts`
/// recognizes the value, the identical scalar-or-list fallback text
/// `author_line` uses when it does not -- unparseable text, or a list,
/// still gets a line of its own rather than silently disappearing.
fn date_line(frontmatter: &Frontmatter) -> Option<String> {
    let value = frontmatter.entries.iter().find(|(k, _)| k == "date").map(|(_, v)| v)?;
    match frontmatter.date_parts() {
        Some((year, month, day)) => Some(format_date_html(year, month, day)),
        None => Some(frontmatter_value_html(value)),
    }
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `Frontmatter::date_parts` already validated `month` as `1..=12`.
/// `MONTH_NAMES`'s own lookup never actually misses in practice. The
/// `None` arm exists only to keep this a total function.
fn format_date_html(year: u32, month: Option<u32>, day: Option<u32>) -> String {
    match (month.and_then(|m| MONTH_NAMES.get((m - 1) as usize)), day) {
        (Some(name), Some(d)) => format!("{name} {d}, {year}"),
        (Some(name), None) => format!("{name} {year}"),
        (None, _) => year.to_string(),
    }
}

fn frontmatter_value_html(v: &Value) -> String {
    match v {
        Value::Scalar(s) => escape(s),
        Value::List(items) => items.iter().map(|s| escape(s)).collect::<Vec<_>>().join(", "),
    }
}

```

## Table of contents

Nested by relative level change, not by absolute heading number -- a
jump from `#` straight to `###` opens one nested list, not two empty
ones. This is a table of contents, not a heading-level validator.

```rust name=toc path=render/weave_html.rs
fn toc(out: &mut String, doc: &Document, slugs: &HashMap<u32, String>) {
    let headings = doc.headings();
    out.push_str("<nav id=\"toc\">\n<label for=\"toc-toggle\" class=\"toc-label\">Contents</label>\n");
    out.push_str("<div class=\"toc-body\">\n");
    if !headings.is_empty() {
        toc_list(out, &headings, slugs);
    }
    out.push_str("</div>\n</nav>\n");
}

fn toc_list(out: &mut String, headings: &[(u8, &[Inline], u32)], slugs: &HashMap<u32, String>) {
    out.push_str("<ul>\n<li>");
    toc_link(out, headings[0], slugs);
    let mut stack: Vec<u8> = vec![headings[0].0];

    for pair in headings.windows(2) {
        let (prev_level, _, _) = pair[0];
        let (level, inlines, line) = pair[1];
        if level > prev_level {
            out.push_str("\n<ul>\n<li>");
            stack.push(level);
        } else if level == prev_level {
            out.push_str("</li>\n<li>");
        } else {
            out.push_str("</li>\n");
            while stack.len() > 1 && *stack.last().unwrap() > level {
                stack.pop();
                out.push_str("</ul>\n</li>\n");
            }
            out.push_str("<li>");
        }
        toc_link(out, (level, inlines, line), slugs);
    }

    out.push_str("</li>\n");
    while stack.len() > 1 {
        stack.pop();
        out.push_str("</ul>\n</li>\n");
    }
    out.push_str("</ul>\n");
}

fn toc_link(out: &mut String, (_, inlines, line): (u8, &[Inline], u32), slugs: &HashMap<u32, String>) {
    let slug = slugs.get(&line).map(String::as_str).unwrap_or("");
    let _ = write!(out, "<a href=\"#{slug}\">{}</a>", escape(&Inline::plain(inlines)));
}
```

## Blocks

The same block set `render::typst` walks (weave's own decision 41: the
whole document, not just named/top-level blocks), emitted as HTML
instead of Typst markup.

A code block tagged `weave=hidden` (decision 48) produces nothing at
all here -- not a placeholder, not a collapsed toggle. It is a
weave-only rendering hint. `dankg tangle` and `dankg eval` never look
at it. A hidden block still tangles and still evaluates exactly as it
would without the tag.

A recognized pair (decision 46) can also hide just one of its own two
halves (decision 53). `weave=source-hidden` drops the source's own
`code_or_data_table` call but renders its output, artifact, and
provenance exactly as before. `weave=output-hidden` drops that entire
second half -- output, artifact, and provenance together -- and
renders the source exactly as before. Both read the same `weave=`
attribute `weave_hidden()` already reads, so a block can only ever
ask for one of the three: hiding both halves is already
`weave=hidden`.

A named `Code` block immediately followed by its own recorded
`<!-- dankg:result ... -->` marker and output fence (decision 46)
renders as one paired unit instead of three unrelated blocks. The
marker itself is never rendered as text; `eval::result::recognize_pair`
reads it as data, not markup.

<!-- dankg:depends target=../../architecture.md#decision-46-a-recorded-eval-result-renders-paired-with-its-source quote="The marker itself is never rendered as text again." -->

A stale result (decision 47) changes nothing here. Staleness is
reported to stderr by `weave::warn_stale_pairs`, never folded into
the rendered document -- the woven page stays a pure function of the
one file's own content, not of whatever state a `deps=`/`xdeps=`
chain happens to be in elsewhere when weave runs.

A produced table (decision 50) is different: `tables`, keyed by the
same source block index, is real content the block actually wrote,
not an editorial judgment about trustworthiness. It renders inside
the pair whenever present, through the identical `code_or_data_table`
below a `csv`/`json` fence's own inline content already goes through.
A produced image (decision 51) is the same idea for `images`, base64-
inlined as a `data:` URI so the woven page stays one self-contained
file (decision 43) with nothing else to ship alongside it.

A reader-authored `caption=` (decision 52) overrides whichever
figcaption is the pair's own payload: the artifact's, when a table
or image is present, otherwise the eval result's own `Output`/
`Output (failed)` label. It replaces one figcaption, never both, so
a pair with both a captured stdout line and a produced chart never
repeats the same caption twice.

An artifact's own `<figcaption>` (decision 54) sits inside a second,
nested `<figure>` -- `table-figure` for a table, `image-figure` for
an image -- rather than beside the raw content directly. HTML
requires a `<figcaption>` to be its `<figure>`'s first or last child,
so this is also where table-above/figure-below caption placement
gets decided: a table's caption is emitted first, an image's last.
A stylesheet still owns the rest -- font, spacing, and any automatic
"Table N"/"Figure N" numbering, via a CSS counter keyed to each
class -- this only gives it a real element to key that off of.

That nested `<figure>` sitting inside the pair's own outer one, or
right after it closes, is a separate question from where its own
caption sits within it (decision 56, decoupled from decision 54's own
concern above): `source_info.figure_outside()`'s own `figure=inside`/
`figure=outside` when present, otherwise `dankg weave`'s own
document-wide `--figures-inside`/`--figures-outside` default.

A figure's own `<figcaption>` carries the number `weave::figures`
counted for it, written as literal text with Typst's own supplement
word and its own `: ` separator (decision 65). HTML cannot count for
itself. This page has no stylesheet counter to key one off either. Its
`id` comes from that same walk's own label (decision 63), on the
nested `<figure>` and never on the pair's own outer one: a reference
has to land on the artifact, not on the source block above it.

<!-- dankg:depends target=../../architecture.md#decision-65-one-numbering-pre-pass-feeds-html-and-typst-still-counts-for-itself quote="HTML writes the number as literal text, using Typst's own supplement word" -->
<!-- dankg:depends target=../../architecture.md#decision-63-a-figures-label-comes-from-its-blocks-own-name-or-from-label quote="on the nested `<figure>`, never on the pair's own outer" -->

```rust name=blocks_and_block path=render/weave_html.rs
/// A named `Code` block immediately followed by its recorded eval
/// result (decision 46) is recognized here, before the ordinary
/// per-block walk ever sees it, and consumed as one unit -- three
/// `Block`s in `items`, one `<figure>` in `out`. Anything else falls
/// through to `block` exactly as before.
fn blocks(
    out: &mut String,
    items: &[Block],
    slugs: &HashMap<u32, String>,
    tables: &HashMap<usize, (String, String)>,
    images: &HashMap<usize, (Vec<u8>, String)>,
    labels: &HashMap<usize, String>,
    numbers: &HashMap<usize, u32>,
    refs: &HashMap<String, (String, String)>,
    figures_outside: bool,
    bibliography: Option<&Bibliography>,
    diags: &mut Diags,
) {
    let mut i = 0;
    while i < items.len() {
        if let Block::Code { info, text, line, .. } = &items[i] {
            if let Some(pair) = result::recognize_pair(items, i) {
                if !info.weave_hidden() {
                    eval_pair(
                        out,
                        info,
                        text,
                        *line,
                        &pair,
                        tables.get(&i),
                        images.get(&i),
                        labels.get(&i).map(String::as_str),
                        numbers.get(&i).copied(),
                        figures_outside,
                        diags,
                    );
                }
                i += 3;
                continue;
            }
        }
        block(out, &items[i], slugs, tables, images, labels, numbers, refs, figures_outside, bibliography, diags);
        i += 1;
    }
}

fn block(
    out: &mut String,
    b: &Block,
    slugs: &HashMap<u32, String>,
    tables: &HashMap<usize, (String, String)>,
    images: &HashMap<usize, (Vec<u8>, String)>,
    labels: &HashMap<usize, String>,
    numbers: &HashMap<usize, u32>,
    refs: &HashMap<String, (String, String)>,
    figures_outside: bool,
    bibliography: Option<&Bibliography>,
    diags: &mut Diags,
) {
    match b {
        Block::Heading { level, inlines, line } => {
            let id = slugs.get(line).map(String::as_str).unwrap_or("");
            let _ = writeln!(out, "<h{level} id=\"{id}\">{}</h{level}>", inline_html(inlines, refs, bibliography));
        }
        Block::Paragraph { inlines, .. } => {
            let _ = writeln!(out, "<p>{}</p>", inline_html(inlines, refs, bibliography));
        }
        Block::Code { info, text, line, .. } => {
            // A lone block has no separate output half, so
            // `source-hidden` has nothing left to preserve and hides
            // it too, the same as `hidden` (decision 53).
            if !info.weave_hidden() && !info.weave_source_hidden() {
                code_or_data_table(out, info, text, *line, diags);
            }
        }
        Block::List(l) => {
            list(out, l, slugs, tables, images, labels, numbers, refs, figures_outside, bibliography, diags)
        }
        Block::ThematicBreak { .. } => out.push_str("<hr>\n"),
        // Outside the subset. Escaped, not raw: an unparsed construct
        // must never become unvalidated HTML.
        Block::Passthrough { text, .. } => {
            let _ = writeln!(out, "<pre class=\"passthrough\">{}</pre>", escape(text));
        }
        Block::Table { aligns, header, rows, .. } => table_block(out, aligns, header, rows, refs, bibliography),
    }
}

/// The `<figure>` decision 46 wraps a source block and its recorded
/// output in. `failed` alone drives both the caption text and the
/// extra `failed` class `WEAVE_CSS` keys its border/caption color off
/// of -- one flag, not two independent things that could disagree.
/// `table`/`image` (decisions 50/51), when present, are the source
/// block's own `produces=file:` artifact, already read off disk.
/// A block declares at most one `produces=file:` today, so at most
/// one of the two is ever `Some`. `weave=source-hidden`/
/// `weave=output-hidden` (decision 53) each drop one of the figure's
/// own two halves; a custom `caption=` (decision 52) only ever
/// changes the *other* half's own text, since a half that is not
/// rendered has no figcaption to override. An artifact's own nested
/// `<figure>` either splices in before this outer `</figure>` closes,
/// or gets appended after it, depending on `figure_outside`
/// (decision 56) -- see `eval_pair_result`'s own doc comment for how
/// that gets resolved.
fn eval_pair(
    out: &mut String,
    source_info: &InfoString,
    source_text: &str,
    source_line: u32,
    pair: &Pair,
    table: Option<&(String, String)>,
    image: Option<&(Vec<u8>, String)>,
    label: Option<&str>,
    number: Option<u32>,
    figures_outside: bool,
    diags: &mut Diags,
) {
    let class = if pair.failed { "eval-pair failed" } else { "eval-pair" };
    let mut inner = String::new();
    if !source_info.weave_source_hidden() {
        code_or_data_table(&mut inner, source_info, source_text, source_line, diags);
    }
    let mut figure = String::new();
    if !source_info.weave_output_hidden() {
        eval_pair_result(&mut inner, &mut figure, source_info, pair, table, image, label, number, source_line, diags);
    }
    // Hiding both halves can leave the wrapper with nothing inside it.
    // An empty `<figure class="eval-pair">` still carries the pair's
    // own border and spacing, so it is dropped rather than emitted,
    // leaving whatever figure the pair produced standing on its own.
    if inner.trim().is_empty() {
        out.push_str(&figure);
        return;
    }
    let _ = writeln!(out, "<figure class=\"{class}\">");
    out.push_str(&inner);
    if source_info.figure_outside().unwrap_or(figures_outside) {
        out.push_str("</figure>\n");
        out.push_str(&figure);
    } else {
        out.push_str(&figure);
        out.push_str("</figure>\n");
    }
}

/// The pair's own result half: the captured output and its
/// provenance line go into `out`, inside the outer `<figure
/// class="eval-pair">`, always. `table`/`image` each get their own
/// real, nested `<figure>` -- `table-figure`/`image-figure` -- built
/// into `figure` instead, regardless of where it ends up: [`eval_pair`]
/// is the one place that decides whether `figure` gets spliced in
/// before the outer `</figure>` closes, or appended after it as a
/// sibling (decision 56), by resolving `source_info.figure_outside()`'s
/// own `figure=inside`/`figure=outside` against `dankg weave`'s own
/// document-wide `--figures-inside`/`--figures-outside` default.
/// Building `figure` here regardless keeps this function ignorant of
/// that choice entirely. A table's own `<figcaption>` sits first,
/// above its `<table>`, the conventional position; an image's own
/// sits last, after its `<img>`, matching HTML's own rule that a
/// `<figcaption>` must be its `<figure>`'s first or last child.
fn eval_pair_result(
    out: &mut String,
    figure: &mut String,
    source_info: &InfoString,
    pair: &Pair,
    table: Option<&(String, String)>,
    image: Option<&(Vec<u8>, String)>,
    label: Option<&str>,
    number: Option<u32>,
    source_line: u32,
    diags: &mut Diags,
) {
    let has_artifact = table.is_some() || image.is_some();
    let output_caption = match source_info.caption() {
        Some(c) if !has_artifact => {
            if pair.failed { format!("{c} (failed)") } else { c.to_string() }
        }
        _ => (if pair.failed { "Output (failed)" } else { "Output" }).to_string(),
    };
    if !hides_output(source_info, pair) {
        let _ = writeln!(out, "<figcaption>{}</figcaption>", escape(&output_caption));
        code_or_data_table(out, pair.output_info, pair.output_text, pair.output_line, diags);
    }
    let anchor = label.map(|l| format!(" id=\"fig-{}\"", escape_attr(l))).unwrap_or_default();
    if let Some((lang, content)) = table {
        let _ = writeln!(figure, "<figure class=\"table-figure\"{anchor}>");
        if let Some(caption) = source_info.caption().or_else(|| source_info.produces()) {
            let _ = writeln!(figure, "<figcaption>{}{}</figcaption>", supplement("Table", number), escape(caption));
        }
        let info = InfoString { lang: Some(lang.clone()), ..Default::default() };
        code_or_data_table(figure, &info, content, source_line, diags);
        figure.push_str("</figure>\n");
    }
    if let Some((bytes, resolved)) = image {
        let _ = writeln!(figure, "<figure class=\"image-figure\"{anchor}>");
        let _ = writeln!(
            figure,
            "<img src=\"data:{};base64,{}\" alt=\"{}\">",
            mime_for(resolved),
            base64_encode(bytes),
            escape_attr(resolved)
        );
        if let Some(caption) = source_info.caption().or_else(|| source_info.produces()) {
            let _ = writeln!(figure, "<figcaption>{}{}</figcaption>", supplement("Figure", number), escape(caption));
        }
        figure.push_str("</figure>\n");
    }
    if !hides_output(source_info, pair) {
        provenance(out, pair);
    }
}

/// Whether a block's own recorded output half -- the caption, the
/// captured text, and the provenance line -- is dropped from the page
/// (decision 53). A block that hides its source is asking to be seen
/// as its artifact alone, so the captured text goes with it. The
/// artifact itself is untouched: [`eval_pair_result`] still runs and
/// still builds `figure`, which is what separates `source-hidden` from
/// `output-hidden` -- the latter returns before the figure is built
/// and so drops it as well.
///
/// A failed run is never hidden, whichever flag is set. Hiding a half
/// is a statement about a working block's own typeset shape, not a
/// licence to swallow an error: a reader given a page with no trace
/// of the failure has no way to know the artifact above it is stale.
/// `Table 1: ` or `Figure 1: `, matching Typst's own default supplement
/// word and its own `: ` separator exactly (decision 65). The wording is
/// Typst's rather than configurable. That is what makes the two backends
/// read alike for the same document. An unnumbered figure gets no prefix
/// at all. Only a figure neither backend renders is unnumbered, so
/// nothing reaching here in practice takes that branch.
fn supplement(kind: &str, number: Option<u32>) -> String {
    match number {
        Some(n) => format!("{kind} {n}: "),
        None => String::new(),
    }
}

fn hides_output(info: &InfoString, pair: &Pair) -> bool {
    !pair.failed && info.weave_source_hidden()
}

/// The MIME type a `data:` URI needs, from the artifact's own extension
/// (`weave::image_ext`'s own recognized set). `image_ext` never hands
/// this an extension outside that set, so the fallback never actually
/// fires; it exists only so this stays a total function.
fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

const BASE64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// A hand-rolled RFC 4648 encoder (decision 1: zero crates), the same
/// choice `data::table`'s own hand-rolled CSV/JSON readers already made
/// for the identical reason. Standard alphabet, `=` padding.
fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_ALPHABET[(n >> 18 & 0x3f) as usize] as char);
        out.push(BASE64_ALPHABET[(n >> 12 & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 { BASE64_ALPHABET[(n >> 6 & 0x3f) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { BASE64_ALPHABET[(n & 0x3f) as usize] as char } else { '=' });
    }
    out
}

/// `produces`/`reads` (decision 33), when either is non-empty, is the
/// only way a reader of typeset output can see what a `db=` block's
/// run actually touched -- the block's own source rarely names its
/// target table directly, since `dankg eval` usually infers this.
fn provenance(out: &mut String, pair: &Pair) {
    if pair.produces.is_empty() && pair.reads.is_empty() {
        return;
    }
    let mut parts = Vec::new();
    if !pair.produces.is_empty() {
        parts.push(format!("writes: {}", pair.produces.join(", ")));
    }
    if !pair.reads.is_empty() {
        parts.push(format!("reads: {}", pair.reads.join(", ")));
    }
    let _ = writeln!(out, "<p class=\"eval-provenance\">{}</p>", escape(&parts.join(" -- ")));
}

fn list(
    out: &mut String,
    l: &List,
    slugs: &HashMap<u32, String>,
    tables: &HashMap<usize, (String, String)>,
    images: &HashMap<usize, (Vec<u8>, String)>,
    labels: &HashMap<usize, String>,
    numbers: &HashMap<usize, u32>,
    refs: &HashMap<String, (String, String)>,
    figures_outside: bool,
    bibliography: Option<&Bibliography>,
    diags: &mut Diags,
) {
    let tag = if l.ordered { "ol" } else { "ul" };
    if l.ordered && l.start != 1 {
        let _ = writeln!(out, "<{tag} start=\"{}\">", l.start);
    } else {
        let _ = writeln!(out, "<{tag}>");
    }
    for item in &l.items {
        out.push_str("<li>");
        blocks(out, &item.blocks, slugs, tables, images, labels, numbers, refs, figures_outside, bibliography, diags);
        out.push_str("</li>\n");
    }
    let _ = writeln!(out, "</{tag}>");
}
```

## Code: a raw block, or a data table

Identical shape to `render::typst::code_or_data_table` (decision 45):
the fence's own language tag decides, `csv`/`tsv` always becomes a
table, and a `json` block that does not recognize as one falls back to
an ordinary code block with a diagnostic.

```rust name=code_and_data_table path=render/weave_html.rs
fn code_or_data_table(out: &mut String, info: &InfoString, text: &str, line: u32, diags: &mut Diags) {
    match info.lang.as_deref() {
        Some("csv") => data_table(out, &table::from_delimited(text, ',')),
        Some("tsv") => data_table(out, &table::from_delimited(text, '\t')),
        Some("json") => match table::from_json(text) {
            Some(data) => data_table(out, &data),
            None => {
                diags.warn(
                    line,
                    "`json` block is not an array of objects or an array of arrays; rendered as code",
                );
                code_block(out, info, text);
            }
        },
        _ => code_block(out, info, text),
    }
}

fn code_block(out: &mut String, info: &InfoString, text: &str) {
    match &info.lang {
        Some(lang) => {
            out.push_str("<pre><code class=\"language-");
            out.push_str(&escape_attr(lang));
            out.push_str("\">");
        }
        None => out.push_str("<pre><code>"),
    }
    out.push_str(&escape(text));
    out.push_str("</code></pre>\n");
}
```

## Tables

`table_block` is the GFM path: `Align` becomes a per-cell `text-align`
inline style, browser default (no style attribute at all) for
`Align::None`. A ragged data row (decision 42 keeps these ragged in
the AST) is padded to the header's own column count right here, the
same rendering-time rectangling `render::typst::table_block` already
does, and for the same reason: padding here has no round-trip
obligation to satisfy, unlike padding in the parser or in `fmt` would.
`data_table` is the CSV/JSON path: no `Align` to carry, so no style is
ever added -- nothing invented that the input did not have. A short
row there reads as empty cells via `.get(i)` instead, since
`TableData` itself is never padded either.

```rust name=table path=render/weave_html.rs
fn table_block(
    out: &mut String,
    aligns: &[Align],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    refs: &HashMap<String, (String, String)>,
    bibliography: Option<&Bibliography>,
) {
    let columns = header.len();
    out.push_str("<table>\n<thead>\n<tr>\n");
    for (i, cell) in header.iter().enumerate() {
        out.push_str("<th");
        push_align(out, aligns.get(i).copied());
        out.push('>');
        out.push_str(&inline_html(cell, refs, bibliography));
        out.push_str("</th>\n");
    }
    out.push_str("</tr>\n</thead>\n<tbody>\n");
    for row in rows {
        out.push_str("<tr>\n");
        for i in 0..columns {
            out.push_str("<td");
            push_align(out, aligns.get(i).copied());
            out.push('>');
            if let Some(cell) = row.get(i) {
                out.push_str(&inline_html(cell, refs, bibliography));
            }
            out.push_str("</td>\n");
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n");
}

fn push_align(out: &mut String, align: Option<Align>) {
    let value = match align {
        Some(Align::Left) => "left",
        Some(Align::Right) => "right",
        Some(Align::Center) => "center",
        Some(Align::None) | None => return,
    };
    let _ = write!(out, " style=\"text-align:{value}\"");
}

fn data_table(out: &mut String, data: &TableData) {
    if data.header.is_empty() {
        return;
    }
    out.push_str("<table>\n<thead>\n<tr>\n");
    for h in &data.header {
        let _ = writeln!(out, "<th>{}</th>", escape(h));
    }
    out.push_str("</tr>\n</thead>\n<tbody>\n");
    for row in &data.rows {
        out.push_str("<tr>\n");
        for i in 0..data.header.len() {
            let cell = row.get(i).map(String::as_str).unwrap_or("");
            let _ = writeln!(out, "<td>{}</td>", escape(cell));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n");
}
```

## Inline HTML and escaping

A same-file fragment resolves here (decision 64), against the map
`weave::references` already built. A bare figure reference reads as
the number that walk counted. A bare heading reference reads as the
heading's own title (decision 68), the identical text `toc` above
already writes for the same target. HTML numbers no heading. A number
there would point at nothing visible on the page. A wikilink naming
another file still renders as its own plain text.

<!-- dankg:depends target=../../architecture.md#decision-68-a-bare-heading-reference-takes-the-headings-own-title-in-html quote="already emits exactly that for the same target" -->
<!-- dankg:depends target=#toc quote="escape(&Inline::plain(inlines))" -->

```rust name=inline_html path=render/weave_html.rs
fn inline_html(
    inlines: &[Inline],
    refs: &HashMap<String, (String, String)>,
    bibliography: Option<&Bibliography>,
) -> String {
    let mut out = String::new();
    for i in inlines {
        match i {
            Inline::Text(t) => out.push_str(&escape(t)),
            Inline::Code(t) => {
                out.push_str("<code>");
                out.push_str(&escape(t));
                out.push_str("</code>");
            }
            Inline::Emph { inner, .. } => {
                out.push_str("<em>");
                out.push_str(&inline_html(inner, refs, bibliography));
                out.push_str("</em>");
            }
            Inline::Strong { inner, .. } => {
                out.push_str("<strong>");
                out.push_str(&inline_html(inner, refs, bibliography));
                out.push_str("</strong>");
            }
            // A fragment destination is a same-file reference
            // (decision 64) and points at the anchor `refs` resolved,
            // which is not the fragment itself: a figure's own id
            // carries a `fig-` prefix (decision 63).
            Inline::Link { dest, title, text } => {
                let resolved = dest.strip_prefix('#').and_then(|f| refs.get(f));
                out.push_str("<a href=\"");
                match resolved {
                    Some((anchor, _)) => {
                        out.push('#');
                        out.push_str(&escape_attr(anchor));
                    }
                    None => out.push_str(&escape_attr(dest)),
                }
                out.push('"');
                if let Some(t) = title {
                    out.push_str(" title=\"");
                    out.push_str(&escape_attr(t));
                    out.push('"');
                }
                out.push('>');
                out.push_str(&inline_html(text, refs, bibliography));
                out.push_str("</a>");
            }
            // A same-file fragment resolves (decision 64). A bare one
            // shows the text the pre-pass counted for it -- a figure's
            // own "Table 1" (decision 65), a heading's own title
            // (decision 68). A labelled one shows the author's own
            // words. A wikilink naming another file still renders as its
            // own plain text: weave is single-file (decision 41), and
            // there is no corpus to resolve that against.
            Inline::WikiLink { target, label } => {
                match target.strip_prefix('#').and_then(|f| refs.get(f)) {
                    Some((anchor, shown)) => {
                        let text = label.as_deref().unwrap_or(shown);
                        let _ = write!(out, "<a href=\"#{}\">{}</a>", escape_attr(anchor), escape(text));
                    }
                    None => out.push_str(&escape(label.as_deref().unwrap_or(target))),
                }
            }
            Inline::Citation { keys, narrative } => citation_html(&mut out, keys, *narrative, bibliography),
            Inline::SoftBreak => out.push('\n'),
            Inline::HardBreak => out.push_str("<br>\n"),
        }
    }
    out
}

/// No bibliography configured at all: the identical literal-text fallback
/// `render::typst` gets, escaped exactly like ordinary prose (decision 59).
/// With one configured, every key reaching here resolves: decision 66 has
/// `weave::bibliography` fail the weave on a key the bibliography does not
/// carry, at that key's own line, before either renderer runs. The one
/// fallback left is a resolved entry carrying no author or no four-digit
/// year, which a narrative citation cannot build `Smith (2020)` from. It
/// shows the literal `@key` text, and nothing here warns.
fn citation_html(out: &mut String, keys: &[String], narrative: bool, bibliography: Option<&Bibliography>) {
    let Some(bib) = bibliography else {
        out.push_str(&escape(&citation_source(keys, narrative)));
        return;
    };
    if narrative {
        let key = &keys[0];
        match bib.entries.get(key).and_then(narrative_citation_text) {
            Some(text) => {
                let _ = write!(out, "<a href=\"#ref-{}\">{text}</a>", escape_attr(key));
            }
            None => out.push_str(&escape(&citation_source(keys, true))),
        }
        return;
    }
    out.push_str("<span class=\"citation\">[");
    let links: Vec<String> = keys
        .iter()
        .map(|k| match citation_position(bib, k) {
            Some(n) => format!("<a href=\"#ref-{}\">{}</a>", escape_attr(k), n + 1),
            // Decision 66 fails the weave on a key the bibliography
            // does not carry, so a document reaching here has none.
            // The arm exists because the lookup returns an `Option`,
            // not because anything can take it.
            None => escape(k),
        })
        .collect();
    out.push_str(&links.join(", "));
    out.push_str("]</span>");
}

/// A cited key's position among the *resolved* cited keys only, in
/// first-appearance order -- what both the in-text link and the reference
/// list's own `<li>` number by. An unresolved key is excluded from the
/// count entirely, not just unlinked: it never occupies a number another
/// citation would otherwise have to skip over.
fn citation_position(bib: &Bibliography, key: &str) -> Option<usize> {
    if !bib.entries.contains_key(key) {
        return None;
    }
    bib.order.iter().filter(|k| bib.entries.contains_key(k.as_str())).position(|k| k == key)
}

/// `Family (Year)`/`Family & Family (Year)`/`Family et al. (Year)` -- the
/// identical family-name extraction and author-count join
/// `reference_entry_html`'s own author line below uses. `None` when the
/// entry has no author or no four-digit leading year to build one from;
/// the caller falls back to the citation's own literal text in that case,
/// the same as a key that never resolved at all.
fn narrative_citation_text(entry: &BibEntry) -> Option<String> {
    let year = entry.date.as_deref().and_then(leading_year)?;
    let who = authors_display(&entry.authors)?;
    Some(format!("{who} ({year})"))
}

fn authors_display(authors: &[Person]) -> Option<String> {
    match authors {
        [] => None,
        [a] => Some(escape(&a.name)),
        [a, b] => Some(format!("{} &amp; {}", escape(&a.name), escape(&b.name))),
        [a, ..] => Some(format!("{} et al.", escape(&a.name))),
    }
}

/// A date's own leading four-digit run -- Hayagriva dates are ISO-ish
/// (`2020`, `2020-01`, `2020-01-15`), always year-first when a year is
/// present at all.
fn leading_year(date: &str) -> Option<String> {
    let digits: String = date.chars().take_while(|c| c.is_ascii_digit()).collect();
    (digits.len() == 4).then_some(digits)
}

/// A citation's own original source text -- `[@a; @b]` or `@key` -- shared
/// by every fallback path that has nothing to resolve a key against yet.
fn citation_source(keys: &[String], narrative: bool) -> String {
    if narrative {
        format!("@{}", keys[0])
    } else {
        format!("[{}]", keys.iter().map(|k| format!("@{k}")).collect::<Vec<_>>().join("; "))
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

/// An attribute value. Every attribute here is double-quoted, so `"`
/// has to go too -- a link destination containing one would otherwise
/// close the attribute early.
fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}
```

## The reference list

HTML has no citation engine to defer to the way Typst's own
`#bibliography(...)` does (decision 1 rules out a CSL/citeproc crate,
and there is no external command to shell out to the way
`typst compile`/`duckdb` get one, since this has to run inline during
HTML rendering). So this is dankg's own complete field-by-field
formatter instead of feature parity with Typst's -- one fixed,
hand-built, non-configurable, non-CSL format, not an attempt at
APA/MLA/Chicago accuracy and not varying by an entry's own `kind`.
"Complete" means every field shows when present. Most fields are
simply absent on most entries. One uniform order already produces a
reasonable-looking entry regardless of what kind of source it is.

Field order is fixed: authors, `(year)`, title, editors, any
`affiliated` persons, the container chain (`parent`, walked outward),
edition, location, organization, publisher, page-range/page-total,
volume-total, chapter, time-range/runtime, language, genre, a
serial-number, a `url` (with `url_date` alongside it), then
archive/archive-location/call-number, and finally note/abstract as
trailing free text. Every present part joins with `. `, the whole
entry closed with one final period -- a plain sentence-like run, not a
styled citation.

```rust name=reference_list path=render/weave_html.rs
fn reference_list_html(out: &mut String, bib: &Bibliography) {
    out.push_str("<ol class=\"reference-list\">\n");
    for key in bib.order.iter().filter(|k| bib.entries.contains_key(k.as_str())) {
        let entry = &bib.entries[key];
        let _ = writeln!(out, "<li id=\"ref-{}\">{}</li>", escape_attr(key), reference_entry_html(entry));
    }
    out.push_str("</ol>\n");
}

fn reference_entry_html(entry: &BibEntry) -> String {
    let mut parts: Vec<String> = Vec::new();

    let authors = authors_display(&entry.authors);
    let year = entry.date.as_deref().and_then(leading_year);
    match (authors, year) {
        (Some(a), Some(y)) => parts.push(format!("{a} ({y})")),
        (Some(a), None) => parts.push(a),
        (None, Some(y)) => parts.push(format!("({y})")),
        (None, None) => {}
    }
    if let Some(t) = &entry.title {
        parts.push(escape(t));
    }
    if !entry.editors.is_empty() {
        let names: Vec<String> = entry.editors.iter().map(|p| escape(&p.name)).collect();
        parts.push(format!("ed. {}", names.join(", ")));
    }
    for a in &entry.affiliated {
        parts.push(format!("{}: {}", escape(a.role.as_deref().unwrap_or("contributor")), escape(&a.person.name)));
    }
    if let Some(chain) = container_chain_html(entry) {
        parts.push(chain);
    }
    if let Some(v) = &entry.edition {
        parts.push(format!("{} ed.", escape(v)));
    }
    if let Some(v) = &entry.location {
        parts.push(escape(v));
    }
    if let Some(v) = &entry.organization {
        parts.push(escape(v));
    }
    if let Some(v) = &entry.publisher {
        parts.push(escape(v));
    }
    if let Some(v) = &entry.page_range {
        parts.push(format!("pp. {}", escape(v)));
    }
    if let Some(v) = &entry.page_total {
        parts.push(format!("{} pp.", escape(v)));
    }
    if let Some(v) = &entry.volume_total {
        parts.push(format!("{} vols.", escape(v)));
    }
    if let Some(v) = &entry.chapter {
        parts.push(format!("ch. {}", escape(v)));
    }
    if let Some(v) = &entry.time_range {
        parts.push(escape(v));
    }
    if let Some(v) = &entry.runtime {
        parts.push(escape(v));
    }
    if let Some(v) = &entry.language {
        parts.push(escape(v));
    }
    if let Some(v) = &entry.genre {
        parts.push(escape(v));
    }
    if let Some(sn) = entry.serial_number.as_ref().and_then(serial_number_html) {
        parts.push(sn);
    }
    if let Some(u) = &entry.url {
        let mut line = format!("<a href=\"{}\">{}</a>", escape_attr(u), escape(u));
        if let Some(d) = &entry.url_date {
            let _ = write!(line, " (accessed {})", escape(d));
        }
        parts.push(line);
    }
    if let Some(v) = &entry.archive {
        parts.push(escape(v));
    }
    if let Some(v) = &entry.archive_location {
        parts.push(escape(v));
    }
    if let Some(v) = &entry.call_number {
        parts.push(escape(v));
    }
    if let Some(v) = &entry.note {
        parts.push(escape(v));
    }
    if let Some(v) = &entry.abstract_ {
        parts.push(escape(v));
    }

    format!("{}.", parts.join(". "))
}

/// Walks `parent` outward one step per level: the outermost ancestor's own
/// title is the container's name (`In <i>Journal</i>`). The first
/// `volume`/`issue` found at any level along the way (typically the issue,
/// one level in from the journal) appends as `vol. N, no. M`. An entry
/// with more than one `parent` -- Hayagriva allows a list -- shows the
/// first chain as the primary container; any further entries in that list
/// append as their own `also in: <title>`, one step of their own chain
/// only, not walked further outward. This is dankg's own fixed
/// simplification, not real CSL container logic (decision 59's own scope).
fn container_chain_html(entry: &BibEntry) -> Option<String> {
    let first = entry.parent.first()?;
    let mut chain = vec![first];
    let mut cur = first;
    while let Some(next) = cur.parent.first() {
        chain.push(next);
        cur = next;
    }
    let outermost = *chain.last()?;
    let title = outermost.title.as_deref()?;
    let mut line = format!("In <i>{}</i>", escape(title));
    if let Some(v) = chain.iter().find_map(|e| e.volume.as_deref()) {
        let _ = write!(line, ", vol. {}", escape(v));
    }
    if let Some(v) = chain.iter().find_map(|e| e.issue.as_deref()) {
        let _ = write!(line, ", no. {}", escape(v));
    }
    for extra in &entry.parent[1..] {
        if let Some(t) = &extra.title {
            let _ = write!(line, "; also in: {}", escape(t));
        }
    }
    Some(line)
}

/// A `doi` renders as a real link (`https://doi.org/<doi>`); every other
/// identifier Hayagriva's `serial-number` can carry is labeled plain text.
fn serial_number_html(sn: &SerialNumber) -> Option<String> {
    match sn {
        SerialNumber::Plain(s) => Some(escape(s)),
        SerialNumber::Structured { doi, isbn, issn, pmid, pmcid, arxiv, serial } => {
            let mut parts = Vec::new();
            if let Some(d) = doi {
                parts.push(format!("<a href=\"https://doi.org/{}\">doi:{}</a>", escape_attr(d), escape(d)));
            }
            for (label, value) in [("isbn", isbn), ("issn", issn), ("pmid", pmid), ("pmcid", pmcid), ("arxiv", arxiv), ("serial", serial)] {
                if let Some(v) = value {
                    parts.push(format!("{label}: {}", escape(v)));
                }
            }
            (!parts.is_empty()).then_some(parts.join(", "))
        }
    }
}
```

## Tests

```rust name=tests path=render/weave_html.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::md::Document;

    fn render_doc(source: &str) -> (String, Diags) {
        render_doc_with_tables(source, &HashMap::new())
    }

    fn render_doc_with_tables(source: &str, tables: &HashMap<usize, (String, String)>) -> (String, Diags) {
        render_doc_with_artifacts(source, tables, &HashMap::new(), false)
    }

    fn render_doc_with_artifacts(
        source: &str,
        tables: &HashMap<usize, (String, String)>,
        images: &HashMap<usize, (Vec<u8>, String)>,
        figures_outside: bool,
    ) -> (String, Diags) {
        render_doc_with_figures(source, tables, images, &HashMap::new(), &HashMap::new(), figures_outside)
    }

    fn render_doc_with_figures(
        source: &str,
        tables: &HashMap<usize, (String, String)>,
        images: &HashMap<usize, (Vec<u8>, String)>,
        labels: &HashMap<usize, String>,
        numbers: &HashMap<usize, u32>,
        figures_outside: bool,
    ) -> (String, Diags) {
        render_doc_with_refs(source, tables, images, labels, numbers, &HashMap::new(), figures_outside)
    }

    /// The same, plus every map `weave` resolves for a figure: its own
    /// label (decision 63), its own number (decision 65), and what each
    /// same-file fragment points at (decision 64). A test asserting on an
    /// `id`, a numbered figcaption, or a reference needs them. Every other
    /// test here has no figure at all, so it passes none.
    fn render_doc_with_refs(
        source: &str,
        tables: &HashMap<usize, (String, String)>,
        images: &HashMap<usize, (Vec<u8>, String)>,
        labels: &HashMap<usize, String>,
        numbers: &HashMap<usize, u32>,
        refs: &HashMap<String, (String, String)>,
        figures_outside: bool,
    ) -> (String, Diags) {
        let mut parse_diags = Diags::new("t.md");
        let doc = Document::parse(source, &mut parse_diags);
        let mut diags = Diags::new("t.md");
        let slugs = test_slugs(&doc);
        let out = render(
            &doc, "Title", None, tables, images, labels, numbers, &slugs, refs, figures_outside, None, &mut diags,
        );
        (out, diags)
    }

    /// The same map `weave::heading_slugs` builds, rebuilt here so a unit
    /// test sees the heading ids a real weave emits rather than a document
    /// with none.
    fn test_slugs(doc: &Document) -> HashMap<u32, String> {
        let mut slugger = crate::graph::slug::Slugger::new();
        doc.headings().iter().map(|(_, inlines, line)| (*line, slugger.assign(&Inline::plain(inlines)))).collect()
    }

    #[test]
    fn page_shell_and_title() {
        let (out, _) = render_doc("# H\n");
        assert!(out.starts_with("<!doctype html>\n"));
        assert!(out.contains("<title>Title</title>"));
        assert!(out.contains("<h1>Title</h1>"));
    }

    #[test]
    fn cover_false_drops_the_title_block_but_never_the_head_title() {
        let (out, _) = render_doc("---\nauthor: Jane Doe\ndate: 2026-09-18\ncover: false\n---\n# H\n");
        assert!(!out.contains("<h1>Title</h1>"), "{out}");
        assert!(!out.contains("Jane Doe"), "the byline is part of the same block: {out}");
        assert!(!out.contains("September 18, 2026"), "{out}");
        assert!(out.contains("<title>Title</title>"), "a browser tab still needs a name: {out}");
        assert!(out.contains("<h1 id=\"h\">H</h1>"), "the document's own heading is untouched: {out}");
    }

    #[test]
    fn cover_true_renders_the_same_title_block_as_no_cover_key() {
        let (with, _) = render_doc("---\nauthor: Jane Doe\ncover: true\n---\n# H\n");
        let (without, _) = render_doc("---\nauthor: Jane Doe\n---\n# H\n");
        assert_eq!(with, without, "dropping the repeated heading is `weave.rs`'s own job, not this module's");
    }

    #[test]
    fn an_unrecognized_cover_value_keeps_the_title_block() {
        let (out, _) = render_doc("---\ncover: yes\n---\n# H\n");
        assert!(out.contains("<h1>Title</h1>"), "never guessed at, so the default stands: {out}");
    }

    #[test]
    fn author_renders_as_an_unlabeled_byline() {
        let (out, _) = render_doc("---\nauthor: Jane Doe\n---\n# H\n");
        assert!(out.contains("<p class=\"byline\">Jane Doe</p>"), "{out}");
        assert!(!out.contains("Author"), "{out}");
    }

    #[test]
    fn multiple_authors_join_with_commas() {
        let (out, _) = render_doc("---\nauthor: [Jane Doe, John Smith]\n---\n# H\n");
        assert!(out.contains("<p class=\"byline\">Jane Doe, John Smith</p>"), "{out}");
    }

    #[test]
    fn date_renders_as_a_formatted_date_not_raw_text() {
        let (out, _) = render_doc("---\ndate: 2026-09-18\n---\n# H\n");
        assert!(out.contains("<p class=\"byline-date\">September 18, 2026</p>"), "{out}");
        assert!(!out.contains("2026-09-18"), "{out}");
    }

    #[test]
    fn a_year_and_month_date_omits_the_day() {
        let (out, _) = render_doc("---\ndate: 2026-09\n---\n# H\n");
        assert!(out.contains("<p class=\"byline-date\">September 2026</p>"), "{out}");
    }

    #[test]
    fn a_bare_year_renders_as_plain_text() {
        let (out, _) = render_doc("---\ndate: 2026\n---\n# H\n");
        assert!(out.contains("<p class=\"byline-date\">2026</p>"), "{out}");
    }

    #[test]
    fn unparseable_date_text_falls_back_to_literal_text() {
        let (out, _) = render_doc("---\ndate: sometime next year\n---\n# H\n");
        assert!(out.contains("<p class=\"byline-date\">sometime next year</p>"), "{out}");
    }

    #[test]
    fn no_author_or_date_means_no_byline_lines_at_all() {
        let (out, _) = render_doc("# H\n");
        assert!(!out.contains("<p class=\"byline\">"), "{out}");
        assert!(!out.contains("<p class=\"byline-date\">"), "{out}");
    }

    #[test]
    fn heading_ids_match_toc_hrefs() {
        let (out, _) = render_doc("# One\n\n## Two\n");
        assert!(out.contains("<h1 id=\"one\">One</h1>"));
        assert!(out.contains("<h2 id=\"two\">Two</h2>"));
        assert!(out.contains("href=\"#one\""));
        assert!(out.contains("href=\"#two\""));
    }

    #[test]
    fn toc_nests_by_relative_level() {
        let (out, _) = render_doc("# One\n\n## Two\n\n### Three\n\n## Four\n");
        // `Two` and `Four` are siblings one level under `One`; `Three`
        // nests one level under `Two` alone.
        let two_pos = out.find("href=\"#two\"").unwrap();
        let three_pos = out.find("href=\"#three\"").unwrap();
        let four_pos = out.find("href=\"#four\"").unwrap();
        let close_after_three = out[three_pos..].find("</ul>").unwrap() + three_pos;
        assert!(two_pos < three_pos && three_pos < close_after_three && close_after_three < four_pos);
    }

    #[test]
    fn toc_toggle_is_pure_css() {
        let (out, _) = render_doc("# H\n");
        assert!(out.contains("id=\"toc-toggle\""));
        assert!(!out.contains("<script"));
    }

    #[test]
    fn emphasis_strong_code_and_link() {
        let (out, _) = render_doc("_a_ and **b** and `c` and [t](https://x)\n");
        assert!(out.contains("<em>a</em>"));
        assert!(out.contains("<strong>b</strong>"));
        assert!(out.contains("<code>c</code>"));
        assert!(out.contains("<a href=\"https://x\">t</a>"));
    }

    #[test]
    fn passthrough_is_escaped_not_raw() {
        let (out, _) = render_doc("> a <script>bad</script>\n");
        assert!(out.contains("&lt;script&gt;"));
        assert!(!out.contains("<script>bad"));
    }

    #[test]
    fn gfm_table_with_alignment() {
        let (out, diags) = render_doc("| A | B |\n|:--|--:|\n| a | b |\n");
        assert!(diags.is_empty());
        assert!(out.contains("<th style=\"text-align:left\">A</th>"));
        assert!(out.contains("<th style=\"text-align:right\">B</th>"));
        assert!(out.contains("<td style=\"text-align:left\">a</td>"));
    }

    #[test]
    fn ragged_row_renders_a_short_row_with_empty_cells() {
        let (out, _) = render_doc("| A | B |\n|---|---|\n| a |\n");
        assert!(out.contains("<td>a</td>\n<td></td>"));
    }

    #[test]
    fn csv_block_becomes_a_table() {
        let (out, _) = render_doc("```csv\na,b\n1,2\n```\n");
        assert!(out.contains("<th>a</th>"));
        assert!(out.contains("<td>1</td>"));
    }

    #[test]
    fn malformed_json_block_falls_back_to_code_with_a_warning() {
        let (out, diags) = render_doc("```json\n{\"a\":1}\n```\n");
        assert!(!diags.is_empty());
        assert!(out.contains("<code class=\"language-json\">"));
        // `"` is not special in element content, only inside an
        // attribute -- `escape` (used for code content) leaves it alone.
        assert!(out.contains("{\"a\":1}"));
    }

    #[test]
    fn ordinary_code_block_has_a_language_class() {
        let (out, _) = render_doc("```rust\nfn f() {}\n```\n");
        assert!(out.contains("<code class=\"language-rust\">"));
        assert!(out.contains("fn f() {}"));
    }

    #[test]
    fn weave_hidden_code_block_produces_nothing() {
        let (out, _) = render_doc("text\n\n```sh weave=hidden\necho hi\n```\n\nmore\n");
        assert!(!out.contains("echo hi"), "{out}");
        assert!(out.contains("<p>text</p>"), "{out}");
        assert!(out.contains("<p>more</p>"), "{out}");
    }

    #[test]
    fn weave_other_value_is_shown_normally() {
        let (out, _) = render_doc("```sh weave=summary\necho hi\n```\n");
        assert!(out.contains("echo hi"), "{out}");
    }

    #[test]
    fn a_recorded_result_renders_as_one_paired_figure() {
        let src = "```sh name=a\necho hi\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nhi\n```\n";
        let (out, _) = render_doc(src);
        assert!(!out.contains("dankg:result"), "{out}");
        assert!(out.contains("<figure class=\"eval-pair\">"), "{out}");
        assert!(out.contains("<figcaption>Output</figcaption>"), "{out}");
        let fig_start = out.find("<figure").unwrap();
        let fig_end = out.find("</figure>").unwrap();
        assert!(out[fig_start..fig_end].contains("echo hi"), "{out}");
        assert!(out[fig_start..fig_end].contains(">hi\n<"), "{out}");
    }

    #[test]
    fn a_failed_result_gets_the_failed_class_and_caption() {
        let src = "```sh name=a\nfalse\n```\n\n<!-- dankg:result name=a hash=0000000000000001 failed -->\n\n```\n```\n";
        let (out, _) = render_doc(src);
        assert!(out.contains("<figure class=\"eval-pair failed\">"), "{out}");
        assert!(out.contains("<figcaption>Output (failed)</figcaption>"), "{out}");
    }

    #[test]
    fn produces_and_reads_render_as_a_provenance_line() {
        let src = "```sql db=w name=a\nselect 1;\n```\n\n<!-- dankg:result name=a hash=0000000000000001 produces=orders reads=customers -->\n\n```\nn\n1\n```\n";
        let (out, _) = render_doc(src);
        assert!(out.contains("writes: orders"), "{out}");
        assert!(out.contains("reads: customers"), "{out}");
    }

    #[test]
    fn a_hidden_source_hides_its_paired_result_too() {
        let src = "```sh name=a weave=hidden\necho hi\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nhi\n```\n";
        let (out, _) = render_doc(src);
        assert!(!out.contains("echo hi"), "{out}");
        assert!(!out.contains("dankg:result"), "{out}");
        assert!(!out.contains("<figure"), "{out}");
    }

    #[test]
    fn a_named_block_with_no_recorded_result_renders_unpaired() {
        let (out, _) = render_doc("```sh name=a\necho hi\n```\n");
        assert!(!out.contains("<figure"), "{out}");
        assert!(out.contains("echo hi"), "{out}");
    }

    #[test]
    fn a_produced_table_renders_inside_the_pair() {
        let src = "```python name=a produces=file:data.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let (out, _) = render_doc_with_tables(src, &tables);
        assert!(out.contains("wrote data.csv"), "{out}");
        assert!(out.contains("<figcaption>file:data.csv</figcaption>"), "{out}");
        assert!(out.contains("<th>a</th>"), "{out}");
        assert!(out.contains("<td>1</td>"), "{out}");
    }

    #[test]
    fn a_produced_table_gets_a_nested_figure_with_its_caption_above() {
        let src = "```python name=a produces=file:data.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let (out, _) = render_doc_with_tables(src, &tables);
        assert!(out.contains("<figure class=\"table-figure\">"), "{out}");
        let fig_start = out.find("<figure class=\"table-figure\">").unwrap();
        let cap = out.find("<figcaption>file:data.csv</figcaption>").unwrap();
        let table_start = out[fig_start..].find("<table").unwrap() + fig_start;
        assert!(fig_start < cap && cap < table_start, "caption must come before the table: {out}");
    }

    #[test]
    fn a_labelled_table_figure_carries_its_own_id() {
        let src = "```python name=a produces=file:data.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let mut labels = HashMap::new();
        labels.insert(0, "revenue".to_string());
        let (out, _) = render_doc_with_figures(src, &tables, &HashMap::new(), &labels, &HashMap::new(), false);
        assert!(out.contains("<figure class=\"table-figure\" id=\"fig-revenue\">"), "{out}");
    }

    #[test]
    fn a_labelled_image_figure_carries_its_own_id() {
        let src = "```python name=c produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=c hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n";
        let mut images = HashMap::new();
        images.insert(0, (vec![0xffu8, 0xd8, 0xff, 0xe0], "chart.jpg".to_string()));
        let mut labels = HashMap::new();
        labels.insert(0, "chart".to_string());
        let (out, _) = render_doc_with_figures(src, &HashMap::new(), &images, &labels, &HashMap::new(), false);
        assert!(out.contains("<figure class=\"image-figure\" id=\"fig-chart\">"), "{out}");
    }

    /// The id goes on the nested figure, never on the pair's own outer
    /// `<figure class="eval-pair">`. A reference has to land on the
    /// artifact itself, not on the source block above it.
    #[test]
    fn a_labelled_figure_leaves_the_pairs_own_outer_figure_alone() {
        let src = "```python name=c produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=c hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n";
        let mut images = HashMap::new();
        images.insert(0, (vec![0xffu8, 0xd8, 0xff, 0xe0], "chart.jpg".to_string()));
        let mut labels = HashMap::new();
        labels.insert(0, "chart".to_string());
        let (out, _) = render_doc_with_figures(src, &HashMap::new(), &images, &labels, &HashMap::new(), false);
        assert!(out.contains("<figure class=\"eval-pair\">"), "{out}");
        assert_eq!(out.matches("id=\"fig-chart\"").count(), 1, "{out}");
    }

    #[test]
    fn an_unlabelled_figure_carries_no_id() {
        let src = "```python name=a produces=file:data.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let (out, _) = render_doc_with_tables(src, &tables);
        assert!(out.contains("<figure class=\"table-figure\">"), "{out}");
        assert!(!out.contains("id=\"fig-"), "{out}");
    }

    /// The number is dankg's own, counted by `weave::figures`, written as
    /// literal text (decision 65). HTML has no way to count for itself.
    #[test]
    fn a_numbered_table_figure_carries_typsts_own_supplement_wording() {
        let src = "```python name=a produces=file:data.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let mut numbers = HashMap::new();
        numbers.insert(0, 3);
        let (out, _) = render_doc_with_figures(src, &tables, &HashMap::new(), &HashMap::new(), &numbers, false);
        assert!(out.contains("<figcaption>Table 3: file:data.csv</figcaption>"), "{out}");
    }

    #[test]
    fn a_numbered_image_figure_carries_typsts_own_supplement_wording() {
        let src = "```python name=c produces=file:chart.png caption=\"A chart\"\nsavefig()\n```\n\n<!-- dankg:result name=c hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n";
        let mut images = HashMap::new();
        images.insert(0, (vec![0xffu8, 0xd8, 0xff, 0xe0], "chart.jpg".to_string()));
        let mut numbers = HashMap::new();
        numbers.insert(0, 2);
        let (out, _) = render_doc_with_figures(src, &HashMap::new(), &images, &HashMap::new(), &numbers, false);
        assert!(out.contains("<figcaption>Figure 2: A chart</figcaption>"), "{out}");
    }

    /// The pair's own output figcaption is not a figure's caption and
    /// takes no number. Only the nested `<figure>` decision 54 builds is
    /// numbered.
    #[test]
    fn the_pairs_own_output_figcaption_stays_unnumbered() {
        let src = "```python name=a produces=file:data.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let mut numbers = HashMap::new();
        numbers.insert(0, 1);
        let (out, _) = render_doc_with_figures(src, &tables, &HashMap::new(), &HashMap::new(), &numbers, false);
        assert!(out.contains("<figcaption>Output</figcaption>"), "{out}");
        assert_eq!(out.matches("Table 1: ").count(), 1, "{out}");
    }

    #[test]
    fn an_unnumbered_figure_carries_no_supplement_at_all() {
        let src = "```python name=a produces=file:data.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let (out, _) = render_doc_with_tables(src, &tables);
        assert!(out.contains("<figcaption>file:data.csv</figcaption>"), "{out}");
        assert!(!out.contains("Table "), "{out}");
    }

    fn ref_map() -> HashMap<String, (String, String)> {
        [
            ("chart".to_string(), ("fig-chart".to_string(), "Table 1".to_string())),
            ("intro".to_string(), ("intro".to_string(), "Intro".to_string())),
        ]
        .into_iter()
        .collect()
    }

    fn render_refs(source: &str) -> String {
        let (out, _) =
            render_doc_with_refs(source, &HashMap::new(), &HashMap::new(), &HashMap::new(), &HashMap::new(), &ref_map(), false);
        out
    }

    /// A bare figure reference reads as the number the pre-pass counted
    /// (decision 65). HTML cannot count for itself.
    #[test]
    fn a_bare_figure_reference_links_to_the_figure_and_reads_as_its_number() {
        let out = render_refs("See [[#chart]].\n");
        assert!(out.contains("<a href=\"#fig-chart\">Table 1</a>"), "{out}");
    }

    #[test]
    fn a_labelled_figure_reference_carries_the_authors_own_words() {
        let out = render_refs("See [[#chart|the revenue chart]].\n");
        assert!(out.contains("<a href=\"#fig-chart\">the revenue chart</a>"), "{out}");
    }

    /// A bare heading reference reads as the heading's own title
    /// (decision 68), the same text `toc` already writes for it. HTML
    /// numbers no heading, so a number would point at nothing visible.
    #[test]
    fn a_bare_heading_reference_reads_as_the_headings_own_title() {
        let out = render_refs("# Intro\n\nSee [[#intro]].\n");
        assert!(out.contains("<a href=\"#intro\">Intro</a>"), "{out}");
    }

    /// The fragment an author writes is the label, not the id: a figure's
    /// own id carries a `fig-` prefix (decision 63), so the link has to be
    /// rewritten rather than passed through.
    #[test]
    fn a_markdown_link_to_a_fragment_points_at_the_resolved_anchor() {
        let out = render_refs("See [the chart](#chart).\n");
        assert!(out.contains("<a href=\"#fig-chart\">the chart</a>"), "{out}");
        assert!(!out.contains("href=\"#chart\""), "{out}");
    }

    #[test]
    fn a_markdown_link_to_an_unresolved_fragment_is_left_exactly_as_written() {
        let out = render_refs("See [elsewhere](#other-page).\n");
        assert!(out.contains("<a href=\"#other-page\">elsewhere</a>"), "{out}");
    }

    /// Decision 41 is narrowed, not repealed.
    #[test]
    fn a_wikilink_naming_another_file_still_renders_as_plain_text() {
        let out = render_refs("See [[Other]] and [[Other#chart]].\n");
        assert!(out.contains("See Other and Other#chart."), "{out}");
        assert!(!out.contains("<a href=\"#Other"), "{out}");
    }

    #[test]
    fn a_produced_artifact_nests_inside_the_pairs_own_figure_by_default() {
        let src = "```python name=a produces=file:data.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let (out, _) = render_doc_with_tables(src, &tables);
        // No `--figures-outside` and no `figure=` attribute: decision 56's
        // own default is `inside`, decision 54's original nesting -- the
        // nested figure must close before the pair's own outer figure does.
        let nested_close = out.find("</figure>\n").unwrap();
        let outer_close = out.rfind("</figure>\n").unwrap();
        assert!(nested_close < outer_close, "{out}");
    }

    #[test]
    fn figures_outside_moves_the_artifact_after_the_pairs_own_figure() {
        let src = "```python name=a produces=file:data.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let (out, _) = render_doc_with_artifacts(src, &tables, &HashMap::new(), true);
        let outer_close = out.find("</figure>\n").unwrap();
        let nested_start = out.find("<figure class=\"table-figure\">").unwrap();
        assert!(outer_close < nested_start, "the outer figure must close before the nested one starts: {out}");
    }

    #[test]
    fn a_block_level_figure_outside_overrides_the_document_default() {
        let src = "```python name=a produces=file:data.csv figure=outside\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let (out, _) = render_doc_with_tables(src, &tables);
        let outer_close = out.find("</figure>\n").unwrap();
        let nested_start = out.find("<figure class=\"table-figure\">").unwrap();
        assert!(outer_close < nested_start, "{out}");
    }

    #[test]
    fn a_block_level_figure_inside_overrides_a_figures_outside_default() {
        let src = "```python name=a produces=file:data.csv figure=inside\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let (out, _) = render_doc_with_artifacts(src, &tables, &HashMap::new(), true);
        let nested_close = out.find("</figure>\n").unwrap();
        let outer_close = out.rfind("</figure>\n").unwrap();
        assert!(nested_close < outer_close, "{out}");
    }

    #[test]
    fn figure_attribute_is_inert_without_a_captioned_artifact() {
        let src = "```sh name=a figure=outside\necho hi\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nhi\n```\n";
        let (out, _) = render_doc(src);
        assert!(out.contains("echo hi"), "{out}");
        assert!(!out.contains("table-figure") && !out.contains("image-figure"), "{out}");
    }

    #[test]
    fn a_json_artifact_that_is_not_table_shaped_falls_back_to_code() {
        let src = "```python name=a produces=file:data.json\nwrite_json()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\ndone\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("json".to_string(), "{\"not\": \"a table\"}".to_string()));
        let (out, diags) = render_doc_with_tables(src, &tables);
        assert!(out.contains("not"), "{out}");
        assert!(!diags.items().is_empty(), "expected a warning about the non-table json");
    }

    #[test]
    fn no_produced_table_means_no_extra_section() {
        let src = "```python name=a produces=file:data.csv\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let (out, _) = render_doc(src);
        assert!(!out.contains("<table"), "{out}");
    }

    #[test]
    fn a_caption_overrides_the_artifact_figcaption_not_output() {
        let src = "```python name=a produces=file:data.csv caption=\"Quarterly revenue\"\nwrite_csv()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote data.csv\n```\n";
        let mut tables = HashMap::new();
        tables.insert(0, ("csv".to_string(), "a,b\n1,2\n".to_string()));
        let (out, _) = render_doc_with_tables(src, &tables);
        assert!(out.contains("<figcaption>Quarterly revenue</figcaption>"), "{out}");
        assert!(!out.contains("<figcaption>file:data.csv</figcaption>"), "{out}");
        assert!(out.contains("<figcaption>Output</figcaption>"), "{out}");
    }

    #[test]
    fn a_caption_with_no_artifact_overrides_output_and_keeps_failed() {
        let src = "```sh name=a caption=Result\nfalse\n```\n\n<!-- dankg:result name=a hash=0000000000000001 failed -->\n\n```\n```\n";
        let (out, _) = render_doc(src);
        assert!(out.contains("<figcaption>Result (failed)</figcaption>"), "{out}");
        assert!(!out.contains("<figcaption>Output"), "{out}");
    }

    #[test]
    fn source_hidden_drops_both_halves_of_a_successful_pair() {
        let src = "```sh name=a weave=source-hidden\necho hi\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nhi\n```\n";
        let (out, _) = render_doc(src);
        assert!(!out.contains("echo hi"), "{out}");
        assert!(!out.contains("<figcaption>Output</figcaption>"), "{out}");
        assert!(!out.contains(">hi\n<"), "{out}");
        // Nothing left inside, so no empty pair wrapper either.
        assert!(!out.contains("class=\"eval-pair\""), "{out}");
    }

    #[test]
    fn source_hidden_still_renders_a_captioned_figure() {
        let src = "```python name=a produces=file:chart.png weave=source-hidden caption=\"A chart\"\nsavefig()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n";
        let mut images = HashMap::new();
        images.insert(0, (b"png".to_vec(), "chart.png".to_string()));
        let (out, _) = render_doc_with_artifacts(src, &HashMap::new(), &images, true);
        assert!(!out.contains("savefig()"), "{out}");
        assert!(!out.contains("wrote chart.png"), "{out}");
        assert!(out.contains("class=\"image-figure\""), "{out}");
        assert!(out.contains("<figcaption>A chart</figcaption>"), "{out}");
    }

    #[test]
    fn a_failed_pair_keeps_its_output_even_when_source_hidden() {
        let src = "```sh name=a weave=source-hidden\nfalse\n```\n\n<!-- dankg:result name=a hash=0000000000000001 failed -->\n\n```\nboom\n```\n";
        let (out, _) = render_doc(src);
        assert!(out.contains("<figcaption>Output (failed)</figcaption>"), "{out}");
        assert!(out.contains("boom"), "{out}");
    }

    #[test]
    fn output_hidden_keeps_the_source_but_drops_everything_after() {
        let src = "```sql db=w name=a weave=output-hidden\nselect 1;\n```\n\n<!-- dankg:result name=a hash=0000000000000001 produces=orders -->\n\n```\nn\n1\n```\n";
        let (out, _) = render_doc(src);
        assert!(out.contains("select 1;"), "{out}");
        // The source's own fence carries a `language-sql` class; the
        // captured output's fence carries none, so its absence is what
        // "the output half is gone" comes down to.
        assert!(!out.contains("<pre><code>"), "{out}");
        assert!(!out.contains("<figcaption>"), "{out}");
        assert!(!out.contains("writes: orders"), "{out}");
    }

    #[test]
    fn source_hidden_hides_an_unpaired_block_entirely() {
        let (out, _) = render_doc("```sh name=a weave=source-hidden\necho hi\n```\n");
        assert!(!out.contains("echo hi"), "{out}");
        assert!(!out.contains("<figure"), "{out}");
    }

    #[test]
    fn output_hidden_is_a_noop_on_an_unpaired_block() {
        let (out, _) = render_doc("```sh name=a weave=output-hidden\necho hi\n```\n");
        assert!(out.contains("echo hi"), "{out}");
    }

    #[test]
    fn source_hidden_or_output_hidden_keep_the_failed_class() {
        let src = "```sh name=a weave=source-hidden\nfalse\n```\n\n<!-- dankg:result name=a hash=0000000000000001 failed -->\n\n```\n```\n";
        let (out, _) = render_doc(src);
        assert!(out.contains("<figure class=\"eval-pair failed\">"), "{out}");

        let src = "```sh name=a weave=output-hidden\nfalse\n```\n\n<!-- dankg:result name=a hash=0000000000000001 failed -->\n\n```\n```\n";
        let (out, _) = render_doc(src);
        assert!(out.contains("<figure class=\"eval-pair failed\">"), "{out}");
    }

    #[test]
    fn base64_encode_matches_rfc_4648_test_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn mime_for_covers_every_recognized_image_extension() {
        assert_eq!(mime_for("a.png"), "image/png");
        assert_eq!(mime_for("a.PNG"), "image/png");
        assert_eq!(mime_for("a.jpg"), "image/jpeg");
        assert_eq!(mime_for("a.jpeg"), "image/jpeg");
        assert_eq!(mime_for("a.gif"), "image/gif");
        assert_eq!(mime_for("a.svg"), "image/svg+xml");
        assert_eq!(mime_for("a.webp"), "image/webp");
    }

    #[test]
    fn a_produced_image_renders_as_a_base64_data_uri() {
        let src = "```python name=a produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n";
        let mut images = HashMap::new();
        images.insert(0, (b"hi".to_vec(), "chart.png".to_string()));
        let (out, _) = render_doc_with_artifacts(src, &HashMap::new(), &images, false);
        assert!(out.contains("<img src=\"data:image/png;base64,aGk=\""), "{out}");
        assert!(out.contains("<figcaption>file:chart.png</figcaption>"), "{out}");
    }

    #[test]
    fn a_produced_image_gets_a_nested_figure_with_its_caption_below() {
        let src = "```python name=a produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n";
        let mut images = HashMap::new();
        images.insert(0, (b"hi".to_vec(), "chart.png".to_string()));
        let (out, _) = render_doc_with_artifacts(src, &HashMap::new(), &images, false);
        assert!(out.contains("<figure class=\"image-figure\">"), "{out}");
        let fig_start = out.find("<figure class=\"image-figure\">").unwrap();
        let img = out[fig_start..].find("<img").unwrap() + fig_start;
        let cap = out.find("<figcaption>file:chart.png</figcaption>").unwrap();
        assert!(fig_start < img && img < cap, "caption must come after the image: {out}");
    }

    #[test]
    fn no_produced_image_means_no_img_tag() {
        let src = "```python name=a produces=file:chart.png\nsavefig()\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nwrote chart.png\n```\n";
        let (out, _) = render_doc(src);
        assert!(!out.contains("<img"), "{out}");
    }

    #[test]
    fn ordered_and_unordered_lists() {
        let (out, _) = render_doc("- a\n- b\n");
        assert!(out.contains("<ul>\n<li>"));
        let (out, _) = render_doc("3. a\n4. b\n");
        assert!(out.contains("<ol start=\"3\">"));
    }

    #[test]
    fn thematic_break_is_hr() {
        let (out, _) = render_doc("---\n");
        assert!(out.contains("<hr>\n"));
    }

    #[test]
    fn extra_css_is_appended_after_weave_css_not_swapped_in() {
        let mut diags = Diags::new("t.md");
        let doc = Document::parse("# H\n", &mut diags);
        let out = render(
            &doc,
            "Title",
            Some("body { font-family: serif; }"),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            false,
            None,
            &mut diags,
        );
        let style_start = out.find("<style>").unwrap();
        let style_end = out.find("</style>").unwrap();
        let style = &out[style_start..style_end];
        let base_pos = style.find(":root {").unwrap();
        let extra_pos = style.find("font-family: serif").unwrap();
        assert!(base_pos < extra_pos, "custom CSS should come after WEAVE_CSS: {style}");
    }

    fn render_doc_with_bib(source: &str, yaml: &str, order: &[&str]) -> String {
        let mut parse_diags = Diags::new("t.md");
        let doc = Document::parse(source, &mut parse_diags);
        let mut bib_diags = Diags::new("t.yml");
        let entries: HashMap<String, BibEntry> =
            crate::data::bib::parse(yaml, &mut bib_diags).into_iter().map(|e| (e.key.clone(), e)).collect();
        let order: Vec<String> = order.iter().map(|s| s.to_string()).collect();
        let bib = Bibliography { entries: &entries, order: &order };
        let mut diags = Diags::new("t.md");
        render(&doc, "Title", None, &HashMap::new(), &HashMap::new(), &HashMap::new(), &HashMap::new(), &HashMap::new(), &HashMap::new(), false, Some(&bib), &mut diags)
    }

    #[test]
    fn a_bracketed_citation_links_to_the_reference_with_the_right_ordinal() {
        let out = render_doc_with_bib("[@a]\n", "a:\n  type: article\n  title: A\n", &["a"]);
        assert!(out.contains("<a href=\"#ref-a\">1</a>"), "{out}");
        assert!(out.contains("id=\"ref-a\""), "{out}");
    }

    #[test]
    fn multiple_keys_in_one_bracket_render_one_group_with_both_links() {
        let yaml = "a:\n  type: article\n  title: A\nb:\n  type: article\n  title: B\n";
        let out = render_doc_with_bib("[@a; @b]\n", yaml, &["a", "b"]);
        assert!(
            out.contains("<span class=\"citation\">[<a href=\"#ref-a\">1</a>, <a href=\"#ref-b\">2</a>]</span>"),
            "{out}"
        );
    }

    #[test]
    fn a_narrative_citation_with_two_authors() {
        let yaml = "a:\n  type: article\n  title: A\n  author:\n    - Smith, John\n    - Doe, Jane\n  date: 2020\n";
        let out = render_doc_with_bib("@a argues\n", yaml, &["a"]);
        assert!(out.contains("<a href=\"#ref-a\">Smith &amp; Doe (2020)</a>"), "{out}");
    }

    #[test]
    fn a_narrative_citation_with_three_plus_authors() {
        let yaml =
            "a:\n  type: article\n  title: A\n  author:\n    - Smith, John\n    - Doe, Jane\n    - Lee, Kim\n  date: 2020\n";
        let out = render_doc_with_bib("@a argues\n", yaml, &["a"]);
        assert!(out.contains("<a href=\"#ref-a\">Smith et al. (2020)</a>"), "{out}");
    }

    #[test]
    fn a_narrative_citation_whose_key_does_not_resolve_falls_back_unlinked() {
        let out = render_doc_with_bib("@missing argues\n", "a:\n  type: article\n  title: A\n", &["missing"]);
        assert!(out.contains("@missing"), "{out}");
        assert!(!out.contains("<a href=\"#ref-missing\""), "{out}");
    }

    #[test]
    fn no_bibliography_configured_shows_the_literal_source_text() {
        let mut parse_diags = Diags::new("t.md");
        let doc = Document::parse("[@a]\n\n@b argues\n", &mut parse_diags);
        let mut diags = Diags::new("t.md");
        let out = render(&doc, "Title", None, &HashMap::new(), &HashMap::new(), &HashMap::new(), &HashMap::new(), &HashMap::new(), &HashMap::new(), false, None, &mut diags);
        assert!(out.contains("[@a]"), "{out}");
        assert!(out.contains("@b argues"), "{out}");
    }

    #[test]
    fn editor_and_affiliated_translator_both_render() {
        let yaml = "a:\n  type: article\n  title: A\n  editor: Ed, One\n  affiliated:\n    - role: translator\n      names: Trans, Lee\n";
        let out = render_doc_with_bib("[@a]\n", yaml, &["a"]);
        assert!(out.contains("ed. Ed"), "{out}");
        assert!(out.contains("translator: Trans"), "{out}");
    }

    #[test]
    fn a_two_level_parent_chain_renders_the_container_line() {
        let yaml =
            "a:\n  type: article\n  title: A\n  parent:\n    title: Issue\n    volume: 3\n    issue: 2\n    parent:\n      title: Journal\n";
        let out = render_doc_with_bib("[@a]\n", yaml, &["a"]);
        assert!(out.contains("In <i>Journal</i>, vol. 3, no. 2"), "{out}");
    }

    #[test]
    fn two_parent_entries_render_the_second_as_also_in() {
        let yaml = "a:\n  type: article\n  title: A\n  parent:\n    - title: P1\n    - title: P2\n";
        let out = render_doc_with_bib("[@a]\n", yaml, &["a"]);
        assert!(out.contains("In <i>P1</i>"), "{out}");
        assert!(out.contains("also in: P2"), "{out}");
    }

    #[test]
    fn a_structured_serial_number_renders_a_doi_link_and_plain_isbn() {
        let yaml = "a:\n  type: article\n  title: A\n  serial-number:\n    doi: 10.1/x\n    isbn: 123\n";
        let out = render_doc_with_bib("[@a]\n", yaml, &["a"]);
        assert!(out.contains("<a href=\"https://doi.org/10.1/x\">doi:10.1/x</a>"), "{out}");
        assert!(out.contains("isbn: 123"), "{out}");
    }

    #[test]
    fn a_url_with_an_access_date_renders_accessed() {
        let yaml = "a:\n  type: article\n  title: A\n  url:\n    value: https://x\n    date: 2020-01-01\n";
        let out = render_doc_with_bib("[@a]\n", yaml, &["a"]);
        assert!(out.contains("<a href=\"https://x\">https://x</a> (accessed 2020-01-01)"), "{out}");
    }

    #[test]
    fn an_uncited_entry_never_gets_an_li() {
        let yaml = "a:\n  type: article\n  title: A\nb:\n  type: article\n  title: B\n";
        let out = render_doc_with_bib("[@a]\n", yaml, &["a"]);
        assert!(out.contains("id=\"ref-a\""), "{out}");
        assert!(!out.contains("id=\"ref-b\""), "{out}");
    }
}
```
