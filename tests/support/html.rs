//! CommonMark-style HTML renderer.
//!
//! Test-only, and deliberately so: DanKG's own graph renderer never turns
//! markdown into HTML at runtime -- it renders a *graph*. `dankg weave
//! --format html` (`render::weave_html`) is the one product exception. This
//! file exists purely as a conformance oracle. The parser is scored against
//! the spec's expected output here, not reused as that renderer.

use dankg::md::{Align, Block, Document, Inline, List};

pub fn render(doc: &Document) -> String {
    let mut out = String::new();
    blocks(&doc.blocks, false, &mut out);
    out
}

fn blocks(items: &[Block], tight: bool, out: &mut String) {
    for b in items {
        block(b, tight, out);
    }
}

fn block(b: &Block, tight: bool, out: &mut String) {
    match b {
        Block::Heading { level, inlines, .. } => {
            out.push_str(&format!("<h{level}>"));
            inlines_to(inlines, out);
            out.push_str(&format!("</h{level}>\n"));
        }
        Block::Paragraph { inlines, .. } => {
            // Paragraphs in a tight list item lose their wrapper.
            if tight {
                inlines_to(inlines, out);
            } else {
                out.push_str("<p>");
                inlines_to(inlines, out);
                out.push_str("</p>\n");
            }
        }
        Block::Code { info, text, .. } => {
            match &info.lang {
                Some(lang) => {
                    out.push_str("<pre><code class=\"language-");
                    out.push_str(&escape_text(lang));
                    out.push_str("\">");
                }
                None => out.push_str("<pre><code>"),
            }
            out.push_str(&escape_text(text));
            out.push_str("</code></pre>\n");
        }
        Block::ThematicBreak { .. } => out.push_str("<hr />\n"),
        Block::List(l) => list(l, out),
        Block::Passthrough { text, .. } => {
            // Outside the subset. Emitted verbatim so the case scores as a
            // failure rather than silently appearing to pass.
            out.push_str(text);
            out.push('\n');
        }
        // GFM tables are not part of the vendored CommonMark spec.json, so
        // this arm is never scored against it. Rendered anyway, rather than
        // stubbed, so the oracle stays a real (if partial) HTML renderer.
        Block::Table { aligns, header, rows, .. } => table(aligns, header, rows, out),
    }
}

fn table(aligns: &[Align], header: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>], out: &mut String) {
    out.push_str("<table>\n<thead>\n<tr>\n");
    for (i, cell) in header.iter().enumerate() {
        out.push_str("<th");
        push_align(aligns.get(i), out);
        out.push('>');
        inlines_to(cell, out);
        out.push_str("</th>\n");
    }
    out.push_str("</tr>\n</thead>\n<tbody>\n");
    for row in rows {
        out.push_str("<tr>\n");
        for (i, cell) in row.iter().enumerate() {
            out.push_str("<td");
            push_align(aligns.get(i), out);
            out.push('>');
            inlines_to(cell, out);
            out.push_str("</td>\n");
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n");
}

fn push_align(align: Option<&Align>, out: &mut String) {
    let value = match align {
        Some(Align::Left) => "left",
        Some(Align::Center) => "center",
        Some(Align::Right) => "right",
        Some(Align::None) | None => return,
    };
    out.push_str(" align=\"");
    out.push_str(value);
    out.push('"');
}

fn list(l: &List, out: &mut String) {
    if l.ordered {
        if l.start == 1 {
            out.push_str("<ol>\n");
        } else {
            out.push_str(&format!("<ol start=\"{}\">\n", l.start));
        }
    } else {
        out.push_str("<ul>\n");
    }

    for item in &l.items {
        if l.tight {
            out.push_str("<li>");
            let mut inner = String::new();
            blocks(&item.blocks, true, &mut inner);
            let trimmed = inner.trim_end_matches('\n');
            out.push_str(trimmed);
            out.push_str("</li>\n");
        } else {
            out.push_str("<li>\n");
            blocks(&item.blocks, false, out);
            out.push_str("</li>\n");
        }
    }

    out.push_str(if l.ordered { "</ol>\n" } else { "</ul>\n" });
}

fn inlines_to(items: &[Inline], out: &mut String) {
    for i in items {
        match i {
            Inline::Text(t) => out.push_str(&escape_text(t)),
            Inline::Code(t) => {
                out.push_str("<code>");
                out.push_str(&escape_text(t));
                out.push_str("</code>");
            }
            Inline::Emph { inner: c, .. } => {
                out.push_str("<em>");
                inlines_to(c, out);
                out.push_str("</em>");
            }
            Inline::Strong { inner: c, .. } => {
                out.push_str("<strong>");
                inlines_to(c, out);
                out.push_str("</strong>");
            }
            Inline::Link { dest, title, text } => {
                out.push_str("<a href=\"");
                out.push_str(&escape_href(dest));
                out.push('"');
                if let Some(t) = title {
                    out.push_str(" title=\"");
                    out.push_str(&escape_text(t));
                    out.push('"');
                }
                out.push('>');
                inlines_to(text, out);
                out.push_str("</a>");
            }
            Inline::WikiLink { target, label } => {
                // Not a CommonMark construct; round-trip it as written.
                out.push_str("[[");
                out.push_str(&escape_text(target));
                if let Some(l) = label {
                    out.push('|');
                    out.push_str(&escape_text(l));
                }
                out.push_str("]]");
            }
            Inline::SoftBreak => out.push('\n'),
            Inline::HardBreak => out.push_str("<br />\n"),
        }
    }
}

fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Percent-encode unsafe bytes, then entity-escape, matching cmark's
/// `houdini_escape_href`.
fn escape_href(s: &str) -> String {
    const SAFE: &[u8] = b"-_.+!*'(),%#@?=;:/,+&$~";
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let ch = b as char;
        if ch.is_ascii_alphanumeric() || SAFE.contains(&b) {
            match b {
                b'&' => out.push_str("&amp;"),
                b'\'' => out.push_str("&#x27;"),
                _ => out.push(ch),
            }
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
