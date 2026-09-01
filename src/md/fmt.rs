//! The formatter: AST back to markdown, in normal form.
//!
//! `dankg fmt` exists so that a knowledge base stays diffable and a graph never
//! changes because someone indented a list differently. That makes losslessness
//! the whole problem: this is a pure function of the AST, so anything the AST
//! does not record cannot be reproduced. Two rules follow.
//!
//! Constructs outside the subset are re-emitted byte for byte -- `Passthrough`
//! blocks and the frontmatter block both. The formatter never rewrites a
//! construct it does not model.
//!
//! Text is escaped on the way out, not merely copied. A `Text` node holds the
//! character the author meant, not the bytes they typed, so anything that would
//! be re-read as markup has to be escaped back. `escape` therefore mirrors the
//! parser's own decisions -- `can_open_close` is shared with `inline.rs` rather
//! than reimplemented, because an escaper that disagrees with the parser about
//! flanking silently mangles emphasis.

use super::{Block, Document, Inline, InfoString, List, KNOWN_ATTRS};
use crate::diag::Diags;

/// Render a document to its normal form.
///
/// The result always ends in exactly one newline, unless it is empty.
pub fn format(doc: &Document) -> String {
    let mut out = doc.frontmatter.raw.clone();
    out.push_str(&blocks(&doc.blocks, true));
    while out.ends_with('\n') {
        out.pop();
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Check that formatting changed the text of the document but not its meaning.
///
/// Round-tripping is the strongest test this parser has, so the formatter runs
/// it on every file it is about to rewrite rather than trusting the suite. A
/// failure here is a formatter or parser bug, and the caller must not write.
pub fn verify(original: &Document, formatted: &str) -> Result<(), String> {
    let mut scratch = Diags::new("<formatted>");
    let reparsed = Document::parse(formatted, &mut scratch);

    if without_lines(&reparsed) != without_lines(original) {
        return Err("formatting would change the document's meaning".to_string());
    }
    let again = format(&reparsed);
    if again != formatted {
        return Err("formatting is not idempotent on this input".to_string());
    }
    Ok(())
}

/// A copy with every source line number zeroed.
///
/// Line numbers are the one part of the AST that formatting is *expected* to
/// change, so they have to come out before two documents can be compared.
pub fn without_lines(doc: &Document) -> Document {
    Document {
        frontmatter: doc.frontmatter.clone(),
        blocks: doc.blocks.iter().map(strip_block).collect(),
    }
}

fn strip_block(b: &Block) -> Block {
    match b {
        Block::Heading { level, inlines, .. } => {
            Block::Heading { level: *level, inlines: inlines.clone(), line: 0 }
        }
        Block::Code { info, text, fence, .. } => {
            Block::Code { info: info.clone(), text: text.clone(), fence: *fence, line: 0, end_line: 0 }
        }
        Block::Paragraph { inlines, .. } => {
            Block::Paragraph { inlines: inlines.clone(), line: 0 }
        }
        Block::ThematicBreak { .. } => Block::ThematicBreak { line: 0 },
        Block::Passthrough { text, .. } => {
            Block::Passthrough { text: text.clone(), line: 0 }
        }
        Block::List(l) => Block::List(List {
            ordered: l.ordered,
            start: l.start,
            tight: l.tight,
            marker: l.marker,
            items: l
                .items
                .iter()
                .map(|it| super::ListItem { blocks: it.blocks.iter().map(strip_block).collect() })
                .collect(),
            line: 0,
        }),
    }
}

/// Blocks in sequence. Every block's text ends in exactly one newline, so the
/// only decision here is whether a blank line goes between them -- which is
/// also the difference between a tight and a loose list item.
fn blocks(list: &[Block], blank_between: bool) -> String {
    let mut out = String::new();
    for (i, b) in list.iter().enumerate() {
        if i > 0 && blank_between {
            out.push('\n');
        }
        out.push_str(&block(b));
    }
    out
}

fn block(b: &Block) -> String {
    match b {
        Block::Heading { level, inlines, .. } => heading(*level, inlines),
        Block::Paragraph { inlines, .. } => format!("{}\n", inlines_text(inlines)),
        Block::Code { info, text, fence, .. } => code(info, text, *fence),
        Block::List(l) => list(l),
        // `---` would be read back as frontmatter at the top of a file and as a
        // bullet inside a list item; `***` is a thematic break everywhere.
        Block::ThematicBreak { .. } => "***\n".to_string(),
        Block::Passthrough { text, .. } => format!("{text}\n"),
    }
}

fn heading(level: u8, inlines: &[Inline]) -> String {
    let hashes = "#".repeat(level as usize);
    let text = inlines_text(inlines);
    if text.is_empty() {
        return format!("{hashes}\n");
    }
    // A trailing run of hashes is a closing sequence, so the run has to be
    // escaped or the heading loses its last word.
    let mut text = text;
    if text.ends_with('#') {
        let run = text.len() - text.trim_end_matches('#').len();
        text.insert(text.len() - run, '\\');
    }
    format!("{hashes} {text}\n")
}

fn code(info: &InfoString, text: &str, fence: char) -> String {
    // Three characters, unless the body contains a longer run of its own and
    // would close the block early.
    let len = longest_leading_run(text, fence).saturating_add(1).max(3);
    let bar: String = std::iter::repeat(fence).take(len).collect();

    let mut out = format!("{bar}{}\n", info_text(info));
    out.push_str(text);
    if !text.is_empty() && !text.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&bar);
    out.push('\n');
    out
}

/// Canonical info string: language, then known attributes in the order
/// `KNOWN_ATTRS` declares them, then anything the parser did not recognise, in
/// the order it was written. Unknown words are ignored everywhere else, but
/// deleting them would make `fmt` lossy.
fn info_text(info: &InfoString) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(lang) = &info.lang {
        parts.push(lang.clone());
    }
    for key in KNOWN_ATTRS {
        if let Some(value) = info.get(key) {
            parts.push(format!("{key}={value}"));
        }
    }
    parts.extend(info.unknown.iter().cloned());
    if parts.is_empty() {
        String::new()
    } else {
        parts.join(" ")
    }
}

fn longest_leading_run(text: &str, ch: char) -> usize {
    text.lines()
        .map(|l| l.trim_start().chars().take_while(|c| *c == ch).count())
        .max()
        .unwrap_or(0)
}

fn list(l: &List) -> String {
    let mut out = String::new();
    let mut number = l.start;

    for (i, item) in l.items.iter().enumerate() {
        if i > 0 && !l.tight {
            out.push('\n');
        }
        let marker = if l.ordered {
            format!("{number}{}", l.marker)
        } else {
            l.marker.to_string()
        };
        let width = marker.chars().count() + 1;
        out.push_str(&item_text(&marker, &blocks(&item.blocks, !l.tight), width));
        number += 1;
    }
    out
}

/// One item: the marker on the first line, then every continuation line
/// indented to the item's content column. Blank lines stay blank rather than
/// becoming a line of spaces.
fn item_text(marker: &str, body: &str, width: usize) -> String {
    if body.is_empty() {
        return format!("{marker}\n");
    }
    let pad = " ".repeat(width);
    let mut out = String::new();
    for (i, line) in body.lines().enumerate() {
        if i == 0 {
            out.push_str(marker);
            if !line.is_empty() {
                out.push(' ');
                out.push_str(line);
            }
        } else if !line.is_empty() {
            out.push_str(&pad);
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn inlines_text(inlines: &[Inline]) -> String {
    let mut w = Writer { out: String::new(), prev: '\n', line_start: true };
    w.run(inlines, ' ');
    w.out
}

struct Writer {
    out: String,
    /// The previous *semantic* character, not the previous output byte. Escapes
    /// must not change what the flanking rules see.
    prev: char,
    line_start: bool,
}

impl Writer {
    /// `after` is the character that will follow the last inline -- a closing
    /// emphasis delimiter, say. Flanking is decided by neighbours, so the last
    /// child has to know what its parent is about to write.
    fn run(&mut self, inlines: &[Inline], after: char) {
        for (i, item) in inlines.iter().enumerate() {
            let next = inlines.get(i + 1).map(lead_char).unwrap_or(after);
            match item {
                Inline::Text(t) => self.text(t, next),
                Inline::Code(t) => self.code(t),
                Inline::Emph { delim, inner } => self.wrap(*delim, 1, inner),
                Inline::Strong { delim, inner } => self.wrap(*delim, 2, inner),
                Inline::Link { dest, title, text } => self.link(dest, title.as_deref(), text),
                Inline::WikiLink { target, label } => self.wikilink(target, label.as_deref()),
                Inline::SoftBreak => self.newline(""),
                // The backslash form, not two trailing spaces: trailing
                // whitespace is exactly what normalizing is supposed to remove.
                Inline::HardBreak => self.newline("\\"),
            }
        }
    }

    fn push(&mut self, s: &str) {
        self.out.push_str(s);
        if let Some(c) = s.chars().last() {
            self.prev = c;
            self.line_start = false;
        }
    }

    fn newline(&mut self, before: &str) {
        self.out.push_str(before);
        self.out.push('\n');
        self.prev = '\n';
        self.line_start = true;
    }

    fn text(&mut self, t: &str, next: char) {
        let chars: Vec<char> = t.chars().collect();
        if chars.is_empty() {
            return;
        }
        let mut start = 0;

        if self.line_start {
            // At the start of a line these would open a block, not a word.
            if matches!(chars[0], '#' | '>' | '-' | '+' | '*' | '~' | '<') {
                self.out.push('\\');
                self.out.push(chars[0]);
                start = 1;
            } else if let Some(n) = ordered_marker(&chars) {
                self.out.extend(&chars[..n]);
                self.out.push('\\');
                self.out.push(chars[n]);
                start = n + 1;
            }
        }

        for i in start..chars.len() {
            let c = chars[i];
            let before = if i == 0 { self.prev } else { chars[i - 1] };
            let after = chars.get(i + 1).copied().unwrap_or(next);
            match c {
                // `]` matters even though a bare one is inert: a real link
                // writes an unescaped `[`, and a stray `]` in its text would
                // close it early.
                '\\' | '`' | '[' | ']' => self.out.push('\\'),
                '*' | '_' => {
                    let (open, close) = super::inline::can_open_close(c, before, after);
                    if open || close {
                        self.out.push('\\');
                    }
                }
                _ => {}
            }
            self.out.push(c);
        }

        self.prev = chars[chars.len() - 1];
        self.line_start = false;
    }

    /// A code span needs a backtick run longer than any inside it, and padding
    /// whenever the content would otherwise be eaten by the parser's own
    /// one-space strip.
    fn code(&mut self, content: &str) {
        let len = longest_run(content, '`') + 1;
        let bar: String = std::iter::repeat('`').take(len).collect();

        let has_text = content.chars().any(|c| c != ' ');
        let pad = content.starts_with('`')
            || content.ends_with('`')
            || (content.starts_with(' ') && content.ends_with(' ') && has_text);

        let mut s = bar.clone();
        if pad {
            s.push(' ');
        }
        s.push_str(content);
        if pad {
            s.push(' ');
        }
        s.push_str(&bar);
        self.push(&s);
    }

    fn wrap(&mut self, delim: char, count: usize, inner: &[Inline]) {
        let bar: String = std::iter::repeat(delim).take(count).collect();
        self.out.push_str(&bar);
        self.prev = delim;
        self.line_start = false;
        self.run(inner, delim);
        self.out.push_str(&bar);
        self.prev = delim;
    }

    fn link(&mut self, dest: &str, title: Option<&str>, text: &[Inline]) {
        self.out.push('[');
        self.prev = '[';
        self.line_start = false;
        self.run(text, ']');
        self.out.push(']');
        self.out.push('(');
        self.out.push_str(&destination(dest));
        if let Some(t) = title {
            self.out.push_str(" \"");
            for c in t.chars() {
                if c == '"' || c == '\\' {
                    self.out.push('\\');
                }
                self.out.push(c);
            }
            self.out.push('"');
        }
        self.out.push(')');
        self.prev = ')';
    }

    fn wikilink(&mut self, target: &str, label: Option<&str>) {
        let body = match label {
            Some(l) => format!("[[{target}|{l}]]"),
            None => format!("[[{target}]]"),
        };
        self.push(&body);
    }
}

/// The first character a rendered inline contributes, used as the right-hand
/// neighbour when deciding whether a trailing `*` or `_` needs escaping.
fn lead_char(i: &Inline) -> char {
    match i {
        Inline::Text(t) => t.chars().next().unwrap_or(' '),
        Inline::Code(_) => '`',
        Inline::Emph { delim, .. } | Inline::Strong { delim, .. } => *delim,
        Inline::Link { .. } | Inline::WikiLink { .. } => '[',
        Inline::SoftBreak | Inline::HardBreak => ' ',
    }
}

/// A leading run of digits followed by `.` or `)`: an ordered list marker if it
/// is left alone. Returns the index of the delimiter.
fn ordered_marker(chars: &[char]) -> Option<usize> {
    let digits = chars.iter().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    match chars.get(digits) {
        Some('.') | Some(')') => Some(digits),
        _ => None,
    }
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

/// A link destination. Whitespace forces the `<...>` form, which is the only
/// way to write it; otherwise parentheses and backslashes are escaped so that
/// the destination cannot end early.
fn destination(dest: &str) -> String {
    if dest.is_empty() || dest.chars().any(|c| c.is_ascii_whitespace()) {
        let mut s = String::from("<");
        for c in dest.chars() {
            if matches!(c, '<' | '>' | '\\') {
                s.push('\\');
            }
            s.push(c);
        }
        s.push('>');
        return s;
    }
    let mut s = String::new();
    for c in dest.chars() {
        if matches!(c, '\\' | '(' | ')' | '<' | '>') {
            s.push('\\');
        }
        s.push(c);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(source: &str) -> String {
        let mut diags = Diags::new("t.md");
        format(&Document::parse(source, &mut diags))
    }

    /// The normal form is a fixed point of itself, which is the property the
    /// integration suite checks over the whole corpus.
    fn stable(source: &str) -> String {
        let once = f(source);
        assert_eq!(f(&once), once, "not idempotent");
        once
    }

    #[test]
    fn normalizes_heading_style() {
        assert_eq!(stable("##   Title   ##\n"), "## Title\n");
    }

    #[test]
    fn a_heading_ending_in_hashes_keeps_them() {
        assert_eq!(stable("# C\\#\n"), "# C\\#\n");
    }

    #[test]
    fn bullet_and_delimiter_are_the_author_s() {
        assert_eq!(stable("+  a\n+  b\n"), "+ a\n+ b\n");
        assert_eq!(stable("1)  a\n2)  b\n"), "1) a\n2) b\n");
    }

    #[test]
    fn ordered_items_are_renumbered_from_the_start() {
        assert_eq!(stable("3. c\n3. d\n3. e\n"), "3. c\n4. d\n5. e\n");
    }

    #[test]
    fn nested_lists_indent_to_the_content_column() {
        assert_eq!(stable("-   a\n    - b\n"), "- a\n  - b\n");
        assert_eq!(stable("10. a\n    - b\n"), "10. a\n    - b\n");
    }

    #[test]
    fn loose_lists_keep_their_blank_lines_and_tight_ones_do_not() {
        assert_eq!(stable("- a\n\n- b\n"), "- a\n\n- b\n");
        assert_eq!(stable("- a\n- b\n"), "- a\n- b\n");
    }

    #[test]
    fn fence_length_is_the_shortest_that_clears_the_body() {
        assert_eq!(stable("`````sh\necho\n`````\n"), "```sh\necho\n```\n");
        assert_eq!(stable("````\n```\n````\n"), "````\n```\n````\n");
        assert_eq!(stable("~~~\na\n~~~\n"), "~~~\na\n~~~\n");
    }

    #[test]
    fn info_string_attributes_are_ordered_and_unknown_words_survive() {
        assert_eq!(
            stable("```python timeout=5 name=x bogus=1\n```\n"),
            "```python name=x timeout=5 bogus=1\n```\n"
        );
    }

    #[test]
    fn thematic_breaks_avoid_the_frontmatter_and_bullet_spellings() {
        // `---` at the top of a file would be read back as frontmatter.
        assert_eq!(stable("---\n"), "***\n");
        assert_eq!(stable("- ***\n"), "- ***\n");
    }

    #[test]
    fn frontmatter_is_re_emitted_verbatim() {
        let source = "---\ntitle:  \"A\"   # note\nodd:\n---\n#  H\n";
        assert_eq!(stable(source), "---\ntitle:  \"A\"   # note\nodd:\n---\n# H\n");
    }

    #[test]
    fn passthrough_blocks_are_re_emitted_verbatim() {
        assert_eq!(stable("> a\n> b\n"), "> a\n> b\n");
        assert_eq!(stable("<div>\n  x\n</div>\n"), "<div>\n  x\n</div>\n");
    }

    #[test]
    fn text_that_would_be_re_read_as_markup_is_escaped() {
        assert_eq!(stable("\\*not emph\\*\n"), "\\*not emph\\*\n");
        assert_eq!(stable("\\# not a heading\n"), "\\# not a heading\n");
        assert_eq!(stable("the year\n1986\\. it was\n"), "the year\n1986\\. it was\n");
        assert_eq!(stable("a \\[bracket\\]\n"), "a \\[bracket\\]\n");
    }

    #[test]
    fn intraword_underscores_are_left_alone() {
        assert_eq!(stable("snake_case_name\n"), "snake_case_name\n");
    }

    #[test]
    fn emphasis_keeps_the_delimiter_the_author_chose() {
        assert_eq!(stable("_a_ and *b* and __c__\n"), "_a_ and *b* and __c__\n");
    }

    #[test]
    fn a_hard_break_becomes_a_backslash_not_trailing_spaces() {
        assert_eq!(stable("a  \nb\n"), "a\\\nb\n");
    }

    #[test]
    fn link_destinations_that_need_the_angle_form_get_it() {
        assert_eq!(stable("[t](<a b.md> \"T\")\n"), "[t](<a b.md> \"T\")\n");
        assert_eq!(stable("[t](f.md#h)\n"), "[t](f.md#h)\n");
        assert_eq!(stable("[[a#b|c]]\n"), "[[a#b|c]]\n");
    }

    #[test]
    fn code_spans_get_a_run_longer_than_their_content() {
        assert_eq!(stable("`` `a` ``\n"), "`` `a` ``\n");
        assert_eq!(stable("`*x*`\n"), "`*x*`\n");
    }

    #[test]
    fn blocks_are_separated_by_exactly_one_blank_line() {
        assert_eq!(stable("# A\n\n\n\ntext\n\n\n"), "# A\n\ntext\n");
    }

    #[test]
    fn an_empty_document_stays_empty() {
        assert_eq!(f(""), "");
        assert_eq!(f("\n\n"), "");
    }
}
