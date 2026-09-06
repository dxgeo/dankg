# TUI draw

Tree rows and panel rows become a character grid here. This is a pure
function, like [`tui::input::decode`](input.md): no I/O, no terminal,
fully unit-testable without one. Unlike this module's earlier Sugiyama-
layout incarnation, nothing here knows about a `Graph`, a `Node`, or a
`Layout` at all. `tui::app` is what turns a selected node into a title,
a badge, a `▸`/`▾` marker; this module only ever turns already-resolved
plain values into characters. The tree pane and the cross-reference
panel are windowed independently -- each can scroll on its own schedule
\-- then joined side by side with one divider column between them.

```rust name=module_doc path=tui/draw.rs
//! Tree rows and panel rows become a character grid here. Pure function,
//! like [`crate::tui::input::decode`]: no I/O, no terminal, fully
//! unit-testable without one.
//!
//! Unlike this module's earlier Sugiyama-layout incarnation, nothing here
//! knows about a graph or a layout at all: `tui::app` turns a selected
//! node into a title, a badge, a marker; this module only ever turns
//! already-resolved plain values into characters. The tree pane and the
//! cross-reference panel are windowed independently, then joined side by
//! side with one divider column between them.

pub type Grid = Vec<Vec<char>>;

/// A drawn pane, or a joined frame: the glyphs, plus two parallel
/// "which cells carry this attribute" grids, kept separate from `grid`
/// rather than a grid of `(char, flags)` cells so `Grid` alone still
/// round-trips through [`render_lines`] and every glyph-content test
/// unchanged.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawing {
    pub grid: Grid,
    /// Same dimensions as `grid`. `true` marks the *focused* pane's
    /// current row: reverse video, "the one active thing on screen."
    /// At most one row across a whole frame.
    pub current: Vec<Vec<bool>>,
    /// Same dimensions as `grid`. `true` marks the *other* pane's own
    /// last-remembered row: underlined, so switching focus back and
    /// forth never loses track of where it was, without inverting or
    /// otherwise altering the row's own text the way `current` does.
    pub secondary: Vec<Vec<bool>>,
}

/// Which attribute [`mark_row`] applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowStyle {
    Current,
    Secondary,
}

/// `grid`'s rows joined into strings, ready to write to a terminal one
/// line at a time. Carries no attributes. See [`render_ansi`] for that.
pub fn render_lines(grid: &Grid) -> Vec<String> {
    grid.iter().map(|row| row.iter().collect()).collect()
}
```

`mark_row` is the whole of how `app.rs` says "this is the row the reader
is on": it never picks colors itself, only flags cells for
\[`render_ansi`\] to wrap later. Out-of-range rows or columns are silently
ignored, the same tolerance the old renderer's `put` had -- a scroll
offset can legitimately leave a pane's own current row off whatever is
actually on screen this frame.

```rust name=mark_row path=tui/draw.rs
pub fn mark_row(drawing: &mut Drawing, row: usize, col_start: usize, width: usize, style: RowStyle) {
    let target = match style {
        RowStyle::Current => &mut drawing.current,
        RowStyle::Secondary => &mut drawing.secondary,
    };
    let Some(row_flags) = target.get_mut(row) else { return };
    let end = (col_start + width).min(row_flags.len());
    if col_start >= end {
        return;
    }
    for flag in &mut row_flags[col_start..end] {
        *flag = true;
    }
}
```

`render_ansi` deliberately uses the terminal's own reverse-video and
underline SGR attributes, rather than hardcoded colors. Both are
relative to whatever foreground and background the terminal already
has, so they read correctly on a dark theme or a light one without
this module ever needing to know which. A fixed color could not do
that. Underline over "faint" (the earlier treatment) is also what
keeps `secondary` from changing the row's own text at all -- weight,
color, everything about the glyphs stays exactly as drawn, with only a
line added beneath them, so a reader scanning the unfocused pane for
its last position is never left wondering whether a dimmer title means
something about the node itself.

```rust name=render_ansi path=tui/draw.rs
/// [`Drawing::current`] wrapped in reverse video (`\x1b[7m`...`\x1b[27m`)
/// and [`Drawing::secondary`] wrapped in underline (`\x1b[4m`...`\x1b[24m`),
/// each tracked as its own run across a row so either attribute can
/// start or end independently of the other.
pub fn render_ansi(drawing: &Drawing) -> Vec<String> {
    drawing
        .grid
        .iter()
        .zip(&drawing.current)
        .zip(&drawing.secondary)
        .map(|((row, current_row), secondary_row)| {
            let mut out = String::new();
            let mut current = false;
            let mut underlined = false;
            for ((&ch, &cur), &sec) in row.iter().zip(current_row).zip(secondary_row) {
                if cur && !current {
                    out.push_str("\x1b[7m");
                    current = true;
                } else if !cur && current {
                    out.push_str("\x1b[27m");
                    current = false;
                }
                if sec && !underlined {
                    out.push_str("\x1b[4m");
                    underlined = true;
                } else if !sec && underlined {
                    out.push_str("\x1b[24m");
                    underlined = false;
                }
                out.push(ch);
            }
            if current {
                out.push_str("\x1b[27m");
            }
            if underlined {
                out.push_str("\x1b[24m");
            }
            out
        })
        .collect()
}
```

```rust name=window_and_scroll path=tui/draw.rs
/// The `rows` x `cols` window of `drawing` starting at `(row, col)`.
/// This is what actually reaches the terminal when the full pane is
/// bigger than it is. Past-the-end rows or columns are simply absent,
/// the same as a terminal that has run out of room.
pub fn window(drawing: &Drawing, row: usize, col: usize, rows: usize, cols: usize) -> Drawing {
    let clip_ch = |line: &[char]| -> Vec<char> { line.iter().skip(col).take(cols).copied().collect() };
    let clip_flag = |line: &[bool]| -> Vec<bool> { line.iter().skip(col).take(cols).copied().collect() };
    Drawing {
        grid: drawing.grid.iter().skip(row).take(rows).map(|l| clip_ch(l)).collect(),
        current: drawing.current.iter().skip(row).take(rows).map(|l| clip_flag(l)).collect(),
        secondary: drawing.secondary.iter().skip(row).take(rows).map(|l| clip_flag(l)).collect(),
    }
}

/// The minimal adjustment to `scroll` so that `range` sits entirely
/// inside a `len`-cell window starting at `scroll`. This is "scroll to
/// reveal," not "centre on the selection," so the reader's sense of
/// where things are does not jump on every keypress that stays inside
/// the window already. The tree pane and the panel each call this with
/// their own `scroll`/`range`, entirely independently of one another.
pub fn scroll_to_show(scroll: usize, range: std::ops::Range<usize>, len: usize) -> usize {
    if len == 0 {
        return scroll;
    }
    if range.start < scroll {
        range.start
    } else if range.end > scroll + len {
        range.end - len
    } else {
        scroll
    }
}
```

Formatting a row's plain text is kept entirely separate from drawing it:
`tree_line`/`panel_line` never touch a `Drawing`, so they are trivial to
unit-test in isolation, the same reasoning `input::decode` being split
from `input::read_key` already follows.

```rust name=formatting path=tui/draw.rs
/// Truncates `text` to at most `cols` *characters*, replacing the last
/// with `…` when it did not fit whole, rather than a hard cut with no
/// sign it happened. A no-op when `text` already fits; an empty string
/// when `cols` is zero, rather than panicking on an out-of-range
/// replacement.
pub fn clip_with_ellipsis(text: &str, cols: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= cols {
        return text.to_string();
    }
    if cols == 0 {
        return String::new();
    }
    let mut out: String = chars[..cols - 1].iter().collect();
    out.push('…');
    out
}

/// One tree row's plain text: two spaces of indent per `depth`, then a
/// `▾`/`▸` marker for `Some(expanded)`, or two blank columns for `None`
/// (a childless leaf -- no marker at all, but titles still line up in a
/// column with everything else), then `title`, then `badge` after two
/// spaces when it is not empty. Clipped to `cols`.
pub fn tree_line(depth: u32, marker: Option<bool>, title: &str, badge: &str, cols: usize) -> String {
    let indent = "  ".repeat(depth as usize);
    let mark = match marker {
        Some(true) => "▾ ",
        Some(false) => "▸ ",
        None => "  ",
    };
    let mut line = format!("{indent}{mark}{title}");
    if !badge.is_empty() {
        line.push_str("  ");
        line.push_str(badge);
    }
    clip_with_ellipsis(&line, cols)
}

/// One panel row's plain text: `prefix` (`"→ "`, `"← "`, or `""` for a
/// `produces:`/`reads:` line that already carries its own label)
/// followed by `text`, clipped to `cols`.
pub fn panel_line(prefix: &str, text: &str, cols: usize) -> String {
    clip_with_ellipsis(&format!("{prefix}{text}"), cols)
}
```

`panel_width` caps at half the terminal, not just its own minimum, so a
narrow terminal still leaves the tree -- the primary navigation surface
\-- most of the room, rather than the panel alone claiming a fixed width
regardless of how little space there is to share.

```rust name=composition path=tui/draw.rs
pub const PANEL_MIN_COLS: usize = 28;

/// The right pane's width in columns, given the terminal's total.
pub fn panel_width(total_cols: usize) -> usize {
    PANEL_MIN_COLS.min(total_cols / 2)
}

/// `lines` turned into a rectangular, unscrolled `Drawing`: each row
/// padded or clipped to exactly `cols` wide, nothing marked yet. Ready
/// to hand straight to [`window`], the same way the old Sugiyama canvas
/// used to be built once and then windowed every frame.
pub fn pane_grid(lines: &[String], cols: usize) -> Drawing {
    let grid: Grid = lines
        .iter()
        .map(|line| {
            let mut row: Vec<char> = line.chars().take(cols).collect();
            row.resize(cols, ' ');
            row
        })
        .collect();
    let flags = vec![vec![false; cols]; grid.len()];
    Drawing { grid, current: flags.clone(), secondary: flags }
}

/// Horizontally joins two already-[`window`]ed panes, both already
/// exactly `rows` tall, into one frame with a single `│` divider column
/// between them. A pane shorter than `rows` -- nothing left to scroll
/// into -- is padded with blank rows of its own width, so the divider
/// still runs the full frame height instead of stopping wherever that
/// pane's real content ran out first.
pub fn compose(tree: &Drawing, panel: &Drawing, rows: usize, tree_cols: usize, panel_cols: usize) -> Drawing {
    let blank = |cols: usize| (vec![' '; cols], vec![false; cols], vec![false; cols]);
    let mut grid = Vec::with_capacity(rows);
    let mut current = Vec::with_capacity(rows);
    let mut secondary = Vec::with_capacity(rows);
    for i in 0..rows {
        let (t_row, t_cur, t_sec) = tree
            .grid
            .get(i)
            .map(|g| (g.clone(), tree.current[i].clone(), tree.secondary[i].clone()))
            .unwrap_or_else(|| blank(tree_cols));
        let (p_row, p_cur, p_sec) = panel
            .grid
            .get(i)
            .map(|g| (g.clone(), panel.current[i].clone(), panel.secondary[i].clone()))
            .unwrap_or_else(|| blank(panel_cols));

        let mut row = t_row;
        row.push('│');
        row.extend(p_row);
        grid.push(row);

        let mut cur = t_cur;
        cur.push(false);
        cur.extend(p_cur);
        current.push(cur);

        let mut sec = t_sec;
        sec.push(false);
        sec.extend(p_sec);
        secondary.push(sec);
    }
    Drawing { grid, current, secondary }
}
```

## Tests

```rust name=tests path=tui/draw.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_clips_to_the_given_rows_and_columns() {
        let drawing = Drawing {
            grid: vec![vec!['a', 'b', 'c'], vec!['d', 'e', 'f'], vec!['g', 'h', 'i']],
            current: vec![vec![false; 3]; 3],
            secondary: vec![vec![false; 3]; 3],
        };
        let w = window(&drawing, 1, 1, 2, 2);
        assert_eq!(render_lines(&w.grid), vec!["ef".to_string(), "hi".to_string()]);
    }

    #[test]
    fn window_past_the_grids_edge_yields_fewer_rows_and_columns_not_padding() {
        let drawing = Drawing { grid: vec![vec!['a', 'b']], current: vec![vec![false; 2]], secondary: vec![vec![false; 2]] };
        let w = window(&drawing, 0, 0, 5, 5);
        assert_eq!(w.grid, drawing.grid, "asking for more room than exists just returns what exists");
    }

    #[test]
    fn scroll_to_show_only_moves_when_the_range_would_otherwise_be_out_of_view() {
        assert_eq!(scroll_to_show(10, 12..15, 20), 10, "already fully visible: no change");
        assert_eq!(scroll_to_show(10, 5..8, 20), 5, "above the window: scroll up to it");
        assert_eq!(scroll_to_show(0, 30..33, 20), 13, "below the window: scroll down to it");
    }

    #[test]
    fn the_current_row_renders_in_reverse_video_and_the_other_panes_row_renders_underlined() {
        let mut drawing = pane_grid(&["one".to_string(), "two".to_string()], 3);
        mark_row(&mut drawing, 0, 0, 3, RowStyle::Current);
        mark_row(&mut drawing, 1, 0, 3, RowStyle::Secondary);
        let joined = render_ansi(&drawing).join("\n");
        assert!(joined.contains("\x1b[7mone\x1b[27m"), "{joined:?}");
        assert!(joined.contains("\x1b[4mtwo\x1b[24m"), "{joined:?}");
    }

    #[test]
    fn mark_row_out_of_range_is_a_no_op() {
        let mut drawing = pane_grid(&["one".to_string()], 3);
        mark_row(&mut drawing, 5, 0, 3, RowStyle::Current); // no such row
        mark_row(&mut drawing, 0, 10, 3, RowStyle::Current); // no such column
        assert!(!drawing.current[0].iter().any(|&f| f));
    }

    #[test]
    fn tree_line_indents_by_depth_and_shows_the_expand_marker() {
        assert_eq!(tree_line(0, Some(true), "One", "", 40), "▾ One");
        assert_eq!(tree_line(1, Some(false), "Two", "", 40), "  ▸ Two");
    }

    #[test]
    fn tree_line_uses_blank_columns_for_a_childless_leaf() {
        assert_eq!(tree_line(0, None, "Leaf", "", 40), "  Leaf");
    }

    #[test]
    fn tree_line_appends_the_badge() {
        assert_eq!(tree_line(0, None, "One", "→1 ←2", 40), "  One  →1 ←2");
    }

    #[test]
    fn tree_line_omits_the_badge_separator_when_there_is_no_badge() {
        assert_eq!(tree_line(0, None, "One", "", 40), "  One");
    }

    #[test]
    fn tree_line_clips_an_overlong_title() {
        let line = tree_line(0, None, "A very long title indeed", "", 10);
        assert_eq!(line.chars().count(), 10);
        assert!(line.ends_with('…'), "{line:?}");
    }

    #[test]
    fn panel_line_prefixes_an_arrow() {
        assert_eq!(panel_line("→ ", "Some Title", 40), "→ Some Title");
        assert_eq!(panel_line("", "produces: orders", 40), "produces: orders");
    }

    #[test]
    fn clip_with_ellipsis_leaves_a_short_line_untouched() {
        assert_eq!(clip_with_ellipsis("hi", 10), "hi");
    }

    #[test]
    fn clip_with_ellipsis_truncates_and_marks_with_an_ellipsis() {
        let clipped = clip_with_ellipsis("hello world", 5);
        assert_eq!(clipped, "hell…");
        assert_eq!(clipped.chars().count(), 5);
    }

    #[test]
    fn clip_with_ellipsis_on_a_zero_width_column_returns_empty() {
        assert_eq!(clip_with_ellipsis("hello", 0), "");
    }

    #[test]
    fn pane_grid_pads_every_row_to_the_same_width() {
        let grid = pane_grid(&["a".to_string(), "bb".to_string()], 4);
        assert_eq!(render_lines(&grid.grid), vec!["a   ".to_string(), "bb  ".to_string()]);
    }

    #[test]
    fn pane_grid_clips_a_row_wider_than_cols() {
        let grid = pane_grid(&["abcdef".to_string()], 3);
        assert_eq!(render_lines(&grid.grid), vec!["abc".to_string()]);
    }

    #[test]
    fn panel_width_never_exceeds_half_the_terminal() {
        assert_eq!(panel_width(20), 10);
    }

    #[test]
    fn panel_width_uses_the_minimum_on_a_wide_terminal() {
        assert_eq!(panel_width(200), PANEL_MIN_COLS);
    }

    #[test]
    fn compose_joins_two_panes_with_a_divider_column_between_them() {
        let tree = pane_grid(&["ab".to_string()], 2);
        let panel = pane_grid(&["cd".to_string()], 2);
        let frame = compose(&tree, &panel, 1, 2, 2);
        assert_eq!(render_lines(&frame.grid), vec!["ab│cd".to_string()]);
    }

    #[test]
    fn compose_pads_the_shorter_pane_with_blank_rows_to_match() {
        let tree = pane_grid(&["a".to_string(), "b".to_string()], 1);
        let panel = pane_grid(&["c".to_string()], 1);
        let frame = compose(&tree, &panel, 2, 1, 1);
        assert_eq!(render_lines(&frame.grid), vec!["a│c".to_string(), "b│ ".to_string()]);
    }
}
```
