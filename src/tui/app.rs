//! The event loop: ties `term`, `input`, `draw`, `editor`, and `expand`
//! together.
//!
//! `App` holds the drawn view (`graph`/`layout`, the same shape `dankg
//! graph` would draw) plus which node is selected. `tab` grows that view by
//! revealing a selected node's hidden neighbours onto the existing grid,
//! per `expand::expand_view`; `base_graph`/`base_layout` are what it grows
//! from, and `expanded` is which anchors the reader has opened, so the
//! whole grown view can be rebuilt from scratch on every toggle rather than
//! edited in place -- see that module for why.
//!
//! The drawn grid is almost always bigger than the terminal -- even a
//! modest view runs to dozens of rows and a hundred-odd columns, and
//! nothing here ever shrinks a box to fit. `render` queries the real
//! terminal size every frame and clips to it (`draw::window`), scrolling
//! (`scroll_row`/`scroll_col`) just enough to keep the selection on
//! screen (`draw::scroll_to_show`) -- not centred, so the reader's sense
//! of where things are does not jump on every keypress that was already
//! visible.

use super::{draw, editor, eval, expand, input, term};
use crate::config::{Config, Keymap};
use crate::diag::Diags;
use crate::graph::{index, resolve, view, Graph, NodeId};
use crate::layout::{self, Layout};
use std::io::{self, Write};
use std::path::PathBuf;

/// Cycle state for `keys.eval`: the selected node's named blocks, and which
/// one the reader has cycled to. `file` is captured (rather than re-derived
/// from the still-selected node at run time) so navigating away and back
/// cannot silently point a stale cycle at the wrong file.
struct BlockSelect {
    file: String,
    /// (position among the file's named top-level blocks, display name) --
    /// the position is what actually runs the block; the name is display
    /// only, since decision 22 means two entries here can legally share one
    /// (a section spanning a nested sub-heading with its own same-named
    /// block).
    blocks: Vec<(usize, String)>,
    cursor: usize,
}

struct App {
    paths: Vec<String>,
    cache: bool,
    depth: Option<u32>,
    all: bool,
    /// Absolute. `Node.file` is root-relative (decision: ids are relative
    /// to the root so output is the same wherever the binary ran from),
    /// but a spawned editor inherits *dankg's* working directory, not the
    /// root's -- joining here is what makes `enter` open the right file
    /// regardless of where `dankg tui` was launched from.
    root: PathBuf,
    config: Config,
    /// `[keys]`, resolved once per load/reload -- see `config::Keymap`.
    /// Arrows are not in here; they always work regardless of this map.
    keys: Keymap,
    /// The whole resolved corpus, kept around only so `tab` has somewhere
    /// to look up a selected node's hidden neighbours; drawing never reads
    /// it directly.
    index: Graph,
    /// The view before any expansion -- exactly what `dankg graph` would
    /// draw. What `r` restores and what every `tab` rebuilds from.
    base_graph: Graph,
    base_layout: Layout,
    /// Anchors the reader has `tab`-opened, oldest first. Rebuilding from
    /// this on every toggle (`expand::expand_view`) is what makes
    /// collapsing an anchor also fold up whatever was only reachable
    /// through it, with no separate bookkeeping.
    expanded: Vec<NodeId>,
    graph: Graph,
    layout: Layout,
    selected: NodeId,
    /// Top-left corner of the visible window into the drawn grid, in grid
    /// rows/columns. Normally `render` is the only thing that moves these,
    /// following the selection; while `panning` is on, arrow/mnemonic keys
    /// move them directly instead (`pan`) and `render` stops following.
    scroll_row: usize,
    scroll_col: usize,
    /// Toggled by `keys.pan`. Repurposes the up/down/left/right keys from
    /// moving the selection to moving the viewport, and is the reason
    /// `render` draws arrows at whichever screen edges still have grid
    /// beyond them -- the mode's only indicator, since there is no status
    /// line to say so in words.
    panning: bool,
    /// `Some` while cycling through the selected node's named blocks
    /// (`keys.eval`); `enter` runs the cycled one, `esc` cancels, and any
    /// navigation key cancels it too, since the cycle belongs to whichever
    /// node was selected when it started.
    block_select: Option<BlockSelect>,
    /// The one line `render` reserves at the bottom of the viewport: the
    /// block-cycle list while `block_select` is active, or the last eval
    /// outcome afterward. `None` means draw the graph full height, with no
    /// status line at all -- there is nothing transient to say.
    status: Option<String>,
    /// `?` toggles this: a full-screen keybinding reference, replacing the
    /// graph rather than overlaying it -- there is no compositing here, and
    /// full-screen takeover is exactly what `enter`'s editor handoff already
    /// does for the same reason. Fully modal while shown: every key but the
    /// dismissers (`?`, esc, `keys.quit`) is swallowed rather than reaching
    /// the graph underneath.
    help: bool,
    diags: Diags,
}

/// `dankg tui <path>...`. Needs a real terminal -- there is nothing sound
/// to do with a pipe or a redirect on the other end of stdin.
pub fn run(paths: &[String], cache: bool, depth: Option<u32>, all: bool) -> Result<(), String> {
    if !term::is_tty() {
        return Err("the TUI needs an interactive terminal; stdin is not one".to_string());
    }
    let mut app = App::load(paths, cache, depth, all)?;

    let mut raw = Some(term::RawMode::enter().map_err(|e| e.to_string())?);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let result = event_loop(&mut app, &mut raw, &mut out);
    raw.take();

    app.diags.sort();
    app.diags.emit();
    result.map_err(|e| e.to_string())
}

fn event_loop(app: &mut App, raw: &mut Option<term::RawMode>, out: &mut impl Write) -> io::Result<()> {
    render(app, out)?;
    // Carries a byte `input::read_key` read but could not yet use (only
    // possible right after a standalone Esc) forward to the next call, so
    // it is decoded as its own key instead of silently vanishing.
    let mut pending = Vec::new();
    loop {
        let key = input::read_key(io::stdin(), &mut pending)?;

        // Fully modal: every other key is swallowed here rather than
        // reaching the match below, so nothing about the graph (selection,
        // panning, an in-progress eval cycle) can change while the help
        // screen has replaced it on screen.
        if app.help {
            let dismiss = matches!(key, input::Key::Char('?') | input::Key::Esc)
                || matches!(key, input::Key::Char(c) if c == app.keys.quit);
            if dismiss {
                app.toggle_help();
            }
            render(app, out)?;
            continue;
        }

        match key {
            input::Key::Char('?') => app.toggle_help(),
            input::Key::Up => {
                app.clear_transient();
                app.up();
            }
            input::Key::Down => {
                app.clear_transient();
                app.down();
            }
            input::Key::Left => {
                app.clear_transient();
                app.left();
            }
            input::Key::Right => {
                app.clear_transient();
                app.right();
            }
            input::Key::Esc => app.cancel_block_select(),
            input::Key::Char(c) if c == app.keys.quit => return Ok(()),
            input::Key::Char(c) if c == app.keys.up => {
                app.clear_transient();
                app.up();
            }
            input::Key::Char(c) if c == app.keys.down => {
                app.clear_transient();
                app.down();
            }
            input::Key::Char(c) if c == app.keys.left => {
                app.clear_transient();
                app.left();
            }
            input::Key::Char(c) if c == app.keys.right => {
                app.clear_transient();
                app.right();
            }
            input::Key::Char(c) if c == app.keys.reset => app.reset_selection(),
            input::Key::Char(c) if c == app.keys.pan => app.toggle_panning(),
            input::Key::Char(c) if c == app.keys.eval => app.eval_key(),
            input::Key::Tab => {
                app.clear_transient();
                app.toggle_expand();
            }
            input::Key::Enter if app.block_select.is_some() => app.run_selected_block(),
            input::Key::Enter => {
                if let Some(node) = app.graph.node(&app.selected) {
                    let file = app.root.join(&node.file).to_string_lossy().into_owned();
                    let line = node.line;
                    raw.take(); // restore the terminal for the editor
                    // Best-effort: a spawn failure has nowhere to report to
                    // yet in this path (there is one now for eval, but not
                    // for this), so it is swallowed rather than crashing
                    // the session.
                    let _ = editor::open(&app.config, &file, line);
                    *raw = Some(term::RawMode::enter()?);
                    app.reload();
                }
            }
            _ => {}
        }
        render(app, out)?;
    }
}

/// `term::size()`'s result, made safe to use as a viewport. A real query
/// failure and a "successful" all-zero `Winsize` -- observed through some
/// multiplexers/wrappers, when the kernel has not yet been told a real
/// size -- are both replaced with the same fallback: the zero case is not
/// an `Err` a plain `unwrap_or` would catch, and would otherwise clip
/// every frame to nothing. A real failure has no interactive terminal to
/// have come from in the first place (checked at startup), so 24x80 here
/// is defensive padding, not a meaningful default either way.
fn term_dimensions(size: io::Result<(u16, u16)>) -> (usize, usize) {
    let (rows, cols) = size.ok().filter(|&(r, c)| r > 0 && c > 0).unwrap_or((24, 80));
    (rows as usize, cols as usize)
}

/// `eval: [setup] index   enter=run esc=cancel`, with the cycled block
/// bracketed. What `render` shows as the status line while cycling.
fn block_select_status(sel: &BlockSelect) -> String {
    let parts: Vec<String> = sel
        .blocks
        .iter()
        .enumerate()
        .map(|(i, (_, n))| if i == sel.cursor { format!("[{n}]") } else { n.clone() })
        .collect();
    format!("eval: {}   enter=run esc=cancel", parts.join(" "))
}

/// Every action bound today, arranged the way `architecture.org`'s
/// interaction table is: fixed keys first (never remappable -- arrows,
/// enter, tab, esc, `?` itself), then the `[keys]`-configurable letters,
/// read live from `keys` so a remap shows up here too rather than the
/// reference silently going stale.
fn help_lines(keys: &Keymap) -> Vec<String> {
    vec![
        "DanKG -- keybindings".to_string(),
        String::new(),
        format!(
            "  arrows / {}{}{}{}    move selection (up/down cross ranks, left/right stay in one)",
            keys.left, keys.down, keys.up, keys.right
        ),
        "  enter              open the selected node in your editor".to_string(),
        "                     (or run the cycled block, while eval-cycling)".to_string(),
        "  tab                expand the selected node's hidden neighbours".to_string(),
        "  esc                cancel an eval cycle".to_string(),
        String::new(),
        format!("  {}                  cycle the selected node's named blocks; enter runs it", keys.eval),
        format!("  {}                  toggle panning (arrows/hjkl move the viewport instead)", keys.pan),
        format!("  {}                  collapse back to the entry view", keys.reset),
        format!("  {}                  quit", keys.quit),
        String::new(),
        "  ?                  toggle this help".to_string(),
        String::new(),
        "press ? (or esc, or q) to close".to_string(),
    ]
}

/// Writes `lines`, taking at most `term_rows` of them, with no trailing
/// `\r\n` after the very last one -- at exactly `term_rows` lines, that
/// `\r\n` would move the cursor past the bottom row and scroll the
/// alternate screen, which has no scrollback to absorb it, so the top line
/// would be gone from view. Shared by the graph frame and the help screen,
/// the only two things this module ever draws.
///
/// Deliberately does *not* clip columns here: a graph line carries ANSI
/// dimming codes (`draw::render_ansi`) that count as characters but not
/// columns, and truncating by character count would cut one off
/// mid-escape-sequence, corrupting terminal state for every line after it.
/// `draw::window` already guarantees a graph line is exactly `term_cols`
/// *glyphs* wide before the ANSI wrapping goes on; callers with plain text
/// of their own (the status line, the help screen) clip themselves, before
/// it reaches here, where there is no escape sequence yet to protect.
fn write_frame(out: &mut impl Write, lines: &[String], term_rows: usize) -> io::Result<()> {
    let mut buf = String::from("\x1b[H");
    let shown = lines.iter().take(term_rows);
    let last = shown.len().saturating_sub(1);
    for (i, line) in shown.enumerate() {
        buf.push_str(line);
        buf.push_str("\x1b[K"); // clear any leftover tail from a wider previous frame
        if i != last {
            buf.push_str("\r\n");
        }
    }
    buf.push_str("\x1b[J"); // and any leftover rows from a taller previous frame
    out.write_all(buf.as_bytes())?;
    out.flush()
}

fn render(app: &mut App, out: &mut impl Write) -> io::Result<()> {
    let (term_rows, term_cols) = term_dimensions(term::size());

    if app.help {
        let clipped: Vec<String> = help_lines(&app.keys).into_iter().map(|l| l.chars().take(term_cols).collect()).collect();
        return write_frame(out, &clipped, term_rows);
    }

    let (total_rows, total_cols) = draw::dimensions(&app.layout);
    // One row reserved at the bottom for `app.status`, when there is one --
    // a fixed upper bound on how much of the viewport the graph itself can
    // ever claim, so leaving room for it never depends on how tall the
    // graph's own content happens to be this frame.
    let status_rows = usize::from(app.status.is_some());
    let content_rows = term_rows.saturating_sub(status_rows).max(1);

    if app.panning {
        // Panning moved `scroll_row`/`scroll_col` directly (`App::pan`),
        // clamped only at zero there; the far end can only be clamped here,
        // where the terminal size and the full grid's size are both known.
        app.scroll_row = app.scroll_row.min(total_rows.saturating_sub(content_rows));
        app.scroll_col = app.scroll_col.min(total_cols.saturating_sub(term_cols));
    } else if let Some((rows, cols)) = app.selected_node().map(draw::extent) {
        app.scroll_row = draw::scroll_to_show(app.scroll_row, rows, content_rows);
        app.scroll_col = draw::scroll_to_show(app.scroll_col, cols, term_cols);
    }

    let full = draw::draw(&app.graph, &app.layout, Some(&app.selected));
    let mut visible = draw::window(&full, app.scroll_row, app.scroll_col, content_rows, term_cols);
    if app.panning {
        draw::overlay_pan_arrows(
            &mut visible,
            app.scroll_row > 0,
            app.scroll_row + content_rows < total_rows,
            app.scroll_col > 0,
            app.scroll_col + term_cols < total_cols,
        );
    }

    let mut lines = draw::render_ansi(&visible);
    if let Some(status) = &app.status {
        lines.push(status.chars().take(term_cols).collect());
    }
    write_frame(out, &lines, term_rows)
}

/// The pipeline `dankg graph` runs, minus rendering: load, resolve, select
/// the view, lay it out. Shared by the initial load and every reload after
/// returning from the editor. Returns the whole resolved corpus alongside
/// the selected view -- `tab` needs it to find a node's hidden neighbours.
fn build(
    paths: &[String],
    cache: bool,
    depth: Option<u32>,
    all: bool,
) -> Result<(PathBuf, Config, Keymap, Graph, Graph, Layout, Diags), String> {
    let mut diags = Diags::new("dankg");
    let corpus = index::load(paths, cache, &mut diags)?;
    let index = resolve::resolve(&corpus.files, &mut diags);
    let default_depth = corpus.config.depth(&mut diags);
    let keys = corpus.config.keymap(&mut diags);
    let graph = view::select_view(&index, &corpus.entries, depth, all, default_depth);
    let laid = layout::layout(&graph);
    Ok((corpus.root, corpus.config, keys, index, graph, laid, diags))
}

impl App {
    fn load(paths: &[String], cache: bool, depth: Option<u32>, all: bool) -> Result<App, String> {
        let (root, config, keys, index, graph, laid, diags) = build(paths, cache, depth, all)?;
        let selected = laid
            .nodes
            .first()
            .map(|n| n.id.clone())
            .ok_or_else(|| "nothing to draw: the view is empty".to_string())?;
        Ok(App {
            paths: paths.to_vec(),
            cache,
            depth,
            all,
            root,
            config,
            keys,
            index,
            base_graph: graph.clone(),
            base_layout: laid.clone(),
            expanded: Vec::new(),
            graph,
            layout: laid,
            selected,
            scroll_row: 0,
            scroll_col: 0,
            panning: false,
            block_select: None,
            status: None,
            help: false,
            diags,
        })
    }

    /// Re-runs `build` after returning from the editor, since the file may
    /// have just changed -- the cache makes a no-op re-index cheap. Best-
    /// effort: a load error or an empty resulting view leaves the previous
    /// state alone rather than dropping into a blank or crashed session.
    ///
    /// Drops every expansion rather than replaying `expanded` against the
    /// fresh index: the edit that triggered this reload may have changed
    /// the shape of the graph the anchors were computed against, and
    /// replaying a stale expansion risks a confusing placement more than
    /// starting clean costs the reader a keypress.
    fn reload(&mut self) {
        let Ok((root, config, keys, index, graph, laid, diags)) = build(&self.paths, self.cache, self.depth, self.all)
        else {
            return;
        };
        self.diags.absorb(diags);
        if graph.nodes.is_empty() {
            return;
        }
        self.root = root;
        // `laid.nodes` is non-empty here: the check above returned early
        // otherwise, and layout preserves the graph's node count exactly.
        if !laid.nodes.iter().any(|n| n.id == self.selected) {
            self.selected = laid.nodes[0].id.clone();
        }
        self.config = config;
        self.keys = keys;
        self.index = index;
        self.expanded.clear();
        self.base_graph = graph.clone();
        self.base_layout = laid.clone();
        self.graph = graph;
        self.layout = laid;
        self.scroll_row = 0;
        self.scroll_col = 0;
        // A cycle belongs to the selection that was current when it
        // started; the reload that just ran may have changed the graph
        // under it. The last eval outcome (`status`), if that is what
        // triggered this reload, is left alone -- the reader just ran it
        // and reloading is not itself a reason to hide the result.
        self.block_select = None;
    }

    /// `r`: per the interaction table, "collapse back to the entry view" --
    /// discarding every expansion, not just moving the cursor.
    fn reset_selection(&mut self) {
        self.expanded.clear();
        self.scroll_row = 0;
        self.scroll_col = 0;
        self.graph = self.base_graph.clone();
        self.layout = self.base_layout.clone();
        if let Some(first) = self.layout.nodes.first() {
            self.selected = first.id.clone();
        }
        self.clear_transient();
    }

    /// `tab`: reveal the selection's hidden neighbours onto the existing
    /// grid, or fold them away again if it is already open -- toggling one
    /// bind, mirroring the HTML renderer's click-to-expand (`assets.rs`).
    /// The whole derived view is rebuilt from `expanded` rather than
    /// edited incrementally; see `expand::expand_view` for why that is also
    /// what makes collapsing an ancestor fold up its descendants for free.
    fn toggle_expand(&mut self) {
        match self.expanded.iter().position(|id| *id == self.selected) {
            Some(at) => {
                self.expanded.remove(at);
            }
            None => self.expanded.push(self.selected.clone()),
        }
        let (graph, layout) = expand::expand_view(&self.index, &self.base_graph, &self.base_layout, &self.expanded);
        self.graph = graph;
        self.layout = layout;
    }

    fn selected_node(&self) -> Option<&crate::layout::LaidNode> {
        self.layout.nodes.iter().find(|n| n.id == self.selected)
    }

    /// Drops any in-progress block cycle and its status line. Called before
    /// every action that moves the selection or the view out from under a
    /// cycle that was tied to the *previous* selection -- navigating,
    /// expanding, or resetting all count.
    fn clear_transient(&mut self) {
        self.block_select = None;
        self.status = None;
    }

    /// `keys.eval`, first press: looks up the selected node's named blocks
    /// (its heading section, `node.line..=node.end_line` -- exactly the
    /// extent `graph/build.rs` already computes for it) and starts cycling
    /// on the first one. A second and later press, already cycling, instead
    /// advances to the next block, wrapping. A node with no named blocks in
    /// its section is a silent no-op: there is nothing to cycle to, the
    /// same "nowhere to report to" reasoning `enter`'s editor spawn already
    /// follows for its own best-effort failures.
    fn eval_key(&mut self) {
        if let Some(sel) = &mut self.block_select {
            sel.cursor = (sel.cursor + 1) % sel.blocks.len();
            self.status = Some(block_select_status(sel));
            return;
        }
        let Some(node) = self.graph.node(&self.selected) else { return };
        let file = self.root.join(&node.file).to_string_lossy().into_owned();
        let blocks = eval::blocks_in_section(&file, node.line, node.end_line);
        if blocks.is_empty() {
            return;
        }
        let sel = BlockSelect { file, blocks, cursor: 0 };
        self.status = Some(block_select_status(&sel));
        self.block_select = Some(sel);
    }

    /// `esc`, while cycling: cancels back to plain node selection, no run.
    /// Off cycle mode, `esc` remains the documented no-op it always was
    /// (architecture.org, Terminal UI open questions).
    fn cancel_block_select(&mut self) {
        if self.block_select.take().is_some() {
            self.status = None;
        }
    }

    /// `enter`, while cycling: runs the currently cycled block and leaves
    /// cycle mode either way, successful or not -- the action is complete,
    /// and the outcome is what the status line shows next. No separate
    /// confirm prompt: cycling to a block and pressing `enter` to run it
    /// already *is* the confirmation, same as `enter` needing none before
    /// it spawns the configured editor on a selected node.
    fn run_selected_block(&mut self) {
        let Some(sel) = self.block_select.take() else { return };
        let (position, name) = sel.blocks[sel.cursor].clone();
        let outcome = eval::run(&sel.file, &self.config, position);
        self.status = Some(match outcome {
            eval::Outcome::Ok => format!("{name}: ok"),
            eval::Outcome::Failed => format!("{name}: failed"),
            eval::Outcome::TimedOut => format!("{name}: timed out"),
            eval::Outcome::Error(e) => format!("{name}: {e}"),
        });
        self.reload();
    }

    /// `keys.pan`: flips whether the four direction keys move the selection
    /// or the viewport. Scroll position is left where it is on toggling
    /// off; `render` snaps it back onto the selection on its own the next
    /// frame, the same as any other selection move.
    fn toggle_panning(&mut self) {
        self.panning = !self.panning;
    }

    /// `?`: flips the full-screen keybinding reference. Not itself
    /// remappable via `[keys]` -- `?` for help is close to universal across
    /// terminal tools (vim, htop, git), and it is not a graph-navigation
    /// action the way the other seven are, so it is fixed the same way
    /// arrows, enter and tab already are.
    fn toggle_help(&mut self) {
        self.help = !self.help;
    }

    /// Moves the viewport directly rather than the selection -- what the
    /// direction keys do while `panning` is on. Clamped to zero here; the
    /// far end is clamped in `render`, the one place that knows the current
    /// terminal size and the full grid's extent.
    fn pan(&mut self, drow: i32, dcol: i32) {
        self.scroll_row = (self.scroll_row as i32 + drow).max(0) as usize;
        self.scroll_col = (self.scroll_col as i32 + dcol).max(0) as usize;
    }

    fn up(&mut self) {
        if self.panning {
            self.pan(-1, 0);
        } else {
            self.move_between_ranks(-1);
        }
    }

    fn down(&mut self) {
        if self.panning {
            self.pan(1, 0);
        } else {
            self.move_between_ranks(1);
        }
    }

    fn left(&mut self) {
        if self.panning {
            self.pan(0, -1);
        } else {
            self.move_within_rank(-1);
        }
    }

    fn right(&mut self) {
        if self.panning {
            self.pan(0, 1);
        } else {
            self.move_within_rank(1);
        }
    }

    /// Left/right (or h/l): stay in the current rank, clamped at either end.
    fn move_within_rank(&mut self, delta: i32) {
        let Some(cur) = self.selected_node() else { return };
        let layers = self.layout.layers();
        let Some(row) = layers.get(cur.rank as usize) else { return };
        if row.is_empty() {
            return;
        }
        let new_order = (cur.order as i32 + delta).clamp(0, row.len() as i32 - 1) as usize;
        self.selected = row[new_order].id.clone();
    }

    /// Up/down (or k/j): cross ranks, landing on whichever node in the
    /// target rank sits closest in `x` -- well-defined without inventing a
    /// second notion of adjacency, since `x` is the layout's own answer to
    /// "where does this node belong horizontally."
    fn move_between_ranks(&mut self, delta: i32) {
        let Some(cur) = self.selected_node() else { return };
        let target = cur.rank as i32 + delta;
        if target < 0 {
            return;
        }
        let x = cur.x;
        let layers = self.layout.layers();
        let Some(row) = layers.get(target as usize) else { return };
        let Some(nearest) = row.iter().min_by_key(|n| (n.x - x).abs()) else { return };
        self.selected = nearest.id.clone();
    }
}

#[cfg(test)]
impl App {
    fn from_graph(graph: Graph) -> App {
        Self::from_view(graph.clone(), graph)
    }

    /// Like `from_graph`, but `index` and the drawn view can differ, so a
    /// test can exercise `tab` against neighbours the view starts without.
    fn from_view(index: Graph, base: Graph) -> App {
        let laid = layout::layout(&base);
        let selected = laid.nodes.first().map(|n| n.id.clone()).expect("test graphs are non-empty");
        App {
            paths: Vec::new(),
            cache: true,
            depth: None,
            all: false,
            root: PathBuf::new(),
            config: Config::none(),
            keys: Keymap::default(),
            index,
            base_graph: base.clone(),
            base_layout: laid.clone(),
            expanded: Vec::new(),
            graph: base,
            layout: laid,
            selected,
            scroll_row: 0,
            scroll_col: 0,
            panning: false,
            block_select: None,
            status: None,
            help: false,
            diags: Diags::new("test"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{graph_of, view};

    fn app(files: &[(&str, &str)]) -> App {
        App::from_graph(graph_of(files))
    }

    /// An `Entry` heading containing a `Child`, with the view starting at
    /// depth 0 -- Entry only, Child hidden until something expands it.
    fn app_with_hidden_child() -> App {
        let index = graph_of(&[("a.md", "# Entry\n\n## Child\n")]);
        let entries = view::entry_nodes(&index, &["a.md".to_string()]);
        let base = view::select(&index, &entries[..1], 0);
        App::from_view(index, base)
    }

    #[test]
    fn left_right_move_within_a_rank_and_clamp_at_the_ends() {
        let mut a = app(&[("a.md", "# One\n\n## Two\n\n## Three\n")]);
        a.move_between_ranks(1); // onto rank 1, which has two siblings
        let first_sibling = a.selected.clone();
        a.move_within_rank(-1); // already leftmost: clamps
        assert_eq!(a.selected, first_sibling);
        a.move_within_rank(1);
        assert_ne!(a.selected, first_sibling, "moved to the next sibling");
        let second_sibling = a.selected.clone();
        a.move_within_rank(1); // already rightmost: clamps
        assert_eq!(a.selected, second_sibling);
    }

    #[test]
    fn up_down_cross_ranks_by_nearest_column() {
        let mut a = app(&[("a.md", "# One\n\n## Two\n\n## Three\n")]);
        assert_eq!(a.selected.slug, "one");
        a.move_between_ranks(1);
        assert_ne!(a.selected.slug, "one", "moved down into rank 1");
        a.move_between_ranks(-1);
        assert_eq!(a.selected.slug, "one", "moved back up to the only rank-0 node");
    }

    #[test]
    fn moving_past_the_first_or_last_rank_is_a_no_op() {
        let mut a = app(&[("a.md", "# One\n")]);
        let start = a.selected.clone();
        a.move_between_ranks(-1);
        assert_eq!(a.selected, start);
        a.move_between_ranks(1);
        assert_eq!(a.selected, start);
    }

    #[test]
    fn reset_selection_returns_to_the_first_node() {
        let mut a = app(&[("a.md", "# One\n\n## Two\n")]);
        a.move_between_ranks(1);
        assert_ne!(a.selected.slug, "one");
        a.reset_selection();
        assert_eq!(a.selected.slug, "one");
    }

    #[test]
    fn tab_reveals_a_hidden_neighbour_without_moving_the_selection() {
        let mut a = app_with_hidden_child();
        let entry = a.selected.clone();
        assert_eq!(a.graph.nodes.len(), 1, "Child starts hidden");

        a.toggle_expand();
        assert_eq!(a.graph.nodes.len(), 2, "tab reveals Child");
        assert_eq!(a.selected, entry, "the selection does not move");
        assert_eq!(a.expanded, vec![entry]);
    }

    #[test]
    fn tab_again_collapses_what_it_revealed() {
        let mut a = app_with_hidden_child();
        a.toggle_expand();
        a.toggle_expand();
        assert_eq!(a.graph.nodes.len(), 1, "collapsed back to just Entry");
        assert!(a.expanded.is_empty());
    }

    #[test]
    fn r_collapses_every_expansion_too() {
        let mut a = app_with_hidden_child();
        a.toggle_expand();
        assert_eq!(a.graph.nodes.len(), 2);

        a.reset_selection();
        assert_eq!(a.graph.nodes.len(), 1, "r folds the expansion away, not just the cursor");
        assert!(a.expanded.is_empty());
    }

    #[test]
    fn toggle_panning_flips_the_flag() {
        let mut a = app(&[("a.md", "# One\n")]);
        assert!(!a.panning);
        a.toggle_panning();
        assert!(a.panning);
        a.toggle_panning();
        assert!(!a.panning);
    }

    #[test]
    fn panning_moves_the_viewport_without_moving_the_selection() {
        let mut a = app(&[("a.md", "# One\n\n## Two\n\n## Three\n")]);
        a.toggle_panning();
        let before = a.selected.clone();
        a.right();
        a.down();
        assert_eq!(a.selected, before, "panning never moves the selection");
        assert_eq!((a.scroll_row, a.scroll_col), (1, 1));
    }

    #[test]
    fn pan_does_not_go_negative() {
        let mut a = app(&[("a.md", "# One\n")]);
        a.toggle_panning();
        a.left();
        a.up();
        assert_eq!((a.scroll_row, a.scroll_col), (0, 0));
    }

    #[test]
    fn without_panning_the_direction_keys_move_the_selection_as_before() {
        let mut a = app(&[("a.md", "# One\n\n## Two\n")]);
        assert!(!a.panning);
        a.down();
        assert_ne!(a.selected.slug, "one", "down still moves the selection when not panning");
    }

    #[test]
    fn render_clamps_an_over_panned_scroll_to_the_grid() {
        let files: Vec<(String, String)> = (0..10)
            .map(|i| {
                let body = if i + 1 < 10 { format!("[next]({}.md#f{})\n", i + 1, i + 1) } else { String::new() };
                (format!("{i}.md"), format!("# F{i}\n\n{body}"))
            })
            .collect();
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
        let mut a = app(&refs);
        a.toggle_panning();
        a.pan(10_000, 0); // way past the bottom of the grid

        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        let (total_rows, _) = draw::dimensions(&a.layout);
        assert!(
            a.scroll_row <= total_rows.saturating_sub(24),
            "render should clamp the over-panned scroll to the grid, got {}",
            a.scroll_row
        );
    }

    #[test]
    fn term_dimensions_falls_back_on_a_real_query_failure() {
        let err = io::Error::other("ENOTTY");
        assert_eq!(term_dimensions(Err(err)), (24, 80));
    }

    #[test]
    fn term_dimensions_falls_back_on_a_successful_but_all_zero_winsize() {
        // Reported success with nothing actually filled in -- not an `Err`,
        // but just as useless as one; this is the case that made `render`
        // draw nothing at all rather than merely something ill-sized.
        assert_eq!(term_dimensions(Ok((0, 0))), (24, 80));
    }

    #[test]
    fn term_dimensions_passes_through_a_real_size() {
        assert_eq!(term_dimensions(Ok((40, 120))), (40, 120));
    }

    /// `eval::blocks_in_section`/`eval::run` re-read from disk, unlike
    /// everything else `App` does with a fixture graph -- so exercising
    /// them needs a real file on disk whose content matches the in-memory
    /// graph built from it, with `root` pointed at the directory holding it.
    /// Every call gets its own directory (a counter, not just the pid):
    /// tests run in parallel, and two tests both naming their fixture
    /// `a.md` would otherwise race to write and re-read the same path.
    fn app_with_real_file(name: &str, content: &str) -> (App, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("dankg-app-eval-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        let mut a = App::from_graph(graph_of(&[(name, content)]));
        a.root = dir.clone();
        (a, path)
    }

    #[test]
    fn eval_key_enters_cycle_mode_and_sets_a_status_line() {
        let (mut a, _path) = app_with_real_file("a.md", "# One\n\n```sh name=x\n:\n```\n");
        assert!(a.block_select.is_none());
        a.eval_key();
        assert!(a.block_select.is_some());
        assert_eq!(a.status.as_deref(), Some("eval: [x]   enter=run esc=cancel"));
    }

    #[test]
    fn eval_key_cycles_through_multiple_blocks_and_wraps() {
        let (mut a, _path) =
            app_with_real_file("a.md", "# One\n\n```sh name=x\n:\n```\n\n```sh name=y\n:\n```\n");
        a.eval_key();
        assert_eq!(a.block_select.as_ref().unwrap().cursor, 0);
        a.eval_key();
        assert_eq!(a.block_select.as_ref().unwrap().cursor, 1);
        assert_eq!(a.status.as_deref(), Some("eval: x [y]   enter=run esc=cancel"));
        a.eval_key(); // wraps back to the first
        assert_eq!(a.block_select.as_ref().unwrap().cursor, 0);
    }

    #[test]
    fn eval_key_is_a_noop_when_the_section_has_no_named_blocks() {
        let (mut a, _path) = app_with_real_file("a.md", "# One\n\nnothing to run here\n");
        a.eval_key();
        assert!(a.block_select.is_none());
        assert!(a.status.is_none());
    }

    #[test]
    fn cancel_block_select_clears_the_cycle_and_status() {
        let (mut a, _path) = app_with_real_file("a.md", "# One\n\n```sh name=x\n:\n```\n");
        a.eval_key();
        assert!(a.block_select.is_some());
        a.cancel_block_select();
        assert!(a.block_select.is_none());
        assert!(a.status.is_none());
    }

    #[test]
    fn navigation_clears_an_in_progress_cycle() {
        let (mut a, _path) =
            app_with_real_file("a.md", "# One\n\n## Two\n\n```sh name=x\n:\n```\n");
        a.eval_key();
        assert!(a.block_select.is_some());
        a.clear_transient();
        assert!(a.block_select.is_none());
        assert!(a.status.is_none());
    }

    #[test]
    fn run_selected_block_runs_writes_back_and_reports_ok() {
        let (mut a, path) = app_with_real_file("a.md", "```sh name=x\necho hi\n```\n");
        a.config = crate::config::Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        a.eval_key();
        assert!(a.block_select.is_some());
        a.run_selected_block();
        assert!(a.block_select.is_none(), "the cycle ends once the block has run");
        assert_eq!(a.status.as_deref(), Some("x: ok"));
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("dankg:result name=x"), "{written:?}");
    }

    #[test]
    fn run_selected_block_reports_failure_without_crashing() {
        let (mut a, _path) = app_with_real_file("a.md", "```sh name=x\nexit 1\n```\n");
        a.config = crate::config::Config::parse("[lang.sh]\ncommand = sh {file}\n", &mut Diags::new("t"));
        a.eval_key();
        a.run_selected_block();
        assert_eq!(a.status.as_deref(), Some("x: failed"));
    }

    #[test]
    fn toggle_help_flips_the_flag() {
        let mut a = app(&[("a.md", "# One\n")]);
        assert!(!a.help);
        a.toggle_help();
        assert!(a.help);
        a.toggle_help();
        assert!(!a.help);
    }

    #[test]
    fn help_lines_reflect_remapped_keys() {
        let keys = Keymap { eval: 'x', ..Keymap::default() };
        let lines = help_lines(&keys);
        assert!(lines.iter().any(|l| l.trim_start().starts_with("x ")), "{lines:?}");
    }

    #[test]
    fn render_shows_the_help_screen_instead_of_the_graph_while_help_is_on() {
        let mut a = app(&[("a.md", "# One\n\n## Two\n")]);
        a.toggle_help();
        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        let out = String::from_utf8(sink).unwrap();
        assert!(out.contains("keybindings"), "{out:?}");
        assert!(!out.contains("One") && !out.contains("Two"), "the graph itself should not be drawn: {out:?}");
    }

    #[test]
    fn render_scrolls_to_keep_the_selection_visible_on_a_grid_taller_than_the_terminal() {
        // Ten files chained by links lay out to ten ranks -- comfortably
        // past the 24-row fallback `render` uses when `term::size()` fails,
        // which it always will here (there is no real tty in a test run).
        let files: Vec<(String, String)> = (0..10)
            .map(|i| {
                let body = if i + 1 < 10 { format!("[next]({}.md#f{})\n", i + 1, i + 1) } else { String::new() };
                (format!("{i}.md"), format!("# F{i}\n\n{body}"))
            })
            .collect();
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
        let mut a = app(&refs);
        assert!(a.layout.ranks > 8, "the fixture should actually span more ranks than fit on screen");

        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        for _ in 0..9 {
            a.move_between_ranks(1);
            render(&mut a, &mut sink).unwrap();
            let (rows, _) = draw::extent(a.selected_node().unwrap());
            assert!(
                rows.start >= a.scroll_row && rows.end <= a.scroll_row + 24,
                "selection at rows {rows:?} is not inside the scrolled 24-row window starting at {}",
                a.scroll_row
            );
        }
    }
}
