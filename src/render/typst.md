# Render typst

`dankg weave --format pdf` never links a PDF library (decision 1). It
emits Typst markup and spawns a configured external `typst compile`,
the same external-command shape `cmd::build` already gives `tangle`'s
own build step. This module is only the markup emitter -- a sibling to
`dot.rs`/`mermaid.rs`, not a PDF generator. `weave.rs` owns writing the
`.typ` file and spawning the command; this module only turns a
`Document` into the text that goes in it.

Weave walks the whole document (plan-weave.md, decision 41), not the
named/top-level blocks `tangle`/`eval` narrow to. A GFM table (decision
42\) and a `csv`/`tsv`/`json`-tagged code block (decision 45) both
become a Typst `#table()`, converging on one internal `emit_table` so
the two sources share one code path and one set of tests. A block
outside DanKG's markdown subset -- `Block::Passthrough` -- is emitted
as escaped literal text, never as raw Typst: an unparsed construct must
never become unvalidated markup.

Typst is still pre-1.0. Its own syntax has changed between releases
before. This module has no way to pin one. `weave.rs` spawns whatever
`typst` binary the reader's own `[weave.pdf] command` configures --
the same trust boundary `[db.*] command` already gives `duckdb`. Pandoc's own Typst writer and Org-mode's export backends
solve the identical problem the identical way: emit text, shell out to
compile it, document a target version, and let a real incompatibility
surface as the compiler's own error rather than a runtime check this
module would otherwise have to maintain. The syntax below was written
and tested against Typst 0.15.1, confirmed by `tests/typst.rs`'s own
real-compile check.

```rust name=module_doc path=render/typst.rs
//! Typst markup emitter for `dankg weave --format pdf`.
//!
//! Emits Typst source only -- never a PDF, never links a Typst library.
//! `weave.rs` spawns a configured external `typst compile` against what
//! this module writes, the same escape hatch `cmd::build` already gives
//! `tangle`'s own build step.
//!
//! Walks the whole document, not just named/top-level blocks. A GFM table
//! and a `csv`/`tsv`/`json`-tagged code block both become a Typst
//! `#table()`, converging on one `emit_table` so the two sources share one
//! code path.
//!
//! The document's own frontmatter renders on a dedicated cover page, not
//! inline with the outline and body -- see this file's own *Cover page*
//! prose for why.
//!
//! Targets Typst 0.15.1's syntax (confirmed by `tests/typst.rs`'s own
//! real-compile check). Typst is still pre-1.0; no `typst --version`
//! check guards this, the same as `duckdb` gets none from `[db.*]
//! command` -- see this file's own prose for why.

use crate::data::table::{self, TableData};
use crate::diag::Diags;
use crate::eval::result::{self, Pair};
use crate::md::{Align, Block, Document, Frontmatter, InfoString, Inline, List, Value};
use std::fmt::Write as _;

/// `title` becomes the cover page's own large centered heading, with the
/// rest of the document's frontmatter printed beneath it. `#outline()`
/// (Typst's table of contents) is inserted right after the page break
/// that follows only when `toc` is true -- a PDF has no runtime to
/// toggle one, so the choice is made once, at compile time, unlike the
/// HTML backend's own in-page toggle.
pub fn render(doc: &Document, title: &str, toc: bool, diags: &mut Diags) -> String {
    let mut out = cover_page(title, &doc.frontmatter);
    if toc {
        out.push_str("#outline()\n\n");
    }
    out.push_str(&blocks(&doc.blocks, diags));
    out
}
```

## Cover page

A dedicated title page, not a `= title` heading inline with the rest
of the document. The alternative -- what this replaced -- put the
title directly in front of the outline and the body, which duplicated
it visibly whenever a file's own frontmatter `title` matched its first
real heading (a common pattern: many static-site generators expect
exactly this, a frontmatter title standing in for a stripped `<h1>`).
A separate page, ended with `#pagebreak()`, has no such collision --
the document's own first heading is free to repeat the title without
looking like a mistake.

```rust name=cover_page path=render/typst.rs
fn cover_page(title: &str, frontmatter: &Frontmatter) -> String {
    let mut out = String::new();
    out.push_str("#align(center)[\n");
    out.push_str("#v(1fr)\n\n");
    let _ = write!(out, "#text(size: 28pt, weight: \"bold\")[{}]\n\n", escape_typst(title));
    for line in frontmatter_lines(frontmatter) {
        let _ = write!(out, "#text(size: 12pt, fill: gray)[{line}]\n\n");
    }
    out.push_str("#v(1fr)\n");
    out.push_str("]\n#pagebreak()\n\n");
    out
}
```

Every frontmatter entry but two prints as its own centered line, in
frontmatter's own declared order: `title` (already the page's own
large heading) and any `dankg.*` key, an internal hint -- decision
27's `dankg.tangle.public`, say -- never meant for a reader. Nothing
else is assumed about what a corpus author's frontmatter contains.
DanKG's own frontmatter has no fixed schema (`md/frontmatter.rs`'s own
"flat key: value subset"). The cover page does not invent one either:
whatever key a reader wrote is whatever label they see.

```rust name=cover_frontmatter path=render/typst.rs
fn frontmatter_lines(frontmatter: &Frontmatter) -> Vec<String> {
    frontmatter
        .entries
        .iter()
        .filter(|(k, _)| k != "title" && !k.starts_with("dankg."))
        .map(|(k, v)| {
            let value = match v {
                Value::Scalar(s) => escape_typst(s),
                Value::List(items) => items.iter().map(|s| escape_typst(s)).collect::<Vec<_>>().join(", "),
            };
            format!("{}: {value}", humanize_key(k))
        })
        .collect()
}

fn humanize_key(key: &str) -> String {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
```

## Blocks

Every block in document order, joined by a blank line -- Typst reads a
blank line as a paragraph break the same way markdown does. A block
outside the subset is escaped literal text; an unparsed construct must
never become unvalidated Typst markup.

A code block tagged `weave=hidden` (decision 48) contributes nothing:
no text, no separator blank line either, as if it were never in the
document at all. It is a weave-only rendering hint. `dankg tangle`
and `dankg eval` never look at it. A hidden block still tangles and
still evaluates exactly as it would without the tag.

A named `Code` block immediately followed by its own recorded
`<!-- dankg:result ... -->` marker and output fence (decision 46)
renders as one `#block(...)`-wrapped unit instead of three unrelated
blocks. The marker itself is never emitted as text;
`eval::result::recognize_pair` reads it as data, not markup.

<!-- dankg:depends target=../../architecture.md#decision-46-a-recorded-eval-result-renders-paired-with-its-source quote="The marker itself is never rendered as text again." -->

```rust name=blocks_and_block path=render/typst.rs
/// A named `Code` block immediately followed by its recorded eval
/// result (decision 46) is recognized here, before `block` ever sees
/// it, and consumed as one unit -- three `Block`s in `items`, one
/// `#block(...)` in the output. Anything else falls through to `block`
/// exactly as before. Filtered before joining, not rendered-then-
/// discarded: a hidden block (decision 48), paired or not, contributes
/// no blank-line separator either, the same as if it were never in
/// `items` at all.
fn blocks(items: &[Block], diags: &mut Diags) -> String {
    let mut rendered = Vec::new();
    let mut i = 0;
    while i < items.len() {
        if let Block::Code { info, text, line, .. } = &items[i] {
            if let Some(pair) = result::recognize_pair(items, i) {
                if !info.weave_hidden() {
                    rendered.push(eval_pair(info, text, *line, &pair, diags));
                }
                i += 3;
                continue;
            }
        }
        if let Some(s) = block(&items[i], diags) {
            rendered.push(s);
        }
        i += 1;
    }
    rendered.join("\n")
}

fn block(b: &Block, diags: &mut Diags) -> Option<String> {
    Some(match b {
        Block::Heading { level, inlines, .. } => {
            let eq = "=".repeat((*level).clamp(1, 6) as usize);
            format!("{eq} {}\n", inline_text(inlines))
        }
        Block::Paragraph { inlines, .. } => format!("{}\n", inline_text(inlines)),
        Block::Code { info, .. } if info.weave_hidden() => return None,
        Block::Code { info, text, line, .. } => code_or_data_table(info, text, *line, diags),
        Block::List(l) => list(l, diags),
        Block::ThematicBreak { .. } => "#line(length: 100%)\n".to_string(),
        Block::Passthrough { text, .. } => format!("{}\n", escape_typst(text)),
        Block::Table { aligns, header, rows, .. } => table_block(aligns, header, rows),
    })
}

/// The `#block(stroke: ...)` decision 46 wraps a source block and its
/// recorded output in. `failed` alone picks the stroke color and the
/// caption text -- one flag, not two independent things that could
/// disagree.
fn eval_pair(source_info: &InfoString, source_text: &str, source_line: u32, pair: &Pair, diags: &mut Diags) -> String {
    let color = if pair.failed { "red" } else { "gray" };
    let caption = if pair.failed { "Output (failed)" } else { "Output" };
    let mut body = code_or_data_table(source_info, source_text, source_line, diags);
    let _ = write!(body, "\n#text(size: 9pt, fill: {color})[{caption}]\n\n");
    body.push_str(&code_or_data_table(pair.output_info, pair.output_text, pair.output_line, diags));
    if let Some(p) = provenance_text(pair) {
        let _ = write!(body, "\n#text(size: 9pt, fill: gray)[{}]\n", escape_typst(&p));
    }
    format!("#block(stroke: (left: 2pt + {color}), inset: (left: 8pt, rest: 4pt))[\n{body}]\n")
}

/// `produces`/`reads` (decision 33), when either is non-empty, is the
/// only way a reader of typeset output can see what a `db=` block's
/// run actually touched.
fn provenance_text(pair: &Pair) -> Option<String> {
    if pair.produces.is_empty() && pair.reads.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !pair.produces.is_empty() {
        parts.push(format!("writes: {}", pair.produces.join(", ")));
    }
    if !pair.reads.is_empty() {
        parts.push(format!("reads: {}", pair.reads.join(", ")));
    }
    Some(parts.join(" -- "))
}

fn list(l: &List, diags: &mut Diags) -> String {
    let marker = if l.ordered { "+" } else { "-" };
    let mut out = String::new();
    for item in &l.items {
        let body = blocks(&item.blocks, diags);
        for (i, line) in body.lines().enumerate() {
            if i == 0 {
                out.push_str(marker);
                out.push(' ');
            } else {
                out.push_str("  ");
            }
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}
```

## Code: a raw block, or a data table

A block's own language tag decides, never its content (plan-weave.md,
decision 45: "content is never inspected before the tag says to"). A
`csv`/`tsv` block always becomes a table -- `data::table::from_delimited`
cannot fail. A `json` block becomes one when `from_json` recognizes its
shape; otherwise it falls back to an ordinary raw block, with a
diagnostic, the same graceful degradation an unconfigured
`[weave.pdf] command` already gets elsewhere in weave.

```rust name=code_and_data_table path=render/typst.rs
fn code_or_data_table(info: &InfoString, text: &str, line: u32, diags: &mut Diags) -> String {
    match info.lang.as_deref() {
        Some("csv") => data_table_block(&table::from_delimited(text, ',')),
        Some("tsv") => data_table_block(&table::from_delimited(text, '\t')),
        Some("json") => match table::from_json(text) {
            Some(data) => data_table_block(&data),
            None => {
                diags.warn(
                    line,
                    "`json` block is not an array of objects or an array of arrays; rendered as code",
                );
                code_block(info, text)
            }
        },
        _ => code_block(info, text),
    }
}

fn code_block(info: &InfoString, text: &str) -> String {
    let lang = info.lang.as_deref().unwrap_or("");
    let len = longest_run(text, '`').max(2) + 1;
    let bar: String = "`".repeat(len);
    let mut out = format!("{bar}{lang}\n{text}");
    if !text.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&bar);
    out.push('\n');
    out
}

fn data_table_block(data: &TableData) -> String {
    let columns = data.header.len();
    let header: Vec<String> = data.header.iter().map(|c| escape_typst(c)).collect();
    let rows: Vec<Vec<String>> = data
        .rows
        .iter()
        .map(|r| {
            let mut cells: Vec<String> = r.iter().map(|c| escape_typst(c)).collect();
            cells.resize(columns, String::new());
            cells
        })
        .collect();
    emit_table(columns, Some(&header), &rows, None)
}
```

## Tables

`table_block` is the GFM path (decision 42): a ragged data row --
never padded by the parser or by `fmt`, decision 42's own round-trip
requirement -- is rectangled here, the one place padding has no
round-trip obligation to satisfy. `emit_table` is the shared tail both
this and `data_table_block` above call into. A GFM table and a
`csv`-tagged block emit through one code path.

```rust name=table path=render/typst.rs
fn table_block(aligns: &[Align], header: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) -> String {
    let columns = header.len();
    let header_cells: Vec<String> = header.iter().map(|c| inline_text(c)).collect();
    let rows: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut cells: Vec<String> = r.iter().map(|c| inline_text(c)).collect();
            cells.resize(columns, String::new());
            cells
        })
        .collect();
    emit_table(columns, Some(&header_cells), &rows, Some(aligns))
}

/// `header`/`rows` cells are already-rendered Typst markup, one `[cell]`
/// content block apiece. `aligns` is `None` for a CSV/JSON-sourced table --
/// it carries no alignment of its own, so every column takes Typst's own
/// `auto`, never an invented one.
fn emit_table(columns: usize, header: Option<&[String]>, rows: &[Vec<String>], aligns: Option<&[Align]>) -> String {
    if columns == 0 {
        return String::new();
    }
    let mut out = format!("#table(\n  columns: {columns},\n");
    if let Some(aligns) = aligns {
        let list: Vec<&str> =
            (0..columns).map(|i| typst_align(aligns.get(i).copied().unwrap_or(Align::None))).collect();
        out.push_str(&format!("  align: ({}),\n", list.join(", ")));
    }
    if let Some(header) = header {
        out.push_str("  table.header(");
        out.push_str(&bracketed(header));
        out.push_str("),\n");
    }
    for row in rows {
        out.push_str("  ");
        out.push_str(&bracketed(row));
        out.push_str(",\n");
    }
    out.push_str(")\n");
    out
}

fn bracketed(cells: &[String]) -> String {
    cells.iter().map(|c| format!("[{c}]")).collect::<Vec<_>>().join(", ")
}

fn typst_align(a: Align) -> &'static str {
    match a {
        Align::Left => "left",
        Align::Right => "right",
        Align::Center => "center",
        Align::None => "auto",
    }
}
```

## Inline text

One escaper for markup content (`#`, `*`, `_`, `` ` ``, `<`, `@`, `$`,
`\`) and a second, narrower one for a Typst string literal (`"`, `\`
only). A link's own destination sits inside `#link("...")`'s quotes,
not in markup position. It needs the string escaper, not the markup
one.

```rust name=inline_text path=render/typst.rs
fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for i in inlines {
        match i {
            Inline::Text(t) => out.push_str(&escape_typst(t)),
            Inline::Code(t) => out.push_str(&code_span(t)),
            Inline::Emph { inner, .. } => {
                out.push('_');
                out.push_str(&inline_text(inner));
                out.push('_');
            }
            Inline::Strong { inner, .. } => {
                out.push('*');
                out.push_str(&inline_text(inner));
                out.push('*');
            }
            Inline::Link { dest, text, .. } => {
                out.push_str("#link(\"");
                out.push_str(&escape_typst_string(dest));
                out.push_str("\")[");
                out.push_str(&inline_text(text));
                out.push(']');
            }
            // Weave is single-file (decision 41): there is no corpus to
            // resolve a wikilink's target against, so it renders as its
            // own label, plain text, never a link.
            Inline::WikiLink { target, label } => {
                out.push_str(&escape_typst(label.as_deref().unwrap_or(target)));
            }
            Inline::SoftBreak => out.push(' '),
            Inline::HardBreak => out.push_str("#linebreak()\n"),
        }
    }
    out
}

/// A code span needs a backtick run longer than any inside it, the same
/// `md/fmt.rs`'s own inline code escaper needs and for the same reason.
fn code_span(content: &str) -> String {
    let len = longest_run(content, '`') + 1;
    let bar: String = "`".repeat(len);
    format!("{bar}{content}{bar}")
}

fn escape_typst(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '#' | '*' | '_' | '`' | '<' | '@' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn escape_typst_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn longest_run(s: &str, ch: char) -> usize {
    let mut best = 0;
    let mut run = 0;
    for c in s.chars() {
        if c == ch {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}
```

## Tests

```rust name=tests path=render/typst.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::md::Document;

    fn render_doc(source: &str) -> (String, Diags) {
        let mut parse_diags = Diags::new("t.md");
        let doc = Document::parse(source, &mut parse_diags);
        let mut diags = Diags::new("t.md");
        let out = render(&doc, "Title", true, &mut diags);
        (out, diags)
    }

    #[test]
    fn title_page_then_outline() {
        let (out, _) = render_doc("# H\n");
        assert!(out.starts_with("#align(center)[\n#v(1fr)\n\n#text(size: 28pt, weight: \"bold\")[Title]"));
        let cover_end = out.find("#pagebreak()").unwrap();
        let outline_pos = out.find("#outline()").unwrap();
        assert!(cover_end < outline_pos, "outline should come after the cover page: {out}");
    }

    #[test]
    fn no_toc_omits_outline() {
        let mut d = Diags::new("t.md");
        let doc = Document::parse("# H\n", &mut d);
        let mut diags = Diags::new("t.md");
        let out = render(&doc, "Title", false, &mut diags);
        assert!(!out.contains("#outline()"));
    }

    #[test]
    fn frontmatter_prints_on_the_cover_page_not_title_or_internal_keys() {
        let mut d = Diags::new("t.md");
        let doc = Document::parse(
            "---\ntitle: Ignored Here\nauthor: Jane Doe\ntags: [rust, typst]\ndankg.tangle.public: true\n---\n# H\n",
            &mut d,
        );
        let mut diags = Diags::new("t.md");
        let out = render(&doc, "Title", true, &mut diags);
        let cover_end = out.find("#pagebreak()").unwrap();
        let cover = &out[..cover_end];
        assert!(cover.contains("Author: Jane Doe"), "{cover}");
        assert!(cover.contains("Tags: rust, typst"), "{cover}");
        assert!(!cover.contains("Ignored Here"), "{cover}");
        assert!(!cover.contains("dankg.tangle.public"), "{cover}");
        assert!(!cover.contains("Public"), "{cover}");
    }

    #[test]
    fn headings_by_level() {
        let (out, _) = render_doc("# One\n\n### Three\n");
        assert!(out.contains("= One\n"));
        assert!(out.contains("=== Three\n"));
    }

    #[test]
    fn emphasis_and_strong_and_code_span() {
        let (out, _) = render_doc("_a_ and **b** and `c`\n");
        assert!(out.contains("_a_"));
        assert!(out.contains("*b*"));
        assert!(out.contains("`c`"));
    }

    #[test]
    fn special_characters_are_escaped() {
        let (out, _) = render_doc("cost is \\$5 and a # sign\n");
        assert!(out.contains("\\$5"));
        assert!(out.contains("\\# sign"));
    }

    #[test]
    fn link_renders_as_a_link_call() {
        let (out, _) = render_doc("[text](https://example.com/a\"b)\n");
        assert!(out.contains("#link(\"https://example.com/a\\\"b\")[text]"));
    }

    #[test]
    fn unordered_and_ordered_lists() {
        let (out, _) = render_doc("- a\n- b\n");
        assert!(out.contains("- a\n- b\n"));
        let (out, _) = render_doc("1. a\n2. b\n");
        assert!(out.contains("+ a\n+ b\n"));
    }

    #[test]
    fn thematic_break_becomes_a_line() {
        let (out, _) = render_doc("---\n");
        assert!(out.contains("#line(length: 100%)\n"));
    }

    #[test]
    fn passthrough_is_escaped_not_raw() {
        let (out, _) = render_doc("> a # b\n");
        assert!(out.contains("\\# b"));
    }

    #[test]
    fn gfm_table_with_alignment_and_a_ragged_row() {
        let (out, diags) = render_doc("| A | B |\n|:--|--:|\n| a |\n");
        assert!(diags.is_empty());
        assert!(out.contains("columns: 2"));
        assert!(out.contains("align: (left, right)"));
        assert!(out.contains("table.header([A], [B])"));
        assert!(out.contains("[a], [],"));
    }

    #[test]
    fn csv_block_becomes_a_table_with_no_alignment() {
        let (out, _) = render_doc("```csv\na,b\n1,2\n```\n");
        assert!(out.contains("columns: 2"));
        assert!(!out.contains("align:"));
        assert!(out.contains("table.header([a], [b])"));
        assert!(out.contains("[1], [2],"));
    }

    #[test]
    fn json_array_of_objects_becomes_a_table() {
        let (out, diags) = render_doc("```json\n[{\"a\":1},{\"a\":2}]\n```\n");
        assert!(diags.is_empty());
        assert!(out.contains("table.header([a])"));
        assert!(out.contains("[1],"));
        assert!(out.contains("[2],"));
    }

    #[test]
    fn malformed_json_block_falls_back_to_code_with_a_warning() {
        let (out, diags) = render_doc("```json\n{\"a\":1}\n```\n");
        assert!(!diags.is_empty());
        assert!(out.contains("```json"));
        assert!(out.contains("{\"a\":1}"));
    }

    #[test]
    fn ordinary_code_block_is_a_raw_block() {
        let (out, _) = render_doc("```rust\nfn f() {}\n```\n");
        assert!(out.contains("```rust\nfn f() {}\n```\n"));
    }

    #[test]
    fn weave_hidden_code_block_produces_nothing() {
        let (out, _) = render_doc("text\n\n```sh weave=hidden\necho hi\n```\n\nmore\n");
        assert!(!out.contains("echo hi"), "{out}");
        assert!(out.contains("text\n"), "{out}");
        assert!(out.contains("more\n"), "{out}");
    }

    #[test]
    fn weave_other_value_is_shown_normally() {
        let (out, _) = render_doc("```sh weave=summary\necho hi\n```\n");
        assert!(out.contains("echo hi"), "{out}");
    }

    #[test]
    fn a_recorded_result_renders_as_one_paired_block() {
        let src = "```sh name=a\necho hi\n```\n\n<!-- dankg:result name=a hash=0000000000000001 -->\n\n```\nhi\n```\n";
        let (out, _) = render_doc(src);
        assert!(!out.contains("dankg:result"), "{out}");
        assert!(out.contains("#block(stroke: (left: 2pt + gray)"), "{out}");
        assert!(out.contains("echo hi"), "{out}");
        assert!(out.contains("Output"), "{out}");
        assert!(out.contains("hi\n```"), "{out}");
    }

    #[test]
    fn a_failed_result_uses_a_red_stroke_and_caption() {
        let src = "```sh name=a\nfalse\n```\n\n<!-- dankg:result name=a hash=0000000000000001 failed -->\n\n```\n```\n";
        let (out, _) = render_doc(src);
        assert!(out.contains("#block(stroke: (left: 2pt + red)"), "{out}");
        assert!(out.contains("Output (failed)"), "{out}");
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
        assert!(!out.contains("#block(stroke"), "{out}");
    }

    #[test]
    fn a_named_block_with_no_recorded_result_renders_unpaired() {
        let (out, _) = render_doc("```sh name=a\necho hi\n```\n");
        assert!(!out.contains("#block(stroke"), "{out}");
        assert!(out.contains("echo hi"), "{out}");
    }
}
```
