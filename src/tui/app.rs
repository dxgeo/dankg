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

use super::{draw, editor, expand, input, term};
use crate::config::Config;
use crate::diag::Diags;
use crate::graph::{index, resolve, view, Graph, NodeId};
use crate::layout::{self, Layout};
use std::io::{self, Write};
use std::path::PathBuf;

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
    /// rows/columns. `render` is the only thing that moves these.
    scroll_row: usize,
    scroll_col: usize,
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
    loop {
        match input::read_key(io::stdin())? {
            input::Key::Char('q') => return Ok(()),
            input::Key::Up | input::Key::Char('k') => app.move_between_ranks(-1),
            input::Key::Down | input::Key::Char('j') => app.move_between_ranks(1),
            input::Key::Left | input::Key::Char('h') => app.move_within_rank(-1),
            input::Key::Right | input::Key::Char('l') => app.move_within_rank(1),
            input::Key::Char('r') => app.reset_selection(),
            input::Key::Tab => app.toggle_expand(),
            input::Key::Enter => {
                if let Some(node) = app.graph.node(&app.selected) {
                    let file = app.root.join(&node.file).to_string_lossy().into_owned();
                    let line = node.line;
                    raw.take(); // restore the terminal for the editor
                    // Best-effort: a spawn failure has nowhere to report to
                    // yet (no status line in `draw` today), so it is
                    // swallowed rather than crashing the session.
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

fn render(app: &mut App, out: &mut impl Write) -> io::Result<()> {
    let (term_rows, term_cols) = term_dimensions(term::size());

    if let Some((rows, cols)) = app.selected_node().map(draw::extent) {
        app.scroll_row = draw::scroll_to_show(app.scroll_row, rows, term_rows);
        app.scroll_col = draw::scroll_to_show(app.scroll_col, cols, term_cols);
    }

    let full = draw::draw(&app.graph, &app.layout, Some(&app.selected));
    let visible = draw::window(&full, app.scroll_row, app.scroll_col, term_rows, term_cols);

    let mut buf = String::from("\x1b[H");
    let lines = draw::render_ansi(&visible);
    let last = lines.len().saturating_sub(1);
    for (i, line) in lines.iter().enumerate() {
        buf.push_str(line);
        buf.push_str("\x1b[K"); // clear any leftover tail from a wider previous frame
        if i != last {
            // Not after the very last line: at exactly `term_rows` lines,
            // that `\r\n` would move the cursor past the bottom row and
            // scroll the alternate screen, which has no scrollback to
            // absorb it -- the top line would be gone from view.
            buf.push_str("\r\n");
        }
    }
    buf.push_str("\x1b[J"); // and any leftover rows from a taller previous frame
    out.write_all(buf.as_bytes())?;
    out.flush()
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
) -> Result<(PathBuf, Config, Graph, Graph, Layout, Diags), String> {
    let mut diags = Diags::new("dankg");
    let corpus = index::load(paths, cache, &mut diags)?;
    let index = resolve::resolve(&corpus.files, &mut diags);
    let default_depth = corpus.config.depth(&mut diags);
    let graph = view::select_view(&index, &corpus.entries, depth, all, default_depth);
    let laid = layout::layout(&graph);
    Ok((corpus.root, corpus.config, index, graph, laid, diags))
}

impl App {
    fn load(paths: &[String], cache: bool, depth: Option<u32>, all: bool) -> Result<App, String> {
        let (root, config, index, graph, laid, diags) = build(paths, cache, depth, all)?;
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
            index,
            base_graph: graph.clone(),
            base_layout: laid.clone(),
            expanded: Vec::new(),
            graph,
            layout: laid,
            selected,
            scroll_row: 0,
            scroll_col: 0,
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
        let Ok((root, config, index, graph, laid, diags)) = build(&self.paths, self.cache, self.depth, self.all)
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
        self.index = index;
        self.expanded.clear();
        self.base_graph = graph.clone();
        self.base_layout = laid.clone();
        self.graph = graph;
        self.layout = laid;
        self.scroll_row = 0;
        self.scroll_col = 0;
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
            index,
            base_graph: base.clone(),
            base_layout: laid.clone(),
            expanded: Vec::new(),
            graph: base,
            layout: laid,
            selected,
            scroll_row: 0,
            scroll_col: 0,
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
