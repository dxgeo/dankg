# TUI draw

`Layout` becomes a character grid here. This is a pure function, like
[`tui::input::decode`](input.md): no I/O, no terminal, fully
unit-testable without one. Node placement reuses the layout directly,
but not uniformly. A node's column comes from dividing its pixel `x` by
`CHAR_WIDTH`. This is sound because `x` and every box width were
themselves built out of `CHAR_WIDTH` in the first place
(`layout::label_width`, `layout::fit_label`). Dividing by it recovers a
column proportional to what the layout already decided. Rank, though,
maps to a *fixed* number of terminal rows, rather than the pixel
`NODE_HEIGHT`/`RANK_SEP` constants. There is no character-row constant
in `layout::mod`, for the same reason there is a `CHAR_WIDTH` and not a
`CHAR_HEIGHT`: the SVG and dot renderers measure in real pixels and
never needed one. Row spacing here is this module's own decision, not a
reuse.

`GAP_ROWS` is sized for the worst case a focused edge can draw, rather
than the minimum an edge needs. A single glyph reads as a dot, not a
direction, and a reciprocated edge needs both termini plus at least one
point between them to read as a trail rather than two unrelated marks.
Three rows guarantees that even between adjacent ranks, at the cost of
more vertical space between every rank than a non-reciprocated edge
alone would need. Edges always route through the same diagonal
interpolation between two ranks' rows. What changes under focus mode is
only the glyph, never the path, and never through the virtual bend
points the SVG/dot layout computed for a multi-rank span either, since
reproducing that exactly would mean duplicating `layout::mod`'s own
`y_of` rank-to-pixel formula here rather than sharing it. A long edge's
line is drawn straight through whatever rows and columns lie in
between, so it can cross an unrelated node's box the way a z-order
rather than a real router would. Boxes are drawn after edges, so the
box wins, but `put_border` leaves `·` where it crossed the box's wall,
rather than erasing the line without a trace. Self-loops are not drawn
at all yet.

```rust name=module_doc path=tui/draw.rs
//! `Layout` becomes a character grid here. Pure function, like
//! [`crate::tui::input::decode`]: no I/O, no terminal, fully
//! unit-testable without one.
//!
//! Node placement reuses the layout directly, but not uniformly. A
//! node's column comes from its pixel `x` divided by `CHAR_WIDTH`. This
//! is sound because `x` and every box width were built out of
//! `CHAR_WIDTH` in the first place (`label_width`, `fit_label`).
//! Dividing by it recovers a column proportional to what the layout
//! already decided. Rank, though, maps to a *fixed* number of terminal
//! rows, rather than the pixel `NODE_HEIGHT`/`RANK_SEP` constants. There
//! is no character-row constant in `layout/`, for the same reason there
//! is a `CHAR_WIDTH` and not a `CHAR_HEIGHT`: the SVG and dot renderers
//! measure in real pixels and never needed one. Row spacing here is
//! this module's own decision, not a reuse. `GAP_ROWS` is sized for the
//! worst case a focused edge can draw, rather than the minimum an edge
//! needs. A single glyph reads as a dot, not a direction, and a
//! reciprocated edge needs both termini plus at least one point between
//! them to read as a trail rather than two unrelated marks. Three rows
//! guarantees that even between adjacent ranks, at the cost of more
//! vertical space between every rank than a non-reciprocated edge alone
//! would need.
//!
//! Edges route through the same diagonal interpolation between two
//! ranks' rows either way. What changes under focus mode is only the
//! glyph, not the path, and not through the virtual bend points the
//! SVG/dot layout computed for a multi-rank span either, since
//! reproducing that exactly would mean re-deriving `y_of`'s rank-to-
//! pixel formula here, duplicating it out of `layout/mod.rs` rather
//! than sharing it. A long edge's line is drawn straight through
//! whatever rows and columns are in between, so it can cross paths
//! with an unrelated node's box the way a z-order rather than a real
//! router would do it. Boxes are drawn after edges, so the box wins,
//! but [`put_border`] leaves `·` where it crossed the box's wall,
//! rather than erasing the line without a trace. Self-loops are not
//! drawn at all yet.

use crate::graph::{Graph, NodeId, NodeKind};
use crate::layout::{fit_label, Layout, CHAR_WIDTH};

const BOX_ROWS: usize = 3;
const GAP_ROWS: usize = 3;
const MIN_BOX_COLS: i32 = 6;

pub type Grid = Vec<Vec<char>>;

/// A drawn view: the glyphs, and which cells are unfocused (see `draw`'s
/// `selected` parameter). Kept as two parallel grids rather than a grid of
/// `(char, bool)` cells so `Grid` alone still round-trips through
/// [`render_lines`] and every glyph-content test unchanged.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawing {
    pub grid: Grid,
    /// Same dimensions as `grid`. `true` marks a cell that belongs to a
    /// node or edge outside focus mode's selection, so [`render_ansi`]
    /// knows to render it faint.
    pub dim: Vec<Vec<bool>>,
}

/// `grid`'s rows joined into strings, ready to write to a terminal one line
/// at a time. Carries no dimming. See [`render_ansi`] for that.
pub fn render_lines(grid: &Grid) -> Vec<String> {
    grid.iter().map(|row| row.iter().collect()).collect()
}
```

`render_ansi` deliberately uses the terminal's own "faint" SGR
attribute, rather than a hardcoded gray. Faint is relative to whatever
foreground and background the terminal already has, so it reads as
lighter on a dark theme and darker on a light one, without this grid
ever needing to know which it is. A fixed color could not do that.

```rust name=render_ansi path=tui/draw.rs
/// [`Drawing::grid`] and [`Drawing::dim`] combined into ANSI-wrapped
/// lines: a run of dimmed cells is wrapped in the "faint" SGR attribute
/// (`\x1b[2m`...`\x1b[22m`). Faint rather than a specific color, because
/// it is relative to the terminal's own foreground/background. It reads
/// as lighter on a dark theme and darker on a light one, without this
/// grid needing to know which it is. A hardcoded gray could not do
/// that.
pub fn render_ansi(drawing: &Drawing) -> Vec<String> {
    drawing
        .grid
        .iter()
        .zip(&drawing.dim)
        .map(|(row, dim_row)| {
            let mut out = String::new();
            let mut dimming = false;
            for (&ch, &dim) in row.iter().zip(dim_row) {
                if dim && !dimming {
                    out.push_str("\x1b[2m");
                    dimming = true;
                } else if !dim && dimming {
                    out.push_str("\x1b[22m");
                    dimming = false;
                }
                out.push(ch);
            }
            if dimming {
                out.push_str("\x1b[22m");
            }
            out
        })
        .collect()
}
```

```rust name=extent_and_scroll path=tui/draw.rs
/// The full-grid row and column extent of one node's box. This is what
/// a viewport must contain, whole, for the reader to see the node they
/// have selected. `crate::tui::app` uses this to keep the scroll
/// position following the selection. Nothing in this module scrolls on
/// its own.
pub fn extent(node: &crate::layout::LaidNode) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
    let top = row_of(node.rank);
    let (left, width) = geometry(node);
    // `left` can be zero but, since `expand::expand_view` normalizes
    // every node's `x` before it reaches a `Layout`, never negative.
    // `max(0)` is only insurance against that invariant breaking
    // elsewhere later.
    let left = left.max(0) as usize;
    (top..top + BOX_ROWS, left..left + width as usize)
}

/// The minimal adjustment to `scroll` so that `range` sits entirely
/// inside a `len`-cell window starting at `scroll`. This is "scroll to
/// reveal," not "centre on the selection," so the reader's sense of
/// where things are on screen does not jump on every keypress that
/// stays inside the window already.
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

/// The `rows` x `cols` window of `drawing` starting at `(row, col)`.
/// This is what actually reaches the terminal when the full grid is
/// bigger than it is. Past-the-end rows or columns are simply absent,
/// the same as a terminal that has run out of room.
pub fn window(drawing: &Drawing, row: usize, col: usize, rows: usize, cols: usize) -> Drawing {
    let clip = |line: &[char]| -> Vec<char> { line.iter().skip(col).take(cols).copied().collect() };
    let clip_dim = |line: &[bool]| -> Vec<bool> { line.iter().skip(col).take(cols).copied().collect() };
    Drawing {
        grid: drawing.grid.iter().skip(row).take(rows).map(|l| clip(l)).collect(),
        dim: drawing.dim.iter().skip(row).take(rows).map(|l| clip_dim(l)).collect(),
    }
}
```

Pan mode's own status indicator is these arrows, applied *after*
windowing rather than before, so they sit at the true screen edges
regardless of where the viewport currently sits inside the full grid.
`app.rs` has no status line to say pan mode is active in words, so this
is the entire indicator. An edge only gets an arrow when there is more
grid beyond it to pan into. This doubles it as feedback about which
directions still have room left: panned all the way to the bottom, `↓`
simply stops appearing.

```rust name=overlay_pan_arrows path=tui/draw.rs
/// Stamps directional arrows onto the edges of an already-windowed
/// `visible` drawing. This is the sole visual indicator that pan mode
/// is on, since `app.rs` has no status line to say so in words. An
/// edge only gets an arrow when there is more grid beyond it to pan
/// into. This doubles the indicator as feedback about which directions
/// still have room: panned all the way to the bottom, `↓` simply stops
/// appearing. Applied after windowing, not before, so the arrows sit at
/// the actual screen edges regardless of where the viewport currently
/// is inside the full grid.
pub fn overlay_pan_arrows(visible: &mut Drawing, can_up: bool, can_down: bool, can_left: bool, can_right: bool) {
    let rows = visible.grid.len();
    if rows == 0 || visible.grid[0].is_empty() {
        return;
    }
    let cols = visible.grid[0].len();
    let mid_row = rows / 2;
    let mid_col = cols / 2;
    let mut mark = |row: usize, col: usize, ch: char| {
        visible.grid[row][col] = ch;
        visible.dim[row][col] = false;
    };
    if can_up {
        mark(0, mid_col, '↑');
    }
    if can_down {
        mark(rows - 1, mid_col, '↓');
    }
    if can_left {
        mark(mid_row, 0, '←');
    }
    if can_right {
        mark(mid_row, cols - 1, '→');
    }
}
```

`dimensions` exists as its own function, not just something `draw`
computes inline, specifically so `tui::app`'s own panning logic can
clamp against the grid's extent *before* drawing anything, rather than
discovering the edge by scrolling off it. `draw` itself calls this same
function for its own sizing, so the two can never quietly disagree
about how big the canvas is. Selecting a node puts the whole drawing
into focus mode: the selection, everything it directly connects to, and
the edges between them draw at full weight, and everything else dims.
Past a handful of crossing lines, telling one path from another by
glyph shape alone stops working. Narrowing to what the reader is
actually looking at is what keeps a large view legible, rather than
adding still more glyph variety on top of an already busy drawing.

```rust name=dimensions_and_draw path=tui/draw.rs
/// The full drawn grid's `(rows, cols)`, without drawing it. This is
/// what `crate::tui::app` clamps panning against, since panning has to
/// know the grid's extent up front, rather than discovering it by
/// scrolling off the end. Kept in step with `draw`'s own sizing by
/// being the thing `draw` calls, not a second computation of the same
/// numbers.
pub fn dimensions(layout: &Layout) -> (usize, usize) {
    if layout.nodes.is_empty() {
        return (0, 0);
    }
    let total_cols = layout
        .nodes
        .iter()
        .map(|n| {
            let (left, width) = geometry(n);
            left + width
        })
        .max()
        .unwrap_or(0)
        .max(1) as usize
        + 1;
    let total_rows = row_of(layout.ranks.saturating_sub(1)) + BOX_ROWS;
    (total_rows, total_cols)
}

pub fn draw(graph: &Graph, layout: &Layout, selected: Option<&NodeId>) -> Drawing {
    if layout.nodes.is_empty() {
        return Drawing::default();
    }

    let (total_rows, total_cols) = dimensions(layout);

    let mut canvas =
        Drawing { grid: vec![vec![' '; total_cols]; total_rows], dim: vec![vec![false; total_cols]; total_rows] };

    // The focused node set: the selection itself, plus the other end of
    // every edge touching it. `None` means focus mode is off, so nothing
    // in this set is ever consulted.
    let mut focused_nodes: Vec<NodeId> = Vec::new();
    if let Some(id) = selected {
        focused_nodes.push(id.clone());
        for edge in &layout.edges {
            if (edge.from == *id || edge.to == *id) && edge.from != edge.to {
                for end in [&edge.from, &edge.to] {
                    if !focused_nodes.contains(end) {
                        focused_nodes.push(end.clone());
                    }
                }
            }
        }
    }

    for edge in &layout.edges {
        if edge.from == edge.to || !layout.is_drawn(edge) {
            continue; // self-loops and the undrawn half of a reciprocated pair
        }
        let (Some(from), Some(to)) =
            (layout.nodes.iter().find(|n| n.id == edge.from), layout.nodes.iter().find(|n| n.id == edge.to))
        else {
            continue;
        };
        // No selection means no focus mode: everything draws at full
        // weight, matching how this looked before selection existed.
        let focused = selected.is_none_or(|id| edge.from == *id || edge.to == *id);
        draw_edge(&mut canvas, from, to, focused, edge.reciprocated);
    }

    for laid in &layout.nodes {
        let node = graph.node(&laid.id);
        let title = node.map(|n| n.title.as_str()).unwrap_or(&laid.id.slug);
        let resolved = node.is_none_or(|n| n.resolved);
        let kind = node.map_or(NodeKind::Heading, |n| n.kind);
        let is_selected = selected == Some(&laid.id);
        let dim = selected.is_some() && !focused_nodes.contains(&laid.id);
        draw_box(&mut canvas, laid, title, resolved, kind, is_selected, dim);
    }

    canvas
}

fn row_of(rank: u32) -> usize {
    rank as usize * (BOX_ROWS + GAP_ROWS)
}

/// A node's `(left column, width in columns)`, the one place this mapping is
/// computed so drawing the box and sizing the grid can never disagree.
fn geometry(node: &crate::layout::LaidNode) -> (i32, i32) {
    let center = (node.x + CHAR_WIDTH / 2) / CHAR_WIDTH;
    let width = ((node.width + CHAR_WIDTH / 2) / CHAR_WIDTH).max(MIN_BOX_COLS);
    (center - width / 2, width)
}
```

The glyph vocabulary itself is small and deliberate. Unfocused
connectors read as `-`/`·`, muted and non-directional, since the point
there is only "something connects here." A focused connector reads as
`o`, visible and traceable but not where the eye should stop. The
terminus nearest `to` reads as `*`. An earlier version used
`v`/`^`/`<`/`>` here, oriented by `reversed` so the glyph still pointed
at the real target on a cycle-broken edge running either way on the
page. A single neutral marker turned out easier to read correctly than
getting that orientation right by eye at a glance was worth. A
reciprocated edge marks both termini, since it represents both
directions at once rather than one favoured end.

```rust name=draw_edge path=tui/draw.rs
/// The glyphs:
///
/// - *unfocused*: `-` where the path jogs sideways, `·` where it runs
///   straight. Muted and non-directional, since the point is only
///   "something connects here."
/// - *focused, not the terminus*: `o`. The connector is visible and
///   traceable but not where the eye should stop.
/// - *focused, at the terminus nearest `to`*: `*`. Marks where the path
///   meets its target without claiming a direction. An earlier version
///   used `v`/`^`/`<`/`>` here, oriented via `reversed` so the glyph
///   still pointed at the real target on a cycle-broken edge running
///   either way on the page. A single neutral marker sidesteps needing
///   to get that orientation right by eye at a glance. That turned out
///   to be a harder call to read correctly than the arrows were worth.
/// - *reciprocated*: both termini get the marker, since the edge
///   represents both directions at once rather than one favoured end.
fn draw_edge(
    canvas: &mut Drawing,
    from: &crate::layout::LaidNode,
    to: &crate::layout::LaidNode,
    focused: bool,
    reciprocated: bool,
) {
    let to_is_top = to.rank < from.rank;
    let (top, bottom) = if from.rank <= to.rank { (from, to) } else { (to, from) };
    let start_row = row_of(top.rank) + BOX_ROWS;
    let end_row = row_of(bottom.rank);
    if start_row >= end_row {
        return; // same rank, or adjacent with no gap row between: nothing to draw
    }
    let (c0, w0) = geometry(top);
    let (c1, w1) = geometry(bottom);
    let center = |left: i32, width: i32| left + width / 2;
    let (col0, col1) = (center(c0, w0), center(c1, w1));

    let span = (end_row - start_row) as i32;
    // Index 0 sits just below `top`'s box. `span - 1` sits just above
    // `bottom`'s. Which one is nearest `to` depends on `to_is_top`.
    let near_to = if to_is_top { 0 } else { span - 1 };
    let near_from = span - 1 - near_to;

    let mut prev = col0;
    for (i, row) in (start_row..end_row).enumerate() {
        let i = i as i32;
        let col = col0 + (col1 - col0) * (i + 1) / (span + 1);
        let jogged = col != prev;

        let ch = if !focused {
            if jogged { '-' } else { '·' }
        } else if i == near_to || (reciprocated && i == near_from) {
            '*'
        } else {
            'o'
        };

        put(canvas, row, col, ch, !focused);
        prev = col;
    }
}
```

`put_border` exists because boxes are drawn strictly after edges, so a
box's own shape always wins visually over whatever line crosses it.
Without this function, a *focused* edge occluded by a node it does not
touch would simply vanish there with no trace at all. That is worse
than an honest gap in the line the reader is actively following. An
*unfocused* crossing gets no such trace: it was already meant to fade
into the background, and marking its crossing would just be more noise
about a path nobody is following right now. The marker itself is never
dimmed, even on an unfocused box's wall, since `dim` describes what a
glyph *represents*, not where it happens to sit. The marker is
standing in for a focused edge, not for the box underneath it.

```rust name=put_and_put_border path=tui/draw.rs
fn put(canvas: &mut Drawing, row: usize, col: i32, ch: char, dim: bool) {
    if col < 0 {
        return;
    }
    let col = col as usize;
    if row < canvas.grid.len() && col < canvas.grid[row].len() {
        canvas.grid[row][col] = ch;
        canvas.dim[row][col] = dim;
    }
}

/// Like [`put`], but for a box's border. If a *focused* edge already
/// drew a glyph at this cell (its path passes directly behind this wall
/// of the box), that crossing is marked with `·`, the same glyph a
/// dimmed straight run already uses, instead of being silently painted
/// over. Boxes are drawn after edges, so a box's own shape always wins
/// over whatever crosses it. Without this, a focused edge occluded by a
/// node it does not touch would vanish there with no trace at all. That
/// is worse than an honest gap in the line the reader is actively
/// tracing. An *unfocused* edge crossing the same wall gets no such
/// trace: it was already meant to fade into the background, and
/// marking its crossing would just be more noise about a path nobody
/// is following right now.
///
/// The marker itself is never dimmed, even when drawn onto an unfocused
/// box's wall. It is standing in for a focused edge, not for the box,
/// and `dim` is a property of what a glyph represents, not of where it
/// happens to sit.
fn put_border(canvas: &mut Drawing, row: usize, col: i32, glyph: char, dim: bool) {
    let crossing = col >= 0
        && canvas.grid.get(row).and_then(|r| r.get(col as usize)).is_some_and(|&existing| existing != ' ')
        && canvas.dim.get(row).and_then(|r| r.get(col as usize)).is_some_and(|&was_dim| !was_dim);
    let (ch, dim) = if crossing { ('·', false) } else { (glyph, dim) };
    put(canvas, row, col, ch, dim);
}
```

Border weight carries the same information `render::dot`/`render::html`
carry with color and fill. Unresolved nodes get dashed box-drawing
glyphs, the character-grid equivalent of "dashed and muted." A block
node (always resolved, never dashed) gets heavy lines instead, the same
underlying `NodeKind::Block` fact those two renderers tint. Selection
overrides both, taking the visually strongest border (double lines)
regardless of resolved state or kind, since it is what the reader is
about to act on. `dim` stays orthogonal to all three. It is
`render_ansi`'s job to apply, not a glyph choice made here, so an
unresolved node outside focus can be dashed *and* faint at once.

```rust name=draw_box path=tui/draw.rs
/// `selected`, when given, draws that node with a double-line border,
/// instead of its usual one. This is a purely character-level cursor,
/// since this grid carries no color yet. It also puts the view into
/// focus mode: the selection, every node it directly connects to, and
/// the edges between them draw at full weight. Everything else dims.
/// Past a handful of crossing lines, telling one path from another by
/// glyph shape alone stops working. Narrowing to what the reader is
/// actually looking at is what makes a large view legible, rather than
/// adding more glyph variety on top of an already busy drawing. See
/// [`draw_edge`] for the glyphs each state actually uses.
/// Unresolved nodes render with dashed box-drawing glyphs, the
/// character-grid equivalent of "dashed and muted" in the HTML and dot
/// renderers. A block node (always resolved, never dashed) gets heavy
/// lines instead, the character-grid equivalent of the tint
/// `dot.rs`/`html.rs` give it: still a plain box, just visibly a
/// different kind of thing. Selection overrides both. It is what the
/// reader is about to act on, so it takes the visually strongest
/// border regardless of resolved state or kind. `dim` is orthogonal to
/// all three. It is [`render_ansi`]'s job, not a glyph choice, so an
/// unresolved node outside focus can be dashed *and* faint at once.
fn draw_box(
    canvas: &mut Drawing,
    laid: &crate::layout::LaidNode,
    title: &str,
    resolved: bool,
    kind: NodeKind,
    selected: bool,
    dim: bool,
) {
    let (left, width) = geometry(laid);
    let top_row = row_of(laid.rank);
    let (h, v, corner) = if selected {
        ('═', '║', ['╔', '╗', '╚', '╝'])
    } else if !resolved {
        ('┄', '┆', ['┌', '┐', '└', '┘'])
    } else if kind == NodeKind::Block {
        ('━', '┃', ['┏', '┓', '┗', '┛'])
    } else {
        ('─', '│', ['┌', '┐', '└', '┘'])
    };

    put_border(canvas, top_row, left, corner[0], dim);
    put_border(canvas, top_row, left + width - 1, corner[1], dim);
    put_border(canvas, top_row + 2, left, corner[2], dim);
    put_border(canvas, top_row + 2, left + width - 1, corner[3], dim);
    for c in 1..(width - 1) {
        put_border(canvas, top_row, left + c, h, dim);
        put_border(canvas, top_row + 2, left + c, h, dim);
    }
    put_border(canvas, top_row + 1, left, v, dim);
    put_border(canvas, top_row + 1, left + width - 1, v, dim);

    let label = fit_label(title, laid.width);
    let inner = (width - 2).max(0) as usize;
    let pad = inner.saturating_sub(label.chars().count()) / 2;
    for (i, ch) in label.chars().enumerate() {
        put(canvas, top_row + 1, left + 1 + (pad + i) as i32, ch, dim);
    }
}
```

## Tests

```rust name=tests path=tui/draw.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::graph_of;
    use crate::layout::layout;

    const UNFOCUSED: [char; 2] = ['-', '·'];

    fn drawn(files: &[(&str, &str)]) -> Vec<String> {
        let graph = graph_of(files);
        render_lines(&draw(&graph, &layout(&graph), None).grid)
    }

    #[test]
    fn window_clips_to_the_given_rows_and_columns() {
        let drawing = Drawing {
            grid: vec![vec!['a', 'b', 'c'], vec!['d', 'e', 'f'], vec!['g', 'h', 'i']],
            dim: vec![vec![false; 3]; 3],
        };
        let w = window(&drawing, 1, 1, 2, 2);
        assert_eq!(render_lines(&w.grid), vec!["ef".to_string(), "hi".to_string()]);
    }

    #[test]
    fn window_past_the_grids_edge_yields_fewer_rows_and_columns_not_padding() {
        let drawing = Drawing { grid: vec![vec!['a', 'b']], dim: vec![vec![false; 2]] };
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
    fn extent_matches_where_draw_actually_puts_the_box() {
        let graph = graph_of(&[("a.md", "# One\n\n## Two\n")]);
        let laid = layout(&graph);
        let two = laid.nodes.iter().find(|n| n.id.slug == "two").unwrap();
        let (rows, cols) = extent(two);
        assert_eq!(rows, row_of(1)..row_of(1) + BOX_ROWS);

        let lines = render_lines(&draw(&graph, &laid, None).grid);
        assert!(lines[rows.start].chars().nth(cols.start) == Some('┌'), "the box's own corner sits at `extent`'s origin");
    }

    #[test]
    fn an_empty_graph_draws_nothing() {
        assert!(draw(&Graph::default(), &layout(&Graph::default()), None).grid.is_empty());
    }

    #[test]
    fn the_selected_node_gets_a_double_line_border() {
        let graph = graph_of(&[("a.md", "# One\n\n## Two\n")]);
        let laid = layout(&graph);
        let id = laid.nodes.iter().find(|n| n.id.slug == "two").unwrap().id.clone();
        let lines = render_lines(&draw(&graph, &laid, Some(&id)).grid);

        let rank0 = &lines[0..BOX_ROWS];
        assert!(rank0.iter().all(|l| !l.contains('╔') && !l.contains('║')), "One is not selected");
        let rank1 = &lines[row_of(1)..row_of(1) + BOX_ROWS];
        assert!(rank1.iter().any(|l| l.contains('╔') || l.contains('║')), "Two should be selected");
    }

    #[test]
    fn only_edges_touching_the_selection_draw_at_full_weight() {
        let graph = graph_of(&[("a.md", "# One\n\n## Two\n\n## Three\n")]);
        let laid = layout(&graph);
        let two = laid.nodes.iter().find(|n| n.id.slug == "two").unwrap().id.clone();
        let lines = render_lines(&draw(&graph, &laid, Some(&two)).grid);

        // One contains both Two and Three, so both connectors share the
        // rows between rank 0 and rank 1. Two and Three jog in opposite
        // directions away from One's column, so their glyphs land at
        // different columns throughout and do not collide.
        let gap = &lines[BOX_ROWS..row_of(1)];
        assert!(
            gap.iter().any(|l| l.contains('*')),
            "the edge to the selected node should show a terminus somewhere in the gap: {gap:?}"
        );
        assert!(
            gap.iter().any(|l| l.chars().any(|c| UNFOCUSED.contains(&c))),
            "the edge to the other sibling should be dimmed somewhere in the gap: {gap:?}"
        );
    }

    #[test]
    fn no_selection_means_no_dimming() {
        // Same shape as above, but nothing selected: both connectors should
        // draw at full weight, as they did before focus mode existed.
        let lines = drawn(&[("a.md", "# One\n\n## Two\n\n## Three\n")]);
        let gap = &lines[BOX_ROWS..row_of(1)];
        assert!(
            !gap.iter().any(|l| l.chars().any(|c| UNFOCUSED.contains(&c))),
            "nothing should be dimmed: {gap:?}"
        );
    }

    #[test]
    fn a_long_focused_edge_shows_o_in_between_and_a_terminus_marker_only_at_the_end() {
        // One links straight to Three, two ranks down, passing straight
        // through Two's box rows in between. Those rows then draw over
        // it, since boxes are drawn after edges. The row right below
        // One's own box also carries One's separate (unfocused)
        // containment edge to Two, colinear with it in this fixture, so
        // check the row just below Two's box instead. That is still an
        // in-between point of the long edge, but past where the two
        // edges could collide. The terminus, at row_of(2) - 1 (just
        // above Three), is unambiguous either way.
        let graph = graph_of(&[("a.md", "# One\n\nsee [three](#three)\n\n## Two\n\n### Three\n")]);
        let laid = layout(&graph);
        let three = laid.nodes.iter().find(|n| n.id.slug == "three").unwrap().id.clone();
        let lines = render_lines(&draw(&graph, &laid, Some(&three)).grid);

        let first = &lines[row_of(1) + BOX_ROWS];
        let last = &lines[row_of(2) - 1];
        assert!(first.contains('o'), "expected 'o' just below Two: {first:?}");
        assert!(last.contains('*'), "expected the terminus marker just above Three: {last:?}");
    }

    #[test]
    fn a_reciprocated_edge_at_minimum_spacing_still_shows_three_points() {
        // A two-node cycle breaks to adjacent ranks. This is the
        // tightest spacing GAP_ROWS ever has to work with, and exactly
        // the case it exists for. Without it, two termini with nothing
        // between them would read as separate marks, rather than a
        // single bidirectional trail.
        let graph = graph_of(&[("a.md", "# A\n\n[b](b.md#b)\n"), ("b.md", "# B\n\n[a](a.md#a)\n")]);
        let laid = layout(&graph);
        let a = laid.nodes.iter().find(|n| n.id.slug == "a").unwrap();
        let b = laid.nodes.iter().find(|n| n.id.slug == "b").unwrap();
        let (low, high) = (a.rank.min(b.rank), a.rank.max(b.rank));
        assert_eq!(high, low + 1, "a two-node cycle breaks to adjacent ranks");

        let lines = render_lines(&draw(&graph, &laid, Some(&a.id.clone())).grid);
        let gap = &lines[row_of(low) + BOX_ROWS..row_of(high)];
        assert_eq!(gap.len(), GAP_ROWS);
        assert_eq!(gap.iter().filter(|l| l.contains('*')).count(), 2, "a terminus at each end: {gap:?}");
        assert!(gap.iter().any(|l| l.contains('o')), "and at least one point between them: {gap:?}");
    }

    #[test]
    fn a_reversed_edges_terminus_still_sits_next_to_its_real_target() {
        // A -> B -> C -> A: one edge must be reversed to break the
        // cycle, and whichever it is, its `to` ends up at a lower rank
        // than its `from` (asserted in layout::tests too). That is
        // physically the higher node on screen. The terminus marker no
        // longer carries direction, but it still has to land next to
        // `to`, not `from`, regardless of which way the edge got drawn
        // on the page.
        let graph = graph_of(&[
            ("a.md", "# A\n\n[b](b.md#b)\n"),
            ("b.md", "# B\n\n[c](c.md#c)\n"),
            ("c.md", "# C\n\n[a](a.md#a)\n"),
        ]);
        let laid = layout(&graph);
        let reversed = laid.edges.iter().find(|e| e.reversed).expect("one back edge breaks a three-cycle");
        let to = laid.nodes.iter().find(|n| n.id == reversed.to).unwrap().clone();
        let from = laid.nodes.iter().find(|n| n.id == reversed.from).unwrap();
        assert!(to.rank < from.rank, "a reversed edge's `to` sits above its `from`");

        let lines = render_lines(&draw(&graph, &laid, Some(&to.id)).grid);
        let terminus = &lines[row_of(to.rank) + BOX_ROWS];
        assert!(terminus.contains('*'), "expected the terminus marker next to `to`'s box: {terminus:?}");
    }

    #[test]
    fn put_border_marks_a_focused_crossing_but_not_an_unfocused_one() {
        let mut canvas = Drawing { grid: vec![vec![' '; 5]], dim: vec![vec![false; 5]] };
        put(&mut canvas, 0, 1, 'o', false); // a focused edge drew through here
        put(&mut canvas, 0, 3, '-', true); // a dimmed edge drew through here

        put_border(&mut canvas, 0, 1, '─', false);
        assert_eq!(canvas.grid[0][1], '·', "a focused crossing should be marked, not erased");
        put_border(&mut canvas, 0, 3, '─', false);
        assert_eq!(canvas.grid[0][3], '─', "an unfocused crossing gets no trace, it fades as intended");

        put_border(&mut canvas, 0, 0, '─', false); // nothing was there
        assert_eq!(canvas.grid[0][0], '─', "an empty cell just gets the border glyph");
    }

    #[test]
    fn a_focused_crossing_marker_is_never_dimmed_even_on_an_unfocused_box() {
        let mut canvas = Drawing { grid: vec![vec![' '; 3]], dim: vec![vec![false; 3]] };
        put(&mut canvas, 0, 1, 'o', false); // a focused edge drew through here

        // The box itself is unfocused (dim=true), but the crossing it's
        // about to paint over belongs to a focused edge, not to this box.
        put_border(&mut canvas, 0, 1, '─', true);
        assert_eq!(canvas.grid[0][1], '·');
        assert!(!canvas.dim[0][1], "the marker represents a focused edge, so it must not be dimmed");
    }

    #[test]
    fn a_single_node_is_a_three_row_box_with_its_title_inside() {
        let lines = drawn(&[("a.md", "# One\n")]);
        assert_eq!(lines.len(), BOX_ROWS);
        assert!(lines[0].contains('┌') && lines[0].contains('┐'));
        assert!(lines[2].contains('└') && lines[2].contains('┘'));
        assert!(lines[1].contains("One"), "{:?}", lines[1]);
    }

    #[test]
    fn an_unresolved_node_gets_a_dashed_border() {
        let lines = drawn(&[("a.md", "[missing](b.md#nope)\n")]);
        let dashed = lines.iter().any(|l| l.contains('┄') || l.contains('┆'));
        assert!(dashed, "expected a dashed border for the placeholder node: {lines:?}");
    }

    #[test]
    fn a_block_node_gets_a_heavy_border() {
        let lines = drawn(&[("a.md", "# One\n\n```sh name=setup\n:\n```\n")]);
        let heavy = lines.iter().any(|l| l.contains('┏') || l.contains('┃') || l.contains('━'));
        assert!(heavy, "expected a heavy border for the block node: {lines:?}");
        assert!(lines.iter().any(|l| l.contains("setup")), "{lines:?}");
    }

    #[test]
    fn a_parent_and_child_are_joined_by_a_connector_in_the_gap_row() {
        let lines = drawn(&[("a.md", "# One\n\n## Two\n")]);
        // The gap between rank 0's box and rank 1's spans GAP_ROWS
        // rows. The terminus (no selection, so this edge is "focused"
        // like everything else) sits at the last one, just above Two's
        // box.
        let terminus = &lines[row_of(1) - 1];
        assert!(terminus.contains('*'), "expected a terminus marker just above Two: {terminus:?}");
        let gap = &lines[BOX_ROWS..row_of(1)];
        assert!(gap.iter().any(|l| l.contains('o')), "expected an 'o' in between: {gap:?}");
    }

    #[test]
    fn siblings_do_not_overlap_each_others_boxes() {
        let lines = drawn(&[("a.md", "# One\n\n## Two\n\n## Three\n")]);
        // Row 1 of rank 1 (row BOX_ROWS + GAP_ROWS + 1) holds both
        // labels. If boxes overlapped, one title would clobber the other.
        let row = &lines[row_of(1) + 1];
        assert!(row.contains("Two"), "{row:?}");
        assert!(row.contains("Three"), "{row:?}");
    }

    #[test]
    fn overlay_pan_arrows_draws_only_the_directions_with_more_to_pan_into() {
        let mut visible = Drawing { grid: vec![vec![' '; 5]; 3], dim: vec![vec![false; 5]; 3] };
        overlay_pan_arrows(&mut visible, true, false, true, false);
        assert_eq!(visible.grid[0][2], '↑', "up is allowed");
        assert_eq!(visible.grid[2][2], ' ', "down is not allowed: no arrow");
        assert_eq!(visible.grid[1][0], '←', "left is allowed");
        assert_eq!(visible.grid[1][4], ' ', "right is not allowed: no arrow");
    }

    #[test]
    fn overlay_pan_arrows_on_an_empty_drawing_is_a_no_op() {
        let mut visible = Drawing::default();
        overlay_pan_arrows(&mut visible, true, true, true, true); // must not panic
        assert!(visible.grid.is_empty());
    }

    #[test]
    fn dimensions_matches_what_draw_actually_produces() {
        let graph = graph_of(&[("a.md", "# One\n\n## Two\n")]);
        let laid = layout(&graph);
        let drawing = draw(&graph, &laid, None);
        assert_eq!(dimensions(&laid), (drawing.grid.len(), drawing.grid[0].len()));
    }

    #[test]
    fn dimensions_of_an_empty_layout_is_zero() {
        assert_eq!(dimensions(&layout(&Graph::default())), (0, 0));
    }

    #[test]
    fn unfocused_node_text_renders_faint_and_focused_text_does_not() {
        // Two and Three sit in the same rank, so their label rows are
        // the same line. Check for the escape immediately adjacent to
        // each name, rather than "is it anywhere in this line," since
        // Three's dimming would otherwise make a same-line check on Two
        // meaningless.
        let graph = graph_of(&[("a.md", "# One\n\n## Two\n\n## Three\n")]);
        let laid = layout(&graph);
        let two = laid.nodes.iter().find(|n| n.id.slug == "two").unwrap().id.clone();
        let joined = render_ansi(&draw(&graph, &laid, Some(&two))).join("\n");

        assert!(joined.contains("\x1b[2mThree"), "Three is not focused, its label should be faint: {joined:?}");
        assert!(!joined.contains("\x1b[2mTwo"), "the selection itself is never faint: {joined:?}");
        assert!(!joined.contains("\x1b[2mOne"), "One touches the selection via an edge: {joined:?}");
    }
}
```
