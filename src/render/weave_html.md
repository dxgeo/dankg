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

use crate::data::table::{self, TableData};
use crate::diag::Diags;
use crate::graph::slug::Slugger;
use crate::md::{Align, Block, Document, InfoString, Inline, List};
use crate::render::assets;
use std::collections::HashMap;
use std::fmt::Write as _;

/// `extra_css`, from `[weave.html] css` (decision 43), is appended after
/// `WEAVE_CSS` inside the same `<style>` tag rather than replacing it.
/// Appended, not swapped in: the cascade lets a reader override one rule
/// -- a font, a color, the reading column's width -- without having to
/// reimplement the CSS-only table-of-contents toggle or the table
/// borders just to get back what they did not mean to lose.
pub fn render(doc: &Document, title: &str, extra_css: Option<&str>, diags: &mut Diags) -> String {
    let slugs = heading_slugs(doc);

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
    toc(&mut out, doc, &slugs);

    out.push_str("<main>\n");
    let _ = writeln!(out, "<h1>{}</h1>", escape(title));
    blocks(&mut out, &doc.blocks, &slugs, diags);
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

/// Every heading's own slug, keyed by source line, from one `Slugger`
/// run in document order. The nav and the body headings below both read
/// the same map, so a nav link and its target agree on the anchor.
fn heading_slugs(doc: &Document) -> HashMap<u32, String> {
    let mut slugger = Slugger::new();
    doc.headings().iter().map(|(_, inlines, line)| (*line, slugger.assign(&Inline::plain(inlines)))).collect()
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

```rust name=blocks_and_block path=render/weave_html.rs
fn blocks(out: &mut String, items: &[Block], slugs: &HashMap<u32, String>, diags: &mut Diags) {
    for b in items {
        block(out, b, slugs, diags);
    }
}

fn block(out: &mut String, b: &Block, slugs: &HashMap<u32, String>, diags: &mut Diags) {
    match b {
        Block::Heading { level, inlines, line } => {
            let id = slugs.get(line).map(String::as_str).unwrap_or("");
            let _ = writeln!(out, "<h{level} id=\"{id}\">{}</h{level}>", inline_html(inlines));
        }
        Block::Paragraph { inlines, .. } => {
            let _ = writeln!(out, "<p>{}</p>", inline_html(inlines));
        }
        Block::Code { info, text, line, .. } => code_or_data_table(out, info, text, *line, diags),
        Block::List(l) => list(out, l, slugs, diags),
        Block::ThematicBreak { .. } => out.push_str("<hr>\n"),
        // Outside the subset. Escaped, not raw: an unparsed construct
        // must never become unvalidated HTML.
        Block::Passthrough { text, .. } => {
            let _ = writeln!(out, "<pre class=\"passthrough\">{}</pre>", escape(text));
        }
        Block::Table { aligns, header, rows, .. } => table_block(out, aligns, header, rows),
    }
}

fn list(out: &mut String, l: &List, slugs: &HashMap<u32, String>, diags: &mut Diags) {
    let tag = if l.ordered { "ol" } else { "ul" };
    if l.ordered && l.start != 1 {
        let _ = writeln!(out, "<{tag} start=\"{}\">", l.start);
    } else {
        let _ = writeln!(out, "<{tag}>");
    }
    for item in &l.items {
        out.push_str("<li>");
        blocks(out, &item.blocks, slugs, diags);
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
fn table_block(out: &mut String, aligns: &[Align], header: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) {
    let columns = header.len();
    out.push_str("<table>\n<thead>\n<tr>\n");
    for (i, cell) in header.iter().enumerate() {
        out.push_str("<th");
        push_align(out, aligns.get(i).copied());
        out.push('>');
        out.push_str(&inline_html(cell));
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
                out.push_str(&inline_html(cell));
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

```rust name=inline_html path=render/weave_html.rs
fn inline_html(inlines: &[Inline]) -> String {
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
                out.push_str(&inline_html(inner));
                out.push_str("</em>");
            }
            Inline::Strong { inner, .. } => {
                out.push_str("<strong>");
                out.push_str(&inline_html(inner));
                out.push_str("</strong>");
            }
            Inline::Link { dest, title, text } => {
                out.push_str("<a href=\"");
                out.push_str(&escape_attr(dest));
                out.push('"');
                if let Some(t) = title {
                    out.push_str(" title=\"");
                    out.push_str(&escape_attr(t));
                    out.push('"');
                }
                out.push('>');
                out.push_str(&inline_html(text));
                out.push_str("</a>");
            }
            // Weave is single-file (decision 41): there is no corpus to
            // resolve a wikilink's target against, the same reasoning
            // `render::typst` already gives.
            Inline::WikiLink { target, label } => {
                out.push_str(&escape(label.as_deref().unwrap_or(target)));
            }
            Inline::SoftBreak => out.push('\n'),
            Inline::HardBreak => out.push_str("<br>\n"),
        }
    }
    out
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

## Tests

```rust name=tests path=render/weave_html.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::md::Document;

    fn render_doc(source: &str) -> (String, Diags) {
        let mut parse_diags = Diags::new("t.md");
        let doc = Document::parse(source, &mut parse_diags);
        let mut diags = Diags::new("t.md");
        let out = render(&doc, "Title", None, &mut diags);
        (out, diags)
    }

    #[test]
    fn page_shell_and_title() {
        let (out, _) = render_doc("# H\n");
        assert!(out.starts_with("<!doctype html>\n"));
        assert!(out.contains("<title>Title</title>"));
        assert!(out.contains("<h1>Title</h1>"));
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
        let out = render(&doc, "Title", Some("body { font-family: serif; }"), &mut diags);
        let style_start = out.find("<style>").unwrap();
        let style_end = out.find("</style>").unwrap();
        let style = &out[style_start..style_end];
        let base_pos = style.find(":root {").unwrap();
        let extra_pos = style.find("font-family: serif").unwrap();
        assert!(base_pos < extra_pos, "custom CSS should come after WEAVE_CSS: {style}");
    }
}
```
