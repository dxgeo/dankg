# TUI app

This is the event loop. It ties `term`, `input`, `draw`, `editor`, and
`eval` together. `App` holds the whole resolved corpus (`index`) plus a
derived tree over it (`children`/`roots`) and which of it is currently
open (`expanded`). There is no separate "drawn view" graph anymore: the
tree always represents the whole corpus, unconditionally. `--depth`/
`[tui] depth`/`--all` only decide which levels start pre-expanded under
the file(s) named on the command line (`entry_roots`/`initial_depth`);
every other top-level file still appears, collapsed to one line, from
the first frame -- the same way a file tree always shows the whole
project. This is a real behaviour change from the graph-layout renderer
this replaces, where naming a file scoped what was even loaded.

The screen is two panes: a tree on the left, and a persistent
cross-reference panel on the right showing the selected node's own
links, backlinks, and relations (`graph::query::links_for`). `tab`
toggles which pane has keyboard focus (`focus`). While the panel has
focus, up/down move among its navigable rows and `enter` jumps to one,
force-expanding every ancestor of the target so it becomes visible
(`reveal_and_select`) -- the same operation `/`-search's own `enter`
already needs, so there is exactly one implementation of "bring this
corpus-wide node on screen."

```rust name=module_doc path=tui/app.rs
//! This is the event loop. It ties `term`, `input`, `draw`, `editor`, and
//! `eval` together.
//!
//! `App` holds the whole resolved corpus (`index`) plus a derived tree
//! over it (`children`/`roots`) and which of it is open (`expanded`).
//! There is no separate "drawn view" graph: the tree always represents
//! the whole corpus. `--depth`/`[tui] depth`/`--all` only decide which
//! levels start pre-expanded under the named entry file(s)
//! (`entry_roots`/`initial_depth`); every other top-level file still
//! appears, collapsed, from the first frame.
//!
//! The screen is two panes: a tree (left) and a persistent
//! cross-reference panel (right) showing the selected node's own links,
//! backlinks, and relations (`graph::query::links_for`). `tab` toggles
//! which pane has focus. The panel's own `enter`-to-jump and `/`-
//! search's `enter`-to-confirm are the identical operation --
//! `reveal_and_select` force-expands a target's ancestors and selects
//! it -- so there is exactly one implementation of it.

use super::{draw, editor, eval, input, term};
use crate::config::{Config, Keymap};
use crate::diag::Diags;
use crate::eval::{files::Files, plan, result};
use crate::graph::{index, query, resolve, view, Graph, NodeId, NodeKind};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Cycle state for `keys.eval`: the selected node's named blocks, and
/// which one the reader has cycled to. `file` is captured, rather than
/// re-derived from the still-selected node at run time. This way,
/// navigating away and back cannot silently point a stale cycle at the
/// wrong file.
struct BlockSelect {
    file: String,
    /// (position among the file's named top-level blocks, display name).
    /// The position is what actually evaluates the block. The name is
    /// display only, since decision 22 means two entries here can legally
    /// share one (a section spanning a nested sub-heading with its own
    /// same-named block).
    blocks: Vec<(usize, String)>,
    cursor: usize,
}

/// Which pane owns the direction keys, enter, and esc right now. `tab`
/// toggles it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Tree,
    Panel,
}

/// Which rows `push_row` keeps, picked from the `f` menu
/// (dependency-surfacing.md, §3). `All` prunes nothing. Every other
/// variant hides a non-matching row while keeping its ancestors
/// visible, the same "hide, not dim" mental model `/`-search's own
/// ancestor-reveal already trained. `Hash` is for `App::filter_history`,
/// keyed by variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Filter {
    All,
    Blocks,
    EvalChain,
    FileArtifact,
}

impl Filter {
    /// The four variants, in menu order -- `next`/`prev` (below) both
    /// wrap through this same list, so the menu's own up/down never
    /// needs a second copy of the ordering.
    const ALL: [Filter; 4] = [Filter::All, Filter::Blocks, Filter::EvalChain, Filter::FileArtifact];

    fn next(self) -> Filter {
        match self {
            Filter::All => Filter::Blocks,
            Filter::Blocks => Filter::EvalChain,
            Filter::EvalChain => Filter::FileArtifact,
            Filter::FileArtifact => Filter::All,
        }
    }

    fn prev(self) -> Filter {
        match self {
            Filter::All => Filter::FileArtifact,
            Filter::Blocks => Filter::All,
            Filter::EvalChain => Filter::Blocks,
            Filter::FileArtifact => Filter::EvalChain,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Blocks => "blocks",
            Filter::EvalChain => "eval-chain",
            Filter::FileArtifact => "file-artifact",
        }
    }
}

/// One visible left-pane row, rebuilt fresh from `index`/`children`/
/// `expanded` every frame -- cheap at this corpus's size, the same
/// "just rebuild it" reasoning `draw.rs`'s own header comment already
/// gives for redrawing the whole grid every frame with no diffing.
struct TreeRow {
    id: NodeId,
    title: String,
    depth: u32,
    /// `None`: no children, never a `▸`/`▾` marker. `Some(expanded)`.
    marker: Option<bool>,
    /// `⇒1 ⇐1 ✗1 ↻1 ▤ →1 ←2 ⚭`-style summary of this node's own
    /// non-containment edges (`query::links_for`) and dependency facts
    /// (`self.deps`, `badge_for`); empty when it touches none.
    badge: String,
}

/// One right-pane row. `Outgoing`/`Backlink`/`DepOut`/`DepIn` are
/// navigable -- `enter`, while the panel has focus, jumps through
/// them. Everything else is plain display text, with nowhere
/// meaningful to jump: `Relation` (a relation node has no file/line,
/// `Node.line == 0`, a synthetic `db:NAME` namespace for `Node.file`),
/// `FileDep` (decision 33's `produces=file:PATH`/`reads=file:PATH`,
/// never resolved on its own -- dependency-surfacing.md, §C), and
/// `DepBroken`/`DepPending` (a `deps=`/`xdeps=` entry that failed to
/// resolve, or resolved but has not run yet -- §D, §E). The panel's
/// own cursor skips every non-navigable row.
enum PanelRow {
    Outgoing { target: NodeId, title: String },
    Backlink { target: NodeId, title: String },
    Relation { text: String },
    DepOut { target: NodeId, title: String },
    DepIn { target: NodeId, title: String },
    FileDep { text: String },
    DepBroken { text: String },
    DepPending { text: String },
}

impl PanelRow {
    fn target(&self) -> Option<&NodeId> {
        match self {
            PanelRow::Outgoing { target, .. }
            | PanelRow::Backlink { target, .. }
            | PanelRow::DepOut { target, .. }
            | PanelRow::DepIn { target, .. } => Some(target),
            PanelRow::Relation { .. } | PanelRow::FileDep { .. } | PanelRow::DepBroken { .. } | PanelRow::DepPending { .. } => None,
        }
    }
}

struct App {
    paths: Vec<String>,
    cache: bool,
    depth: Option<u32>,
    all: bool,
    /// Absolute. `Node.file` is root-relative (decision: ids are relative
    /// to the root so output is the same wherever the binary ran from).
    /// A spawned editor inherits *dankg's* working directory, not the
    /// root's. Joining here is what makes `enter` open the right file
    /// regardless of where `dankg tui` was launched from.
    root: PathBuf,
    config: Config,
    /// `[keys]`, resolved once per load/reload. See `config::Keymap`.
    /// Arrows are not in here. They always work regardless of this map.
    keys: Keymap,
    /// The whole resolved corpus. The only graph now -- the tree always
    /// represents this, unconditionally.
    index: Graph,
    /// `Node.parent -> children`, in `index.nodes`' own deterministic
    /// order. `Relation` nodes are excluded from both keys and values:
    /// they carry no `parent` and are never a tree row, only ever panel
    /// text (`build_children`).
    children: HashMap<NodeId, Vec<NodeId>>,
    /// `deps=`/`xdeps=`/file-artifact facts about the corpus, computed
    /// once alongside `children`/`roots` (`compute_dep_data`,
    /// dependency-surfacing.md).
    deps: DepData,
    /// Every top-level tree row: each node with `parent: None` that is
    /// not a `Relation` -- one per file's own top-level heading, or its
    /// synthetic wrapper (`graph/build.rs`'s `file_node`) -- in
    /// `index.nodes`' order.
    roots: Vec<NodeId>,
    /// Which of `roots` the command line actually named. Every other
    /// root starts (and re-collapses, on `r`) as one collapsed line.
    entry_roots: Vec<NodeId>,
    /// How many containment levels below each of `entry_roots` start
    /// expanded -- `depth.unwrap_or(default_depth)`, or `u32::MAX` when
    /// `--all` was given or no specific file was named
    /// (`resolve_entry_roots`).
    initial_depth: u32,
    /// `[tui] depth`'s own resolved value, kept only so `r`/`reload`
    /// never need to re-read config.
    default_depth: u32,
    /// Which tree nodes are currently expanded. Presence is the flag: a
    /// childless node is never a member regardless of what that would
    /// mean. `r`/`reload` both replace this wholesale with a fresh
    /// `initial_expansion(...)` rather than editing it in place.
    expanded: HashSet<NodeId>,
    /// Which rows `push_row` keeps, picked from the fixed-key `f`
    /// menu. `r`/`reload` leave this alone -- unlike `expanded`, a
    /// filter is the reader's own standing choice, not tree-shape
    /// state that a reload could invalidate.
    filter: Filter,
    /// `Some` while the filter-picker overlay is open: which `Filter`
    /// the menu's own cursor currently highlights, not yet applied to
    /// `self.filter` until `enter` confirms it (`apply_filter`). `esc`
    /// closes without applying.
    filter_menu: Option<Filter>,
    /// Where `self.selected` was the last time each `Filter` was
    /// active, keyed by variant. `apply_filter` both reads this (to
    /// restore a remembered position) and writes it (recording the
    /// row being left, every time the filter changes).
    filter_history: HashMap<Filter, NodeId>,
    /// Whether the reader has explicitly navigated (arrows, `/`-search,
    /// a panel jump) since the last filter change. `apply_filter`'s own
    /// tie-breaker: a reader who has already moved to a new row since
    /// picking the current filter gets to keep it, even if that row
    /// disagrees with what `filter_history` remembers; a reader who
    /// has not moved gets that remembered position back instead.
    moved_since_filter_change: bool,
    selected: NodeId,
    /// Which pane owns the direction keys/enter/esc right now.
    focus: Focus,
    /// The panel's own cursor: an index into its navigable
    /// (non-`Relation`) rows only. Reset to 0 whenever focus moves onto
    /// the panel or the selection changes, since a cursor position from
    /// a different node's link list means nothing here.
    panel_cursor: usize,
    /// The tree pane's own vertical scroll, in visible tree rows.
    scroll_row: usize,
    /// The panel's own vertical scroll, in panel lines -- independent of
    /// `scroll_row`, since the two panes can each be taller than the
    /// screen on their own schedule.
    scroll_panel: usize,
    /// `Some` while cycling through the selected node's named blocks
    /// (`keys.eval`). `enter` evaluates the cycled one. `esc` cancels. Any
    /// navigation key cancels it too, since the cycle belongs to whichever
    /// node was selected when it started.
    block_select: Option<BlockSelect>,
    /// `Some` while `/`-jumping: the title text typed so far. `enter`
    /// jumps to the first match and clears it; `esc` cancels with no
    /// jump. Modal the same way `block_select` is -- every key but
    /// enter/esc/backspace/a-character is swallowed while it is active
    /// (`event_loop`), so navigation cannot run out from under a query
    /// still being typed.
    search: Option<String>,
    /// The last *confirmed* search query, lowercased, kept after
    /// `search` itself is cleared -- vim's own `n`/`N` convention: the
    /// pattern survives the search prompt closing, so `n`/`N` can keep
    /// cycling through its matches long after `/`'s own buffer is gone.
    last_search: Option<String>,
    /// The one line `render` reserves at the bottom of the viewport: the
    /// block-cycle list while `block_select` is active, the `/query` typed
    /// so far while `search` is active, or the last eval outcome
    /// afterward. `None` means rendering the panes at full height, with
    /// no status line at all. There is nothing transient to say.
    status: Option<String>,
    /// `?` toggles this: a keybinding reference, drawn as a small box
    /// over the tree/panel (`draw::overlay`) rather than replacing
    /// either. Still fully modal while on screen: every key but the
    /// dismissers (`?`, esc, `keys.quit`) is swallowed rather than
    /// reaching the tree/panel underneath.
    help: bool,
    /// `[tui] breadcrumb`'s resolved value at load, then `keys.breadcrumb`'s
    /// own toggle from then on. While the panel has focus and this is
    /// `true`, `render` shows `self.selected`'s own title on the status
    /// line, unless a search or an eval outcome already claims that line.
    /// This exists because the tree's own underlined row moves to whatever
    /// panel row is being hovered (`App::preview_target`) while the panel
    /// has focus. Without it, the origin the reader tabbed away from has
    /// no marker at all once they start exploring links elsewhere in the
    /// tree.
    breadcrumb: bool,
    diags: Diags,
}
```

`run` needs a real terminal. There is nothing sound to do with a pipe or
a redirect on the other end of stdin, so it refuses outright rather than
degrading. `event_loop`'s `pending` carries forward a byte
`input::read_key` read but could not yet decode (only possible right
after a standalone Esc). This way, it becomes its own key on the next
iteration instead of silently vanishing. The help screen is fully modal:
every key but its own dismissers is swallowed before it ever reaches the
match below. `/`-searching and panel-focus are each modal one step
narrower: only the keys each actually uses reach them, so navigation
cannot run out from under a query still being typed or a jump still
being picked.

Waiting for the next key does not block on `read` directly: it polls
stdin with a short timeout (`term::stdin_ready`) so a resize with no
keypress after it still gets noticed and redrawn (`term::take_resized`,
*Resize notifications* in `term.md`) instead of leaving a stale frame
on screen until the reader happens to press something. The one
exception is `pending` already decoding to a full key on its own (the
byte right after a standalone Esc) -- that never waits on stdin at
all, matching `read_key`'s own "replay before reading again" rule, so
a resize check never delays a keystroke that had already arrived.

```rust name=run_and_event_loop path=tui/app.rs
/// `dankg tui <path>...`. Needs a real terminal. There is nothing sound
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

/// How often a wait for the next key checks in on a resize when
/// nothing has been typed. Short enough that a resize feels immediate;
/// `recv`-style waits like this one sleep rather than spin, so an idle
/// TUI still costs nothing between ticks.
const RESIZE_POLL_MS: i32 = 100;

fn event_loop(app: &mut App, raw: &mut Option<term::RawMode>, out: &mut impl Write) -> io::Result<()> {
    render(app, out)?;
    // Carries a byte `input::read_key` read but could not yet use (only
    // possible right after a standalone Esc) forward to the next call.
    // This way, it is decoded as its own key instead of silently
    // vanishing.
    let mut pending = Vec::new();
    loop {
        let key = loop {
            if input::decode(&pending).is_some() || term::stdin_ready(RESIZE_POLL_MS) {
                break input::read_key(io::stdin(), &mut pending)?;
            }
            if term::take_resized() {
                render(app, out)?;
            }
        };

        // Fully modal: every other key is swallowed here rather than
        // reaching the match below. This way, nothing about the tree,
        // panel, or an in-progress eval cycle can change while the help
        // screen has replaced them on screen.
        if app.help {
            let dismiss = matches!(key, input::Key::Char('?') | input::Key::Esc)
                || matches!(key, input::Key::Char(c) if c == app.keys.quit);
            if dismiss {
                app.toggle_help();
            }
            render(app, out)?;
            continue;
        }

        if app.search.is_some() {
            match key {
                input::Key::Enter => app.confirm_search(),
                input::Key::Esc => app.cancel_search(),
                input::Key::Backspace => app.search_backspace(),
                input::Key::Char(c) => app.search_push(c),
                _ => {}
            }
            render(app, out)?;
            continue;
        }

        // Fully modal, the same way `app.help` is above: every key but
        // its own up/down/enter/esc is swallowed rather than reaching
        // the tree/panel underneath.
        if app.filter_menu.is_some() {
            match key {
                input::Key::Enter => app.confirm_filter_menu(),
                input::Key::Esc => app.cancel_filter_menu(),
                input::Key::Up => app.move_filter_menu_cursor(-1),
                input::Key::Down => app.move_filter_menu_cursor(1),
                input::Key::Char(c) if c == app.keys.up => app.move_filter_menu_cursor(-1),
                input::Key::Char(c) if c == app.keys.down => app.move_filter_menu_cursor(1),
                _ => {}
            }
            render(app, out)?;
            continue;
        }

        // Panel-focused: up/down move its own cursor, enter jumps, left
        // or esc return focus to the tree without acting. keys.breadcrumb
        // still works here too -- this is exactly the state it exists for.
        if app.focus == Focus::Panel {
            match key {
                input::Key::Char('?') => app.toggle_help(),
                input::Key::Up => app.up(),
                input::Key::Down => app.down(),
                input::Key::Left | input::Key::Esc => app.cancel_panel_focus(),
                input::Key::Char(c) if c == app.keys.up => app.up(),
                input::Key::Char(c) if c == app.keys.down => app.down(),
                input::Key::Char(c) if c == app.keys.left => app.cancel_panel_focus(),
                input::Key::Char(c) if c == app.keys.quit => return Ok(()),
                input::Key::Char(c) if c == app.keys.breadcrumb => app.toggle_breadcrumb(),
                input::Key::Enter => app.confirm_panel_row(),
                input::Key::Tab => app.toggle_focus(),
                _ => {}
            }
            render(app, out)?;
            continue;
        }

        match key {
            input::Key::Char('?') => app.toggle_help(),
            input::Key::Char('/') => app.start_search(),
            input::Key::Char('f') => app.open_filter_menu(),
            input::Key::Char('n') => {
                app.clear_transient();
                app.search_next();
            }
            input::Key::Char('N') => {
                app.clear_transient();
                app.search_prev();
            }
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
            input::Key::Char(c) if c == app.keys.eval => app.eval_key(),
            input::Key::Char(c) if c == app.keys.breadcrumb => app.toggle_breadcrumb(),
            input::Key::Tab => {
                app.clear_transient();
                app.toggle_focus();
            }
            input::Key::Enter if app.block_select.is_some() => app.run_selected_block(),
            input::Key::Enter => {
                if let Some(node) = app.index.node(&app.selected) {
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
```

`term_dimensions` folds two failure shapes into one fallback: a real
query error, and a "successful" all-zero `Winsize` observed through some
multiplexers/wrappers when the kernel has not yet been told a real size.
The zero case is not an `Err` a plain `unwrap_or` would catch. Left alone,
it would clip every frame to nothing rather than merely something
ill-sized. 24x80 here is defensive padding, not a meaningful default
either way, since a real query failure has no interactive terminal to
have come from in the first place.

```rust name=term_dimensions_and_status path=tui/app.rs
/// `term::size()`'s result, made safe to use as a viewport. A real query
/// failure and a "successful" all-zero `Winsize` (observed through some
/// multiplexers/wrappers, when the kernel has not yet been told a real
/// size) are both replaced with the same fallback. The zero case is not
/// an `Err` a plain `unwrap_or` would catch, and would otherwise clip
/// every frame to nothing. A real failure has no interactive terminal to
/// have come from in the first place (checked at startup). 24x80 here
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

/// Every action bound today. Fixed keys first (never remappable --
/// arrows, enter, tab, esc, `/`, `?`), then the `[keys]`-configurable
/// letters, read live from `keys`. This way, a remap shows up here too
/// rather than the reference silently going stale.
fn help_lines(keys: &Keymap) -> Vec<String> {
    vec![
        "DanKG -- keybindings".to_string(),
        String::new(),
        format!(
            "  arrows / {}{}{}{}    move; left/right collapse/expand a node or step to its parent/child",
            keys.left, keys.down, keys.up, keys.right
        ),
        "  enter              open the selected node in your editor".to_string(),
        "                     (or run the cycled block, while eval-cycling)".to_string(),
        "                     (or jump to the focused link, while the panel has focus)".to_string(),
        "  tab                toggle focus between the tree and the link panel".to_string(),
        "  /                  jump to a node by title; enter confirms, esc cancels".to_string(),
        "  n / N              jump to the next / previous match of the last search".to_string(),
        "  f                  open the filter menu; enter applies it, esc cancels".to_string(),
        "  esc                cancel an eval cycle or a search; leave the panel".to_string(),
        String::new(),
        format!("  {}                  cycle the selected node's named blocks; enter runs it", keys.eval),
        format!("  {}                  collapse back to the entry view", keys.reset),
        format!("  {}                  quit", keys.quit),
        format!("  {}                  toggle the origin breadcrumb (status line, panel focus only)", keys.breadcrumb),
        String::new(),
        "  ?                  toggle this help".to_string(),
        String::new(),
        "press ? (or esc, or q) to close".to_string(),
    ]
}

/// The filter-picker's own overlay content, `cursor`'s row already
/// marked `RowStyle::Current` so its highlight survives
/// `draw::overlay`'s own paste onto the real frame (`draw::overlay`'s
/// own doc comment). Row 0 of the box is its top border, row 1 is the
/// title, and the four options start at row 2 in `Filter::ALL`'s own
/// order -- the same order the menu's up/down cursor (`Filter::next`/
/// `prev`) already walks.
fn filter_menu_box(cursor: Filter) -> draw::Drawing {
    let mut lines = vec!["Filter".to_string()];
    lines.extend(Filter::ALL.iter().map(|f| format!("  {}", f.label())));
    let mut menu = draw::box_grid(&lines);
    let cursor_index = Filter::ALL.iter().position(|&f| f == cursor).unwrap_or(0);
    let width = menu.grid.first().map_or(0, Vec::len);
    draw::mark_row(&mut menu, 2 + cursor_index, 0, width, draw::RowStyle::Current);
    menu
}
```

`write_frame` deliberately never emits a trailing `\r\n` after the very
last line. At exactly `term_rows` lines, that would move the cursor past
the bottom row and scroll the alternate screen. The alternate screen has
no scrollback to absorb it. So the top line would simply be gone from
view.

It also deliberately does *not* clip columns. A pane line already
carries ANSI attribute codes (`draw::render_ansi`) that count as
characters but not columns. Truncating by character count would cut one
off mid-escape-sequence, corrupting terminal state for every line after
it. `render`'s own composition already guarantees a frame line is
exactly `term_cols` *glyphs* wide before the ANSI wrapping goes on. So
this function trusts that. A caller with plain text of its own (the
status line, the help screen) clips itself before it ever reaches here.

```rust name=write_frame path=tui/app.rs
/// Writes `lines`, taking at most `term_rows` of them, with no trailing
/// `\r\n` after the very last one. At exactly `term_rows` lines, that
/// `\r\n` would move the cursor past the bottom row and scroll the
/// alternate screen. The alternate screen has no scrollback to absorb it,
/// so the top line would be gone from view.
///
/// Deliberately does *not* clip columns here. A pane line carries ANSI
/// attribute codes (`draw::render_ansi`) that count as characters but not
/// columns, and truncating by character count would cut one off
/// mid-escape-sequence, corrupting terminal state for every line after it.
/// `render`'s own composition already guarantees a frame line is exactly
/// `term_cols` *glyphs* wide before the ANSI wrapping goes on. Callers
/// with plain text of their own (the status line, the help screen) clip
/// themselves before it reaches here, where there is no escape sequence
/// yet to protect.
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
```

`render`'s status line reserves exactly one row, a fixed upper bound on
how much of the viewport the panes can ever claim, so leaving room for
it never depends on how tall either pane's own content happens to be
this frame. The tree and the panel each scroll entirely independently,
each following its own current row via `draw::scroll_to_show` -- there
is no shared viewport position the way panning once moved one for both.
The focused pane's current row renders in reverse video; the other
pane's own remembered row renders underlined -- its text otherwise
untouched -- so switching focus back and forth never loses track of
where it was.

While the panel has focus, that underlined tree row is not
`self.selected`'s own -- it is `App::preview_target`'s: the currently
hovered panel row's own target, previewed with its ancestors opened
just for this frame (`App::visible_rows_with`, never touching
`self.expanded`), so the tree shows where `enter` would actually land
before the reader commits to it. Moving the panel cursor to a
different link updates the preview immediately; leaving the panel
without pressing `enter` (`esc`/left) leaves
`self.expanded`/`self.selected` exactly as they were, and the very
next frame's `scroll_to_show` snaps the tree pane back onto the real
selection on its own, the same way any other selection move already
does. A panel with nothing navigable to hover (only `Relation` rows,
or none at all) has no preview target, and the tree simply shows
`self.selected` underlined, as it always did before this existed.

### Origin breadcrumb

Once a preview target takes the tree's underlined row, `self.selected`
itself has no marker left anywhere on screen. A reader who tabbed to
the panel specifically to explore a node's own links can lose track of
that node the moment they hover a second one. `app.breadcrumb`
(`[tui] breadcrumb`, on by default; `keys.breadcrumb` toggles it for
the session) fixes this without touching the tree at all: while the
panel has focus and this is on, `render` prints `self.selected`'s own
title on the status line. Search and an eval outcome both outrank it
\-- both are the reader's own immediate action, so neither should have
to fight a breadcrumb for the one line they share. The tree's own
scroll window already cannot promise both the origin and a distant
preview target fit in view together over a large corpus, which is why
this lives on the status line rather than as a second highlight inside
the tree.

```rust name=render path=tui/app.rs
fn render(app: &mut App, out: &mut impl Write) -> io::Result<()> {
    let (term_rows, term_cols) = term_dimensions(term::size());

    // A standing filter claims the status line one tier below search and
    // an eval outcome -- it is a mode the reader turned on, not a
    // one-shot action, the same reasoning `eval: [...]` and `/query`
    // already outrank it for (dependency-surfacing.md, §3).
    let filter_line =
        (app.filter != Filter::All && app.status.is_none() && app.search.is_none()).then(|| format!("filter: {}", app.filter.label()));

    // The breadcrumb only ever claims the status line when nothing more
    // urgent already does (search, then an eval outcome, then a standing
    // filter), and only while the panel has focus -- see *Origin
    // breadcrumb* above.
    let breadcrumb_line = (app.focus == Focus::Panel
        && app.breadcrumb
        && app.status.is_none()
        && app.search.is_none()
        && filter_line.is_none())
    .then(|| app.index.node(&app.selected).map(|n| n.title.clone()).unwrap_or_else(|| app.selected.to_string()));

    // One row reserved at the bottom for `app.status`, the `/query` typed
    // so far while `app.search` is active, the standing filter, or the
    // origin breadcrumb.
    let status_rows =
        usize::from(app.status.is_some() || app.search.is_some() || filter_line.is_some() || breadcrumb_line.is_some());
    let content_rows = term_rows.saturating_sub(status_rows).max(1);
    let panel_cols = draw::panel_width(term_cols);
    let tree_cols = term_cols.saturating_sub(panel_cols + 1).max(1);

    // While the panel has focus and its cursor sits on a navigable row,
    // the tree previews that row's own target -- ancestors opened for
    // this frame only (never `self.expanded`) -- rather than showing
    // `self.selected`'s stale position. `enter` is what makes it real.
    let preview = app.preview_target();
    let tree_rows = match &preview {
        Some(id) => {
            let mut expanded = app.expanded.clone();
            expanded.extend(ancestors_of(&app.index, id));
            app.visible_rows_with(&expanded)
        }
        None => app.visible_rows(),
    };
    let tree_lines: Vec<String> =
        tree_rows.iter().map(|r| draw::tree_line(r.depth, r.marker, &r.title, &r.badge, tree_cols)).collect();
    let tree_highlight = preview.as_ref().unwrap_or(&app.selected);
    let tree_current_row = tree_rows.iter().position(|r| &r.id == tree_highlight);

    let panel_rows = app.panel_rows();
    let panel_lines: Vec<String> = panel_rows
        .iter()
        .map(|r| match r {
            PanelRow::Outgoing { title, .. } => draw::panel_line("→ ", title, panel_cols),
            PanelRow::Backlink { title, .. } => draw::panel_line("← ", title, panel_cols),
            PanelRow::Relation { text } => draw::panel_line("", text, panel_cols),
            PanelRow::DepOut { title, .. } => draw::panel_line("⇒ ", title, panel_cols),
            PanelRow::DepIn { title, .. } => draw::panel_line("⇐ ", title, panel_cols),
            PanelRow::FileDep { text } => draw::panel_line("", text, panel_cols),
            PanelRow::DepBroken { text } => draw::panel_line("✗ ", text, panel_cols),
            PanelRow::DepPending { text } => draw::panel_line("↻ ", text, panel_cols),
        })
        .collect();
    let navigable: Vec<usize> = panel_rows.iter().enumerate().filter(|(_, r)| r.target().is_some()).map(|(i, _)| i).collect();
    let panel_current_line = navigable.get(app.panel_cursor).copied();

    if let Some(row) = tree_current_row {
        app.scroll_row = draw::scroll_to_show(app.scroll_row, row..row + 1, content_rows);
    }
    if let Some(row) = panel_current_line {
        app.scroll_panel = draw::scroll_to_show(app.scroll_panel, row..row + 1, content_rows);
    }

    let tree_grid = draw::pane_grid(&tree_lines, tree_cols);
    let panel_grid = draw::pane_grid(&panel_lines, panel_cols);
    let windowed_tree = draw::window(&tree_grid, app.scroll_row, 0, content_rows, tree_cols);
    let windowed_panel = draw::window(&panel_grid, app.scroll_panel, 0, content_rows, panel_cols);
    let mut frame = draw::compose(&windowed_tree, &windowed_panel, content_rows, tree_cols, panel_cols);

    if let Some(row) = tree_current_row {
        if row >= app.scroll_row && row < app.scroll_row + content_rows {
            let style = if app.focus == Focus::Tree { draw::RowStyle::Current } else { draw::RowStyle::Secondary };
            draw::mark_row(&mut frame, row - app.scroll_row, 0, tree_cols, style);
        }
    }
    if let Some(row) = panel_current_line {
        if row >= app.scroll_panel && row < app.scroll_panel + content_rows {
            let style = if app.focus == Focus::Panel { draw::RowStyle::Current } else { draw::RowStyle::Secondary };
            draw::mark_row(&mut frame, row - app.scroll_panel, tree_cols + 1, panel_cols, style);
        }
    }

    // `help`/`filter_menu` each draw as a small box floating over the
    // tree and panel already composed above, rather than replacing
    // either -- `frame` still shows real content around the box.
    let overlay_box = if app.help {
        // Some help lines are long enough to make an unclipped box
        // wider than the terminal itself, pushing its own right border
        // off screen. Clipped to what the frame can actually fit,
        // minus the box's own four columns of border and padding.
        let max_line = term_cols.saturating_sub(4).max(1);
        let lines: Vec<String> = help_lines(&app.keys).iter().map(|l| draw::clip_with_ellipsis(l, max_line)).collect();
        Some(draw::box_grid(&lines))
    } else {
        app.filter_menu.map(filter_menu_box)
    };
    if let Some(b) = &overlay_box {
        let box_rows = b.grid.len();
        let box_cols = b.grid.first().map_or(0, Vec::len);
        let (row, col) = draw::centered(box_rows, box_cols, content_rows, term_cols);
        frame = draw::overlay(&frame, b, row, col);
    }

    let mut lines = draw::render_ansi(&frame);
    if let Some(query) = &app.search {
        lines.push(format!("/{query}").chars().take(term_cols).collect());
    } else if let Some(status) = &app.status {
        lines.push(status.chars().take(term_cols).collect());
    } else if let Some(line) = &filter_line {
        lines.push(line.chars().take(term_cols).collect());
    } else if let Some(title) = &breadcrumb_line {
        lines.push(format!("from: {title}").chars().take(term_cols).collect());
    }
    write_frame(out, &lines, term_rows)
}
```

## Tree construction

`build_children` groups the whole index by `Node.parent` once per
load/reload, rather than walking `Contains` edges: `parent` is already
exactly this, computed once by `graph::build`, and is the more direct
answer. `Relation` nodes are excluded -- they have no natural
containment parent (a relation belongs to a database, not a file) and
are never a tree row, only ever panel text.

A `Block` node's own row gets a `» ` prefix, folded into `push_row`'s
`TreeRow.title` rather than a new parameter on `draw::tree_line` --
`draw` stays exactly as graph-agnostic as its own module doc already
insists on. This never touches `Node.title` itself, only
`TreeRow.title`, so `/`-search (`jump_to_search_match`, matching
against `index.nodes` directly) never sees the glyph. `kind_marker`
matches every `NodeKind` by name rather than falling back on a
wildcard arm, the same way `NodeKind::as_str` already does -- a
future variant fails to compile here until its own marker is
decided, instead of silently rendering as unmarked as a heading.

```rust name=tree_construction path=tui/app.rs
/// Groups `index.nodes` by `.parent`, excluding `Relation` nodes.
/// Returns the map plus the top-level roots (`parent: None`) in
/// `index.nodes`' own order -- `Graph::sort`'s deterministic
/// `(file, line, id)` order, so tree row order never depends on hash
/// iteration.
fn build_children(index: &Graph) -> (HashMap<NodeId, Vec<NodeId>>, Vec<NodeId>) {
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    let mut roots = Vec::new();
    for node in &index.nodes {
        if node.kind == NodeKind::Relation {
            continue;
        }
        match &node.parent {
            Some(parent) => children.entry(parent.clone()).or_default().push(node.id.clone()),
            None => roots.push(node.id.clone()),
        }
    }
    (children, roots)
}

/// Which tree nodes should start expanded: each of `roots` opens `depth`
/// containment levels down (`u32::MAX` for "fully"); every top-level root
/// *not* in `roots` starts collapsed simply by never being visited.
fn initial_expansion(children: &HashMap<NodeId, Vec<NodeId>>, roots: &[NodeId], depth: u32) -> HashSet<NodeId> {
    let mut expanded = HashSet::new();
    let mut frontier: Vec<(NodeId, u32)> = roots.iter().map(|id| (id.clone(), 0)).collect();
    while let Some((id, level)) = frontier.pop() {
        if level >= depth {
            continue;
        }
        let Some(kids) = children.get(&id) else { continue };
        if kids.is_empty() {
            continue;
        }
        expanded.insert(id.clone());
        frontier.extend(kids.iter().map(|k| (k.clone(), level + 1)));
    }
    expanded
}

/// The glyph folded into a row's own title (`push_row`). Matches every
/// `NodeKind` by name rather than falling back on a wildcard arm, the
/// same way `NodeKind::as_str` already does -- a future variant fails
/// to compile here until its own marker is decided, instead of
/// silently rendering as unmarked as a heading. `Relation` never
/// actually reaches this (`build_children` excludes it from the
/// tree), but still gets its own arm for the same reason.
fn kind_marker(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Block => "» ",
        NodeKind::Heading => "",
        NodeKind::Relation => "",
    }
}

/// "⇒1 ⇐1 ✗1 ↻1 ▤ →2 ←1 ⚭": resolved `deps=`/`xdeps=` out and in,
/// broken and not-yet-run entries, a declared file artifact, then the
/// existing outgoing/backlink/relation-touch summary, unchanged
/// (dependency-surfacing.md §B-§E). Zero-valued pieces are omitted,
/// never printed as `⇒0`. The four dep-glyphs are a first cut, not a
/// settled choice (§7).
fn badge_for(id: &NodeId, links: &query::NodeLinks, deps: &DepData) -> String {
    let mut parts = Vec::new();
    let count = |m: &HashMap<NodeId, Vec<String>>| m.get(id).map_or(0, Vec::len);
    let out = deps.dep_out.get(id).map_or(0, Vec::len);
    let inc = deps.dep_in.get(id).map_or(0, Vec::len);
    if out > 0 {
        parts.push(format!("⇒{out}"));
    }
    if inc > 0 {
        parts.push(format!("⇐{inc}"));
    }
    let broken = count(&deps.dep_broken);
    if broken > 0 {
        parts.push(format!("✗{broken}"));
    }
    let pending = count(&deps.dep_pending);
    if pending > 0 {
        parts.push(format!("↻{pending}"));
    }
    if deps.file_deps.contains_key(id) {
        parts.push("▤".to_string());
    }
    if !links.outgoing.is_empty() {
        parts.push(format!("→{}", links.outgoing.len()));
    }
    if !links.backlinks.is_empty() {
        parts.push(format!("←{}", links.backlinks.len()));
    }
    if !links.produces.is_empty() || !links.reads.is_empty() {
        parts.push("⚭".to_string());
    }
    parts.join(" ")
}

impl App {
    /// One row per visible node, depth-first, respecting `self.expanded`
    /// -- exactly what the left pane draws this frame, absent a live
    /// preview. Rebuilt fresh every call rather than cached: cheap at
    /// this corpus's size, and simpler than keeping a second structure
    /// in sync with `expanded`.
    fn visible_rows(&self) -> Vec<TreeRow> {
        self.visible_rows_with(&self.expanded)
    }

    /// Like `visible_rows`, but against a caller-supplied expansion set
    /// rather than `self.expanded` -- what `render`'s own live preview
    /// uses to show a hovered panel link's ancestors opened for this one
    /// frame, without touching `self.expanded` at all (see *Jump and
    /// default depth*, architecture.md).
    fn visible_rows_with(&self, expanded: &HashSet<NodeId>) -> Vec<TreeRow> {
        let membership = self.filter_membership();
        let mut rows = Vec::new();
        for root in &self.roots {
            self.push_row(root, 0, expanded, membership.as_ref(), &mut rows);
        }
        rows
    }

    /// The node ids `self.filter` lets through, plus every ancestor of
    /// each match, so a reader can still see where a match lives -- the
    /// same "hide, not dim" mental model `/`-search's own ancestor-
    /// reveal already trained, as a standing state instead of a
    /// one-shot jump (dependency-surfacing.md, §3). `None` for
    /// `Filter::All`: no membership set to consult, so `push_row` skips
    /// the check entirely rather than paying for a no-op filter.
    fn filter_membership(&self) -> Option<HashSet<NodeId>> {
        if self.filter == Filter::All {
            return None;
        }
        let mut set = HashSet::new();
        for node in &self.index.nodes {
            if self.matches_filter(&node.id) {
                set.insert(node.id.clone());
                set.extend(ancestors_of(&self.index, &node.id));
            }
        }
        Some(set)
    }

    fn matches_filter(&self, id: &NodeId) -> bool {
        match self.filter {
            Filter::All => true,
            Filter::Blocks => self.index.node(id).is_some_and(|n| n.kind == NodeKind::Block),
            // "declares or is targeted" (§3's leaning): a block that
            // only ever gets named by others' `deps=`/`xdeps=` -- a
            // shared `setup`, say -- still belongs in its own filter.
            Filter::EvalChain => {
                self.deps.dep_out.contains_key(id)
                    || self.deps.dep_in.contains_key(id)
                    || self.deps.dep_broken.contains_key(id)
                    || self.deps.dep_pending.contains_key(id)
            }
            Filter::FileArtifact => self.deps.file_deps.contains_key(id),
        }
    }

    fn push_row(&self, id: &NodeId, depth: u32, expanded: &HashSet<NodeId>, filter: Option<&HashSet<NodeId>>, out: &mut Vec<TreeRow>) {
        if filter.is_some_and(|allowed| !allowed.contains(id)) {
            return;
        }
        let Some(node) = self.index.node(id) else { return };
        let kids = self.children.get(id);
        let has_children = kids.is_some_and(|k| !k.is_empty());
        let is_expanded = has_children && expanded.contains(id);
        let badge = badge_for(id, &query::links_for(&self.index, id), &self.deps);
        let title = format!("{}{}", kind_marker(node.kind), node.title);
        out.push(TreeRow { id: id.clone(), title, depth, marker: has_children.then_some(is_expanded), badge });
        if is_expanded {
            for kid in kids.unwrap() {
                self.push_row(kid, depth + 1, expanded, filter, out);
            }
        }
    }

    /// The currently hovered panel row's own target, while the panel has
    /// focus and its cursor sits on a navigable row -- what `render`'s
    /// live preview shows in the tree before `enter` makes it permanent.
    /// `None` off the panel, or on a `Relation` row with nowhere to
    /// preview.
    fn preview_target(&self) -> Option<NodeId> {
        if self.focus != Focus::Panel {
            return None;
        }
        self.panel_rows().into_iter().filter_map(|r| r.target().cloned()).nth(self.panel_cursor)
    }

    /// The selected node's own cross-references, against `self.index` --
    /// the panel describes the selection's place in the whole corpus,
    /// the same scope the tree itself always represents. Outgoing
    /// links, then backlinks, then produces, then reads, then this same
    /// node's own dependency facts from `self.deps` (dep-out, dep-in,
    /// file artifacts, broken, pending -- dependency-surfacing.md
    /// §B-§E).
    fn panel_rows(&self) -> Vec<PanelRow> {
        let links = query::links_for(&self.index, &self.selected);
        let title_of = |id: &NodeId| self.index.node(id).map(|n| n.title.clone()).unwrap_or_else(|| id.to_string());
        let mut rows = Vec::new();
        for target in &links.outgoing {
            rows.push(PanelRow::Outgoing { target: target.clone(), title: title_of(target) });
        }
        for source in &links.backlinks {
            rows.push(PanelRow::Backlink { target: source.clone(), title: title_of(source) });
        }
        for rel in &links.produces {
            rows.push(PanelRow::Relation { text: format!("produces: {}", title_of(rel)) });
        }
        for rel in &links.reads {
            rows.push(PanelRow::Relation { text: format!("reads: {}", title_of(rel)) });
        }
        for target in self.deps.dep_out.get(&self.selected).into_iter().flatten() {
            rows.push(PanelRow::DepOut { target: target.clone(), title: title_of(target) });
        }
        for source in self.deps.dep_in.get(&self.selected).into_iter().flatten() {
            rows.push(PanelRow::DepIn { target: source.clone(), title: title_of(source) });
        }
        for text in self.deps.file_deps.get(&self.selected).into_iter().flatten() {
            rows.push(PanelRow::FileDep { text: text.clone() });
        }
        for text in self.deps.dep_broken.get(&self.selected).into_iter().flatten() {
            rows.push(PanelRow::DepBroken { text: text.clone() });
        }
        for text in self.deps.dep_pending.get(&self.selected).into_iter().flatten() {
            rows.push(PanelRow::DepPending { text: text.clone() });
        }
        rows
    }
}
```

## Dependency data

`compute_dep_data` loads every corpus file a second time (`Files`,
never `index`'s own already-resolved `Document`s: `ParsedFile` keeps
none of its source around) and correlates each of
`eval::plan::top_level_blocks`'s own `BlockRef`s back to this same
node by `(file, line)`, not `(file, name)` -- names collide and get
`Slugger`-suffixed; lines don't (dependency-surfacing.md, §4). It
resolves `deps=` and non-`table:` `xdeps=` the same way, through
`plan::resolve_dep` directly rather than `plan::resolve_xdeps`: the
latter fails fast on a whole block's first broken entry, but this
wants one outcome per *declared* entry, matching one row per entry in
the panel (§B, §D). A `table:NAME` entry resolves through
`graph::query::find_producer` instead (decision 35). Either way, a
resolved `xdeps=` target that has not actually run yet -- `xdeps=`
alone never triggers a run (§E) -- is flagged through
`result::verified_hash`, the same staleness check `dankg check`
already runs, not a second one invented here.

```rust name=dep_data path=tui/app.rs
/// Every dependency-surfacing fact `App` needs, computed once at load
/// time and cached alongside `children`/`roots` -- the same staleness
/// policy as the rest of the tree (dependency-surfacing.md, §4). Empty
/// for every test built from an in-memory `Graph` (`App::from_graph`):
/// there is no real file on disk for `Files` to read there.
struct DepData {
    /// This node's own resolved `deps=`/`xdeps=` targets (§B).
    dep_out: HashMap<NodeId, Vec<NodeId>>,
    /// Other nodes whose resolved `deps=`/`xdeps=` name this one (§B).
    dep_in: HashMap<NodeId, Vec<NodeId>>,
    /// This node's own declared `deps=`/`xdeps=` entries that failed to
    /// resolve, as `PlanError`'s own `Display` text (§D).
    dep_broken: HashMap<NodeId, Vec<String>>,
    /// This node's own resolved `xdeps=` entries whose target has not
    /// actually run yet, as `eval::result`'s own error text (§E).
    dep_pending: HashMap<NodeId, Vec<String>>,
    /// This node's own declared `produces=file:PATH`/`reads=file:PATH`,
    /// raw, one row of text per entry (§C).
    file_deps: HashMap<NodeId, Vec<String>>,
}

impl DepData {
    fn empty() -> DepData {
        DepData {
            dep_out: HashMap::new(),
            dep_in: HashMap::new(),
            dep_broken: HashMap::new(),
            dep_pending: HashMap::new(),
            file_deps: HashMap::new(),
        }
    }
}

/// The read-only pieces every per-entry resolver below needs. Bundled
/// so each one takes a single reference instead of five, and so adding
/// a sixth someday touches this struct, not every call site.
struct DepCtx<'a> {
    blocks: &'a [plan::BlockRef<'a>],
    files: &'a Files,
    config: &'a Config,
    index: &'a Graph,
    node_by_line: &'a HashMap<(String, u32), NodeId>,
    index_of_node: &'a HashMap<NodeId, usize>,
}

impl DepCtx<'_> {
    /// A block's own row, by `(file, line)` -- see `compute_dep_data`'s
    /// own doc comment for why not `(file, name)`. `Node.file` and
    /// `BlockRef.file` are both the on-disk path, extension included --
    /// no `strip_extension` needed here, unlike `NodeId.file`.
    fn node_of(&self, b: &plan::BlockRef) -> Option<NodeId> {
        self.node_by_line.get(&(b.file.to_string(), b.line)).cloned()
    }
}

fn compute_dep_data(root: &Path, config: &Config, index: &Graph, corpus_paths: &[String]) -> DepData {
    let mut files = Files::new(root.to_path_buf());
    let mut diags = Diags::new("dankg");
    files.load_all(corpus_paths, &mut diags);
    let blocks = files.all_blocks();

    // `(file, line) -> NodeId`, `Block` nodes only -- the only kind
    // `top_level_blocks` ever produces, and the only kind a `deps=`/
    // `xdeps=` entry can ever legally name.
    let mut node_by_line: HashMap<(String, u32), NodeId> = HashMap::new();
    for node in &index.nodes {
        if node.kind == NodeKind::Block {
            node_by_line.insert((node.file.clone(), node.line), node.id.clone());
        }
    }

    // The reverse: a `blocks` index for a `NodeId`, needed to call
    // `result::verified_hash` on a `table:NAME` producer that
    // `graph::query::find_producer` only ever hands back as a `NodeId`.
    let index_of_node: HashMap<NodeId, usize> = blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| node_by_line.get(&(b.file.to_string(), b.line)).map(|id| (id.clone(), i)))
        .collect();

    let ctx = DepCtx { blocks: &blocks, files: &files, config, index, node_by_line: &node_by_line, index_of_node: &index_of_node };
    let mut data = DepData::empty();
    let mut hash_cache: HashMap<usize, Result<u64, String>> = HashMap::new();

    for b in &blocks {
        let Some(nid) = ctx.node_of(b) else { continue };
        for dep in &b.deps {
            resolve_plain_dep(&ctx, &nid, b, dep, &mut data);
        }
        for xdep in &b.xdeps {
            resolve_xdep(&ctx, &nid, b, xdep, &mut hash_cache, &mut data);
        }
        push_file_deps(&nid, b, &mut data);
    }
    data
}

/// One `deps=` entry: resolved links this block to its target both
/// ways (§B); unresolved records why, in `PlanError`'s own words (§D).
fn resolve_plain_dep(ctx: &DepCtx, nid: &NodeId, b: &plan::BlockRef, dep: &str, data: &mut DepData) {
    match plan::resolve_dep(ctx.blocks, b.file, dep) {
        Ok(idx) => link(nid, ctx.node_of(&ctx.blocks[idx]), data),
        Err(e) => broken(nid, plan::dep_error(b.name.to_string(), dep, e).to_string(), data),
    }
}

/// One `xdeps=` entry, block- or `table:`-targeted (decision 35). Never
/// `plan::resolve_xdeps`: that fails fast on a block's *first* broken
/// entry, but this wants one outcome per declared entry (§4, §D).
fn resolve_xdep(
    ctx: &DepCtx,
    nid: &NodeId,
    b: &plan::BlockRef,
    xdep: &str,
    hash_cache: &mut HashMap<usize, Result<u64, String>>,
    data: &mut DepData,
) {
    let Some(table_name) = xdep.strip_prefix("table:") else {
        return match plan::resolve_dep(ctx.blocks, b.file, xdep) {
            Ok(idx) => {
                link(nid, ctx.node_of(&ctx.blocks[idx]), data);
                check_pending(ctx, nid, idx, hash_cache, data);
            }
            Err(e) => broken(nid, plan::xdep_error(b.name.to_string(), xdep, e).to_string(), data),
        };
    };
    match query::find_producer(ctx.index, table_name) {
        Ok(producer_id) => {
            link(nid, Some(producer_id.clone()), data);
            if let Some(&idx) = ctx.index_of_node.get(&producer_id) {
                check_pending(ctx, nid, idx, hash_cache, data);
            }
        }
        Err(msg) => broken(nid, format!("`{}` xdeps on `table:{table_name}`: {msg}", b.name), data),
    }
}

/// Flags a resolved `xdeps=` target that has not actually run yet --
/// `xdeps=` alone never triggers a run (§E) -- reusing `dankg check`'s
/// own staleness check rather than a second copy of it.
fn check_pending(ctx: &DepCtx, nid: &NodeId, target_idx: usize, hash_cache: &mut HashMap<usize, Result<u64, String>>, data: &mut DepData) {
    let mut visiting = HashSet::new();
    if let Err(msg) = result::verified_hash(ctx.blocks, ctx.files, ctx.config, Some(ctx.index), target_idx, &mut visiting, hash_cache) {
        data.dep_pending.entry(nid.clone()).or_default().push(msg);
    }
}

fn link(nid: &NodeId, target: Option<NodeId>, data: &mut DepData) {
    let Some(tid) = target else { return };
    data.dep_out.entry(nid.clone()).or_default().push(tid.clone());
    data.dep_in.entry(tid).or_default().push(nid.clone());
}

fn broken(nid: &NodeId, msg: String, data: &mut DepData) {
    data.dep_broken.entry(nid.clone()).or_default().push(msg);
}

fn push_file_deps(nid: &NodeId, b: &plan::BlockRef, data: &mut DepData) {
    if let Some(p) = b.produces {
        data.file_deps.entry(nid.clone()).or_default().push(format!("produces: {p}"));
    }
    if let Some(r) = b.reads {
        data.file_deps.entry(nid.clone()).or_default().push(format!("reads: {r}"));
    }
}
```

## Load, reload, reset

`build` is the `dankg tui` pipeline minus rendering and tree state:
load, resolve, resolve `[tui] depth`/`keymap`. Shared by the initial
load and every reload after returning from the editor or running a
block. `resolve_entry_roots` mirrors `graph::view::select_view`'s own
"`--all`, or no named entry, means the whole index" priority, reread as
"start it open" rather than "load it": naming a directory, or passing
`--all`, opens every top-level root without limit; naming specific
files opens only those, `depth` levels down.

```rust name=build_and_entries path=tui/app.rs
fn build(paths: &[String], cache: bool) -> Result<(PathBuf, Config, Keymap, Graph, u32, Vec<String>, Vec<String>, Diags), String> {
    let mut diags = Diags::new("dankg");
    let corpus = index::load(paths, cache, &mut diags)?;
    let corpus_paths: Vec<String> = corpus.files.iter().map(|f| f.path.clone()).collect();
    let index = resolve::resolve(&corpus.files, &mut diags);
    let default_depth = corpus.config.tui_depth(&mut diags);
    let keys = corpus.config.keymap(&mut diags);
    Ok((corpus.root, corpus.config, keys, index, default_depth, corpus.entries, corpus_paths, diags))
}

/// Which of `roots` the command line named, and how many levels below
/// each should start expanded. `all`, or no specific file named
/// (`entries.is_empty()`, a directory was named), means every top-level
/// root counts as an entry, expanded without limit. Otherwise, exactly
/// the named file(s) are the entry, expanded `depth.unwrap_or
/// (default_depth)` levels.
fn resolve_entry_roots(
    index: &Graph,
    roots: &[NodeId],
    entries: &[String],
    depth: Option<u32>,
    all: bool,
    default_depth: u32,
) -> (Vec<NodeId>, u32) {
    if all || entries.is_empty() {
        return (roots.to_vec(), u32::MAX);
    }
    let entry_ids = view::entry_nodes(index, entries);
    let entry_roots = roots.iter().filter(|r| entry_ids.contains(r)).cloned().collect();
    (entry_roots, depth.unwrap_or(default_depth))
}

impl App {
    fn load(paths: &[String], cache: bool, depth: Option<u32>, all: bool) -> Result<App, String> {
        let (root, config, keys, index, default_depth, entries, corpus_paths, mut diags) = build(paths, cache)?;
        let breadcrumb = config.tui_breadcrumb(&mut diags);
        let (children, roots) = build_children(&index);
        if roots.is_empty() {
            return Err("nothing to draw: the index is empty".to_string());
        }
        let deps = compute_dep_data(&root, &config, &index, &corpus_paths);
        let (entry_roots, initial_depth) = resolve_entry_roots(&index, &roots, &entries, depth, all, default_depth);
        let expanded = initial_expansion(&children, &entry_roots, initial_depth);
        let selected = entry_roots.first().cloned().unwrap_or_else(|| roots[0].clone());
        Ok(App {
            paths: paths.to_vec(),
            cache,
            depth,
            all,
            root,
            config,
            keys,
            index,
            children,
            deps,
            roots,
            entry_roots,
            initial_depth,
            default_depth,
            expanded,
            filter: Filter::All,
            filter_menu: None,
            filter_history: HashMap::new(),
            moved_since_filter_change: false,
            selected,
            focus: Focus::Tree,
            panel_cursor: 0,
            scroll_row: 0,
            scroll_panel: 0,
            block_select: None,
            search: None,
            last_search: None,
            status: None,
            help: false,
            breadcrumb,
            diags,
        })
    }
}
```

`reload` drops every expansion rather than replaying `expanded` against
the fresh index, the same reasoning it always has: the edit that
triggered it may have changed the shape of the tree the expansion was
computed against, and replaying a stale one risks a confusing placement
more than starting clean costs a keypress. The selection survives only
if it is *actually visible* under that freshly-reset expansion, not
merely present somewhere in the new index -- `is_visible_under` walks
its whole ancestor chain, since existing behind a now-collapsed ancestor
is no more useful to the reader than not existing at all. It is also
best-effort in the other direction: a load error or an empty resulting
index leaves the previous state alone entirely, rather than dropping
into a blank or crashed session over a transient problem with the file
the reader just went and edited.

```rust name=is_visible_under path=tui/app.rs
/// Whether `id`'s whole ancestor chain (per `Node.parent`) is present in
/// `expanded` -- i.e. whether it would actually appear in
/// `App::visible_rows`, not merely exist in `index`.
fn is_visible_under(index: &Graph, id: &NodeId, expanded: &HashSet<NodeId>) -> bool {
    let mut ancestor = index.node(id).and_then(|n| n.parent.clone());
    while let Some(a) = ancestor {
        if !expanded.contains(&a) {
            return false;
        }
        ancestor = index.node(&a).and_then(|n| n.parent.clone());
    }
    true
}

/// Every ancestor of `id`, walking `Node.parent`, nearest first. Shared
/// by `App::reveal_and_select` (which needs to force them all into
/// `expanded` permanently) and `render`'s own live preview (which needs
/// the identical chain, but only for one frame -- see *Jump and default
/// depth*, architecture.md).
fn ancestors_of(index: &Graph, id: &NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut ancestor = index.node(id).and_then(|n| n.parent.clone());
    while let Some(a) = ancestor {
        ancestor = index.node(&a).and_then(|n| n.parent.clone());
        out.push(a);
    }
    out
}
```

```rust name=reload path=tui/app.rs
impl App {
    /// Re-runs `build` after returning from the editor or running a
    /// block, since the file may have just changed. The cache makes a
    /// no-op re-index cheap. Best-effort: a load error or an empty
    /// resulting index leaves the previous state alone rather than
    /// dropping into a blank or crashed session.
    ///
    /// Drops every expansion rather than replaying it against the fresh
    /// index: the edit that triggered this reload may have changed the
    /// shape of the tree the expansion was computed against, and a stale
    /// one risks a confusing placement more than starting clean costs a
    /// keypress.
    fn reload(&mut self) {
        let Ok((root, config, keys, index, default_depth, entries, corpus_paths, diags)) = build(&self.paths, self.cache) else {
            return;
        };
        self.diags.absorb(diags);
        let (children, roots) = build_children(&index);
        if roots.is_empty() {
            return;
        }
        let deps = compute_dep_data(&root, &config, &index, &corpus_paths);
        let (entry_roots, initial_depth) = resolve_entry_roots(&index, &roots, &entries, self.depth, self.all, default_depth);
        let expanded = initial_expansion(&children, &entry_roots, initial_depth);
        // Existing behind a now-collapsed ancestor is no more useful to
        // the reader than not existing at all, so "survives" means
        // actually visible under the fresh expansion, not merely present.
        let still_visible = index.contains(&self.selected) && is_visible_under(&index, &self.selected, &expanded);

        self.root = root;
        self.config = config;
        self.keys = keys;
        self.index = index;
        self.children = children;
        self.deps = deps;
        self.roots = roots;
        self.entry_roots = entry_roots;
        self.initial_depth = initial_depth;
        self.default_depth = default_depth;
        self.expanded = expanded;
        if !still_visible {
            self.selected = self.entry_roots.first().cloned().unwrap_or_else(|| self.roots[0].clone());
        }
        self.focus = Focus::Tree;
        self.panel_cursor = 0;
        self.scroll_row = 0;
        self.scroll_panel = 0;
        // A cycle belongs to the selection that was current when it
        // started. The reload that just ran may have changed the tree
        // under it. The last eval outcome (`status`), if that is what
        // triggered this reload, is left alone. The reader just evaluated
        // it. Reloading is not itself a reason to hide the result.
        self.block_select = None;
    }
}
```

```rust name=reset_selection path=tui/app.rs
impl App {
    /// `r`: per the interaction table, "collapse back to the entry view".
    /// Re-collapses everything to the initial state, not just moving the
    /// cursor.
    fn reset_selection(&mut self) {
        self.expanded = initial_expansion(&self.children, &self.entry_roots, self.initial_depth);
        self.selected = self.entry_roots.first().cloned().unwrap_or_else(|| self.roots[0].clone());
        self.focus = Focus::Tree;
        self.scroll_row = 0;
        self.scroll_panel = 0;
        // `r` is itself a kind of fresh start, not a considered "stay
        // here" choice -- the next filter change should restore
        // whatever that filter remembers, not preserve this reset.
        self.moved_since_filter_change = false;
        self.clear_transient();
    }

    /// Drops any in-progress block cycle and its status line. Called before
    /// every action that moves the selection or the tree out from under a
    /// cycle that was tied to the *previous* selection. Navigating,
    /// expanding, or resetting all count.
    fn clear_transient(&mut self) {
        self.block_select = None;
        self.status = None;
    }

    /// `f`: opens the filter-picker overlay, its cursor starting on
    /// whichever `Filter` is already active.
    fn open_filter_menu(&mut self) {
        self.clear_transient();
        self.filter_menu = Some(self.filter);
    }

    /// Up/down while the filter menu is open: moves its own cursor,
    /// never `self.filter` itself -- nothing is applied until `enter`.
    fn move_filter_menu_cursor(&mut self, delta: i32) {
        let Some(current) = self.filter_menu else { return };
        self.filter_menu = Some(if delta < 0 { current.prev() } else { current.next() });
    }

    /// `esc`, while the filter menu is open: closes it without
    /// applying anything.
    fn cancel_filter_menu(&mut self) {
        self.filter_menu = None;
    }

    /// `enter`, while the filter menu is open: applies its cursor and
    /// closes it.
    fn confirm_filter_menu(&mut self) {
        if let Some(chosen) = self.filter_menu.take() {
            self.apply_filter(chosen);
        }
    }

    /// Switches to `new_filter`, choosing `self.selected` the same way
    /// regardless of how `new_filter` was picked. Records where the
    /// reader is leaving from (`filter_history`, keyed by the *old*
    /// filter), then decides the new position: stays exactly where it
    /// is when the reader has explicitly navigated since the last
    /// filter change (`moved_since_filter_change`) and that row is
    /// still valid under `new_filter`; otherwise restores whatever
    /// `new_filter` itself last remembered, force-expanding its
    /// ancestors (`reveal_and_select`) since a plain "select if
    /// visible" could silently fail on a collapsed ancestor and strand
    /// the reader the same way the original bug did; otherwise falls
    /// back to `reselect_after_filter_change`'s own nearest-ancestor-
    /// or-first-match search, the same safety net a first-ever visit
    /// to a filter (nothing remembered yet) already needs.
    ///
    /// Even the "stays exactly where it is" case can still move the
    /// selection's own screen row. Filtering hides or reveals rows
    /// *above* it without moving `scroll_row`, so the highlighted row
    /// would otherwise jump on every filter change, even when the
    /// selection itself never changed. `anchor` captures the
    /// selection's row offset from the top of the pane, under the old
    /// filter/expansion. The end of this function re-derives
    /// `scroll_row` so that offset holds under the new one -- the
    /// reader's eye stays on the same screen row, not just on the
    /// same node.
    fn apply_filter(&mut self, new_filter: Filter) {
        let anchor =
            self.visible_rows().iter().position(|r| r.id == self.selected).map(|row| row.saturating_sub(self.scroll_row));

        self.filter_history.insert(self.filter, self.selected.clone());
        self.filter = new_filter;

        let stay = self.moved_since_filter_change && self.is_valid_under_current_filter(&self.selected);
        if !stay {
            match self.filter_history.get(&new_filter).cloned() {
                Some(id) if self.is_valid_under_current_filter(&id) => self.reveal_and_select(id),
                _ => self.reselect_after_filter_change(),
            }
        }
        self.moved_since_filter_change = false;
        self.clear_transient();

        if let Some(offset) = anchor {
            if let Some(row) = self.visible_rows().iter().position(|r| r.id == self.selected) {
                self.scroll_row = row.saturating_sub(offset);
            }
        }
    }

    /// Whether `id` both still exists and is a member of `self.filter`'s
    /// own current membership (`filter_membership`) -- `true` for
    /// every existing node under `Filter::All`, which prunes nothing.
    fn is_valid_under_current_filter(&self, id: &NodeId) -> bool {
        self.index.contains(id) && self.filter_membership().is_none_or(|m| m.contains(id))
    }

    /// After a filter change with nothing usable to restore, walks
    /// `self.selected`'s own `Node.parent` chain to the nearest
    /// ancestor the new filter still keeps -- mirroring `reload`'s own
    /// "still visible" handling, but for a filter change rather than a
    /// fresh index. Falls back to the first matching node anywhere, in
    /// `index.nodes`' own deterministic order, when nothing in that
    /// chain (not even the file root) survives either. Leaves
    /// `self.selected` untouched, same as `reload`'s own best-effort
    /// policy, only when the filter matches nothing in the whole
    /// corpus.
    fn reselect_after_filter_change(&mut self) {
        let Some(membership) = self.filter_membership() else { return }; // Filter::All: nothing to check
        if membership.contains(&self.selected) {
            return;
        }
        let mut ancestor = self.index.node(&self.selected).and_then(|n| n.parent.clone());
        while let Some(id) = ancestor {
            if membership.contains(&id) {
                self.selected = id;
                return;
            }
            ancestor = self.index.node(&id).and_then(|n| n.parent.clone());
        }
        if let Some(node) = self.index.nodes.iter().find(|n| membership.contains(&n.id)) {
            self.selected = node.id.clone();
        }
    }
}
```

## Movement, expand/collapse, focus, and search

Left/right are the standard nerdtree convention: right always reveals
more (expands a collapsed node, or steps onto its first child if it was
already open); left always reveals less (collapses an open node, or
steps onto its parent if it was already closed or has no children). Up
and down walk the flattened visible-row list, so "next"/"previous" is
well-defined without inventing a second notion of adjacency the way rank
crossing once needed one. While the panel has focus, up/down instead
move its own cursor, and left returns focus to the tree rather than
collapsing anything -- `right` is simply a no-op there, since there is
nothing further right of the panel to reveal.

```rust name=movement path=tui/app.rs
impl App {
    fn up(&mut self) {
        if self.focus == Focus::Panel {
            self.move_panel_cursor(-1);
        } else {
            self.move_tree_cursor(-1);
        }
    }

    fn down(&mut self) {
        if self.focus == Focus::Panel {
            self.move_panel_cursor(1);
        } else {
            self.move_tree_cursor(1);
        }
    }

    fn move_tree_cursor(&mut self, delta: i32) {
        let rows = self.visible_rows();
        let Some(pos) = rows.iter().position(|r| r.id == self.selected) else { return };
        let new_pos = (pos as i32 + delta).clamp(0, rows.len() as i32 - 1) as usize;
        self.selected = rows[new_pos].id.clone();
        self.moved_since_filter_change = true;
    }

    fn move_panel_cursor(&mut self, delta: i32) {
        let navigable = self.panel_rows().iter().filter(|r| r.target().is_some()).count();
        if navigable == 0 {
            return;
        }
        self.panel_cursor = (self.panel_cursor as i32 + delta).clamp(0, navigable as i32 - 1) as usize;
    }

    /// Right: expands a collapsed node in place. On one already
    /// expanded, moves onto its first child instead -- right always
    /// reveals more, never re-opens what is already open. A no-op while
    /// the panel has focus.
    fn right(&mut self) {
        if self.focus == Focus::Panel {
            return;
        }
        let Some(kids) = self.children.get(&self.selected).cloned() else { return };
        if kids.is_empty() {
            return;
        }
        if self.expanded.insert(self.selected.clone()) {
            return; // was collapsed: now open, selection stays put
        }
        self.selected = kids[0].clone();
        self.moved_since_filter_change = true;
    }

    /// Left: collapses an expanded node in place. On one already
    /// collapsed (or childless), moves onto its parent instead. While
    /// the panel has focus, left returns focus to the tree instead of
    /// collapsing anything.
    fn left(&mut self) {
        if self.focus == Focus::Panel {
            self.focus = Focus::Tree;
            return;
        }
        if self.expanded.remove(&self.selected) {
            return; // was open: now collapsed, selection stays put
        }
        if let Some(parent) = self.index.node(&self.selected).and_then(|n| n.parent.clone()) {
            self.selected = parent;
            self.moved_since_filter_change = true;
        }
    }
}
```

```rust name=focus_and_jump path=tui/app.rs
impl App {
    /// `tab`: toggles which pane owns the direction keys, enter, and
    /// esc.
    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Tree => Focus::Panel,
            Focus::Panel => Focus::Tree,
        };
        self.panel_cursor = 0;
    }

    /// `esc`/left, while the panel has focus: returns focus to the tree
    /// without acting.
    fn cancel_panel_focus(&mut self) {
        self.focus = Focus::Tree;
    }

    /// `enter`, while the panel has focus: jumps to the focused row's
    /// target, if the cursor is on a navigable one.
    fn confirm_panel_row(&mut self) {
        let targets: Vec<NodeId> = self.panel_rows().into_iter().filter_map(|r| r.target().cloned()).collect();
        if let Some(id) = targets.get(self.panel_cursor).cloned() {
            self.reveal_and_select(id);
            self.moved_since_filter_change = true;
        }
    }

    /// Makes `id` visible in the tree -- force-expanding every ancestor
    /// along its `Node.parent` chain, even ones the reader never opened
    /// -- selects it, and returns focus to the tree. Shared by
    /// `confirm_search` and `confirm_panel_row`: under a tree that
    /// always spans the whole corpus, both are asking the identical
    /// question ("bring this node on screen"), so there is exactly one
    /// implementation of it.
    fn reveal_and_select(&mut self, id: NodeId) {
        for a in ancestors_of(&self.index, &id) {
            self.expanded.insert(a);
        }
        self.selected = id;
        self.focus = Focus::Tree;
        self.panel_cursor = 0;
    }
}
```

`/`, "jump to a node by title" (architecture.md, *Terminal UI*):
`start_search` opens the typed-query buffer, `search_push`/
`search_backspace` edit it one character at a time, and `esc`/`enter`
(`cancel_search`/`confirm_search`) close it. `confirm_search` searches
the whole corpus (`self.index`) directly -- there is no separate drawn
view to try first the way there once was, since the tree already spans
everything -- and hands off to `reveal_and_select` exactly like a panel
jump does. `Relation` nodes are excluded from matches the same way they
are excluded from the tree: there is nowhere to reveal one into.

```rust name=search path=tui/app.rs
impl App {
    /// `/`: opens the typed-query buffer. Cancels any in-progress block
    /// cycle first (`clear_transient`), the same as any other action
    /// about to move the selection out from under one.
    fn start_search(&mut self) {
        self.clear_transient();
        self.search = Some(String::new());
    }

    fn search_push(&mut self, c: char) {
        if let Some(q) = &mut self.search {
            q.push(c);
        }
    }

    fn search_backspace(&mut self) {
        if let Some(q) = &mut self.search {
            q.pop();
        }
    }

    /// `esc`, while searching: cancels with no jump. The typed text is
    /// discarded, not kept for a later re-open. `last_search` is left
    /// alone: cancelling the prompt says nothing about the pattern `n`/
    /// `N` are still cycling through from an earlier confirmed search.
    fn cancel_search(&mut self) {
        self.search = None;
    }

    /// `enter`, while searching: confirms the typed text as the new
    /// search pattern and jumps to the first match at or after the
    /// current selection, wrapping -- vim's own `/pattern<enter>`
    /// behaviour, reusing the identical search-from-here logic `n`/`N`
    /// use afterward (`jump_to_search_match`).
    fn confirm_search(&mut self) {
        let query = self.search.take().unwrap_or_default();
        if query.is_empty() {
            return;
        }
        self.last_search = Some(query.to_lowercase());
        self.jump_to_search_match(1);
    }

    /// `n`: jumps to the next match of `last_search`, searching forward
    /// from the current selection and wrapping past the end.
    fn search_next(&mut self) {
        self.jump_to_search_match(1);
    }

    /// `N`: jumps to the previous match of `last_search`, searching
    /// backward from the current selection and wrapping past the start.
    /// Vim's own shift-`N`, not a DanKG-specific letter -- see
    /// architecture.md's *Interaction* section for why an earlier pass
    /// used `p` here instead, and why that turned out to be the wrong
    /// call.
    fn search_prev(&mut self) {
        self.jump_to_search_match(-1);
    }

    /// The one search-jump primitive `confirm_search`/`search_next`/
    /// `search_prev` all reduce to: every non-`Relation` node whose
    /// title contains `last_search`, then the first one whose position
    /// in `index.nodes` is past the current selection's own position in
    /// the direction `delta` names, wrapping to the far end if none is.
    /// Searching from the selection's own position, not from wherever
    /// the previous match happened to land, is what makes `n`/`N` keep
    /// working sensibly even after the reader has navigated away from
    /// the last match by hand -- the same thing vim's own `n`/`N` do
    /// relative to the cursor, not relative to the last search hit. No
    /// match (on any of the three callers) leaves the selection alone
    /// and reports so on the status line -- the same "nowhere else to
    /// report to" reasoning `enter`'s editor spawn already follows for
    /// its own best-effort failures. A successful jump reports too: its
    /// own rank in `matches`, one-based, out of the total -- there is no
    /// other way for the reader to know how many other hits `n`/`N` still
    /// have left to cycle through. The rank is always the match's plain
    /// position in corpus order, even right after a wrap; it says
    /// nothing about which direction the jump came from, the same way
    /// vim's own `n`/`N` never mark a wrap either.
    fn jump_to_search_match(&mut self, delta: i32) {
        let Some(needle) = self.last_search.clone() else { return };
        let matches: Vec<(usize, NodeId)> = self
            .index
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind != NodeKind::Relation && n.title.to_lowercase().contains(&needle))
            .map(|(i, n)| (i, n.id.clone()))
            .collect();
        if matches.is_empty() {
            self.status = Some(format!("/{needle}: not found"));
            return;
        }
        let current = self.index.nodes.iter().position(|n| n.id == self.selected);
        let next = if delta > 0 {
            current.and_then(|p| matches.iter().find(|(i, _)| *i > p)).or_else(|| matches.first())
        } else {
            current.and_then(|p| matches.iter().rev().find(|(i, _)| *i < p)).or_else(|| matches.last())
        };
        if let Some((_, id)) = next {
            let id = id.clone();
            let rank = matches.iter().position(|(_, m)| *m == id).unwrap() + 1;
            self.status = Some(format!("/{needle}: {rank} of {}", matches.len()));
            self.reveal_and_select(id);
            self.moved_since_filter_change = true;
        }
    }
}
```

`eval_key`, `cancel_block_select`, and `run_selected_block` are the
whole of `keys.eval` cycling. The first press looks up the selected
node's own named blocks (exactly `node.line..=node.end_line`, the
extent `graph::build` already computes for it) and starts cycling. A
later press just advances, wrapping. A node with no named blocks in
its section is a silent no-op, the same "nowhere to report to"
reasoning `enter`'s editor spawn already follows for its own
best-effort failures. `enter`
while cycling needs no separate confirmation, the same way `enter` needs
none before it spawns the configured editor on a plain selected node --
cycling to a block and pressing `enter` to run it already *is* the
confirmation.

```rust name=eval_cycle path=tui/app.rs
impl App {
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
        let Some(node) = self.index.node(&self.selected) else { return };
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

    fn toggle_help(&mut self) {
        self.help = !self.help;
    }

    /// `keys.breadcrumb`: on or off for the running session, regardless of
    /// `[tui] breadcrumb`'s own default. Not touched by `reload`, the same
    /// as `help` -- a reader's own runtime preference, not corpus state.
    fn toggle_breadcrumb(&mut self) {
        self.breadcrumb = !self.breadcrumb;
    }
}
```

## Tests

Two constructors exist only for tests: `from_graph` builds an `App`
whose tree spans an in-memory `Graph`, fully expanded -- the common
case; `from_graph_with_depth` lets a test start with less than
everything open, for exercising expand/collapse and jumps against a
node that starts hidden.

```rust name=test_helpers path=tui/app.rs
#[cfg(test)]
impl App {
    /// Builds an App directly from an in-memory `Graph`, opened `depth`
    /// containment levels down. Test-only.
    fn from_graph_with_depth(graph: Graph, depth: u32) -> App {
        let (children, roots) = build_children(&graph);
        let expanded = initial_expansion(&children, &roots, depth);
        let selected = roots.first().cloned().expect("test graphs are non-empty");
        App {
            paths: Vec::new(),
            cache: true,
            depth: None,
            all: false,
            root: PathBuf::new(),
            config: Config::none(),
            keys: Keymap::default(),
            index: graph,
            children,
            deps: DepData::empty(),
            entry_roots: roots.clone(),
            roots,
            initial_depth: depth,
            default_depth: crate::config::DEFAULT_TUI_DEPTH,
            expanded,
            filter: Filter::All,
            filter_menu: None,
            filter_history: HashMap::new(),
            moved_since_filter_change: false,
            selected,
            focus: Focus::Tree,
            panel_cursor: 0,
            scroll_row: 0,
            scroll_panel: 0,
            block_select: None,
            search: None,
            last_search: None,
            status: None,
            help: false,
            breadcrumb: true,
            diags: Diags::new("test"),
        }
    }

    /// Fully expanded -- the common case for tests that do not care
    /// about collapse state.
    fn from_graph(graph: Graph) -> App {
        Self::from_graph_with_depth(graph, u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::graph_of;

    fn app(files: &[(&str, &str)]) -> App {
        App::from_graph(graph_of(files))
    }

    /// An `Entry` heading containing a `Child`, with the tree starting
    /// fully collapsed -- Entry visible, Child hidden until something
    /// expands it.
    fn app_with_hidden_child() -> App {
        App::from_graph_with_depth(graph_of(&[("a.md", "# Entry\n\n## Child\n")]), 0)
    }

    fn ids(rows: &[TreeRow]) -> Vec<String> {
        rows.iter().map(|r| r.id.to_string()).collect()
    }

    #[test]
    fn every_top_level_file_becomes_a_root_even_when_not_named_on_the_command_line() {
        // The whole corpus backs the tree unconditionally: naming one
        // file no longer hides every other file's own top-level node.
        let a = App::from_graph(graph_of(&[("a.md", "# A\n"), ("b.md", "# B\n"), ("c.md", "# C\n")]));
        assert_eq!(a.roots.len(), 3);
    }

    #[test]
    fn only_the_named_entry_file_starts_expanded_every_other_root_stays_collapsed() {
        let index = graph_of(&[("a.md", "# A\n\n## Child\n"), ("b.md", "# B\n\n## Other\n")]);
        let (children, roots) = build_children(&index);
        let a_id = roots.iter().find(|r| r.file == "a").unwrap().clone();
        let entry_roots = vec![a_id.clone()];
        let expanded = initial_expansion(&children, &entry_roots, 1);
        assert!(expanded.contains(&a_id), "the named entry should start expanded");
        let b_id = roots.iter().find(|r| r.file == "b").unwrap().clone();
        assert!(!expanded.contains(&b_id), "every other root starts collapsed");
    }

    #[test]
    fn visible_rows_shows_only_the_entry_at_depth_zero() {
        let a = app_with_hidden_child();
        assert_eq!(ids(&a.visible_rows()), vec!["a#entry"]);
    }

    #[test]
    fn block_rows_get_a_kind_marker_prefix_heading_rows_do_not() {
        let a = app(&[("a.md", "# One\n\n```sh name=setup\necho hi\n```\n")]);
        let rows = a.visible_rows();
        let heading = rows.iter().find(|r| r.id.slug == "one").unwrap();
        let block = rows.iter().find(|r| r.id.slug == "setup").unwrap();
        assert_eq!(heading.title, "One");
        assert_eq!(block.title, "» setup");
    }

    #[test]
    fn right_expands_a_collapsed_node_without_moving_the_selection() {
        let mut a = app_with_hidden_child();
        let entry = a.selected.clone();
        a.right();
        assert_eq!(ids(&a.visible_rows()), vec!["a#entry", "a#child"]);
        assert_eq!(a.selected, entry, "the selection does not move on first expand");
    }

    #[test]
    fn right_on_an_already_expanded_node_steps_onto_its_first_child() {
        let mut a = app_with_hidden_child();
        a.right(); // expand
        a.right(); // already expanded: step onto the child
        assert_eq!(a.selected.slug, "child");
    }

    #[test]
    fn right_on_a_childless_node_is_a_no_op() {
        let mut a = app(&[("a.md", "# One\n")]);
        let before = a.selected.clone();
        a.right();
        assert_eq!(a.selected, before);
    }

    #[test]
    fn left_collapses_an_expanded_node_without_moving_the_selection() {
        let mut a = app_with_hidden_child();
        a.right();
        assert_eq!(a.visible_rows().len(), 2);
        a.left();
        assert_eq!(a.visible_rows().len(), 1, "collapsed back to just Entry");
        assert_eq!(a.selected.slug, "entry");
    }

    #[test]
    fn left_on_a_collapsed_or_childless_node_moves_to_its_parent() {
        let mut a = app_with_hidden_child();
        a.right(); // expand Entry
        a.right(); // step onto Child
        a.left(); // Child is collapsed/childless: moves to its parent
        assert_eq!(a.selected.slug, "entry");
    }

    #[test]
    fn left_at_a_top_level_root_with_nothing_open_is_a_no_op() {
        let mut a = app_with_hidden_child();
        let before = a.selected.clone();
        a.left();
        assert_eq!(a.selected, before);
    }

    #[test]
    fn up_and_down_move_across_visible_rows_and_clamp_at_the_ends() {
        let mut a = app(&[("a.md", "# One\n\n## Two\n\n## Three\n")]);
        assert_eq!(a.selected.slug, "one");
        a.down();
        assert_eq!(a.selected.slug, "two");
        a.down();
        assert_eq!(a.selected.slug, "three");
        a.down(); // clamps at the end
        assert_eq!(a.selected.slug, "three");
        a.up();
        a.up();
        a.up(); // clamps at the start
        assert_eq!(a.selected.slug, "one");
    }

    #[test]
    fn up_and_down_never_land_on_a_collapsed_nodes_hidden_children() {
        let mut a = app_with_hidden_child();
        a.down(); // Child is hidden: nothing to move to
        assert_eq!(a.selected.slug, "entry");
    }

    #[test]
    fn reset_selection_returns_to_the_first_entry_root_and_re_collapses() {
        let mut a = app_with_hidden_child();
        a.right();
        assert_eq!(a.visible_rows().len(), 2);
        a.reset_selection();
        assert_eq!(a.visible_rows().len(), 1, "r folds the expansion away, not just the cursor");
        assert_eq!(a.selected.slug, "entry");
    }

    #[test]
    fn tab_toggles_focus_between_tree_and_panel_and_back() {
        let mut a = app(&[("a.md", "# One\n")]);
        assert_eq!(a.focus, Focus::Tree);
        a.toggle_focus();
        assert_eq!(a.focus, Focus::Panel);
        a.toggle_focus();
        assert_eq!(a.focus, Focus::Tree);
    }

    #[test]
    fn panel_rows_lists_outgoing_backlinks_then_produces_and_reads_in_that_order() {
        let a = app(&[("a.md", "# One\n\n[to two](b.md#two)\n"), ("b.md", "# Two\n")]);
        let rows = a.panel_rows();
        assert!(matches!(rows[0], PanelRow::Outgoing { .. }));
    }

    #[test]
    fn panel_rows_is_empty_for_a_node_touching_nothing() {
        let a = app(&[("a.md", "# One\n")]);
        assert!(a.panel_rows().is_empty());
    }

    #[test]
    fn panel_cursor_skips_relation_rows_which_are_display_only() {
        // A plain link-only fixture already has no Relation rows to
        // skip; this asserts the *filter itself* rather than corpus
        // shape: move_panel_cursor only ever counts navigable rows.
        let mut a = app(&[("a.md", "# One\n\n[to two](b.md#two)\n"), ("b.md", "# Two\n")]);
        a.toggle_focus();
        assert_eq!(a.panel_rows().iter().filter(|r| r.target().is_some()).count(), 1);
        a.move_panel_cursor(5); // clamps to the one navigable row, does not panic
        assert_eq!(a.panel_cursor, 0);
    }

    #[test]
    fn enter_on_a_focused_panel_row_jumps_and_returns_focus_to_the_tree() {
        let mut a = app(&[("a.md", "# One\n\n[to two](b.md#two)\n"), ("b.md", "# Two\n")]);
        a.toggle_focus();
        a.confirm_panel_row();
        assert_eq!(a.focus, Focus::Tree);
        assert_eq!(a.selected.slug, "two");
    }

    #[test]
    fn enter_on_a_focused_panel_row_force_expands_every_ancestor_of_its_target() {
        let index = graph_of(&[("a.md", "# One\n\n[to child](b.md#child)\n"), ("b.md", "# Entry\n\n## Child\n")]);
        let mut a = App::from_graph_with_depth(index, 0); // nothing expanded to start
        a.toggle_focus();
        a.confirm_panel_row();
        assert_eq!(a.selected.slug, "child");
        assert!(ids(&a.visible_rows()).contains(&"b#child".to_string()), "Child's ancestor should now be expanded");
    }

    #[test]
    fn esc_while_the_panel_is_focused_returns_focus_without_moving_the_selection() {
        let mut a = app(&[("a.md", "# One\n\n[to two](b.md#two)\n"), ("b.md", "# Two\n")]);
        let before = a.selected.clone();
        a.toggle_focus();
        a.cancel_panel_focus();
        assert_eq!(a.focus, Focus::Tree);
        assert_eq!(a.selected, before);
    }

    #[test]
    fn left_while_the_panel_is_focused_returns_focus_without_collapsing_anything() {
        let mut a = app_with_hidden_child();
        a.right(); // Entry now expanded
        a.toggle_focus();
        a.left();
        assert_eq!(a.focus, Focus::Tree);
        assert_eq!(a.visible_rows().len(), 2, "left while panel-focused must not also collapse the tree");
    }

    #[test]
    fn hovering_a_panel_row_previews_its_target_without_selecting_it() {
        let mut a = app(&[("a.md", "# One\n\n[to two](b.md#two)\n"), ("b.md", "# Two\n")]);
        let before = a.selected.clone();
        a.toggle_focus();
        assert_eq!(a.preview_target().unwrap().slug, "two");
        assert_eq!(a.selected, before, "hovering a link must not change the real selection");
    }

    #[test]
    fn moving_the_panel_cursor_updates_which_row_is_previewed() {
        let mut a = app(&[
            ("a.md", "# One\n\n[to two](b.md#two)\n\n[to three](c.md#three)\n"),
            ("b.md", "# Two\n"),
            ("c.md", "# Three\n"),
        ]);
        a.toggle_focus();
        let first = a.preview_target().unwrap();
        a.down();
        let second = a.preview_target().unwrap();
        assert_ne!(first, second, "moving the panel cursor should move the preview to the new row's target");
    }

    #[test]
    fn render_reveals_the_previewed_targets_ancestors_without_expanding_them_for_real() {
        let index = graph_of(&[("a.md", "# One\n\n[to child](b.md#child)\n"), ("b.md", "# Entry\n\n## Child\n")]);
        let mut a = App::from_graph_with_depth(index, 0); // nothing expanded to start
        a.toggle_focus();
        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        let out = String::from_utf8(sink).unwrap();
        assert!(out.contains("Child"), "the preview should reveal Child in the tree this frame: {out:?}");
        assert!(a.expanded.is_empty(), "the preview must never mutate self.expanded permanently");
    }

    #[test]
    fn leaving_the_panel_without_enter_leaves_selection_and_expansion_unchanged() {
        let index = graph_of(&[("a.md", "# One\n\n[to child](b.md#child)\n"), ("b.md", "# Entry\n\n## Child\n")]);
        let mut a = App::from_graph_with_depth(index, 0);
        let before_selected = a.selected.clone();
        a.toggle_focus();
        assert!(a.preview_target().is_some());
        a.cancel_panel_focus();
        assert_eq!(a.selected, before_selected);
        assert!(a.expanded.is_empty(), "cancelling out of the panel must never have mutated expanded");
    }

    #[test]
    fn preview_target_is_none_when_the_panel_has_nothing_navigable() {
        let mut a = app(&[("a.md", "# One\n")]);
        a.toggle_focus();
        assert!(a.preview_target().is_none());
    }

    #[test]
    fn slash_opens_an_empty_query_and_typing_and_backspace_edit_it() {
        let mut a = app(&[("a.md", "# One\n")]);
        assert!(a.search.is_none());
        a.start_search();
        assert_eq!(a.search.as_deref(), Some(""));
        a.search_push('f');
        a.search_push('o');
        assert_eq!(a.search.as_deref(), Some("fo"));
        a.search_backspace();
        assert_eq!(a.search.as_deref(), Some("f"));
    }

    #[test]
    fn esc_cancels_search_without_moving_the_selection() {
        let mut a = app(&[("a.md", "# One\n\n## Two\n")]);
        let before = a.selected.clone();
        a.start_search();
        a.search_push('t');
        a.cancel_search();
        assert!(a.search.is_none());
        assert_eq!(a.selected, before);
    }

    #[test]
    fn search_jumps_to_a_node_by_title_case_insensitively() {
        let mut a = app(&[("a.md", "# One\n\n## Alpha\n\n## Beta\n")]);
        a.start_search();
        for c in "BETA".chars() {
            a.search_push(c);
        }
        a.confirm_search();
        assert!(a.search.is_none(), "confirming closes the query");
        assert_eq!(a.selected.slug, "beta");
    }

    #[test]
    fn search_matches_a_substring_not_just_the_whole_title() {
        let mut a = app(&[("a.md", "# One\n\n## Alphabet\n")]);
        a.start_search();
        for c in "phab".chars() {
            a.search_push(c);
        }
        a.confirm_search();
        assert_eq!(a.selected.slug, "alphabet");
    }

    #[test]
    fn search_reveals_a_match_outside_the_initially_expanded_roots() {
        let mut a = app_with_hidden_child();
        assert_eq!(a.visible_rows().len(), 1, "Child starts hidden");
        a.start_search();
        for c in "child".chars() {
            a.search_push(c);
        }
        a.confirm_search();
        assert_eq!(a.selected.slug, "child");
        assert!(ids(&a.visible_rows()).contains(&"a#child".to_string()));
    }

    #[test]
    fn search_with_no_match_reports_on_the_status_line_and_leaves_selection_alone() {
        let mut a = app(&[("a.md", "# One\n")]);
        let before = a.selected.clone();
        a.start_search();
        for c in "nope".chars() {
            a.search_push(c);
        }
        a.confirm_search();
        assert_eq!(a.selected, before);
        assert_eq!(a.status.as_deref(), Some("/nope: not found"));
    }

    fn searched(a: &mut App, query: &str) {
        a.start_search();
        for c in query.chars() {
            a.search_push(c);
        }
        a.confirm_search();
    }

    #[test]
    fn confirm_search_jumps_to_the_first_match_at_or_after_the_selection() {
        // Selection starts on One; searching "al" should land on the
        // first match forward from there, not necessarily the first
        // match in the whole corpus.
        let mut a = app(&[("a.md", "# One\n\n## Alphabet\n\n## Alphonso\n\n## Beta\n")]);
        searched(&mut a, "al");
        assert_eq!(a.selected.slug, "alphabet");
        assert_eq!(a.status.as_deref(), Some("/al: 1 of 2"));
    }

    #[test]
    fn n_cycles_to_the_next_match_and_wraps_past_the_end() {
        let mut a = app(&[("a.md", "# One\n\n## Alphabet\n\n## Alphonso\n\n## Beta\n")]);
        searched(&mut a, "al");
        assert_eq!(a.selected.slug, "alphabet");
        a.search_next();
        assert_eq!(a.selected.slug, "alphonso");
        assert_eq!(a.status.as_deref(), Some("/al: 2 of 2"));
        a.search_next(); // past the last match: wraps to the first
        assert_eq!(a.selected.slug, "alphabet");
        assert_eq!(a.status.as_deref(), Some("/al: 1 of 2"), "a wrap reports the plain rank, not that it wrapped");
    }

    #[test]
    fn shift_n_cycles_to_the_previous_match_and_wraps_past_the_start() {
        let mut a = app(&[("a.md", "# One\n\n## Alphabet\n\n## Alphonso\n\n## Beta\n")]);
        searched(&mut a, "al");
        assert_eq!(a.selected.slug, "alphabet");
        a.search_prev(); // before the first match: wraps to the last
        assert_eq!(a.selected.slug, "alphonso");
        assert_eq!(a.status.as_deref(), Some("/al: 2 of 2"));
        a.search_prev();
        assert_eq!(a.selected.slug, "alphabet");
        assert_eq!(a.status.as_deref(), Some("/al: 1 of 2"));
    }

    #[test]
    fn n_and_shift_n_search_from_the_current_selection_not_from_the_last_match() {
        // vim's own n/N search from the cursor, not from wherever the
        // last hit landed -- moving away by hand and pressing n still
        // finds the next match relative to wherever the reader actually
        // is now.
        let mut a = app(&[("a.md", "# One\n\n## Alphabet\n\n## Alphonso\n\n## Beta\n")]);
        searched(&mut a, "al");
        assert_eq!(a.selected.slug, "alphabet");
        a.down(); // onto Alphonso
        a.down(); // onto Beta, past every match
        a.search_next(); // wraps forward from Beta to the first match again
        assert_eq!(a.selected.slug, "alphabet");
    }

    #[test]
    fn n_and_shift_n_are_a_no_op_with_no_prior_search() {
        let mut a = app(&[("a.md", "# One\n\n## Two\n")]);
        let before = a.selected.clone();
        a.search_next();
        a.search_prev();
        assert_eq!(a.selected, before);
    }

    #[test]
    fn cancelling_the_search_prompt_keeps_the_last_confirmed_pattern_for_n_and_shift_n() {
        let mut a = app(&[("a.md", "# One\n\n## Alphabet\n\n## Alphonso\n\n## Beta\n")]);
        searched(&mut a, "al");
        assert_eq!(a.selected.slug, "alphabet");
        a.start_search();
        a.search_push('x');
        a.cancel_search(); // discards "x", must not touch last_search
        a.search_next();
        assert_eq!(a.selected.slug, "alphonso", "n should still cycle the earlier confirmed pattern");
    }

    #[test]
    fn jump_to_search_match_reports_its_rank_and_the_total_match_count() {
        let mut a = app(&[("a.md", "# One\n\n## Alphabet\n\n## Alphonso\n\n## Almanac\n\n## Beta\n")]);
        searched(&mut a, "al");
        assert_eq!(a.status.as_deref(), Some("/al: 1 of 3"));
        a.search_next();
        assert_eq!(a.status.as_deref(), Some("/al: 2 of 3"));
        a.search_next();
        assert_eq!(a.status.as_deref(), Some("/al: 3 of 3"));
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

    /// Writes a small corpus to a fresh temp directory and returns its
    /// root, so `build` (which reads real config off disk via
    /// `index::load`, unlike `graph_of`'s in-memory-only corpus) has
    /// something real to walk. Every call gets its own directory (a
    /// counter, not just the pid), the same reasoning `app_with_real_file`
    /// below already follows: tests run in parallel.
    fn write_corpus(files: &[(&str, &str)]) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("dankg-tui-build-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, content) in files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, content).unwrap();
        }
        dir
    }

    #[test]
    fn build_defaults_to_the_tui_specific_depth_not_graph_depth() {
        // An unset `[tui] depth` must not fall back to `[graph] depth`
        // (decision: *Terminal UI*) -- the whole point of a separate,
        // narrower default is that configuring one never silently moves
        // the other.
        let dir = write_corpus(&[(".dankg/config", "[graph]\ndepth = 5\n"), ("a.md", "# A\n\n[to b](b.md#b)\n"), ("b.md", "# B\n")]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let a = App::load(&[path], false, None, false).unwrap();
        assert_eq!(a.default_depth, crate::config::DEFAULT_TUI_DEPTH);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_honors_an_explicit_tui_depth_over_the_narrower_default() {
        let dir = write_corpus(&[
            (".dankg/config", "[tui]\ndepth = 3\n"),
            ("a.md", "# A\n\n[to b](b.md#b)\n"),
            ("b.md", "# B\n\n[to c](c.md#c)\n"),
            ("c.md", "# C\n\n[to d](d.md#d)\n"),
            ("d.md", "# D\n"),
        ]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let a = App::load(&[path], false, None, false).unwrap();
        assert_eq!(a.default_depth, 3);
        assert_eq!(a.initial_depth, 3);
        assert!(
            ids(&a.visible_rows()).contains(&"d#d".to_string()),
            "depth 3 from a should reach d, 3 hops away: {:?}",
            ids(&a.visible_rows())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_resolved_dep_links_both_nodes() {
        let dir = write_corpus(&[("a.md", "```sh name=setup\necho hi\n```\n\n```sh name=top deps=setup\necho hi\n```\n")]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let a = App::load(&[path], false, None, false).unwrap();
        let top = a.index.nodes.iter().find(|n| n.id.slug == "top").unwrap().id.clone();
        let setup = a.index.nodes.iter().find(|n| n.id.slug == "setup").unwrap().id.clone();
        assert_eq!(a.deps.dep_out.get(&top), Some(&vec![setup.clone()]));
        assert_eq!(a.deps.dep_in.get(&setup), Some(&vec![top]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unresolved_dep_is_recorded_as_broken_with_planerrors_own_wording() {
        let dir = write_corpus(&[("a.md", "```sh name=top deps=ghost\necho hi\n```\n")]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let a = App::load(&[path], false, None, false).unwrap();
        let top = a.index.nodes.iter().find(|n| n.id.slug == "top").unwrap().id.clone();
        let msgs = a.deps.dep_broken.get(&top).cloned().unwrap_or_default();
        assert_eq!(msgs, vec!["`top` depends on `ghost`, which is not a top-level named block".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_resolved_but_never_run_xdep_is_recorded_as_pending_not_broken() {
        let dir =
            write_corpus(&[("a.md", "```sh name=setup\necho hi\n```\n\n```sh name=top xdeps=setup\necho hi\n```\n")]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let a = App::load(&[path], false, None, false).unwrap();
        let top = a.index.nodes.iter().find(|n| n.id.slug == "top").unwrap().id.clone();
        assert!(a.deps.dep_broken.get(&top).is_none(), "a resolved xdep is not broken");
        let msgs = a.deps.dep_pending.get(&top).cloned().unwrap_or_default();
        assert_eq!(msgs, vec!["`setup` has no recorded result yet -- run it first".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn declared_file_artifacts_become_plain_panel_text() {
        let dir = write_corpus(&[("a.md", "```sh name=top produces=file:out.csv\necho hi\n```\n")]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let a = App::load(&[path], false, None, false).unwrap();
        let top = a.index.nodes.iter().find(|n| n.id.slug == "top").unwrap().id.clone();
        assert_eq!(a.deps.file_deps.get(&top), Some(&vec!["produces: file:out.csv".to_string()]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tree_badge_shows_dep_out_dep_in_and_file_artifact_glyphs() {
        let dir = write_corpus(&[(
            "a.md",
            "```sh name=setup produces=file:out.csv\necho hi\n```\n\n```sh name=top deps=setup\necho hi\n```\n",
        )]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let a = App::load(&[path], false, None, true).unwrap(); // all: true, fully expanded
        let rows = a.visible_rows();
        let setup = rows.iter().find(|r| r.id.slug == "setup").unwrap();
        let top = rows.iter().find(|r| r.id.slug == "top").unwrap();
        assert_eq!(setup.badge, "⇐1 ▤");
        assert_eq!(top.badge, "⇒1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tree_badge_shows_broken_and_pending_glyphs() {
        let dir = write_corpus(&[(
            "a.md",
            "```sh name=setup\necho hi\n```\n\n```sh name=top deps=ghost xdeps=setup\necho hi\n```\n",
        )]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let a = App::load(&[path], false, None, true).unwrap(); // all: true, fully expanded
        let top = a.visible_rows().into_iter().find(|r| r.id.slug == "top").unwrap();
        // deps=ghost fails to resolve (✗1); xdeps=setup resolves fine, so
        // it still counts toward ⇒N, but setup has never run (↻1) --
        // "resolved" and "not yet run" are independent, both true here.
        assert_eq!(top.badge, "⇒1 ✗1 ↻1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn panel_rows_expose_dep_out_dep_in_broken_and_pending_with_correct_navigability() {
        let dir = write_corpus(&[(
            "a.md",
            "```sh name=setup\necho hi\n```\n\n```sh name=top deps=setup xdeps=ghost\necho hi\n```\n",
        )]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let mut a = App::load(&[path], false, None, false).unwrap();
        a.selected = a.index.nodes.iter().find(|n| n.id.slug == "top").unwrap().id.clone();
        let rows = a.panel_rows();
        let dep_out = rows.iter().find(|r| matches!(r, PanelRow::DepOut { .. })).unwrap();
        assert!(dep_out.target().is_some(), "DepOut is navigable");
        let broken = rows.iter().find(|r| matches!(r, PanelRow::DepBroken { .. })).unwrap();
        assert!(broken.target().is_none(), "DepBroken is not navigable");

        a.selected = a.index.nodes.iter().find(|n| n.id.slug == "setup").unwrap().id.clone();
        let dep_in = a.panel_rows().into_iter().find(|r| matches!(r, PanelRow::DepIn { .. })).unwrap();
        assert!(dep_in.target().is_some(), "DepIn is navigable");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn filter_next_and_prev_cycle_through_every_variant_and_wrap() {
        assert_eq!(Filter::All.next(), Filter::Blocks);
        assert_eq!(Filter::Blocks.next(), Filter::EvalChain);
        assert_eq!(Filter::EvalChain.next(), Filter::FileArtifact);
        assert_eq!(Filter::FileArtifact.next(), Filter::All, "wraps forward");
        assert_eq!(Filter::All.prev(), Filter::FileArtifact, "wraps backward");
        assert_eq!(Filter::FileArtifact.prev(), Filter::EvalChain);
    }

    #[test]
    fn apply_filter_sets_the_active_filter() {
        let mut a = app(&[("a.md", "# One\n")]);
        assert_eq!(a.filter, Filter::All);
        a.apply_filter(Filter::EvalChain);
        assert_eq!(a.filter, Filter::EvalChain);
    }

    #[test]
    fn applying_a_filter_never_touches_expanded() {
        let mut a = app_with_hidden_child();
        let expanded_before = a.expanded.clone();
        a.apply_filter(Filter::Blocks);
        assert_eq!(a.expanded, expanded_before, "a filter is a standing choice, not tree-shape state");
    }

    #[test]
    fn applying_a_filter_leaves_selected_alone_when_nothing_matches_at_all() {
        // `app_with_hidden_child` has no blocks anywhere, so `Blocks`
        // membership is empty -- reselect_after_filter_change's own
        // best-effort fallback, mirroring reload's, has nothing to fall
        // back to.
        let mut a = app_with_hidden_child();
        let selected_before = a.selected.clone();
        a.apply_filter(Filter::Blocks);
        assert_eq!(a.selected, selected_before);
    }

    #[test]
    fn applying_a_filter_reselects_when_the_current_row_gets_filtered_out() {
        // The bug found by hand: select a heading with no block
        // descendant, filter to Blocks, and every arrow key used to be
        // stuck -- move_tree_cursor's own `position` lookup never found
        // a `self.selected` that visible_rows no longer produces.
        let mut a = app(&[("a.md", "# One\n\n## Two\n\n```sh name=x\n:\n```\n\n## Three\n")]);
        a.selected = a.index.nodes.iter().find(|n| n.id.slug == "three").unwrap().id.clone();
        a.apply_filter(Filter::Blocks); // Three has no block descendant
        assert_ne!(a.selected.slug, "three", "no longer pointed at a hidden row");
        assert!(a.visible_rows().iter().any(|r| r.id == a.selected), "reselected onto something visible");
        assert_eq!(a.selected.slug, "one", "One is the nearest surviving ancestor of the old selection");

        let before = a.selected.clone();
        a.down();
        assert_ne!(a.selected, before, "movement works again");
    }

    #[test]
    fn applying_a_filter_falls_back_to_the_first_match_when_no_ancestor_survives() {
        // Two unrelated top-level files: selecting a heading in the one
        // with no blocks at all, then filtering to Blocks, has no
        // surviving ancestor in that file's own chain to fall back to.
        let mut a = app(&[("a.md", "# NoBlocks\n"), ("b.md", "# HasBlock\n\n```sh name=x\n:\n```\n")]);
        a.selected = a.index.nodes.iter().find(|n| n.id.slug == "noblocks").unwrap().id.clone();
        a.apply_filter(Filter::Blocks);
        // Falls back to the first matching node anywhere, in `index.nodes`'
        // own order -- b's containing heading (an ancestor of the match)
        // comes before the block itself in that order.
        assert_eq!(a.selected.slug, "hasblock");
        assert!(a.visible_rows().iter().any(|r| r.id == a.selected));
    }

    #[test]
    fn returning_to_a_filter_with_no_manual_navigation_restores_its_remembered_position() {
        // NoBlocks has no path into Blocks' own membership at all (not
        // even as an ancestor), so leaving All genuinely relocates the
        // selection rather than trivially keeping it as a valid ancestor.
        let mut a = app(&[("a.md", "# NoBlocks\n"), ("b.md", "# HasBlock\n\n```sh name=x\n:\n```\n")]);
        a.selected = a.index.nodes.iter().find(|n| n.id.slug == "noblocks").unwrap().id.clone();
        let original = a.selected.clone();
        a.apply_filter(Filter::Blocks);
        assert_ne!(a.selected, original, "genuinely relocated, not just left in place");
        a.apply_filter(Filter::EvalChain); // no manual move under Blocks
        a.apply_filter(Filter::All); // no manual move under EvalChain either
        assert_eq!(a.selected, original, "nothing moved along the way, so All restores where it started");
    }

    #[test]
    fn navigating_under_a_filter_makes_the_next_filter_change_stay_put() {
        let mut a = app(&[("a.md", "# One\n\n```sh name=setup\n:\n```\n\n```sh name=top deps=setup\n:\n```\n")]);
        let original = a.selected.clone();
        a.apply_filter(Filter::Blocks);
        a.down(); // manual navigation -- moves onto a different block row
        let moved_to = a.selected.clone();
        assert_ne!(moved_to, original);
        a.apply_filter(Filter::All);
        assert_eq!(a.selected, moved_to, "manual navigation wins over All's own remembered position");
    }

    #[test]
    fn each_filter_remembers_its_own_last_position_independently() {
        let dir = write_corpus(&[(
            "a.md",
            "```sh name=setup produces=file:o.csv\necho hi\n```\n\n```sh name=other\necho hi\n```\n",
        )]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let mut a = App::load(&[path], false, None, true).unwrap();
        let setup = a.index.nodes.iter().find(|n| n.id.slug == "setup").unwrap().id.clone();
        let other = a.index.nodes.iter().find(|n| n.id.slug == "other").unwrap().id.clone();

        // Force a known starting point rather than assuming what the
        // corpus's own root happens to be with no heading in this
        // fixture at all.
        a.selected = setup.clone();
        a.apply_filter(Filter::FileArtifact); // records All's position as setup; setup is already valid here, so it stays
        assert_eq!(a.selected, setup);
        a.apply_filter(Filter::Blocks); // records FileArtifact's position as setup; setup is valid under Blocks too, so it stays
        assert_eq!(a.selected, setup);

        a.selected = other.clone(); // manual move, will be recorded under Blocks
        a.moved_since_filter_change = true;
        a.apply_filter(Filter::FileArtifact); // other isn't valid under FileArtifact, so history wins over "stay"
        assert_eq!(a.selected, setup, "FileArtifact still remembers its own last row, unaffected by Blocks");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn f_opens_the_filter_menu_with_its_cursor_on_the_active_filter() {
        let mut a = app(&[("a.md", "# One\n")]);
        a.filter = Filter::EvalChain;
        a.open_filter_menu();
        assert_eq!(a.filter_menu, Some(Filter::EvalChain));
        assert_eq!(a.filter, Filter::EvalChain, "opening the menu never applies anything on its own");
    }

    #[test]
    fn opening_the_filter_menu_clears_an_in_progress_eval_cycle() {
        let (mut a, _path) = app_with_real_file("a.md", "# One\n\n```sh name=x\n:\n```\n");
        a.eval_key();
        assert!(a.block_select.is_some());
        a.open_filter_menu();
        assert!(a.block_select.is_none(), "the same clear_transient discipline every other mode-entry action follows");
    }

    #[test]
    fn the_filter_menus_own_cursor_moves_independently_of_the_active_filter_and_wraps() {
        let mut a = app(&[("a.md", "# One\n")]);
        a.open_filter_menu(); // cursor starts on All
        a.move_filter_menu_cursor(1);
        assert_eq!(a.filter_menu, Some(Filter::Blocks));
        assert_eq!(a.filter, Filter::All, "the cursor moving never touches the active filter");
        a.move_filter_menu_cursor(-1);
        a.move_filter_menu_cursor(-1); // past the start: wraps to the last variant
        assert_eq!(a.filter_menu, Some(Filter::FileArtifact));
    }

    #[test]
    fn esc_closes_the_filter_menu_without_applying_its_cursor() {
        let mut a = app(&[("a.md", "# One\n")]);
        a.open_filter_menu();
        a.move_filter_menu_cursor(1); // cursor now on Blocks
        a.cancel_filter_menu();
        assert_eq!(a.filter_menu, None);
        assert_eq!(a.filter, Filter::All, "cancelling never applies the cursor's own filter");
    }

    #[test]
    fn enter_applies_the_filter_menus_cursor_and_closes_it() {
        let mut a = app(&[("a.md", "# One\n\n```sh name=x\n:\n```\n")]);
        a.open_filter_menu();
        a.move_filter_menu_cursor(1); // Blocks
        a.confirm_filter_menu();
        assert_eq!(a.filter_menu, None);
        assert_eq!(a.filter, Filter::Blocks);
    }

    #[test]
    fn render_shows_the_filter_menu_box_floating_over_the_still_visible_tree() {
        let mut a = app(&[("a.md", "# One\n")]);
        a.open_filter_menu();
        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        let out = String::from_utf8(sink).unwrap();
        assert!(out.contains("Filter"), "{out:?}");
        assert!(out.contains("eval-chain"), "{out:?}");
        assert!(out.contains('┌') && out.contains('┐'), "{out:?}");
        assert!(out.contains("One"), "the tree stays visible around the box: {out:?}");
    }

    #[test]
    fn blocks_filter_hides_headings_but_keeps_a_blocks_own_ancestor() {
        let mut a = app(&[("a.md", "# One\n\n## Two\n\n```sh name=x\n:\n```\n\n## Three\n")]);
        a.filter = Filter::Blocks;
        let ids = a.visible_rows().into_iter().map(|r| r.id.slug).collect::<Vec<_>>();
        assert!(ids.contains(&"x".to_string()), "the block itself stays: {ids:?}");
        assert!(ids.contains(&"two".to_string()), "its containing heading stays too, as an ancestor: {ids:?}");
        assert!(ids.contains(&"one".to_string()), "One is also an ancestor of x, so it stays too: {ids:?}");
        assert!(!ids.contains(&"three".to_string()), "a heading with no block descendant is hidden: {ids:?}");
    }

    #[test]
    fn eval_chain_filter_matches_a_shared_target_even_though_it_declares_nothing_of_its_own() {
        // §3's leaning: "declares or is targeted", so a block only ever
        // named by someone else's deps= (nothing of its own to declare)
        // still shows under this filter -- the leaves this filter exists
        // to reveal must not be the thing it hides.
        let dir =
            write_corpus(&[("a.md", "```sh name=setup\necho hi\n```\n\n```sh name=top deps=setup\necho hi\n```\n")]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let mut a = App::load(&[path], false, None, true).unwrap();
        a.filter = Filter::EvalChain;
        let ids = a.visible_rows().into_iter().map(|r| r.id.slug).collect::<Vec<_>>();
        assert!(ids.contains(&"setup".to_string()), "targeted-only still shows: {ids:?}");
        assert!(ids.contains(&"top".to_string()), "{ids:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_artifact_filter_keeps_only_blocks_declaring_one() {
        let dir = write_corpus(&[(
            "a.md",
            "```sh name=setup\necho hi\n```\n\n```sh name=out produces=file:o.csv\necho hi\n```\n",
        )]);
        let path = dir.join("a.md").to_string_lossy().into_owned();
        let mut a = App::load(&[path], false, None, true).unwrap();
        a.filter = Filter::FileArtifact;
        let ids = a.visible_rows().into_iter().map(|r| r.id.slug).collect::<Vec<_>>();
        assert!(ids.contains(&"out".to_string()), "{ids:?}");
        assert!(!ids.contains(&"setup".to_string()), "declares no file artifact: {ids:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_standing_filter_shows_on_the_status_line() {
        let mut a = app(&[("a.md", "# One\n")]);
        a.filter = Filter::EvalChain;
        let mut out = Vec::new();
        render(&mut a, &mut out).unwrap();
        let rendered = String::from_utf8_lossy(&out);
        assert!(rendered.contains("filter: eval-chain"), "{rendered:?}");
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
    fn help_lines_include_the_breadcrumb_key() {
        let lines = help_lines(&Keymap::default());
        assert!(
            lines.iter().any(|l| l.trim_start().starts_with("b ") && l.contains("breadcrumb")),
            "{lines:?}"
        );
    }

    #[test]
    fn render_shows_the_help_box_floating_over_the_still_visible_tree_while_help_is_on() {
        let mut a = app(&[("a.md", "# One\n\n## Two\n")]);
        a.toggle_help();
        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        let out = String::from_utf8(sink).unwrap();
        assert!(out.contains("keybindings"), "{out:?}");
        assert!(out.contains('┌') && out.contains('┐'), "the help reference draws as a boxed overlay: {out:?}");
    }

    #[test]
    fn render_scrolls_to_keep_the_selection_visible_on_a_tree_taller_than_the_terminal() {
        // Thirty independent top-level files, each its own root -- more
        // rows than fit in the 24-row fallback `render` uses when
        // `term::size()` fails, which it always will here (no real tty
        // in a test run).
        let files: Vec<(String, String)> = (0..30).map(|i| (format!("{i}.md"), format!("# F{i}\n"))).collect();
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
        let mut a = app(&refs);

        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        for _ in 0..25 {
            a.down();
            render(&mut a, &mut sink).unwrap();
            let rows = a.visible_rows();
            let row = rows.iter().position(|r| r.id == a.selected).unwrap();
            assert!(
                row >= a.scroll_row && row < a.scroll_row + 24,
                "selection at row {row} is not inside the scrolled 24-row window starting at {}",
                a.scroll_row
            );
        }
    }

    #[test]
    fn render_marks_the_focused_panes_row_current_and_the_others_secondary() {
        let mut a = app(&[("a.md", "# One\n\n[to two](b.md#two)\n"), ("b.md", "# Two\n")]);
        a.toggle_focus(); // panel now has focus
        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        let out = String::from_utf8(sink).unwrap();
        assert!(out.contains("\x1b[7m"), "the focused panel row should be reverse video: {out:?}");
        assert!(out.contains("\x1b[4m"), "the tree's own remembered row should be underlined: {out:?}");
    }

    #[test]
    fn toggle_breadcrumb_flips_the_flag() {
        let mut a = app(&[("a.md", "# One\n")]);
        assert!(a.breadcrumb, "on by default");
        a.toggle_breadcrumb();
        assert!(!a.breadcrumb);
        a.toggle_breadcrumb();
        assert!(a.breadcrumb);
    }

    #[test]
    fn render_shows_the_origin_breadcrumb_while_previewing_a_different_node_in_the_panel() {
        let mut a = app(&[("a.md", "# One\n\n[to two](b.md#two)\n"), ("b.md", "# Two\n")]);
        a.toggle_focus(); // panel now has focus, its cursor already on the one link to "Two"
        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        let out = String::from_utf8(sink).unwrap();
        assert!(out.contains("from: One"), "the origin's own title should reach the status line: {out:?}");
    }

    #[test]
    fn render_omits_the_breadcrumb_while_the_tree_has_focus() {
        let mut a = app(&[("a.md", "# One\n\n[to two](b.md#two)\n"), ("b.md", "# Two\n")]);
        // Focus stays on the tree: self.selected is already the row shown
        // current, so there is nothing for a breadcrumb to add.
        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        let out = String::from_utf8(sink).unwrap();
        assert!(!out.contains("from: One"), "{out:?}");
    }

    #[test]
    fn render_omits_the_breadcrumb_once_toggled_off() {
        let mut a = app(&[("a.md", "# One\n\n[to two](b.md#two)\n"), ("b.md", "# Two\n")]);
        a.toggle_focus();
        a.toggle_breadcrumb();
        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        let out = String::from_utf8(sink).unwrap();
        assert!(!out.contains("from: One"), "{out:?}");
    }

    #[test]
    fn render_prefers_an_eval_status_over_the_breadcrumb() {
        let mut a = app(&[("a.md", "# One\n\n[to two](b.md#two)\n"), ("b.md", "# Two\n")]);
        a.toggle_focus();
        a.status = Some("x: ok".to_string());
        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        let out = String::from_utf8(sink).unwrap();
        assert!(out.contains("x: ok"), "{out:?}");
        assert!(!out.contains("from: One"), "the status message should win the shared line: {out:?}");
    }

    #[test]
    fn render_prefers_the_search_prompt_over_the_breadcrumb() {
        let mut a = app(&[("a.md", "# One\n\n[to two](b.md#two)\n"), ("b.md", "# Two\n")]);
        a.toggle_focus();
        a.start_search();
        let mut sink = Vec::new();
        render(&mut a, &mut sink).unwrap();
        let out = String::from_utf8(sink).unwrap();
        assert!(out.contains("/"), "{out:?}");
        assert!(!out.contains("from: One"), "the search prompt should win the shared line: {out:?}");
    }
}
```
