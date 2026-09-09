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

/// A drawn pane, or a joined frame: the glyphs, plus three parallel
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
    /// Same dimensions as `grid`. `true` marks a cell inside a standing
    /// search's (`app.last_search`) matched text -- just the matched
    /// characters, not the whole row. Reverse video, the same as
    /// `current`: [`render_ansi`] ORs the two together into one run
    /// rather than giving `matches` a channel of its own, so a match
    /// that is also the current selection stays exactly as prominent as
    /// `current` alone, not competing with it for attention. Any number
    /// of cells across a frame can carry this, not just one row's worth
    /// -- every hit stays marked wherever the reader scrolls, until
    /// `esc` ends the search.
    pub matches: Vec<Vec<bool>>,
}

/// Which attribute [`mark_row`] applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowStyle {
    Current,
    Secondary,
    Match,
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
        RowStyle::Match => &mut drawing.matches,
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
this module ever needing to know which -- a fixed color could not do
that. Underline over "faint" (the earlier treatment for `secondary`)
is also what keeps `secondary` from changing the row's own text at
all -- weight, color, everything about the glyphs stays exactly as
drawn, with only a line added beneath them.

`matches` shares `current`'s reverse-video run rather than getting a
third SGR attribute of its own: a cell is reversed when *either* is
`true`. Bold was tried first and dropped -- weight is a weak, easily
missed signal for "this is a hit," and it is not what a reader expects
here anyway. Reverse video is the terminal convention `less` and vim's
own `hlsearch` already use for exactly this. Sharing the channel
`current` already has, rather than adding a fourth, also sidesteps the
one real collision a separate attribute would create: after `n`/`N`,
the row a match lands on usually *is* the current selection, and two
different attributes stacked on the same cells would visually compete
for the reader's attention on the row that most deserves it. Folding
them into one run means that row simply reads as reverse video, same
as `current` alone -- correct, since it is both at once. `secondary`'s
underline stays fully distinct from either, so a matched row on the
unfocused pane still shows both: an underlined row with a reverse-video
patch marking the hit inside it.

```rust name=render_ansi path=tui/draw.rs
/// [`Drawing::current`] and [`Drawing::matches`] are ORed together
/// into one reverse-video run (`\x1b[7m`...`\x1b[27m`) -- see the
/// prose above for why they share a channel rather than each getting
/// their own. [`Drawing::secondary`] is wrapped in underline
/// (`\x1b[4m`...`\x1b[24m`) independently, so it can start or end
/// without disturbing whichever reverse-video run is already open.
pub fn render_ansi(drawing: &Drawing) -> Vec<String> {
    drawing
        .grid
        .iter()
        .zip(&drawing.current)
        .zip(&drawing.secondary)
        .zip(&drawing.matches)
        .map(|(((row, current_row), secondary_row), match_row)| {
            let mut out = String::new();
            let mut reversed = false;
            let mut underlined = false;
            for (((&ch, &cur), &sec), &mat) in row.iter().zip(current_row).zip(secondary_row).zip(match_row) {
                let rev = cur || mat;
                if rev && !reversed {
                    out.push_str("\x1b[7m");
                    reversed = true;
                } else if !rev && reversed {
                    out.push_str("\x1b[27m");
                    reversed = false;
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
            if reversed {
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
        matches: drawing.matches.iter().skip(row).take(rows).map(|l| clip_flag(l)).collect(),
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

/// The column [`tree_line`] starts `title` at, for a row at `depth`:
/// two columns of indent per level, plus the marker's own two columns
/// -- `▾ `/`▸ `/`  ` are all exactly two characters wide, whichever one
/// a row gets. Exposed so a caller marking up something *inside* the
/// title text, rather than the row as a whole, can find where it
/// actually starts without re-deriving `tree_line`'s own layout by
/// hand -- `app::render`'s search-match highlight (*Jump and default
/// depth*, `architecture.md`) is the one caller today.
pub fn tree_line_title_col(depth: u32) -> usize {
    2 * depth as usize + 2
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
    Drawing { grid, current: flags.clone(), secondary: flags.clone(), matches: flags }
}

/// Horizontally joins two already-[`window`]ed panes, both already
/// exactly `rows` tall, into one frame with a single `│` divider column
/// between them. A pane shorter than `rows` -- nothing left to scroll
/// into -- is padded with blank rows of its own width, so the divider
/// still runs the full frame height instead of stopping wherever that
/// pane's real content ran out first.
pub fn compose(tree: &Drawing, panel: &Drawing, rows: usize, tree_cols: usize, panel_cols: usize) -> Drawing {
    let blank = |cols: usize| (vec![' '; cols], vec![false; cols], vec![false; cols], vec![false; cols]);
    let mut grid = Vec::with_capacity(rows);
    let mut current = Vec::with_capacity(rows);
    let mut secondary = Vec::with_capacity(rows);
    let mut matches = Vec::with_capacity(rows);
    for i in 0..rows {
        let (t_row, t_cur, t_sec, t_mat) = tree
            .grid
            .get(i)
            .map(|g| (g.clone(), tree.current[i].clone(), tree.secondary[i].clone(), tree.matches[i].clone()))
            .unwrap_or_else(|| blank(tree_cols));
        let (p_row, p_cur, p_sec, p_mat) = panel
            .grid
            .get(i)
            .map(|g| (g.clone(), panel.current[i].clone(), panel.secondary[i].clone(), panel.matches[i].clone()))
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

        let mut mat = t_mat;
        mat.push(false);
        mat.extend(p_mat);
        matches.push(mat);
    }
    Drawing { grid, current, secondary, matches }
}
```

`box_grid` and `overlay` are how the filter menu and the help
reference each draw as a small box floating over the tree/panel,
rather than replacing the whole frame the way help once did.
`box_grid` never knows where it will land -- sizing itself from its
own longest line -- and `overlay` never knows what it is pasting --
just glyphs and where they go -- the same separation `pane_grid`/
`compose` already keep between building a pane and placing it.

```rust name=overlay path=tui/draw.rs
/// `lines` wrapped in a `┌─┐│└┘` box, sized to its own longest line
/// plus one column of padding on each side. Ready to hand straight to
/// [`overlay`].
pub fn box_grid(lines: &[String]) -> Drawing {
    let inner_width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let border = "─".repeat(inner_width + 2);
    let mut boxed = vec![format!("┌{border}┐")];
    for line in lines {
        let pad = inner_width - line.chars().count();
        boxed.push(format!("│ {line}{} │", " ".repeat(pad)));
    }
    boxed.push(format!("└{border}┘"));
    pane_grid(&boxed, inner_width + 4)
}

/// Pastes `content` on top of `base` at `(row, col)`, overwriting
/// whatever was already there, including any `current`/`secondary`/
/// `matches` attribute underneath it -- a stale reverse-video tree row
/// must never bleed through a box drawn on top of it. Out-of-range rows or
/// columns are silently clipped, the same tolerance [`mark_row`]
/// already has for a scroll offset that leaves a row off screen.
pub fn overlay(base: &Drawing, content: &Drawing, row: usize, col: usize) -> Drawing {
    let mut out = base.clone();
    for (r, content_row) in content.grid.iter().enumerate() {
        let Some(target) = out.grid.get_mut(row + r) else { break };
        let Some(cur) = out.current.get_mut(row + r) else { break };
        let Some(sec) = out.secondary.get_mut(row + r) else { break };
        let Some(mat) = out.matches.get_mut(row + r) else { break };
        for (c, &ch) in content_row.iter().enumerate() {
            let Some(cell) = target.get_mut(col + c) else { break };
            *cell = ch;
            // Copied from `content`'s own attributes, not just cleared
            // to `false`: a filter-menu box marks its own highlighted
            // row (`mark_row`, on `content` itself, before this call)
            // and that highlight has to survive the paste. A plain
            // help box never marks anything, so this still clears
            // whatever the base had underneath it either way.
            if let Some(f) = cur.get_mut(col + c) {
                *f = content.current[r][c];
            }
            if let Some(f) = sec.get_mut(col + c) {
                *f = content.secondary[r][c];
            }
            if let Some(f) = mat.get_mut(col + c) {
                *f = content.matches[r][c];
            }
        }
    }
    out
}

/// A box's own top-left corner, centered over a `rows` x `cols` frame.
/// Clamped to `0` rather than going negative on a box bigger than the
/// frame -- the box still draws, just clipped by `overlay`'s own
/// out-of-range tolerance, instead of panicking on an underflowed
/// `usize` subtraction.
pub fn centered(box_rows: usize, box_cols: usize, rows: usize, cols: usize) -> (usize, usize) {
    (rows.saturating_sub(box_rows) / 2, cols.saturating_sub(box_cols) / 2)
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
            matches: vec![vec![false; 3]; 3],
        };
        let w = window(&drawing, 1, 1, 2, 2);
        assert_eq!(render_lines(&w.grid), vec!["ef".to_string(), "hi".to_string()]);
    }

    #[test]
    fn window_past_the_grids_edge_yields_fewer_rows_and_columns_not_padding() {
        let drawing = Drawing {
            grid: vec![vec!['a', 'b']],
            current: vec![vec![false; 2]],
            secondary: vec![vec![false; 2]],
            matches: vec![vec![false; 2]],
        };
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
    fn a_search_match_renders_reverse_video() {
        let mut drawing = pane_grid(&["one".to_string()], 3);
        mark_row(&mut drawing, 0, 0, 3, RowStyle::Match);
        let joined = render_ansi(&drawing).join("\n");
        assert!(joined.contains("\x1b[7mone\x1b[27m"), "{joined:?}");
    }

    #[test]
    fn a_matched_current_row_renders_as_one_reverse_video_run_not_two() {
        // `current` and `matches` share a channel (see the prose
        // above `render_ansi`) -- overlapping cells must not toggle
        // `\x1b[7m` off and back on again between them.
        let mut drawing = pane_grid(&["one".to_string()], 3);
        mark_row(&mut drawing, 0, 0, 3, RowStyle::Current);
        mark_row(&mut drawing, 0, 0, 3, RowStyle::Match);
        let joined = render_ansi(&drawing).join("\n");
        assert_eq!(joined, "\x1b[7mone\x1b[27m", "{joined:?}");
    }

    #[test]
    fn only_the_matched_part_of_a_row_renders_in_reverse_video() {
        let mut drawing = pane_grid(&["one two".to_string()], 7);
        mark_row(&mut drawing, 0, 4, 3, RowStyle::Match); // "two" only
        let joined = render_ansi(&drawing).join("\n");
        assert_eq!(joined, "one \x1b[7mtwo\x1b[27m", "{joined:?}");
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
    fn tree_line_title_col_matches_where_tree_line_actually_starts_the_title() {
        for depth in 0..3 {
            let line = tree_line(depth, Some(true), "X", "", 40);
            assert_eq!(
                line.chars().nth(tree_line_title_col(depth)),
                Some('X'),
                "depth {depth}: {line:?}"
            );
        }
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

    #[test]
    fn compose_carries_a_search_match_from_either_pane_through_to_the_frame() {
        let mut tree = pane_grid(&["a".to_string()], 1);
        mark_row(&mut tree, 0, 0, 1, RowStyle::Match);
        let panel = pane_grid(&["c".to_string()], 1);
        let frame = compose(&tree, &panel, 1, 1, 1);
        assert_eq!(frame.matches[0], vec![true, false, false], "the divider and panel column are untouched");
    }

    #[test]
    fn box_grid_sizes_itself_to_its_own_longest_line() {
        let b = box_grid(&["hi".to_string(), "longer".to_string()]);
        assert_eq!(
            render_lines(&b.grid),
            vec!["┌────────┐".to_string(), "│ hi     │".to_string(), "│ longer │".to_string(), "└────────┘".to_string()]
        );
    }

    #[test]
    fn overlay_pastes_content_at_the_given_offset_leaving_the_rest_of_base_untouched() {
        let base = pane_grid(&["aaaa".to_string(), "aaaa".to_string(), "aaaa".to_string()], 4);
        let content = pane_grid(&["bb".to_string()], 2);
        let out = overlay(&base, &content, 1, 1);
        assert_eq!(render_lines(&out.grid), vec!["aaaa".to_string(), "abba".to_string(), "aaaa".to_string()]);
    }

    #[test]
    fn overlay_clears_attributes_underneath_it() {
        let mut base = pane_grid(&["aaaa".to_string()], 4);
        mark_row(&mut base, 0, 0, 4, RowStyle::Current);
        let content = pane_grid(&["bb".to_string()], 2);
        let out = overlay(&base, &content, 0, 1);
        assert_eq!(out.current[0], vec![true, false, false, true], "only the pasted-over cells lose the attribute");
    }

    #[test]
    fn overlay_carries_over_the_contents_own_highlight() {
        let base = pane_grid(&["aaaa".to_string()], 4);
        let mut content = pane_grid(&["bb".to_string()], 2);
        mark_row(&mut content, 0, 0, 2, RowStyle::Current);
        let out = overlay(&base, &content, 0, 1);
        assert_eq!(out.current[0], vec![false, true, true, false], "a menu box's own selected row survives the paste");
    }

    #[test]
    fn overlay_clears_a_search_match_underneath_it_and_carries_its_own() {
        let mut base = pane_grid(&["aaaa".to_string()], 4);
        mark_row(&mut base, 0, 0, 4, RowStyle::Match);
        let content = pane_grid(&["bb".to_string()], 2);
        let out = overlay(&base, &content, 0, 1);
        assert_eq!(out.matches[0], vec![true, false, false, true], "only the pasted-over cells lose the match highlight");
    }

    #[test]
    fn overlay_clips_rather_than_panics_when_content_runs_past_bases_edge() {
        let base = pane_grid(&["aa".to_string()], 2);
        let content = pane_grid(&["bbbb".to_string(), "cccc".to_string()], 4);
        let out = overlay(&base, &content, 0, 0); // wider and taller than base
        assert_eq!(render_lines(&out.grid), vec!["bb".to_string()]);
    }

    #[test]
    fn centered_places_a_box_in_the_middle_of_the_frame() {
        assert_eq!(centered(2, 4, 10, 20), (4, 8));
    }

    #[test]
    fn centered_clamps_to_zero_rather_than_underflowing_on_an_oversized_box() {
        assert_eq!(centered(20, 20, 10, 10), (0, 0));
    }
}
```
