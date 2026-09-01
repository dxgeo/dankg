//! Block structure: headings, fenced code, lists, paragraphs, thematic breaks.
//!
//! Constructs outside the subset -- indented code, block quotes, HTML blocks --
//! are gathered into `Block::Passthrough` and kept verbatim. They are not
//! errors and do not warn; they simply carry no graph meaning.

use super::{Block, InfoString, List, ListItem, KNOWN_ATTRS};
use crate::diag::Diags;

/// A source line paired with its original 1-based line number, so that nesting
/// and dedenting never lose the true location.
#[derive(Debug, Clone)]
struct Line {
    text: String,
    num: u32,
}

pub fn parse(source: &str, first_line: u32, diags: &mut Diags) -> Vec<Block> {
    let lines: Vec<Line> = source
        .lines()
        .enumerate()
        .map(|(i, text)| Line { text: text.to_string(), num: first_line + i as u32 })
        .collect();
    parse_lines(&lines, diags)
}

fn parse_lines(lines: &[Line], diags: &mut Diags) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = &lines[i];
        if line.text.trim().is_empty() {
            i += 1;
            continue;
        }

        let indent = indent_of(&line.text);

        if indent >= 4 {
            let (block, next) = gather_passthrough(lines, i, |l| indent_of(&l.text) >= 4);
            blocks.push(block);
            i = next;
        } else if is_thematic_break(&line.text) {
            blocks.push(Block::ThematicBreak { line: line.num });
            i += 1;
        } else if let Some((level, text)) = atx_heading(&line.text) {
            blocks.push(Block::Heading {
                level,
                inlines: super::inline::parse(&text),
                line: line.num,
            });
            i += 1;
        } else if let Some(fence) = fence_open(&line.text) {
            let (block, next) = gather_fence(lines, i, fence, diags);
            blocks.push(block);
            i = next;
        } else if line.text.trim_start().starts_with('>') {
            let (block, next) =
                gather_passthrough(lines, i, |l| !l.text.trim().is_empty());
            blocks.push(block);
            i = next;
        } else if starts_html_block(&line.text) {
            let (block, next) =
                gather_passthrough(lines, i, |l| !l.text.trim().is_empty());
            blocks.push(block);
            i = next;
        } else if let Some(marker) = list_marker(&line.text) {
            let (block, next) = gather_list(lines, i, marker, diags);
            blocks.push(block);
            i = next;
        } else {
            let (block, next) = gather_paragraph(lines, i);
            blocks.push(block);
            i = next;
        }
    }

    blocks
}

fn gather_paragraph(lines: &[Line], start: usize) -> (Block, usize) {
    let mut text = String::new();
    let mut i = start;

    while i < lines.len() {
        let line = &lines[i];
        if line.text.trim().is_empty() {
            break;
        }
        if i > start && interrupts_paragraph(&line.text) {
            break;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(line.text.trim_start());
        i += 1;
    }

    let block = Block::Paragraph {
        inlines: super::inline::parse(text.trim_end()),
        line: lines[start].num,
    };
    (block, i)
}

fn interrupts_paragraph(text: &str) -> bool {
    if indent_of(text) >= 4 {
        return false;
    }
    is_thematic_break(text)
        || atx_heading(text).is_some()
        || fence_open(text).is_some()
        || text.trim_start().starts_with('>')
        // Only a list that starts at 1 may interrupt a paragraph, which keeps
        // "the year 1986. It was" from becoming a list.
        || list_marker(text).is_some_and(|m| !m.ordered || m.start == 1)
}

fn gather_passthrough(
    lines: &[Line],
    start: usize,
    keep: impl Fn(&Line) -> bool,
) -> (Block, usize) {
    let mut text = String::new();
    let mut i = start;
    while i < lines.len() && keep(&lines[i]) {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&lines[i].text);
        i += 1;
    }
    (Block::Passthrough { text, line: lines[start].num }, i)
}

/// An opening fence line. Only `ch` survives into the AST; `len` and `indent`
/// govern gathering and are then normalized away.
#[derive(Debug, Clone, Copy)]
struct FenceOpen {
    ch: char,
    len: usize,
    indent: usize,
}

fn gather_fence(lines: &[Line], start: usize, fence: FenceOpen, diags: &mut Diags) -> (Block, usize) {
    let open = &lines[start];
    let raw_info = open.text.trim_start()[fence.len..].trim().to_string();
    let info = parse_info(&raw_info, open.num, diags);

    let mut text = String::new();
    let mut i = start + 1;
    let mut closed = false;

    while i < lines.len() {
        if let Some(close) = fence_open(&lines[i].text) {
            if close.ch == fence.ch && close.len >= fence.len && lines[i].text.trim_start()[close.len..].trim().is_empty() {
                closed = true;
                i += 1;
                break;
            }
        }
        // The opening fence's indentation is removed from each content line.
        let stripped = strip_indent(&lines[i].text, fence.indent);
        text.push_str(stripped);
        text.push('\n');
        i += 1;
    }

    if !closed {
        diags.warn(open.num, "unclosed code fence, treated as running to end of file");
    }

    // `i` sits one past the last line consumed either way: the closing
    // fence's own line when `closed`, or the last content line otherwise.
    let end_line = lines[i - 1].num;
    (Block::Code { info, text, fence: fence.ch, line: open.num, end_line }, i)
}

/// Parse a fence info string: first word is the language, the rest is DanKG
/// `key=value` metadata. Unknown keys warn rather than being silently dropped.
fn parse_info(raw: &str, line: u32, diags: &mut Diags) -> InfoString {
    let mut info = InfoString::default();
    let mut words = raw.split_whitespace();

    if let Some(lang) = words.next() {
        if lang.contains('=') {
            // No language, straight into attributes.
            handle_attr(&mut info, lang, line, diags);
        } else {
            info.lang = Some(lang.to_string());
        }
    }
    for word in words {
        handle_attr(&mut info, word, line, diags);
    }
    info
}

fn handle_attr(info: &mut InfoString, word: &str, line: u32, diags: &mut Diags) {
    let Some((key, value)) = word.split_once('=') else {
        diags.warn(line, format!("ignoring `{word}` in info string: expected key=value"));
        info.unknown.push(word.to_string());
        return;
    };
    if !KNOWN_ATTRS.contains(&key) {
        diags.warn(line, format!("unknown info string attribute `{key}`, ignored"));
        info.unknown.push(word.to_string());
        return;
    }
    if info.attrs.iter().any(|(k, _)| k == key) {
        diags.warn(line, format!("duplicate info string attribute `{key}`, last value wins"));
        info.attrs.retain(|(k, _)| k != key);
    }
    info.attrs.push((key.to_string(), value.to_string()));
}

#[derive(Debug, Clone, Copy)]
struct Marker {
    ordered: bool,
    start: u64,
    /// Bullet char, or the delimiter for ordered lists. A change of character
    /// starts a new list.
    ch: char,
    /// Column at which the item's content begins; continuation lines must be
    /// indented at least this far.
    content: usize,
    /// Byte offset of the content on the marker line itself. Distinct from
    /// `content` because tabs advance the column by more than one byte.
    content_byte: usize,
}

fn list_marker(text: &str) -> Option<Marker> {
    let (indent, indent_byte) = indent_info(text);
    if indent >= 4 {
        return None;
    }
    let rest = &text[indent_byte..];
    let bytes = rest.as_bytes();

    // Bullet list.
    if matches!(bytes.first(), Some(b'-') | Some(b'+') | Some(b'*')) {
        let after = &rest[1..];
        if after.is_empty() || after.starts_with(' ') || after.starts_with('\t') {
            let spaces = spaces_after_marker(after);
            return Some(Marker {
                ordered: false,
                start: 1,
                ch: rest.as_bytes()[0] as char,
                content: indent + 1 + spaces,
                content_byte: indent_byte + 1 + spaces,
            });
        }
        return None;
    }

    // Ordered list: up to nine digits, then `.` or `)`.
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    let delim = *bytes.get(digits)?;
    if delim != b'.' && delim != b')' {
        return None;
    }
    let after = &rest[digits + 1..];
    if !after.is_empty() && !after.starts_with(' ') && !after.starts_with('\t') {
        return None;
    }
    let spaces = spaces_after_marker(after);
    Some(Marker {
        ordered: true,
        start: rest[..digits].parse().unwrap_or(1),
        ch: delim as char,
        content: indent + digits + 1 + spaces,
        content_byte: indent_byte + digits + 1 + spaces,
    })
}

/// Spaces between the marker and the content. More than four means the content
/// is an indented code block, so only one space counts.
fn spaces_after_marker(after: &str) -> usize {
    let n = after.chars().take_while(|c| *c == ' ').count();
    if n == 0 || n > 4 { 1 } else { n }
}

fn gather_list(lines: &[Line], start: usize, first: Marker, diags: &mut Diags) -> (Block, usize) {
    let mut items: Vec<ListItem> = Vec::new();
    let mut loose = false;
    let mut i = start;
    let mut trailing_blank = false;

    while i < lines.len() {
        if lines[i].text.trim().is_empty() {
            trailing_blank = true;
            i += 1;
            continue;
        }

        let Some(marker) = list_marker(&lines[i].text) else { break };
        if marker.ordered != first.ordered || marker.ch != first.ch {
            break;
        }
        // A blank line before a sibling item makes the whole list loose.
        if trailing_blank && !items.is_empty() {
            loose = true;
        }
        trailing_blank = false;

        let mut item: Vec<Line> = Vec::new();
        // The marker is consumed here. Using strip_indent would leave it in
        // place, and parse_lines would rediscover the same list forever.
        let first_text = lines[i].text.get(marker.content_byte..).unwrap_or("");
        item.push(Line { text: first_text.to_string(), num: lines[i].num });
        i += 1;

        // Continuation lines: anything indented to the item's content column,
        // plus blank lines that are followed by more of the same item.
        let mut pending_blanks = 0usize;
        while i < lines.len() {
            let line = &lines[i];
            if line.text.trim().is_empty() {
                pending_blanks += 1;
                i += 1;
                continue;
            }
            if indent_of(&line.text) < marker.content {
                break;
            }
            if pending_blanks > 0 {
                // A blank line before the item has any content is just space
                // after the marker, not a paragraph break, so it does not make
                // the list loose. `-\n\n  foo` is one tight item.
                if item.iter().any(|l| !l.text.trim().is_empty()) {
                    loose = true;
                }
                for _ in 0..pending_blanks {
                    item.push(Line { text: String::new(), num: line.num });
                }
                pending_blanks = 0;
            }
            item.push(Line {
                text: strip_indent(&line.text, marker.content).to_string(),
                num: line.num,
            });
            i += 1;
        }

        if pending_blanks > 0 {
            trailing_blank = true;
        }

        items.push(ListItem { blocks: parse_lines(&item, diags) });
    }

    let block = Block::List(List {
        ordered: first.ordered,
        start: first.start,
        tight: !loose,
        marker: first.ch,
        items,
        line: lines[start].num,
    });
    (block, i)
}

fn atx_heading(text: &str) -> Option<(u8, String)> {
    let (indent, indent_byte) = indent_info(text);
    if indent >= 4 {
        return None;
    }
    let rest = &text[indent_byte..];
    let hashes = rest.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let after = &rest[hashes..];
    if !after.is_empty() && !after.starts_with(' ') && !after.starts_with('\t') {
        return None;
    }
    // A trailing run of hashes is a closing sequence, not content.
    let mut content = after.trim();
    if let Some(stripped) = content.strip_suffix(|c| c == '#') {
        let trimmed = stripped.trim_end_matches('#');
        if trimmed.is_empty() || trimmed.ends_with(' ') {
            content = trimmed.trim_end();
        }
    }
    Some((hashes as u8, content.to_string()))
}

fn fence_open(text: &str) -> Option<FenceOpen> {
    let (indent, indent_byte) = indent_info(text);
    if indent >= 4 {
        return None;
    }
    let rest = &text[indent_byte..];
    let ch = rest.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = rest.chars().take_while(|c| *c == ch).count();
    if len < 3 {
        return None;
    }
    // A backtick fence's info string may not itself contain backticks.
    if ch == '`' && rest[len..].contains('`') {
        return None;
    }
    Some(FenceOpen { ch, len, indent })
}

fn is_thematic_break(text: &str) -> bool {
    let (indent, indent_byte) = indent_info(text);
    if indent >= 4 {
        return false;
    }
    let rest = text[indent_byte..].trim_end();
    let Some(ch) = rest.chars().next() else { return false };
    if ch != '-' && ch != '_' && ch != '*' {
        return false;
    }
    let mut count = 0;
    for c in rest.chars() {
        if c == ch {
            count += 1;
        } else if c != ' ' && c != '\t' {
            return false;
        }
    }
    count >= 3
}

fn starts_html_block(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('<') && trimmed.len() > 1
}

fn indent_of(text: &str) -> usize {
    indent_info(text).0
}

/// Leading whitespace measured both in columns (tabs advance to the next
/// multiple of four) and in bytes. The two differ whenever tabs are involved,
/// and conflating them corrupts every slice taken against them.
fn indent_info(text: &str) -> (usize, usize) {
    let mut cols = 0;
    for (bytes, c) in text.char_indices() {
        match c {
            ' ' => cols += 1,
            '\t' => cols += 4 - (cols % 4),
            _ => return (cols, bytes),
        }
    }
    (cols, text.len())
}

/// Remove up to `width` columns of leading whitespace.
fn strip_indent(text: &str, width: usize) -> &str {
    let mut col = 0;
    for (i, c) in text.char_indices() {
        if col >= width {
            return &text[i..];
        }
        match c {
            ' ' => col += 1,
            '\t' => col += 4 - (col % 4),
            _ => return &text[i..],
        }
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md::Inline;

    fn blocks(src: &str) -> (Vec<Block>, Diags) {
        let mut d = Diags::new("t.md");
        let b = parse(src, 1, &mut d);
        (b, d)
    }

    #[test]
    fn atx_headings_with_levels_and_lines() {
        let (b, _) = blocks("# One\n\n### Three\n");
        assert!(matches!(b[0], Block::Heading { level: 1, line: 1, .. }));
        assert!(matches!(b[1], Block::Heading { level: 3, line: 3, .. }));
    }

    #[test]
    fn closing_hashes_are_not_content() {
        let (b, _) = blocks("## Title ##\n");
        let Block::Heading { inlines, .. } = &b[0] else { panic!() };
        assert_eq!(Inline::plain(inlines), "Title");
    }

    #[test]
    fn seven_hashes_is_a_paragraph() {
        let (b, _) = blocks("####### nope\n");
        assert!(matches!(b[0], Block::Paragraph { .. }));
    }

    #[test]
    fn fenced_code_with_info_attrs() {
        let (b, d) = blocks("```python name=index deps=setup\nprint(1)\n```\n");
        let Block::Code { info, text, line, end_line, .. } = &b[0] else { panic!() };
        assert_eq!(info.lang.as_deref(), Some("python"));
        assert_eq!(info.name(), Some("index"));
        assert_eq!(info.deps(), vec!["setup"]);
        assert_eq!(text, "print(1)\n");
        assert_eq!(*line, 1);
        assert_eq!(*end_line, 3, "the closing fence is line 3");
        assert!(d.is_empty());
    }

    #[test]
    fn unknown_info_attr_warns() {
        let (_, d) = blocks("```python bogus=1\n```\n");
        assert!(d.items()[0].message.contains("unknown info string attribute"));
    }

    #[test]
    fn non_kv_info_word_warns() {
        let (_, d) = blocks("```python stray\n```\n");
        assert!(d.items()[0].message.contains("expected key=value"));
    }

    #[test]
    fn unclosed_fence_warns() {
        let (b, d) = blocks("```sh\necho hi\n");
        assert!(d.items()[0].message.contains("unclosed code fence"));
        let Block::Code { text, end_line, .. } = &b[0] else { panic!() };
        assert_eq!(text, "echo hi\n");
        assert_eq!(*end_line, 2, "runs to the last line of the file, unclosed");
    }

    #[test]
    fn fence_does_not_swallow_headings() {
        let (b, _) = blocks("```\ncode\n```\n# After\n");
        assert!(matches!(b[1], Block::Heading { level: 1, line: 4, .. }));
    }

    #[test]
    fn thematic_break_versus_bullet() {
        let (b, _) = blocks("---\n");
        assert!(matches!(b[0], Block::ThematicBreak { .. }));
        let (b, _) = blocks("- item\n");
        assert!(matches!(b[0], Block::List(_)));
    }

    #[test]
    fn nested_list_recurses() {
        let (b, _) = blocks("- a\n  - b\n");
        let Block::List(outer) = &b[0] else { panic!() };
        assert_eq!(outer.items.len(), 1);
        assert!(matches!(outer.items[0].blocks[1], Block::List(_)));
    }

    #[test]
    fn ordered_list_start_preserved() {
        let (b, _) = blocks("3. c\n4. d\n");
        let Block::List(l) = &b[0] else { panic!() };
        assert!(l.ordered);
        assert_eq!(l.start, 3);
        assert_eq!(l.items.len(), 2);
    }

    #[test]
    fn changing_bullet_char_starts_new_list() {
        let (b, _) = blocks("- a\n* b\n");
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn blank_line_between_items_makes_list_loose() {
        let (b, _) = blocks("- a\n\n- b\n");
        let Block::List(l) = &b[0] else { panic!() };
        assert!(!l.tight);
    }

    #[test]
    fn indented_and_quoted_blocks_pass_through() {
        let (b, d) = blocks("    code\n");
        assert!(matches!(b[0], Block::Passthrough { .. }));
        let (b, _) = blocks("> quoted\n");
        assert!(matches!(b[0], Block::Passthrough { .. }));
        assert!(d.is_empty(), "passthrough is not a warning");
    }

    #[test]
    fn heading_interrupts_paragraph() {
        let (b, _) = blocks("text\n# Head\n");
        assert!(matches!(b[0], Block::Paragraph { .. }));
        assert!(matches!(b[1], Block::Heading { .. }));
    }

    #[test]
    fn ordered_list_not_starting_at_one_cannot_interrupt() {
        let (b, _) = blocks("the year\n2. was\n");
        assert_eq!(b.len(), 1);
        assert!(matches!(b[0], Block::Paragraph { .. }));
    }
}
