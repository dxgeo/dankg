---
title: DanKG Architecture
author: Daniel J. Okuniewicz
---
This document records *how* DanKG is built. [project.md](project.md) records *what* it is
and why. Where the two disagree, project.md wins and this file is wrong.

# Decisions

Each row is settled. The rationale column is the reason to revisit it, not a
defence of it.

## Decision 1: Dependency policy

Zero crates, std only, forever.

**Rationale:** Project constraint. Output HTML may carry hand-written JS.

## Decision 2: Layout

Sugiyama layered, computed in Rust.

**Rationale:** Deterministic and diffable; every format gets coordinates.

## Decision 3: Markdown scope

Documented subset; spec suite vendored.

**Rationale:** Ships now, measures honestly, closes gaps opportunistically.

## Decision 4: Link syntax

`[t](f.md#h)` canonical, `[[f#h]]` also read.

**Rationale:** Files stay portable; wikilinks stay fast to type.

## Decision 5: Node identity

Every heading, any depth, plus a top-level named block.

**Rationale:** Containment and link edges are distinct edge kinds.

## Decision 6: Index scope

Whole root, always.

**Rationale:** Forced: bidirectional links need the full corpus.

## Decision 7: View scope

Entry + 2 hops, expandable in the browser.

**Rationale:** Readable at any size; index is embedded so expansion is free.

## Decision 8: Unresolved links

Placeholder node + stderr warning.

**Rationale:** Dangling links are signal. Nothing drops silently.

## Decision 9: Eval trigger

`dankg eval` only; serve mode deferred.

**Rationale:** project.md:34 -- never automatic. Static output stays static.

## Decision 10: Block dependencies

`name` and `deps` in the fence info string.

**Rationale:** Explicit DAG; info strings are ignored by other renderers.

## Decision 11: Execution model

Deps prepended into one process.

**Rationale:** No PTY, no per-language state protocol, works for compiled langs.

## Decision 12: Results

Written back into the markdown, hash-tagged.

**Rationale:** File stays the source of truth; results survive in git.

## Decision 13: CLI shape

`dankg graph --format=<fmt>`.

**Rationale:** JSON is a first-class, testable surface from day one.

## Decision 14: Frontmatter

Flat `key: value` subset only.

**Rationale:** ~80 lines; covers real usage; warns rather than guessing.

## Decision 15: Format safety

Re-parse and compare before writing.

**Rationale:** `fmt` writes in place; a round-trip bug must not reach a note.

## Decision 16: Database engine

DuckDB, spawned as a configured command.

**Rationale:** Single file, no server; decision 1 holds because nothing links.

## Decision 17: Editor integration

`[editor] command`, spawned like `[lang.*]`.

**Rationale:** No GUI/TUI toolkit; the user's editor is the buffer, always.

## Decision 18: Keybindings

`[keys]` remaps letters only; arrows fixed.

**Rationale:** Arrows are positional, not mnemonic; nothing about them to remap.

## Decision 19: Eval scope

One file; `deps`/`--all` never cross files.

**Rationale:** No cross-file DAG to walk; matches `plan.rs`' top-level-only view.

## Decision 20: Block nodes

Node scope == eval scope, exactly (decision 19).

**Rationale:** A selectable node that is not eval-able would be a dead end.

## Decision 21: Eval "all" scope

`--all` = DAG leaves only; `--each` = every block.

**Rationale:** A pure dependency needs no redundant standalone spawn just for being named.

## Decision 22: Block name scope

Whole file, flat; `deps` resolves the same way, any heading to any other.

**Rationale:** Heading-scoped uniqueness (tried, reverted) broke a plain sequence of sibling sections each depending on the last.

## Decision 23: Tangle block scope

Exactly decision 20's node scope (named, top-level).

**Rationale:** A block invisible to the graph and un-eval-able alone is a blind spot in the shipped program.

## Decision 24: Tangle placement

Heading containment + document order; `deps` not consulted.

**Rationale:** Placement is presentational structure; `deps` stays eval's own DAG concern.

## Decision 25: Command name

`dankg tangle`, not `compile`.

**Rationale:** DanKG assembles; a configured command does any actual compiling, and some languages compile nothing at all.

## Decision 26: Tangle corpus scope

One file unchanged; a directory (or several paths) walks the corpus, nesting each contributing file under its own subdirectory once more than one is involved.

**Rationale:** A crate is a whole-corpus artifact; `deps=` has no bearing since tangle never reads it.

## Decision 27: Module glue

An external, per-language, spawned `[tangle.<lang>] glue` command; never DanKG's own code.

**Rationale:** Matches the spawned-command pattern (decision 1); lets an extension do things DanKG's own code must not (read a block's source).

## Decision 28: Per-file tangle hints

`dankg.tangle.public` frontmatter, surfaced to `glue` only through a sidecar manifest.

**Rationale:** Visibility is a per-file authorial call; whether the mechanism runs at all stays root-config only (decision 27).

## Decision 29: Cross-file `deps`

`deps=other.md#name` reaches another file's top-level block; `--block`/`--all`/`--each`' own targets stay one file (decision 19 unchanged).

**Rationale:** Concatenation (decision 11) makes "the chain crosses a file" and "the chain crosses a language" the same *kind* of non-problem; only resolution needed widening, not the execution model.

## Decision 30: Manifest schema v2

Adds a `blocks` array (name, `line`, `end_line`) per file, straight off the same `BlockRef` fields `result.rs` already tracks.

**Rationale:** A richer glue script needs block-level source position to compare against, never a paragraph's own text; DanKG hands over structure, judgment stays glue's (decision 27).

# Terminology

- root :: The directory defining one knowledge base. Everything under it is in
  the corpus; everything outside is not. Never called a "vault".
- node :: One heading, at any depth, in one file.
- containment edge :: Parent heading to child heading. Derived from document
  structure, not written by hand.
- link edge :: An explicit reference written by the author.
- entry :: The file or node named on the command line. Sets the view, never the
  index.
- block :: A fenced code block carrying a `name` in its info string.

# Data model

```rust
struct Root { path: PathBuf, config: Config }

struct NodeId { file: String, slug: String }   // displays as "file#slug"

enum NodeKind { Heading, Block }  // a block is always a leaf; see decision 20

struct Node {
    id: NodeId,
    title: String,          // heading text, or a block's own `name=`
    file: String,           // relative to root
    line: u32,              // the heading's, or the block's fence, own line
    end_line: u32,          // heading: next same-or-higher heading, minus one;
                             // block: its own closing fence (md/block.rs)
    level: u8,              // 1..=6, or 0 for a synthetic file-level node;
                             // for a block, a sentinel above every real level
    parent: Option<NodeId>,
    tags: Vec<String>,      // from frontmatter, carried onto every node
    external: Vec<String>,  // absolute URLs: recorded, never graphed
    resolved: bool,         // false => placeholder from a dangling link;
                             // always true for a block
    kind: NodeKind,
}

enum EdgeKind { Contains, Link }

struct Edge {
    from: NodeId,
    to: NodeId,
    kind: EdgeKind,
    line: u32,           // where the link was written
    reciprocated: bool,  // computed; drives arrowhead rendering
}
```

Extent is a line range rather than a byte range: the parser carries line numbers
throughout, and a line range is what diagnostics and editors both want.

A file with no headings, or with links written above its first heading, gets a
synthetic level-0 node named from its frontmatter title or file name, so that
`[text](file.md)` always has something to land on.

## Block nodes

A named, top-level code block is a node too (decision 20), scoped to
`eval::plan::top_level_blocks`'s exact definition (decision 19) rather than to
"every fenced block" -- a selectable node that `dankg eval` cannot run would be
a dead end, so "is this a node" and "is this eval-able" are made the same
question by construction, not by convention. Concretely: not nested in a
list, and named (`name=` present in its info string); an unnamed block, or one
inside a list item, is invisible to the graph exactly as it already is to
eval.

`graph/build.rs` builds a block's node in the same single pass as headings and
paragraphs, attached to whichever heading is `current` at that point in the
document (or, before the first heading, the lazily-created file-level node --
the same fallback a pre-heading paragraph already triggers). A block never
becomes `current` itself: nothing nests inside one, so later content keeps
attaching to whichever heading was already open. Its slug comes from the same
per-file `Slugger` a heading's does, so a block named `index` colliding with a
heading titled "Index" gets the `-1` suffix exactly as two same-titled
headings would.

`level` carries no heading depth for a block -- there is none -- only a
sentinel (`BLOCK_LEVEL = 7`) picked to sit above every real heading level
(1..=6). That single fact is what keeps `set_extents` correct without a
special case in its search: scanning past a block while looking for a
heading's own next sibling-or-higher never mistakes one for it, since a
block's level can never satisfy `<= a real heading's`. Overwriting a block's
own end_line the way a heading's is computed would be wrong regardless --
`Block::Code::end_line`, straight from the parser, is already exactly right --
so `set_extents`'s outer loop is scoped to `kind == Heading` nodes, full stop;
by the time it runs, no block node exists yet to touch anyway, since block
nodes are appended to the same pass that produces headings, and `set_extents`
runs once, after.

Edges are stored directed. `reciprocated` is filled in after the whole corpus is
indexed: if `a -> b` and `b -> a` both exist, both are marked, and the renderer
draws one undirected edge instead of two arrows. This is the mechanism behind
"the link goes both ways but displays as unidirectional unless linked back".

# Pipeline

```
  dankg graph notes/project.md
        |
        v
  1. discover root      walk up for .dankg/ ; else the file's own directory
  2. walk corpus        every *.md under root, honouring .dankgignore
  3. parse              frontmatter -> blocks -> inline  (per file, cacheable)
  4. index              headings -> nodes, structure -> containment edges
  5. resolve            link targets -> NodeId, or placeholder + warning
  6. reciprocate        mark mutual link edges
        |
        +--> full graph (this is what --format=json emits, schema version 1)
        |
  7. select view        entry + N hops, both edge kinds
  8. layout             Sugiyama over the selected subgraph
  9. render             html | json | dot | mermaid
```

Steps 1-6 are the index and never depend on the entry file. Steps 7-9 are the
view and are cheap to recompute.

# Module layout

```
src/
  main.rs            dispatch
  cli.rs             hand-rolled arg parsing
  config.rs          .dankg/config reader
  diag.rs            stderr diagnostics, counted and summarised
  hash.rs            FNV-1a, for cache keys and the config stamp
  md/
    mod.rs           Document = frontmatter + blocks + inlines
    frontmatter.rs   flat key: value subset
    block.rs         headings, fences, lists, paragraphs
    inline.rs        links, wikilinks, emphasis, code spans
    fmt.rs           AST -> normal-form markdown, and the write guard
  graph/
    slug.rs          heading -> anchor, collision suffixes
    model.rs         Node, Edge, Graph
    build.rs         document -> nodes, containment edges, raw links
    index.rs         root discovery, corpus walk, orchestration
    ignore.rs        .dankgignore matching
    cache.rs         per-file index cache
    resolve.rs       link resolution, reciprocation
    view.rs          entry + N hops -> the subgraph to draw
  layout/
    mod.rs           Sugiyama driver
    acyclic.rs       cycle breaking (DFS back-edge reversal)
    rank.rs          longest-path layering + virtual nodes
    order.rs         crossing reduction (median heuristic, 4 sweeps)
    coord.rs         x-assignment (priority method), edge splines
  render/
    json.rs          canonical graph dump
    dot.rs           graphviz
    mermaid.rs       flowchart
    html.rs          single self-contained file
    assets.rs        CSS + JS as const &str, inlined at build
  eval/
    plan.rs          dep DAG, topological order, "what will run" report
    files.rs         loads the (usually zero) extra files a cross-file
                     deps= chain reaches -- never a corpus walk
    run.rs           process spawn, timeout, output capture
    result.rs        hashing, write-back, staleness detection
  tangle.rs          heading -> file placement, tree assembly, build spawn
```

# Markdown subset

Implemented: ATX headings, fenced code with info strings, inline and reference
links, wikilinks, unordered and ordered lists including nesting, emphasis and
strong, code spans, paragraphs, thematic breaks, hard breaks.

Passed through as literal text: setext headings, HTML blocks, tables, block
quotes, indented code blocks, entity references, autolinks, images, and link
reference definitions.

Link titles are parsed and kept even though DanKG has no use for them. An AST
that discards input it has already read cannot be rendered faithfully, and the
field costs nothing.

The vendored CommonMark `spec.json` lives at `tests/data/commonmark/spec.json`.
It is test *data*, not a dependency. Scoring it requires rendering the AST to
HTML, so `tests/support/html.rs` implements a CommonMark HTML renderer used
*only* as a conformance oracle -- DanKG itself never renders markdown to HTML,
it renders a graph.

The harness is a regression gate: unimplemented sections never fail a build, but
a section that loses ground does.

```
cargo test --test commonmark -- --nocapture   # full per-section table
DANKG_SHOW=Links cargo test --test commonmark -- --nocapture
DANKG_BLESS=1 cargo test --test commonmark    # re-record the baseline
```

Current conformance is 368/652 (56%). The implemented sections score as
intended -- emphasis 94%, ATX headings 94%, code spans 90%, fenced code 89% --
and nearly all remaining failures are constructs deliberately outside the
subset. Notably, most list failures are not list bugs: the list structure is
correct and the case fails only because it also contains an indented code block
or a block quote.

## Fence info strings

````
```python name=index deps=setup,fetch timeout=30
```
````

The first word is the language. Remaining `key=value` pairs are DanKG metadata.
Every other markdown renderer ignores everything after the language, so files
stay portable. Unknown keys warn and are ignored.

# Slugs and node identity

GitHub-compatible: lowercase via `char::to_lowercase`, drop anything that is not
`char::is_alphanumeric` or `-` or `_`, collapse whitespace runs to a single `-`.
std is Unicode-aware for both, so non-ASCII headings work without tables.

Collisions within one file take a `-1`, `-2` suffix in document order, matching
GitHub. `NodeId` is `<file path relative to root, extension stripped>#<slug>`.

# Root discovery and the corpus walk

The root is the single most load-bearing value in the program. Node identity is
taken relative to it, so output is the same wherever the binary was run from,
and the resolver refuses any link that climbs above it.

Discovery walks up from the named path looking for a `.dankg/` directory; the
directory containing it is the root. Failing that the root is the common
ancestor of the paths given, which for a single file is its own directory. A
`.dankg/` is never created: a directory the user did not mark is not a root, it
is just where a file happens to live.

Naming a second path in a different root is refused. Cross-root graphs would
need a second boundary, and one boundary is the point.

The walk covers the whole root, always (decision 6), and yields root-relative
`/`-separated paths in sorted order -- filesystem order is not deterministic and
would leak into the output. Three exclusions are structural rather than
configured:

- Dot-entries are skipped outright, which is what keeps `.dankg/` and `.git/`
  out of the corpus without anyone writing a pattern for them.
- Symlinks are never followed. They are the one way a walk could leave the
  root, and the root is a hard boundary. Every skipped link is reported.
- Paths are resolved lexically, never with `canonicalize`, for the same reason:
  resolving a link before the boundary check is what would defeat it.

A path named on the command line is always indexed, even when `.dankgignore`
excludes it, with a warning saying so. Naming a file is an explicit request,
and it should not fail silently -- but neither should the override be invisible.

## .dankgignore

One pattern per line; `#` comments; `!` un-ignores; a leading `/` anchors to the
root; a trailing `/` matches directories only; `*` and `?` stay within one path
segment and `**` crosses them. Later rules win, so an exception can follow the
rule it excepts. A pattern with no `/` applies at every depth.

Deliberately a subset of gitignore rather than a clone of it. The whole point
of a root is that its contents are predictable, and a matcher nobody can
predict would undo that.

# Link resolution

- `[t](#heading)` -- Slug within the current file.
- `[t](other.md#heading)` -- Path relative to the current file, then slug.
- `[t](other.md)` -- That file's first heading; else a file-level node.
- `[[Heading]]` -- Slug search across the root; ambiguity warns and picks the lexicographically first path.
- `[[other#Heading]]` -- Filename stem search across the root, then slug.
- `[t](https://...)` -- External. Recorded on the node, never a graph node.
- Anything escaping the root -- Refused, with a warning. The root is a hard boundary.

The escape rule matters because "an LLM can run it for you" means the markdown
being graphed is frequently untrusted input.

# View selection

Step 7, and the first thing in the pipeline that depends on the entry. The
index is the whole root; the view is what a reader can take in.

Hops are counted in *both* directions. A link pointing at the entry is as much
a neighbour as one the entry points at -- which is the entire reason the index
is built over the corpus rather than over the one file. Containment counts as a
hop too, which is the open question below: it can make depth 2 feel shallow in
a deeply nested file, and `--all` is the answer until something better is
decided.

The result is the *induced* subgraph, not a spanning tree: an edge between two
nodes that both made it in survives even when it was not the edge that brought
either of them there. Dropping it would draw a graph missing structure it can
plainly see.

Naming a file selects every heading in it, not just the first. Naming a
directory leaves no entry to start from, and the only sensible reading of
"graph this corpus" is all of it, so that is what it does.

# Layout

Standard four-phase Sugiyama over the selected subgraph.

1. *Acyclic* -- DFS, reverse back edges, remember them so arrowheads still point
   the original way. Nothing is dropped: a self-link is marked instead, since
   it cannot be layered, and is drawn as a loop.
2. *Rank* -- longest-path layering. Edges spanning more than one layer are split
   into one-layer segments joined by virtual nodes, which become the bend
   points of the drawn polyline.
3. *Order* -- median heuristic, four alternating sweeps, keeping the best
   ordering seen. Ties broken by NodeId so the result is deterministic.
4. *Coordinate* -- priority method for x, fixed layer height for y. Edges through
   virtual nodes become polylines.

Containment edges carry weight 2 and link edges weight 1. The weight is what
the *ordering and coordinate* phases read, not the ranking: longest-path
layering already puts a child one layer below its parent, and what weight 2
buys is horizontal -- a parent is pulled into line with its children rather
than with whatever else happens to link to it. That is the sense in which a
heading sits directly above them.

Priority in phase 4 goes to the virtual nodes, ahead of every real one. A long
edge that zigzags is far harder to follow than a box sitting slightly
off-centre, so the bend points get the position they ask for and the real nodes
move aside. Ordering is never changed here -- a node may slide within its layer
but can never overtake a neighbour, so phase 3's crossing count survives.

Coordinates are integers. Whole pixels have no float formatting to disagree
about, and the output is meant to be committed.

Determinism is a hard requirement, not a nicety: it is what lets a rendered
graph be committed and diffed. Nothing here iterates a hash map, the starting
order comes from a depth-first walk in index order rather than from whatever
the graph handed over, and there are tests asserting byte-identical output
across repeated runs and across the order the corpus was read in.

# Drawn formats

Both take the same `Layout`; what they can do with it differs, and pretending
otherwise would be the dishonest part.

- *dot* -- node positions are emitted as pinned `pos` attributes, so
  `neato -n -Tsvg` reproduces DanKG's layout exactly. Plain `dot -Tsvg` throws
  them away and re-lays the graph out, but the `rank=same` groups and
  `weight=2` on containment mean it still agrees about which node belongs on
  which layer. Edge geometry is deliberately *not* emitted: graphviz wants
  B-spline control points, DanKG has polylines, and converting between them to
  satisfy a flag most people will not pass is not worth a bug in the
  arithmetic.
- *mermaid* -- runs its own layout and will not take coordinates at all. What
  it takes is the ordering: nodes are emitted rank by rank, left to right, and
  edge direction is DanKG's, including which edge of a cycle was turned around.
  Identifiers are `n0`, `n1`, ... because mermaid's identifier grammar does not
  admit `#` or `/`.

A reciprocated pair is one undirected line in both, and the half that gets
drawn is the one the layout ran *down* the page -- so a renderer doing its own
layering ends up agreeing with DanKG about which node sits above which.
Unresolved nodes are dashed and muted in both. A block node (decision 20)
gets a tint and a monospace label in `dot` (`fillcolor`/`fontname`, since
`resolved` is always true for one, the two states never fight for the same
box) and its own `classDef` in `mermaid`, collected into a `class` line the
same way dangling nodes already are -- distinct from an ordinary heading's,
never dashed, since a block is never unresolved.

# HTML renderer

One file. CSS and JS are `const &str` in `assets.rs`, inlined at render time. No
network requests, no build step, no server. The only URL in the page is the SVG
namespace, which is an identifier rather than an address, and a test asserts
there is never a second one.

The Rust side emits final SVG coordinates. The JS does three things only: pan
and zoom, click a node to expand its hidden neighbours, and click through to
the source file. Expansion is instant because the full index ships in the page
as a JSON blob alongside the visible subgraph -- and it is the canonical
`--format json` dump rather than a bespoke shape, so the page and the
scriptable surface cannot disagree about what the graph is.

Unresolved nodes render dashed and muted. A block node (decision 20) renders
with a `.node.block` class -- a tint and a monospace label, distinct from an
ordinary heading's, both server-rendered and script-grown alike, since the
grown path reads `data.kind` straight out of the same embedded index the
server-rendered path reads `Node.kind` from. Reciprocated edges render as a
single line with no arrowhead. Stale results render with a warning badge.

## Expansion is a placement, not a second layout

Clicking a node reveals its hidden neighbours from the blob. They are dropped
into the nearest free slot on the rank the edge puts them, on the same grid
`layout/` already fixed -- not laid out again. Running Sugiyama in the browser
would move every box on screen, which is exactly what a reader tracing one link
does not want, and it would need a second layout engine in a second language.

The price is that the expanded drawing is not the drawing `--depth N+1` would
produce, so grown boxes are drawn as provisional rather than passed off as
authoritative. Re-running DanKG is how you get the real layout of the larger
graph, and `reset` returns the page to exactly what Rust drew.

## One click never means two things

The box is the expand toggle, so opening the source file needs its own target:
a small badge on the box's top-right corner, in the gap the layout already
leaves between boxes, costing the label no characters.

The `href` is the root-relative path, which means the page works where it is
meant to live -- at the root. An absolute path would work from anywhere and
could not be committed, and committing the output is the point.

## What the script is told rather than trusted to know

Anything both halves need is sent in the meta blob: the box metrics
(`CHAR_WIDTH` and friends, so an expanded box is sized the way the layout sized
the drawn ones) and the edge-key separator. The rule is that a value used by
both languages lives in Rust and travels, because nothing type-checks the two
against each other. See the implementation note below for what it cost to learn
that.

# Terminal UI

Milestone 7, placed before eval: it depends only on `graph/` and `layout/`
(milestones 2 and 5), not on anything eval or the database milestone add, so
there is no ordering reason to wait.

Selecting a node and handing it to the reader's own editor only works
unconditionally from inside a terminal. A browser click cannot spawn an
arbitrary local process, and only editors that register an OS URI scheme
(`vscode://`) can be reached from HTML at all -- `vim`, `nvim`, and `emacs` are
structurally unreachable from a page. A TUI lives in the same terminal as the
editor it hands off to, so any configured `[editor] command` (decision 17)
reaches it, no scheme required.

## What is reused unchanged

`graph/` and `layout/` do not change. The TUI consumes the same `Layout` the
HTML renderer does, over the same selected view (decision 7). One piece of the
existing box-metrics work already speaks the TUI's language: `CHAR_WIDTH` and
friends are computed in character units so an HTML-expanded box matches the
layout's sizing -- a terminal cell *is* that unit, so drawing the same
Sugiyama output as text needs no rescaling.

## What is new

```
src/tui/
  term.rs     raw mode, alternate screen, size query; a Drop guard restores
              the terminal on panic so a crash never leaves a broken shell
  input.rs    hand-parsed ANSI escape sequences -> key events
  draw.rs     Layout -> character grid, box-drawing glyphs for nodes and
              polylines; one buffered write per frame, no diffing
  app.rs      event loop, selection state, viewport scroll, status line
  editor.rs   suspend term.rs, spawn [editor] command with {file}/{line},
              wait, resume, re-index (the file may have just changed)
  eval.rs     find a node's named blocks, run one via eval::session
              (milestone 8) without leaving the graph view
  expand.rs   tab-to-expand: hidden-neighbour lookup and nearest-free-slot
              placement, mirroring the HTML renderer's script
```

## Interaction

- `arrows/hjkl` -- move selection; up/down cross ranks, left/right stay in one
- `enter` -- suspend, spawn the configured editor at the node's line, resume -- or, while cycling a node's blocks, run the cycled one
- `tab` -- expand the selected node's hidden neighbours -- a placement on the existing grid, not a second layout, same rule as HTML
- `/` -- jump to a node by title
- `e` -- cycle the selected node's named blocks; enter runs the cycled one, in place, without leaving the graph (see *Eval* below)
- `esc` -- cancel an in-progress block cycle; otherwise unbound
- `p` -- toggle panning: direction keys move the viewport, not selection
- `r` -- collapse back to the entry view
- `q` -- quit, restoring the terminal
- `?` -- toggle a full-screen keybinding reference

Movement follows the rank/order structure `layout/order.rs` already computed,
so "down" is well-defined without inventing a second notion of adjacency.

Every letter here but the mode-independent bindings above is remappable in
`[keys]` (decision 18); arrows, enter, tab, esc and `?` are not, since they
are not graph-navigation letters to begin with -- `?` specifically because it
is close to universal for "help" across terminal tools (vim, htop, git) and
is not itself a graph action.

## Panning

`p` repurposes the direction keys from moving the selection to moving the
viewport directly (`App::pan`), for surveying a region of the graph with
nothing selected nearby -- the gap the *Open questions* below used to name.
A status line exists now (see *Eval* below), but panning still says nothing
in words there -- toggling pan on and off is exactly the kind of thing that
happens on nearly every keypress while surveying a graph, and a status line
that changed that often would be more noise than signal. The indicator stays
visual instead: whichever screen edges still have grid beyond them draw an
arrow (`draw::overlay_pan_arrows`), stamped onto the already-windowed frame
after scrolling, not into the full grid before it, so the glyphs sit at the
real screen edges wherever the viewport currently is. An edge with nothing
further to pan into simply grows no arrow, which makes the indicator double
as feedback: panned all the way down, `↓` stops appearing.

`render` stops calling `scroll_to_show` while panning is on, which is what
lets the viewport actually separate from the selection -- otherwise the very
next frame would snap the scroll straight back to wherever the (unmoved)
selection sits. `pan` clamps only the near end, at zero, because that is all
it can know; the far end -- not scrolling past the last row or column of
content -- is clamped in `render`, the one place that already has both the
terminal size and `draw::dimensions`'s full-grid extent in hand. Toggling
panning back off needs no explicit re-clamp of its own: the very next frame
resumes calling `scroll_to_show`, which snaps the viewport back onto the
selection the same way any other selection move would.

## Eval in the TUI

Milestone 8 built `eval/` as a library, not a `main.rs` orchestration function
like `graph`/`fmt`, specifically so this could reuse it: `eval::session::run_one`
\-- rebuild a named block's plan, resolve its language, spawn its chain once,
write the result back -- is now the one place "run this block" is
implemented, called both by `dankg eval`'s own multi-target loop and by
`tui::eval::run`. The TUI cannot depend on the `main` binary, so a function
two different front ends both need has to live in the library either way;
this is the same reasoning as `cmd.rs` already being shared between the
editor handoff and `eval`'s own command spawning.

`e` (`keys.eval`) looks up the selected node's named blocks -- every
top-level block whose own line falls in `node.line..=node.end_line`, exactly
the section `graph/build.rs` already computes that heading to span -- and
starts cycling on the first one. A second press advances to the next,
wrapping; `enter` runs whichever is currently cycled, exactly the way `enter`
already runs the editor, except it never leaves the TUI: `eval::run` spawns
and captures output through pipes (`eval/run.rs`), not through inherited
stdio, so nothing about it needs the terminal suspended. No separate confirm
prompt either -- cycling to a block and pressing `enter` to run it already
*is* the confirmation, the same reasoning decision 9's `dankg eval` prompt
does not apply to `enter`'s editor handoff.

A node with no named blocks in its section is a silent no-op on `e`: there is
nowhere to cycle to, the same "nowhere to report to" call `enter`'s
best-effort editor-spawn failure already makes. Navigating away (any
direction key, `tab`, `r`) cancels an in-progress cycle -- it was scoped to
whichever node was selected when it started, and moving off that node makes
it stale. `esc` cancels it explicitly, without moving anything.

### The status line

One row, reserved at the bottom of the viewport whenever `app.status` is
`Some`, so the graph's own content never has to reflow around it -- `render`
computes `content_rows = term_rows - 1` up front and windows the graph into
that, the same fixed-upper-bound reasoning as everywhere else column/row
budgets get clamped in this module. It shows the block-cycle list while
cycling (`eval: [setup] index   enter=run esc=cancel`, the cycled name
bracketed) and the last run's outcome afterward (`index: ok`, `index: failed`, `index: timed out`, or the error text for something that could not
even be attempted, such as an unconfigured language). `reload` -- which a
completed run always triggers, since the file just changed -- leaves
`status` alone on purpose: the reader just ran the block and reloading is not
itself a reason to hide what happened.

### Block nodes and the cycle key, side by side

Decision 20 made a named top-level block a node in its own right, drawn with
a heavy border (`draw_box`'s third glyph set, alongside plain and dashed --
`┏━┓┃┗━┛`) wherever it already sits in the graph -- reachable by ordinary
arrow/hjkl navigation, not only by `e`. This was added after `e`'s cycle mode
already existed, deliberately left standing rather than replaced: cycling
answers "what can I run from here" without moving the selection or the
viewport at all, which staying on the current node and pressing `e`
repeatedly still does more directly than navigating to a block node instead.
Both paths end at the same call (`eval::run`, `run_selected_block`), so they
cannot disagree about what running a block does -- only about how a reader
gets there.

### Discoverability outside the TUI

`dankg eval <path>... --list` (`eval::session::list_blocks`/`list_corpus_text`)
answers the same question a CI script or a reader without a terminal needs
answered: every top-level named block, its language, source line, containing
heading (the nearest heading at or above the block's own line -- a cheap
approximation of the same containment `graph/build.rs` computes properly for
the node itself, without needing the whole graph pipeline just to answer
"what can I run here"), and whether that language is configured at all. It
runs nothing and needs no confirmation, the same as `--format json` being the
graph's own pipeable, "requested output" surface (decision 13's reasoning
applied to eval).

Unlike `--block`/`--all`, scoped to exactly one file because `deps=` only
resolves within one (decision 19), `--list` has no execution to scope: a
named file lists just its own blocks, a directory (or several paths, or
nothing -- defaulting to `.`) walks the whole corpus via `index::load` the
same way `graph`/`index`/`check` already do (decision 6) and lists every
file's, each line still prefixed by its own root-relative path.

## Help screen

`?` is fixed, not remappable (see *Interaction* above), and toggles a
full-screen keybinding reference (`app::help_lines`) that *replaces* the
graph rather than overlaying it -- there is no compositing in this module,
and full-screen takeover is exactly what `enter`'s editor handoff already
does for the same reason. It reads `keys` live, so a remapped letter shows up
correctly rather than the reference silently going stale next to a config
that no longer matches it.

Help mode is fully modal in the event loop: every key but the dismissers
(`?`, esc, `keys.quit`) is swallowed before it reaches the graph's own match
arms, so nothing about the selection, panning, or an in-progress eval cycle
can change while help is on screen. `write_frame` -- the buffered-write,
no-trailing-`\r\n`-on-the-last-line logic decision-critical to not scrolling
the alternate screen (see the Terminal UI intro) -- is shared between the
graph frame and the help screen, the only two things this module ever draws;
it deliberately does not clip columns itself, since a graph line carries
ANSI dimming codes that count as characters but not screen columns, and
column-clipping those would cut one off mid-escape-sequence. Plain-text
callers (the status line, help's own lines) clip themselves before handing
`write_frame` anything.

### A byte lost after a standalone Esc

Binding `esc` to something real surfaced a latent bug in `input.rs` that
nothing had ever exercised: \[`decode`\] tells a lone Esc apart from
`ESC [ <letter>` by reading one more byte, and when that byte is not `[`, it
correctly reports using only 1 of the 2 bytes it looked at -- but
`read_key`'s loop discarded its whole buffer between calls, so that second,
unused byte -- the start of whatever the reader actually typed *next* --
simply vanished. In a debug build this tripped a `debug_assert_eq!` the loop
carried for exactly this invariant; in a release build there was no assert
to catch it, so the keystroke right after every standalone Esc was silently
eaten and the reader had to press it twice. Nothing before this session ever
bound standalone Esc to anything, so the path was real but unreachable by
any interaction table entry -- true "documented gap" until eval's cycle mode
gave Esc a job and made it load-bearing.

The fix threads a small `pending: Vec<u8>` through `read_key` across calls,
owned by `event_loop`: a byte `decode` reports as unused is carried into the
next call instead of discarded, decoded first (before any new read, so a
`pending` buffer that already resolves to a full key never blocks trying to
read more), and drained back out to whatever is left over each time. Found by
driving the real binary through a pty, not by the unit tests alone -- the
existing suite only ever fed `decode` and `read_key` complete, single
sequences in one shot, never a standalone Esc immediately followed by
another real keystroke in the same read.

## Tab-to-expand

`expand::expand_view` mirrors the HTML renderer's `freeSlot`/`expand`
(`render/assets.rs`) -- a revealed node drops into the nearest free slot on
the rank its edge puts it on, never a second Sugiyama pass, the same
reasoning as "Expansion is a placement, not a second layout" above. It
cannot reuse that code: one edits a DOM incrementally, this rebuilds the
whole `Layout` from scratch on every toggle, which the TUI can afford
because `draw.rs` already redraws the whole grid every frame with no
diffing (see its own header comment).

Rebuilding from scratch also means collapse needs no ownership
bookkeeping: an anchor no longer reachable -- because whatever revealed
it was itself just collapsed -- is silently skipped when `expanded` is
replayed, so whatever it had revealed folds away too. The HTML renderer's
`collapse` has to reassign or recursively remove nodes another expansion
might still reach; here that behaviour falls out of the rebuild for free.

A hidden neighbour can sit above the base view's rank 0 as easily as
below it -- an anchor's *incoming* edge from something the view never
reached. `LaidNode.rank` is unsigned, so revealed nodes are tracked with a
signed rank internally and the whole set is renumbered from its minimum
before becoming a `Layout`: every base node shifts down uniformly to make
room, rather than being re-laid out.

`r` ("collapse back to the entry view") clears every expansion, not just
the cursor. Returning from the editor (`enter`) clears it too, rather than
replaying `expanded` against the freshly re-read graph: the edit that
triggered the reload may have changed the shape of the graph the anchors
were computed against, and a stale expansion risks a confusing placement
more than starting clean costs a keypress.

## Scope decisions this would actually need

- *Platform*: raw mode is POSIX termios (`ioctl`, no crate) on Linux/macOS.
  Windows needs the separate Console API. Shipping Unix-only first and calling
  Windows a follow-up matches how the markdown subset shipped (decision 3): a
  documented gap beats a blocked release.
- *Redraw strategy*: one full buffered redraw per frame, not a diffed one.
  DanKG's graphs are the size of a knowledge-base neighbourhood, not a video
  frame; diffing is complexity bought for a problem this doesn't have.
- *Color*: 16-color ANSI, no truecolor assumption. Unlike the drawn formats,
  nothing here is committed or diffed, so determinism does not apply, but
  portability across terminals still does.
- *Additive, not a replacement.* HTML stays the committable, server-free
  artifact (decision 9); the TUI is a live session with no output to commit.
  Losing either would lose what the other is for.

## Scrolling

A drawn grid is almost always bigger than the terminal -- even a modest
view runs to dozens of rows and a hundred-odd columns, since no box ever
shrinks to fit. HTML solves this by being a scrollable page; a terminal is
a fixed grid, so `app.rs`'s `render` queries the real size (`term::size`)
every frame and clips the drawing to it (`draw::window`), scrolling just
far enough to keep the selection on screen (`draw::scroll_to_show`). "Just
far enough" rather than centring the selection: the reader's sense of
where things are on screen should not jump on every keypress that was
already visible, only on one that would otherwise leave the window.

Without panning, movement alone drives the scroll: the selection is always
what render keeps visible, and there was no way to look at a region with
nothing selected nearby. `p` (see *Panning* above) is that way now --
`scroll_row`/`scroll_col` move directly instead of following the selection,
reusing the same fields and the same terminal-clipped `draw::window`, just
under different control.

## Open questions

- `/`, "jump to a node by title": in the interaction table, but not yet
  wired up -- `input.rs` decodes the key, `app.rs` does not bind it.

# Code evaluation

Never automatic. `dankg graph` only ever *displays* stored results; it cannot
execute anything.

```
$ dankg eval notes.md --block index
  will run (2 blocks, in order):
    setup [python] notes.md:71   via: uv run python
    index [python] notes.md:88   via: uv run python
  proceed? [y/N]
```

The plan is always printed before anything runs, and every block it names has
its language resolved *before* the prompt too: one unconfigured `[lang.*]`
refuses the whole plan rather than running part of what the reader approved
and silently skipping the rest. `--yes` skips the prompt for scripted use; it
never becomes the default. `--no-write` runs everything but prints the
captured output instead of writing it back -- a preview of what would change.

`deps` resolves within the file named on the command line by default
(decision 19): `plan.rs` walks `doc.blocks` directly rather than the
recursive walk `Document::named_blocks` uses for the graph, so a named block
nested inside a list is invisible to eval entirely. Every block reached from
one target must also share its language -- concatenating a `[sh]`
dependency ahead of a `[python]` target and running the result through one
interpreter is far more likely a mistake than an intentional pipeline, so
`plan_for` refuses it before anything is spawned.

A `deps=` entry may also name another file -- `deps=other.md#name`,
mirroring a written link's own `other.md#heading` shape exactly (decision
29\) -- resolved relative to the *declaring* block's own file, via the same
`graph::resolve::join_normalize`/`dir_of` a link already uses, refused the
same way if it would climb above the root. `eval::files::Files` is what
this actually costs: given the target file, it follows only the cross-file
`deps=` edges any of its blocks declare, transitively, loading exactly
those files (over-inclusive by *block* rather than by *chain* -- see its
own doc comment -- but never the whole root); `plan.rs` itself stays a pure
function of an already-built, multi-file `&[BlockRef]` slice, the same
"boundary logic lives with the code that touches the filesystem" discipline
`resolve.rs`/`index.rs` already split the same way. Concatenation (decision
11\) is what makes this cheap: the chain is still one flat file handed to
one interpreter, so a dependency living in another file is no different,
mechanically, from one living in another language-compatible heading of the
same file -- only *which* files must be read changes. Two blocks in
different files may share a `name` (decision 22's uniqueness is per-file,
not per-corpus); a local (no `#`) reference inside a cross-file-reached
block still resolves within *that* block's own file, never back against
wherever the walk started. `--block`/`--all`/`--each`' own targets stay
exactly one file (decision 19's original scope, unchanged) -- only a
target's *chain* may now reach outside it, so a pulled-in dependency from
another file is never itself treated as one of `--all`/`--each`'s targets.

For a language with no real per-file module system reachable from within
one compiled unit (Rust among them), this is enough to write a genuinely
multi-file literate program with no `mod`/`use` at all: every file eval
reaches contributes raw text to the same one concatenated compile, so
there is exactly one flat item namespace regardless of how many `.md`
files a chain spans. DanKG still never reads what a block's code means
(decision 1) -- two blocks defining the same Rust identifier collide
exactly as they would pasted into one file by hand, and `rustc`'s own
"duplicate definition" is the whole error-reporting story here, on
purpose.

`dankg check`'s staleness loop re-verifies every result against
`eval::files::Files::load_all`, loaded with the *whole* corpus up front
rather than `discover`'s on-demand, chain-only loading: `check` already
visits every file for its unresolved-link pass (decision 6), so there is no
"avoid reading files a target's own chain does not reach" reason to hold
back the way `eval` itself does, and loading everything is what lets a
cross-file `deps=` resolve during a staleness recheck at all, regardless of
which file `check`'s own loop happens to be iterating at the time. Editing
only the *dependency* file -- `main.md` untouched, `lib.md` it reaches via
`deps=lib.md#helper` changed -- correctly marks `main.md`'s stored result
stale.

A named block's uniqueness, and `deps` resolution, are both whole-file and
flat (decision 22): any block can depend on any other, in any heading,
without regard for containment. This is a reversal -- the first cut of
decision 22 scoped a name's uniqueness to its own heading (two blocks
named `setup`, one under "Parsing" and one under "Rendering", would not
collide) and resolved `deps` lexically, walking from a block's own heading
up through its ancestors only, never sideways to a sibling. It shipped,
and the very first real document written against it broke: a plain
sequence of sibling sections, each one heading depending on the last --
arguably *the* standard shape for a literate pipeline -- has no ancestor
relationship between the sections at all, so a later stage's `deps`
could never reach an earlier one. The relaxed uniqueness that scoping
bought was never load-bearing for anything but a nice symmetry with real
module namespacing; nothing downstream actually needed two blocks to
share a name, since `tangle` (decisions 23-25) groups by heading directly
and never consults a block's name to do it. Flat resolution is what
"any block can depend on any other" actually requires, so that is what
shipped instead. `containing_heading`/`root_heading` (`plan.rs`) survive
the reversal unchanged -- they were always `tangle`'s placement helpers,
never eval's, and eval's own naming/resolution code needs neither.

`--each` and `--all` (decision 21) both run every target as its own spawn
with its own transitive `deps` re-concatenated from scratch (decision 11's
one-process-per-chain model is unchanged either way), in an order that
respects the DAG -- if `b` depends on `a`, `a`'s own run happens first even
though it may be declared later in the file -- and they differ only in
*which* blocks get that treatment. `--each` is every named block in the
file: the original `--all` behaviour, kept under its own name because it is
still the right tool for smoke-testing every block in isolation, dependency
or not, each with its own recorded result. `--all` is narrower: only the
DAG's *leaves* -- blocks nothing else in the file names in a `deps` -- each
still pulling its own full transitive chain exactly as `--each` would, so a
pure dependency like `setup` is no longer *also* given a standalone spawn
merely because it happens to be named. That standalone spawn was pure waste
under the old `--all`: its result was never anything a reader asked to see
independently, only a side effect of treating every named block uniformly.

Nothing about decision 11 changes to make this possible, which is also why
it does not fully eliminate repetition: a block depended on by two different
leaves still runs twice in one `--all` invocation, once inside each leaf's
own chain, because nothing here shares process state across spawns. `--all`
removes the *wasted* standalone run, not the DAG's own real repetition --
that repetition is decision 11's consequence, stated plainly above, not a
bug `--all` owes a fix to. A pure dependency also gets no
`<!-- dankg:result -->` of its own under `--all`, since there is no
standalone run to hang one on; `--each` is what still gives every block a
recorded result, dependency or not.

`run_one` (`session.rs`), the one place "run this block" is implemented
(see *Terminal UI*'s *Eval in the TUI*), identifies its target by
*position* -- where it sits among the file's named top-level blocks, in
document order -- rather than re-resolving it by name on every call.
`run_single`'s own multi-target loop (`--block`/`--all`/`--each` alike)
and `tui::eval::run` both already hold the exact block they mean, from a
plan or a cycle list built moments earlier; routing that back through a
name lookup would be strictly weaker information for no benefit, a habit
worth keeping regardless of whether names happen to be unique at the
moment. That position stays valid across every write-back `run_single`'s
loop makes against the same file, because a result marker is never itself
a named block and so never changes how many named top-level blocks exist
or their relative order, only their line numbers -- which every call
re-derives fresh from its own re-parse regardless. `dankg check`'s
per-file staleness loop uses the same positional entry point
(`plan_for_index`) for the same reason.

## Execution

`plan.rs` topologically sorts the target block's transitive `deps`, then `run.rs`
concatenates those sources in order ahead of the target, writes one temporary
file, and spawns the configured command once. Consequences worth stating plainly:
dependency side effects re-run on every eval, and there is no persistent state
between separate `dankg eval` invocations. Both are the price of not writing a
PTY session manager, and both keep evaluation reproducible.

Timeout defaults to 30s, overridable per block via `timeout`; the target's own
value governs the one spawn covering its whole chain. stdout and stderr are
captured on their own threads, not read after the fact: a piped stream fills
its OS buffer and blocks the child once it is full, and a timeout loop that
only polled `try_wait` would deadlock on a chatty process instead of ever
reaching its deadline. stderr is shown on DanKG's own stderr but never stored;
stdout is truncated at 64 KiB with a warning and is what gets written back (or,
under `--no-write`, printed). A non-zero exit stores the output and marks the
result failed.

A timed-out kill has to reach more than the one process `run.rs` spawned
directly: a shell script's `sleep` runs as *its* child, and killing only the
shell leaves `sleep` orphaned and still holding the output pipes open, so the
reader threads block until it exits on its own regardless of the timeout --
found by timing the real binary, not by the unit tests alone, and now pinned
by one that asserts wall-clock time rather than only the `timed_out` flag. The
fix spawns into a fresh process group (pgid equal to its own pid) and kills
the negated pid on timeout, reaching the whole group in one signal; std has no
group-kill of its own, so this shells out to the system `kill` rather than
hand-rolling the syscall. Unix only, the same kind of documented gap as the
TUI's termios (decision 3's precedent): off Unix, a lone `Child::kill` is used
instead, which cannot reach a grandchild.

## Results

````
```python name=index deps=setup
print(count())
```

<!-- dankg:result name=index hash=0000000000a3f9c1 -->
```
312
```
````

`hash` covers the concatenated source of the target's whole chain and the
*template* string its language resolved to (`[lang.*] command`, e.g. `uv run python {file}`) -- never the fully-substituted argv, which would embed eval's
own ephemeral temp file path and make every result look stale the instant it
was checked. Editing `[lang.*] command` is exactly the kind of change that
should mark a result stale; a different temp path on every run is not a
change at all. The hash itself is the fixed-width 16 hex digit form
`hash::hex` uses everywhere else in the codebase (the cache stamp included),
not the shortened form the pipeline sketch above once implied.

On any mismatch the result is stale: it still renders, but `dankg check`
reports it. A failed run's marker carries a trailing `failed` word
(`<!-- dankg:result name=index hash=... failed -->`) rather than a separate
attribute -- the output is still whatever stdout produced, staleness is still
just the hash, and `failed` only changes what a reader sees at a glance.
Write-back is text splicing over the original source, not a second pass
through `md/fmt.rs`: locating the exact line range to touch -- reusing the
already-parsed `Document` only to find that range -- and replacing nothing
else is what keeps every other block in the file untouched by an eval run,
which a full AST-to-text rewrite could not promise. Blank-line spacing around
the result is renormalized to exactly one blank line on both sides every
time, so hand-introduced drift self-heals rather than accumulating.

## `dankg check`

The CI gate (decision 9's flip side: `eval` never runs anything automatically,
`check` is what confirms nothing needs to). Two independent checks, both
reported and both able to fail the exit code on their own:

- *Unresolved links*, from the same whole-root index `graph`/`tui` build
  (decision 6 -- the index is always the whole root, so this needs no
  separate walk).
- *Stale results*, per file: every top-level named block with an existing
  `<!-- dankg:result -->` has its expected hash recomputed from the file's
  *current* source and config, exactly the way `eval` would, and compared
  against the stored one. This is necessarily file-scoped, the same as
  `eval` itself (decision 19) -- there is no cross-file dependency DAG to
  walk. A block whose language has since been removed from config cannot be
  re-verified and is not counted stale on that account alone; a plan error
  (a dependency renamed or removed, a cycle introduced) *is* counted stale,
  since the result can no longer be reproduced from what the file says now.

# Tangle

Milestone 9's precedent, pointed at a directory instead of a database: a
markdown file's named blocks are, collectively, a real program, and
`dankg tangle` materializes it as one. "Tangle" rather than "compile"
(decision 25) because DanKG's own job is assembly, not compilation --
DanKG never touches a compiler, it writes a source tree and hands it to a
configured command (`rustc`, `cargo`, or nothing at all for a language
with no separate build step) the same way `[lang.*]` is spawned for
`eval`. It is also the term literate programming itself already uses for
exactly this operation -- extracting and reassembling code chunks into
compilable source, as opposed to "weaving" them into typeset
documentation -- so it costs no new vocabulary once `project.md`'s own
framing is taken seriously.

```
dankg tangle <path>... --lang <lang> [-o DIR]
```

## Scope

A tangled block is exactly an eval-able one (decision 23): named,
top-level, not nested in a list -- `eval::plan::top_level_blocks`'s own
definition, unchanged. Decision 20 already made "is this a node" and "is
this eval-able" the same question so that nothing selectable could be
un-runnable; extending that equivalence to tangle keeps every block that
ships in a compiled program visible to the graph, listable via
`eval --list`, and individually eval-able on its own -- a block that
compiled into the real artifact but could not be seen or run any other
way would be exactly the blind spot decision 20 already refused to
create. `Document::named_blocks` -- the recursive, list-descending walk
that already exists and is already tested (`tests/parsing.rs`) -- is
deliberately not what tangle uses, for the same reason `eval` does not
use it either.

A block not written in `--lang`'s language is skipped; a file with no
blocks in that language tangles to an empty tree, which is reported
rather than treated as an error, on the same "half a plan is still a real
answer" principle as an empty `--list`.

One file tangles just it, unchanged from decisions 23-25; a directory (or
several paths) walks the corpus instead (decision 26), the same choice
`--list` already offers and via the same distinction `session::list_corpus_text`
already draws: naming only files tangles exactly those, naming a directory
tangles everything under it. Unlike `--block`/`--all`/`--each`, nothing
about `deps=` forces this to stop at one file -- tangle never reads that
attribute (see *Placement*), so there is no single-file DAG standing in
the way, and a real compiled program is a whole-corpus artifact in a way a
disposable eval script never has to be: a crate is built once, not once
per source file. A single named file still skips the whole-root walk
`index::load` would otherwise do, and the graph build that comes with it,
for the same reason `eval`'s own single-file path already avoids both
(decision 19) -- nothing here needs to know about any file but the one
named.

## Placement

Structure comes from containment, never from `deps` (decision 24): the
heading with no parent of its own -- the top of a block's own ancestor
chain, usually but not necessarily level 1, since a document that opens
straight at level 2 still has a well-defined top -- becomes one file, and a
block under a more deeply nested heading folds into it, attaching in
document order. Finding a block's immediate heading reuses the same
approximation `eval::session::list_blocks` already computes ("the nearest
heading at or above" a block's line); walking from there to the top reuses
the containing heading's own containment, the same shape `graph/build.rs`'s
`current`-heading stack tracks, over bare line numbers rather than
`NodeId=s (=plan::containing_heading`, `plan::root_heading`). Blocks within
one generated file are
emitted in document order, not DAG order: `deps` is eval's own concept
for concatenating a disposable, throwaway script, and has nothing to say
about how a persistent, structured source file should be laid out. A
block with no `deps` at all tangles exactly the same as one with several
\-- tangle never reads the attribute in the first place.

Corpus-wide (decision 26), each contributing source file's own heading
placement gets nested under one more directory: its own root-relative
path, extension stripped (`graph::build::strip_extension`, the same
convention a `NodeId`'s own `key` already uses), so `notes/lexer.md`'s
"Tokenize" heading tangles to `notes/lexer/tokenize.rs` rather than
colliding with an unrelated file's identically-titled heading. That
nesting only appears once more than one file actually contributes a
matching block -- naming a single file, even one sitting inside a larger
corpus with nothing else in `--lang`'s language, tangles exactly as if it
had been named alone, with no directory to explain that was not already
there for a reason.

DanKG's own code does not write `use`/`import` statements, or infer what
one block's code means well enough to name a specific item from another --
that stops being placement and starts being a small compiler, in tension
with decision 1 in spirit if not in the letter, and it would eventually
guess wrong about some reference in a way decision 1's zero-crates policy
was never at risk of. The author writes those themselves, the same way
they write the rest of a block's source, and corpus-wide placement
(decision 26) is exactly as deterministic as single-file placement always
was, so the author has everything needed to get a cross-file reference
right by hand: `notes/lexer.md`'s "Tokenize" heading always tangles to
`notes/lexer/tokenize.rs`, corpus-wide or not.

Purely *structural* connective tissue is a different question, and
decision 27 answers it differently: some languages (Rust) will not
compile a directory tree at all unless something declares `mod x;` for
every file and subdirectory, and that declaration needs no understanding
of what a block's code means -- only the shape of the tree tangle already
computed, which is exactly the kind of thing a tool that owns the whole
tree can safely do. But *that* safely-mechanical property does not hold
for every language: Java's `package` declaration has to sit inside each
`.java` file, first line, and the filename has to match the public class
name the block defines -- which DanKG cannot know without reading the
code, the same reading it must not do for a `use` statement either. So
this is never DanKG's own generator, for any language, no matter how
mechanical the case looks: it is `[tangle.<lang>] glue`, a second,
independent, optional command (alongside `command`, which builds; `glue`
prepares) spawned against the already-materialized `\{dir\}`, exactly the
way every other per-language capability in DanKG is a spawned external
program rather than linked-in code (decision 1). An extension is free to
do what DanKG's own code must not -- read a block's source to find a
class name, say -- because it is the reader's own configured, opted-in
program, the same trust model `[lang.*] command` already has (decision 9:
the risk was accepted the moment it was configured). DanKG ships a
documented reference `glue` script for Rust, because the mechanical case
holds there; Python needs none, since a directory of `.py` files is
already an importable namespace package with nothing to generate; anyone
wanting Java support writes their own external program -- no change to
DanKG itself required, ever, to add a language this way.

`glue/rust.py` only ever writes a `mod.rs` where none exists; it never
touches one that is already there (see the script's own doc comment).
That backoff means porting an *existing*, hand-written multi-file Rust
module into literate form never actually exercises the generator: a human
already wrote that module's `mod.rs` by hand, tangle reproduces it
verbatim like any other block, and `glue` finds a file already sitting
where it would have written one and leaves it alone. The mechanism is
real, but only load-bearing for a module authored *through* dankg from
the start, with no hand-written `mod.rs` anywhere in its literate source
\-- confirmed by a pilot (`literate/layout-pilot/`) built specifically to
withhold that hand-authored piece and prove the generator fires without
it, including a negative control: deleting `glue` from config and
re-tangling fails the build with rustc's own `E0583`, the same error a
human would get for forgetting the declaration themselves.

A block wants a path outside the heading-derived tree entirely more often
than it wants to be reassigned to a different heading's file -- a
`Cargo.toml`, a `pyproject.toml` -- so the fence info string grows one
more optional key: `path=`, an explicit output path that overrides the
heading-derived default, the same way `db` already names a target
distinct from a block's language. A block naming `path` still needs a
`name` (decision 23 does not relax for it) and is still a node the graph
and `eval --list` can show; `path` changes only where its tangled content
is written, not whether it is one.

Identifiers are derived from the same per-file `Slugger` a heading's slug
already comes from, then run through one more, language-specific pass
that narrows a slug (already lowercase, already `[a-z0-9_-]`) into a
valid identifier for the target language. Two slug-and-sanitize passes
sharing one `Slugger` is the same "the escaper and the parser must agree"
reasoning that keeps `fmt`'s emphasis-escaping logic calling into
`inline.rs` rather than reimplementing it, applied to a second consumer
of slugs instead of a second consumer of flanking rules.

## Per-file hints and the glue manifest

`dankg.tangle.public` (decision 28), frontmatter's own way into this: a
per-file hint that this file's tangled output should be declared publicly
visible wherever a `glue` command's generated declarations would
otherwise default to private. Frontmatter has no scope finer than a whole
file, so this is a whole-file decision -- a source file wanting different
visibility for different sections is outside what it can express, and
would need those sections split into separate files instead. Unset, or
anything other than exactly `true`, means private: a typo here must never
silently widen visibility, the same "half-understood is worse than
refused" reasoning frontmatter and config both already apply elsewhere.

Whether *the mechanism* runs at all stays root config's call, never
frontmatter's (decision 27's own principle, applied here): two files
cannot coherently disagree about whether the one shared output tree gets
glue at all, so that decision has to live above any single file. What
frontmatter narrows is how *this file's own* contribution to a mechanism
root config already turned on gets declared -- the same "may narrow but
never widen" shape `dankg.*` frontmatter keys already have for `[lang.*]`
(see *Config*, below).

A `glue` command only ever sees a plain directory of already-written
files, which is deliberate -- the simplest possible glue script needs
nothing more than that to work. A richer one that wants to honour
per-file visibility reads an optional sidecar,
`\{dir\}/.dankg-tangle-manifest.json`, written only when `glue` is
actually configured (a language with none gets no extra file cluttering
its output) and listing every tangled file's own output path, source
`.md` path, `public` flag, and (version 2) its own contributing blocks.
Hand-rolled JSON, the same reasoning as `render/json.rs`: DanKG takes no
crates, and the shape here is small and fully under our control -- small
enough that the escaper is literally shared (`render::json::string`)
rather than reimplemented a second time for one more, equally small, JSON
writer.

### Manifest version 2: per-block source position

Decision 30, and the first real instance of "a richer glue implementation
needs more" the schema's own version-1 comment already anticipated. Each
file's manifest entry gained a `blocks` array -- `name`, `line`,
`end_line` for every contributing block, in document order -- straight off
the same `BlockRef` fields `result.rs` already carries for write-back
positioning (`src/tangle.rs`'s `ManifestEntry`/`BlockEntry`). Nothing new
is computed for the manifest's sake; tangle already has these coordinates
in hand at the exact point it was building a `ManifestEntry` anyway, so
this is exposure, not a feature.

What a `line`/`end_line` pair is *for* stays outside DanKG entirely: which
paragraph of source prose "belongs" to a block, and what counts as a
suspicious echo of it, is a glue script's own judgment to make, the same
trust boundary decision 27 already draws -- DanKG hands over structure,
never meaning. `glue/rust-doclint.py` is the reference consumer: it flags
a tangled block's own `//!`/`///` comment when it borrows a clause from
the literate source's prose rather than pointing back to it, windowing its
comparison to the prose between one block and the previous one rather than
the whole file, which is exactly what block-level position makes possible
that file-level position (version 1) could not. A version-1 manifest (no
`blocks` field, an older `dankg`) still parses correctly -- the field is
purely additive, and `rust.py`'s own `path`/`source`/`public` reads are
unaffected by its presence or absence.

`glue/rust-doclint.py` has to resolve a manifest entry's `source` back to
a real file, which `rust.py` never needs to do -- and `source` is not
consistently resolvable from wherever a glue script happens to run.
Tangling one named file makes `source` exactly the path given on the
command line; tangling a corpus makes it root-relative instead, and
neither `\{dir\}` nor anything else glue is handed carries that root. The
script takes an optional second argument, `\[source-root\]`, for this
reason -- found only because a corpus-wide pilot
(`literate/layout-pilot/`) tangled clean and silent, which looked like a
passing check until it turned out every `source` lookup was silently
missing the file. Both reference scripts have their own test suite now
(`glue/test_glue.py`, stdlib `unittest`, run directly with `python3`),
covering `rust.py`'s generation rules and `rust-doclint.py`'s windowing
and fallback behaviour; chaining the two in one `glue` line (structural
generation before the doc-comment check, via `sh -c '... && ...'`, the
same quote-aware split `cmd.rs` already tests) is what surfaced the
`source-root` gap in the first place.

## Config

```
[tangle.rust]
glue    = dankg-glue-rust {dir}
command = cargo build --manifest-path {dir}/Cargo.toml
ext     = rs

[tangle.python]
ext     = py
```

`command` is optional, spawned once against the whole assembled tree via
a new `\{dir\}` substitution alongside `[lang.*]`'s `\{file\}` -- Python
has nothing to compile, so materializing the module tree *is* the whole
operation, and a `[tangle.python]` with no `command` simply stops there,
successfully. `glue` is a second, equally optional command, using the
same `\{dir\}` substitution, spawned first: it exists to add structural
connective tissue (decision 27) before `command` tries to build the
result, so a build command that needs it never has to ask for it itself.
`ext` identifies which fence language tag the section gathers, the same
role it already plays for `[lang.*]`; the two sections are independent --
an `eval`-configured language needs no `[tangle.*]` entry and vice versa,
since a block can be exercised as a disposable script, tangled into a
persistent program, both, or neither. Unconfigured, `ext` falls back to
the `--lang` value itself rather than guessing a per-language table --
serviceable for a first, config-free invocation, but in practice anyone
naming a real `command` (needing a real compiler to find real files) sets
`ext` alongside it anyway.

## Output

Tangled output goes to `-o DIR`, defaulting to `.dankg/build/<lang>/`,
and is never written back into the markdown -- unlike an eval result, a
tangled tree is wholly derived and disposable, and the markdown stays the
one thing worth committing (decision 12's reasoning, from the other side
of the same principle: the file is the source of truth, so nothing
generated from it gets to also claim that title). The build directory is
excluded from the corpus walk the same structural way `.dankg/` and
`.git/` already are, not via `.dankgignore`, so a tangled tree sitting
under the root never becomes something `dankg graph` tries to read as
notes. Every generated file opens with a banner comment naming the
heading and source file it came from and saying it is generated --
`fmt`'s "nothing unverified reaches the disk" guard has no equivalent
here, since the generated tree is not the thing being trusted; the
markdown that produced it is.

## Trigger

Never automatic, the same principle as decision 9: `graph`/`check` never
invoke tangle, and tangling requires its own explicit command. Unlike
`eval`, there is no interactive plan/confirm prompt -- tangle does not
run the reader's program, only assembles and (optionally) builds it, and
build-time code execution (a `build.rs`, a setup script, or `glue` itself,
decision 27's own external program) is a risk the reader already accepted
the moment they configured that `command` or `glue`, the same way
configuring `[lang.*]` is what authorizes `eval` to run anything at all.
`glue` runs before `command`, so a build failure and a glue failure are
both reported the same way -- tangle already wrote every file either
would need; only whichever spawned step comes next can still fail.

## Pilots 3 and 4: visibility, and eval/tangle composed over one corpus

Pilots 1 (`literate/hash.md`) and 2 (`literate/layout-pilot/`) each proved
tangle's own assemble-and-build loop; neither touched `dankg eval` at all,
and neither ever set `dankg.tangle.public` (decision 28) -- pilot 2's five
sibling modules only ever reach each other through `super::`, which a
*private* `mod x;` already permits between siblings under one parent.
Two more pilots close both gaps.

`literate/visibility-pilot/` is the first real exercise of decision 28
end to end: `exposed.md` sets `dankg.tangle.public: true` on its one
heading, and the crate root's `lib.rs` -- not a descendant of `exposed/`,
so ordinary sibling privacy cannot save it -- calls
`exposed::greeting::hello()` across that boundary. It only compiles
because the flag made `glue/rust.py` write `pub mod greeting;` rather than
`mod greeting;` into the `mod.rs` it generated for `exposed/`, confirmed
by inspecting both the manifest (`.dankg-tangle-manifest.json`'s
`exposed/greeting.rs` entry: `public: true`) and the generated `mod.rs`
directly. The negative control -- done by hand, not committed broken, the
same reason pilot 2's own `glue`-disabled check is prose -- sets the
frontmatter to `false` and re-tangles: `cargo test` then fails with
`error[E0603]: module `greeting` is private`, pointing at `lib.rs`'s own
`use` line, proof the flag is load-bearing rather than cosmetic.

`literate/composition-pilot/` asks a question neither earlier pilot could:
can the same block serve eval's flat, module-free concatenation (decision
11\) and tangle's real multi-file structure at once? Two corpora under one
root answer it oppositely. `same-file.md` puts a dependency and its
target under one heading, so tangle's placement (containment only, decision
24\) and eval's own concatenation (`deps`-ordered, decision 11) agree by
construction -- there is no cross-file boundary for the two mechanisms to
disagree about, and `dankg eval same-file.md --block quadruple` and
`dankg tangle same-file.md --lang rust` followed by `rustc --test` on the
one file it produces both pass, unmodified, on the identical block source.
`producer.md`/`consumer.md` puts the dependency in a different file, and
`dankg eval consumer.md --block use_greeting` still passes -- decision
29's cross-file `deps=` had unit coverage (`src/eval/plan.rs`,
`src/eval/files.rs`) but never a pilot spawning the real binary until this
one -- while tangling the same two files apart
(`producer/greeting.rs`, `consumer/use_greeting.rs`) and compiling
`consumer/use_greeting.rs` alone fails with
`error[E0425]: cannot find function `make_greeting` in this scope`: its
`use super::*` reaches `consumer`'s own module, not `producer`'s, and
nothing here writes the `use crate::producer::greeting::make_greeting;`
that would fix it, because writing it would just as surely break the
identical block's use as a flat, concatenation-eval'd script, which has no
`crate::producer` to resolve at all. The finding, not a bug in either
mechanism: a block gets eval-smoke-testing across a file boundary or
real cross-tangle-file linkage at that same boundary, never both
unmodified, and the corpus is committed exactly as written -- eval-oriented,
with no tangle scaffold wired up -- because the tangled pair genuinely
does not build. For a real self-hosting attempt, the actionable shape is
`same-file.md`'s: a module's internal helpers, smoke-tested and tangled
together, stay under the module's own heading; a cross-file compile-time
dependency (`layout-pilot`'s own `super::acyclic::Role`) already accepts
hand-written `use` and should not also be asked to serve one leg of a
cross-file eval chain.

## Pilots 5 and 6: doc-comment injection and cross-reference rewriting

A gap the visibility/composition pilots' own discussion surfaced: a
tangled module's `//!`/`///` comments, by convention, point back at the
literate `.md` (`hash.md`'s own `module_doc` block) rather than restating
it, which is exactly right for a reader with the corpus in hand and
exactly wrong for a reader of a binary-only build, who cannot open a file
that was never shipped. `glue/rust-docinject.py` answers it without any
core DanKG change at all: manifest v2's `line`/`end_line` per block
already give an external, opt-in program everything needed to compute a
block's own *owned prose* -- the paragraphs between the previous block's
own close and this one's own open -- and write it in as a real `///`,
backing off exactly the way `rust.py` already backs off an existing
`mod.rs` (decision 27's own idiom, one level deeper): a block that
already carries a hand-written `//!`/`///` is left untouched entirely.
Deliberately `///` only, never a synthesized `//!` -- a file-level module
doc characterizes the whole file, and nothing here can tell whether a
given block's own prose was ever meant to carry that weight.

A DanKG link inside the copied prose is rewritten too: `[text](other.md#name)`
or `[text](#name)` becomes `[text](crate::path::to::name)` when `name`
names a block the same tangle run produced, resolved through the exact
same manifest name/path data `glue` already has. A rustdoc /intra-doc
link/ is not a bare pointer -- `cargo doc` resolves it via the compiler's
own name resolution and fails the build on one that does not, so a
`#![deny(rustdoc::broken_intra_doc_links)]` crate attribute turns "the
rewrite is correct" into a checked build property rather than an
assertion. `literate/docinject-pilot/` (pilot 6) proves it end to end:
`producer.md`'s `make_greeting` is linked from `consumer.md`'s own prose,
tangled apart into `producer/greeting.rs`/`consumer/caller.rs`, and the
generated `fn.shout.html` carries a real `<a href="../../producer/greeting/fn.make_greeting.html">`
\-- grepped out of the actual `cargo doc` output, not inferred from the
build merely succeeding. `literate/hash.md` (pilot 1) gained
`rust-docinject.py` in its own `glue` chain as pilot 5 -- proof the
mechanism holds on a purely single-file corpus with no cross-references
at all, and that `module_doc`'s own hand-written `//!` is correctly left
alone while `fnv1a`/`hex`/`parse_hex`/`tests` each gain a real `///` for
the first time.

Doc-comment content turns out to be exempt from the composition-pilot's
own finding, for a reason worth stating plainly: `rustc` strips comments
before anything resembling name resolution runs, so a `///`'s content --
rewritten link or not -- has zero effect on whether a block compiles
under `eval`'s flat concatenation or `tangle`'s real module tree. Only
`cargo doc` ever looks inside one. Code composing across a tangle-file
boundary is genuinely constrained (pilot 4); documentation doing the same
is not, for the exact opposite reason.

Three real bugs, found only by actually running this against real
corpora, never by reasoning about the design in advance -- the same
pattern every earlier pilot's own notes already describe:

- A `path=Cargo.toml` block is selected by *fence* language matching
  `--lang rust`, but its own output is TOML, not Rust; the first real run
  against `hash.md` tried to write `///` into it and broke the manifest
  with `error: key with no value, expected =`. Fixed by skipping
  injection for any output whose own path does not end `.rs`, while still
  letting that block's own prose count as spent so a *later* real `.rs`
  block's window does not silently widen to include it.
- A leading YAML-style frontmatter block (`---` ... `---`) is not a
  paragraph; the crude, independent paragraph splitter every glue script
  here already has to write for itself (decision 27: no access to
  `md/`) read it as one and swept `dankg.tangle.public: true` itself into
  the very first block's own injected comment on `docinject-pilot`'s
  first real run. Fixed by blanking a detected leading frontmatter block
  in place of its own lines before windowing, preserving every line
  number so nothing downstream needs to know it happened.
- A block that opens with a hand-written `use` -- exactly the escape
  hatch the composition pilot's own finding requires for a real
  cross-tangle-file reference -- is itself a Rust item, and a doc comment
  attaches to whatever comes *directly* after it. Inserting at the very
  start of a block's content put the synthesized `///` on the `use`
  itself; `cargo doc` built successfully regardless, and simply rendered
  no docblock at all on the function it was meant to document, which is
  what made the bug easy to miss without grepping the actual generated
  HTML rather than trusting a green build. Fixed by skipping leading
  blank lines, `use` statements, and attributes before choosing where a
  synthesized comment actually lands.

All three are pinned in `glue/test_glue.py` (46 tests total across the
three reference scripts), each named for the run that found it rather
than the mechanism it now guards.

## Open questions

- Should a named, eval-able block be excludable from tangle specifically
  \-- a demo or scratch snippet that should stay runnable and visible
  without shipping in the built artifact? No such escape hatch exists
  yet; `path` is an override for *where* a block lands, not an opt-out.
- Whether to run a target language's own compile-check (`py_compile`,
  `rustc --crate-type` with no output) when no `[tangle.*] command` is
  configured, versus leaving "no command" to mean "materialize only,"
  full stop, as written above.
- The manifest schema (decision 28, widened by decision 30) reached
  `version: 2` once a richer glue implementation
  (`glue/rust-doclint.py`) was actually written and needed block-level
  source position. Declaration order across a directory -- rather than
  whatever order tangle happened to write files in -- remains
  unaddressed, since no glue consumer has needed it yet.
- `dankg.tangle.public` is the one frontmatter hint that exists. Whether
  other purely-authorial, per-file glue preferences ever justify their own
  `dankg.tangle.*` key, versus staying a `glue` script's own problem to
  solve by reading the block source it is free to read, is open.

# Literate database management

Milestone 9. Literate programming put the prose and the code that implements it
in one file; this does the same for data. The markdown file holds the
explanation, the ETL that produces a table, and a link from the table back to
both -- so "where did this number come from" is a graph query rather than an
archaeology project.

The pieces are already here. A `sql` block is a code block, so it has a `name`
and `deps` and takes part in the same DAG. `dankg eval` already prints a plan,
spawns a configured command, and writes results back into the file. What
milestone 9 adds is a database as the thing being written to, and the relations
in it as graph nodes.

## DuckDB, and why the dependency policy survives

DuckDB is a single-file embedded database that reads CSV, Parquet and JSON in
place and ships as one static CLI binary. That is close to the same set of
constraints DanKG holds itself to.

DanKG does not link it. There is no crate, no FFI, no bindings -- decision 1 is
unchanged. `duckdb` is spawned exactly like `python` or `sh`, configured in the
same file, and subject to the same allowlist rule: a `sql` block with no
configured command is reported and never run.

```
# .dankg/config
[db.warehouse]
command = duckdb -csv {db} < {file}
path    = data/warehouse.duckdb
```

A `[db.*]` section names one database. Other engines fit the same shape; DuckDB
is the first implementation, not a special case in the code.

## Blocks

````
```sql db=warehouse name=load_orders deps=schema
CREATE OR REPLACE TABLE orders AS
SELECT * FROM read_parquet('raw/orders/*.parquet');
```
````

`db` names the target database and is the only new attribute. Everything else --
`name`, `deps`, `timeout`, the plan-then-prompt, the hash-tagged result block,
the stale badge -- works as it already does for code.

## Provenance without a driver

The point of this milestone is the edges, not the execution.

DanKG snapshots `duckdb_tables()` and `duckdb_views()` before a block runs and
again afterwards, and diffs the two. Relations that appeared or changed are the
block's outputs; relations the block's SQL names but did not create are its
inputs. Both are ordinary command output, parsed the same way any other block's
stdout is -- no bindings, no catalog format to track.

Each relation becomes a node with id `<db>::<schema>.<table>`, and two new edge
kinds join `Contains` and `Link`:

```rust
enum EdgeKind { Contains, Link, Produces, Reads }
```

`Produces` runs from the block's node to the relation; `Reads` runs from the
relation to the block that consumed it. A table therefore edges back to the
prose section that explains it, and forward to everything downstream of it.
Lineage is just a traversal, and it renders in the same graph as everything
else.

## What this buys an agent

project.md's third key feature is that an LLM can run DanKG because it only
touches plaintext. That claim gets much stronger with a database behind it: an
agent reading the corpus sees what each table means, which block builds it,
what that block depends on, and whether the stored result is stale -- without a
connection, credentials, or a schema dump. Answering "what breaks if I change
`orders`" becomes reading a file.

## Open questions for this milestone

- Does `dankg graph` ever touch the database? It must not execute, but reading
  the catalog to show relations that no block produced is tempting. Currently
  no: `graph` displays stored results only, and an unknown relation is an
  unresolved node like any other dangling link.
- Are relation nodes counted against `--depth`, or are they always shown with
  their producing block? Probably the latter; a table one hop from its ETL is
  not really a hop.
- Write-back for a `SELECT`: the result block holds the query output as a
  markdown table, truncated at the same 64 KiB. Whether the row count belongs
  in the hash -- so that new data marks the result stale -- is undecided, and
  the answer differs for a snapshot than for a running pipeline.

# Config

No serde, so the format is a minimal INI. Sections, `key ` value`, =#` comments,
no nesting, no arrays.

```
# .dankg/config
[graph]
depth = 2

[lang.python]
command = uv run python {file}
ext     = py

[lang.sh]
command = sh {file}
ext     = sh

[editor]
command = code -g {file}:{line}

[keys]
up    = k
down  = j
left  = h
right = l
quit  = q
reset = r
pan   = p
```

`[keys]` remaps the TUI's letter mnemonics (decision 18); arrows are always
up/down/left/right regardless of what is written here, since remapping a
positional key to another position is not a meaningful request. A value that
is not exactly one character, or that collides with another action's
binding, warns and the whole map falls back to the defaults above -- a
config half-remapped would leave one key doing two things with no
indication which, so this is the same "half-understood is worse than
refused" principle as frontmatter and section names, not a special case for
`[keys]`.

`[editor] command` is the template a future `dankg open <node>` spawns to jump
to a node's source line, substituting `\{file\}` and `\{line\}` the same way
`[lang.*]` substitutes `\{file\}`. Unset means unconfigured, not "no editor" --
falling back to `$EDITOR`/`$VISUAL` is left to whatever spawns the command,
since that is an environment concern and config.rs only reports what the file
said. This is the answer to "should DanKG grow a GUI or TUI to edit files":
it does not. The editor a reader already has is the buffer; DanKG's job stops
at pointing it at the right line (decision 17).

A `[lang.*]` section is also the allowlist: a fenced block in a language with no
configured command is never executed, only reported. Per-file overrides go in
frontmatter under `dankg.*` keys and may narrow but never widen what the root
config permits. Not every `dankg.*` key fits that "permission" framing --
`dankg.tangle.public` (decision 28) is a per-file authorial preference, not
something to narrow or widen, but it keeps the same prefix and the same
principle in spirit: it can only ever change what happens to *this* file
within a mechanism root config already turned on, never turn the
mechanism on itself.

A `#` or `;` starts a comment only as the first non-blank character of a line.
A `command` value legitimately contains a `#`, and a value is not a place to
start guessing. Quotes wrapping a whole value are stripped, so a command with
significant trailing space can be written down.

Unknown sections and unknown keys warn with a line number and are dropped, on
the same principle as frontmatter: a config half-understood is worse than one
refused. `[db.*]` is parsed today even though milestone 9 implements it, so a
config written ahead of the code does not warn.

# CLI

```
dankg graph <path> [--format html|json|dot|mermaid] [--depth N] [--all] [-o FILE]
dankg eval  <path> [--block NAME | --all | --each] [--yes] [--no-write]
dankg eval  [<path>...] --list [--no-cache]
dankg tangle <path>... --lang LANG [-o DIR] [--no-cache]   assemble named blocks into a source tree
dankg fmt   <path>... [--check]   rewrite to normal form; --check only reports
dankg check [<path>...]       exit non-zero on unresolved links or stale results
dankg index [<path>]          corpus statistics, cache state

=--format json= always emits the whole index, which is what makes it the
scriptable surface: a consumer that asked for the graph should not silently get
a fragment of it. =--depth= and =--all= shape the drawn formats only, and
passing =--depth= alongside =json= warns rather than being quietly ignored.

=--no-cache= applies to =graph=, =index=, =check=, =eval --list=, and
=tangle= when it is walking a corpus rather than reading one named file
directly (which never opens a cache to begin with). =index=
and =check= both default to the working directory, because "tell me about the
corpus I am standing in" is the common case for either. =index= is the
command to run when the graph is not what you expected, since that produces
exactly two questions -- which root am I in, and which files did it decide
were mine.

=--block=, =--all= and =--each= take exactly one path: =deps== resolves
within one file only (decision 19), so a second path would have nothing to
mean. =--block=, =--all=, =--each= and =--list= are mutually exclusive and
one of them is required -- there is no default target to fall back to, on
the same "never automatic" principle as decision 9 itself. =--all= and
=--each= differ only in which blocks get run, not in how (decision 21).
=--list= is the odd one out in every direction at once: it explores rather
than runs, needs no confirm prompt, prints to stdout as the requested output
rather than to stderr the way the plan/confirm/result messages of the others
do, and -- since it has no execution to scope the way
=--block=/=--all=/=--each= do -- takes any number of paths, defaulting to
=.=, and walks a whole directory as a corpus exactly like
=graph=/=index=/=check= (decision 6).
```

`tangle`'s `--lang` is required, but unlike `--block`/`--all`/`--each`
(decision 19's file-only scope), it takes any number of paths: naming
files tangles just them, naming a directory walks the whole corpus
(decision 26), the same split `--list` already has. There is nothing to
confirm: `-o DIR` names where the assembled tree lands, defaulting to
`.dankg/build/<lang>/` (see *Tangle*, *Output*).

`check` is the CI gate. It is deliberately separate from `graph` so that drafting
a half-written note never fails.

# Formatting

`dankg fmt` rewrites markdown into a normal form, so that a knowledge base stays
diffable and a graph never changes because someone indented a list differently.

```
dankg fmt notes/*.md           rewrite in place
dankg fmt --check notes/*.md   name the files that would change; write nothing
```

`--check` prints the offending paths on stdout and exits non-zero, which makes
it the second CI gate alongside `dankg check`.

## Losslessness is the whole problem

The formatter is a pure AST-to-text function, so anything the AST does not
record cannot be reproduced. Two consequences:

- Passthrough blocks are re-emitted byte for byte. The formatter never
  reformats a construct it does not model, which keeps block quotes and HTML
  safe while they remain outside the subset. The frontmatter block is treated
  the same way and for the same reason: `Frontmatter.entries` is a lossy view
  that has already dropped quoting, comments and every unsupported line, so
  `Frontmatter.raw` is what gets written back.
- The AST records the authorial choices the formatter would otherwise silently
  erase: `List.marker` (bullet character, or the `.` / `)` of an ordered list),
  `Inline::Emph{delim}` and `Inline::Strong{delim}` (`*` versus `_`), and
  `Block::Code{fence}` (backtick versus tilde). Fence *length* is not recorded,
  because it is normalized: three characters, or the shortest run that clears
  the body. `InfoString.unknown` keeps the words the parser warned about and
  ignored, so an attribute DanKG does not understand survives a rewrite.

## Text is escaped on the way out, not copied

An `Inline::Text` node holds the character the author meant, not the bytes they
typed, so anything that would be re-read as markup has to be escaped back.
`\`, \`\`\`, `[` and `]` are always escaped; `#`, `>`, `-`, `+`, `*`, `~`, `<` and
a leading `1.` are escaped at the start of a line, where they would open a
block; `*` and `_` are escaped only where they could actually open or close
emphasis, which is what keeps `snake_case_name` intact.

`]` is escaped even though a bare one is inert, because a real link writes an
unescaped `[` and a stray `]` inside its text would close it early.

Thematic breaks are written `***`, never `---`. A leading `---` would be read
back as frontmatter, and `- ---` inside a list item is a thematic break rather
than a bullet.

## Round-tripping is the strongest parser test available

Two properties, both checked over the fixture corpus and every markdown input
in the vendored spec:

- *Idempotence* -- formatting twice equals formatting once.
- *Fixed point* -- already-formatted input is returned unchanged.

Together these exercise the parser far harder than the conformance suite does,
because every construct must survive a full parse-print cycle rather than
merely producing the right HTML. Writing them found four real bugs: emphasis
delimiters escaped against the wrong neighbour, unescaped `]` truncating link
text, an info string rebuilt in the wrong order, and a leading blank line
inside a list item wrongly making the whole list loose.

648 of the spec's 652 markdown inputs round-trip. The four that do not are one
shape: an indented block that the parser keeps as `Passthrough` sitting next to
a list whose items were indented more deeply than normal form allows.
Normalizing the list narrows its content column, and the indented block -- which
the formatter is obliged to re-emit byte for byte -- is then deep enough to be
swallowed as a continuation line. Implementing indented code blocks closes all
four. They are listed by example number in `tests/fmt.rs` rather than counted,
so fixing one fails the test and prompts its removal.

## Nothing unverified reaches the disk

`fmt` re-parses its own output before writing and compares the two documents,
ignoring source line numbers -- the one thing formatting is expected to change.
If they differ, or if a second pass is not byte-identical, the file is left
alone and the reason goes to stderr. The four spec cases above take this path.

The guard exists because `fmt` is the only command that writes to a user's
notes. A conformance regression costs a number in a table; a formatter
regression costs the note.

## What it does and does not normalize

Normalized: heading style, bullet and delimiter consistency, ordered-list
renumbering from the first item's start, fence length, info-string attribute
order, list indentation, blank-line discipline, trailing whitespace, a single
trailing newline, and hard breaks (a trailing backslash, never two spaces --
trailing whitespace is exactly what normalizing removes).

Not normalized: paragraph line breaks. Reflowing prose to a column limit turns
a one-word edit into a rewritten paragraph in every diff, and where an author
breaks a line is authorial. Soft breaks are preserved exactly.

# Cache

`.dankg/cache/` holds one entry per source file, keyed on `(mtime, len)` and
verified by an FNV-1a content hash. The pair is the cheap rejection; the hash
closes the window where a file is rewritten inside one filesystem timestamp
tick. The config hash is stamped into every entry, so changing `.dankg/config`
invalidates all of them at once. Entries are named by a hash of the path, so a
nested source file needs no nested cache directory.

What is stored is a file's *index contribution* -- its nodes, containment
edges, raw links and aliases -- rather than the markdown AST. That is exactly
what pipeline steps 4-6 consume and a far smaller thing to write a codec for.
`dankg fmt`, which needs the whole AST, does not use the cache and does not want
to: it reads every file it is given anyway. A node's row grew a `kind` field
when block nodes were added (decision 20); the row format's own `VERSION`
was bumped alongside it, so every existing entry becomes a miss rather than
being misread by a decoder now expecting one field more than it has.

An entry also stores the diagnostics the parse raised, and a hit replays them.
Without that, "deleting the cache changes nothing but runtime" would be false
in the one way a user would actually notice -- warnings vanishing on the second
run. The claim is tested rather than asserted: `--no-cache` exists so that a
cached run and a cold one can be compared byte for byte, and `tests/index.rs`
does exactly that for both the graph and the diagnostics.

Nothing in the cache is load-bearing. A missing, truncated, corrupt, or
older-version entry is a miss, not an error; a write failure is counted and
ignored. Writes go to a temporary file and are renamed into place, so a
half-written entry is never readable as a whole one and two concurrent runs do
not interleave. No cache is opened at all for a root with no `.dankg/`.

# Diagnostics

Every dropped, skipped, or unresolved thing warns on stderr with `file:line`, and
runs end with a one-line summary (`2 unresolved of 31 edges`). stdout carries
only the requested output, so `--format=json` stays pipeable.

Diagnostics are sorted by `(file, line)` before they are emitted, stably, so
that the same corpus reports the same things in the same order however the walk
or the resolver happened to reach them. Determinism is a hard requirement for
output; there is no reason for it to stop at stderr.

# Implementation notes

Two invariants that are easy to break and whose failure modes do not point at
their cause.

## List item content is sliced by byte offset, never stripped as indentation

`Marker` carries both `content` (a column, used to decide which continuation
lines belong to the item) and `content_byte` (a byte offset into the marker
line). The item's *first* line must be sliced at `content_byte`, because the
marker itself is not whitespace: `strip_indent` stops at the `-` and returns the
line unchanged, `parse_lines` rediscovers the same list in the recursive call,
and the parser recurses until the stack is exhausted. The symptom is a stack
overflow in an unrelated-looking test, not a parse error.

Continuation lines *are* genuinely whitespace-indented, and do use
`strip_indent`.

## Columns and byte offsets are not interchangeable

`indent_info` returns both because a tab advances the column to the next
multiple of four while consuming a single byte. Slicing a line by a column count
silently corrupts the slice on any tab-indented line. Rule: anything compared
against the four-column block rules uses columns; anything used to slice uses
bytes. The original code used `&text[indent..]` throughout, which is correct
only for space-indented input -- so it passed every test until a tab appeared.

## The root boundary is only as strong as the root

`join_normalize` refuses a link that climbs above the root, but "the root" has to
be a real value. It now comes from `.dankg/`, falling back to the common
ancestor of the paths named on the command line, and every `NodeId` is taken
relative to it.

Having a real one matters for more than tidiness. With no root, the boundary
silently becomes the working directory, so whether `../../etc/passwd.md` is
refused depends on how deep the path you happened to type was. The unit tests
all passed against fixtures at depth zero, where climbing out is impossible; the
leak only appeared when the binary was run against the real corpus. Any change
to path handling needs an end-to-end test, not just a unit test -- both
`tests/graph.rs` and `tests/index.rs` have a `binary` module for exactly this.

The same reasoning is why discovery lives in `index.rs` rather than in
`resolve.rs` as first sketched: deciding where the boundary is belongs with the
code that touches the filesystem, and `resolve.rs` stays a pure function of
already-parsed input.

Making ids root-relative has a second benefit: output is identical no matter
which directory DanKG was invoked from, which is a precondition for committing
it.

## The escaper and the parser must agree about flanking

`fmt` decides whether a literal `*` or `_` needs a backslash by asking whether
it could open or close emphasis. That question already has an answer in
`inline.rs`, so `can_open_close` is shared rather than reimplemented. Two copies
would drift, and the drift shows up as emphasis silently appearing or vanishing
on the first `dankg fmt` -- in a file the user has already committed.

The same rule forces the formatter to track the *semantic* previous and next
character, not the previous output byte. Emphasis is decided by neighbours, so
a child inline has to be told what its parent is about to write on either side
of it: `Writer::run` takes the trailing character for exactly this reason. The
symptom of getting it wrong is `foo _\__` formatting to `foo ___`.

## The renderer and its script cannot be type-checked against each other

`html.rs` stamps `data-key` on every line it draws; `assets.rs` recomputes that
key for lines it adds, and skips any key already present. If the two spellings
differ, nothing fails -- the script simply draws a second copy of every edge
Rust already drew, on top of the first, the moment a reader expands anything.

They did differ. The first `assets.rs` carried literal NUL bytes where the
separator should have been: invisible in the editor, invisible in the diff, and
invisible in every Rust test, because a test comparing `edge_key` to itself
agrees with itself no matter what the JS says. It surfaced only by running the
shipped script against the shipped page.

Two things came out of it. The separator now lives in Rust and is sent in the
meta blob, so there is one spelling rather than two; and `assets::CSS` and
`assets::JS` are asserted to contain no control characters, because a
hand-written asset has no compiler looking at it and an invisible byte in one is
otherwise found by nobody.

The general rule for this file: a constant both languages depend on is Rust's,
and travels. A constant duplicated in the JS is a bug waiting for a reader.

## Most list conformance failures are not list failures

When triaging the conformance table, check what else is in the failing case
before touching `gather_list`. Nearly every failure in "List items" and "Lists"
has correct list structure and fails only because the case also contains an
indented code block or a block quote, both of which are outside the subset and
render as passthrough. Implementing indented code blocks would lift three
sections at once; changing the list code would lift none.

## A syscall reporting success is not the same as it reporting something useful

`term::size`'s `ioctl(TIOCGWINSZ)` returned `rc == 0` -- success -- while handing
back an all-zero `Winsize`, observed through a terminal multiplexer that had
not yet told the kernel a real size. `Result::unwrap_or` only catches an
`Err`; a "successful" zero sailed straight through it and into `draw::window`,
which clipped every frame to a `0=x=0` rectangle. The symptom was total: not a
badly-sized drawing but no drawing at all, on every single frame, which is
what made it findable -- a partial failure would have been read as more
scrolling to do, not a bug. `app::term_dimensions` now filters on the value,
not just the `Result` variant, and is a pure function specifically so this
case has a unit test rather than depending on catching a multiplexer in the
act again.

## A placement algorithm inherits no floor it was not given one

`expand::free_slot` (tab-to-expand's nearest-free-slot placement, mirroring
the HTML renderer's `freeSlot`) has no lower bound: asked to avoid a box to
its right, "the nearest clear spot" can legally be a negative x. The base
Sugiyama layout never produces one -- `layout/coord.rs` starts every rank at
`MARGIN` and there are tests asserting left edges are `>= 0` -- so nothing
downstream had ever needed to guard against it. `draw.rs`'s `put` silently
drops anything at a negative column rather than erroring, so a crowded
expansion could make a freshly revealed node disappear with no trace,
exactly the failure mode "The renderer and its script cannot be
type-checked against each other" above describes for a different pair of
files: the bug is invisible until something happens to exercise the exact
shape that trips it, here five links crowding one anchor from the same
side. `expand::assemble` now re-establishes the invariant explicitly --
shifting every node right if the minimum placement went negative -- rather
than trusting a second, ad hoc placement algorithm to have preserved an
invariant the first one enforced only by construction.

# Testing

- CommonMark conformance table against the vendored spec, baseline-gated,
  scored through the test-only HTML oracle.
- Format round-tripping: idempotence and fixed point over the fixture corpus
  and every markdown input in the vendored spec, with the known-unformattable
  examples listed by number rather than counted.
- `dankg fmt` against the real binary: in-place rewriting, the `--check` exit
  code, and the refusal to write a file that does not verify.
- Golden `--format=json` dumps over a fixture corpus in `tests/data/corpus/`.
- Root discovery, the walk and `.dankgignore` against the real fixture root,
  which carries a `.dankg/` and an ignore file so discovery has something to
  find. An ignored file links back into the corpus, so its absence from the
  graph proves exclusion rather than merely being asserted.
- Cache equivalence, over corpora built in `CARGO_TARGET_TMPDIR`: a warm run,
  a cold run and a `--no-cache` run must agree on both the JSON and the
  diagnostics, and a corrupted or config-invalidated cache must change neither.
- Golden `--format=dot` and `--format=mermaid` dumps over the same fixture
  corpus. Layout determinism is checked three ways: repeated runs of the
  library, repeated runs of the binary, and the same corpus read in the
  opposite order.
- Layout invariants that a reader notices the moment they break -- no two boxes
  in a layer overlap, nothing starts left of the margin, every polyline begins
  at its source and ends at its target, and no bend point escapes the ranks its
  edge spans.
- The drawn formats are checked structurally rather than by shelling out to
  graphviz, which is not installed everywhere: braces balance, and every
  identifier an edge references was declared as a node.
- Golden `--format=html` over the same fixture corpus, which pins the markup,
  the stylesheet and the script in one file -- the only honest way to review a
  format whose three halves are maintained by hand. Structurally: tags balance,
  every edge endpoint was declared as a box, every marker an edge points at is
  defined, and every element id the script asks for is one the renderer emits.
  That last one is the check that the two hand-written halves still agree.
- The page is asserted to be self-contained (no URL but the SVG namespace) and
  to ship the whole index however little it draws, since a blob missing a node
  is a node that can never be expanded into.
- Link resolution table tests, including root-escape refusal.
- Eval tests use `sh` only, never the network: shared dependencies collapsed to
  one run, a cycle and an unknown dependency both reported rather than
  guessed at, a different-language dependency refused, real process spawns
  for stdout/stderr capture and a non-zero exit, a timeout that kills a
  grandchild holding the output pipes open -- pinned by wall-clock time, not
  only the `timed_out` flag, since that is exactly the dimension the bug it
  caught broke. Write-back is checked as pure text splicing: inserting after
  a block with nothing following, replacing an existing result in place,
  renormalizing drifted blank-line spacing, and preserving a file's missing
  trailing newline.
- `[keys]` parsing: defaults with no section, individual remaps, a
  multi-character value rejected in favour of its default, and a collision
  between two actions reverting the whole map rather than half of it.
- Panning: direction keys move the viewport and never the selection while
  panning is on and move the selection as before while it is off, a pan
  cannot go negative, and `render` clamps a scroll panned past the far edge
  back onto the grid.
- Eval in the TUI: cycling finds only the blocks inside a node's own line
  range, wraps, and is a silent no-op with none to find; running writes back
  and reports `ok`/`failed` on the status line and clears the cycle either
  way; navigation and `esc` both cancel a cycle, the former via the same
  `clear_transient` path movement, `tab` and `r` all share. `render` is
  checked to draw the help screen instead of the graph while `help` is on.
  `read_key` is checked to carry an unused byte from a standalone Esc into
  the next call rather than losing it, and to resolve a `pending` buffer
  that already decodes to a full key without blocking on a read that would
  hang forever against an empty reader -- the two ends of the bug a real pty
  session against the built binary found (see *Terminal UI*, *Help
  screen*), neither reachable from feeding `decode`/`read_key` one complete
  sequence at a time the way the rest of the suite already did.
- Block nodes (decision 20): a named top-level block becomes a node
  contained by its heading with the right `(line, end_line)`; an unnamed
  block or one nested in a list does not; a block before any heading
  attaches to the lazily-created file-level node; a block's own end_line
  survives `set_extents` untouched while a heading's still correctly runs
  past it to the *real* next sibling; a block name colliding with a heading
  slug gets the same `-1` suffix two same-titled headings would. Each
  drawn format is checked to give a block node its own look -- `dot`'s
  fillcolor and monospace font, `mermaid`'s `classDef block`, the HTML
  page's `.node.block` class (both server-rendered and script-grown), and
  the TUI's heavy border -- and `eval --list` is checked to list only the
  named file's blocks when given one, and every file's when given a
  directory.
- `--each`/`--all` (decision 21): `--each` still gives every block its own
  chain, dependency or not, and `--all` reports only DAG leaves, still
  pulling a leaf's full transitive chain. Whole-file, flat naming and
  `deps` resolution (decision 22, after its reversal): a dependency across
  sibling headings, with no ancestor relationship between them, still
  plans; two blocks anywhere in the file sharing a name are refused
  (`DuplicateName`) rather than allowed to coexist; `plan_for_index` runs
  the block at a given position directly, which is what `run_one` (both
  `run_single`'s loop and `tui::eval::run`) and `dankg check`'s staleness
  loop use rather than a name lookup.
- Tangle (decisions 23-28): a level-one heading (or the topmost heading of
  one with no level-one wrapper) becomes one file; two headings become two
  files; a nested heading folds into its top-level ancestor; a block before
  any heading goes to `main.<ext>`; a block nested in a list or in another
  language is not tangled; `path=` overrides the heading-derived path; the
  default output directory is `.dankg/build/<lang>/`; `ext` falls back to
  the `--lang` value when unconfigured; a configured `[tangle.*] command`
  runs against the assembled directory via `\{dir\}`, and no command
  configured stops at materializing files. A directory walks the whole
  corpus, nesting each contributing file under its own subdirectory;
  naming one file out of a larger corpus, or several files explicitly,
  tangles only what was named, with no extra nesting for a single
  contributor. `glue` runs before `command` and can see what `command`
  will (a file `glue` created is still there when `command` runs); the
  sidecar manifest is written only when `glue` is configured, never
  otherwise, and carries each tangled file's `dankg.tangle.public`
  frontmatter hint, defaulting to `false` with none.

# Milestones

1. \[DONE\] `md/` subset + frontmatter + conformance harness. 368/652 (56%).
2. \[DONE\] `graph/` model, slugs, resolution, `--format=json`. The graph is fully
   testable here, before anything is drawn: `tests/data/corpus.golden.json`.
3. \[DONE\] `dankg fmt`. Placed here deliberately: the round-trip properties
   harden the parser before the graph work starts depending on it, and the AST
   additions it needs were cheapest to make then. 648/652 spec inputs round-trip;
   the four that do not are documented above and are refused rather than
   mangled.
4. \[DONE\] Root discovery, corpus walk, cache, diagnostics. `.dankg/` marks a
   root; the walk covers all of it and honours `.dankgignore`; the cache stores
   each file's index contribution and its diagnostics, so `--no-cache` is
   byte-identical. `dankg index` reports what the walk found.
5. \[DONE\] `layout/` Sugiyama, `--format=dot|mermaid`. Pipeline step 7 came with
   it, since layout is defined over the selected subgraph: `graph/view.rs`,
   `--depth` and `--all`. Golden dot and mermaid dumps sit alongside the golden
   JSON.
6. \[DONE\] `render/html.rs` + `render/assets.rs` and the JS interaction layer.
   One self-contained file: Rust emits the coordinates, the script pans, zooms,
   expands from the embedded index, and opens source files. Expansion is a
   placement on the existing rank grid rather than a second layout engine, and
   says so on screen. `tests/data/corpus.golden.html` sits alongside the other
   goldens.
7. \[DONE\] Terminal UI: `src/tui/` (term, input, draw, app, editor, expand) over the
   existing `layout/` output -- no change to `graph/` or `layout/`.
   Selecting a node hands off to the configured `[editor] command`
   (decision 17) directly, which a browser click structurally cannot do
   for a terminal editor; `tab` grows the view onto the existing grid the
   same way the HTML renderer's script does, without a second layout; the
   drawn grid scrolls to keep the selection on screen when it outgrows the
   terminal, which is the common case rather than the exception; `p` detaches
   that scroll into an explicit pan mode, edge arrows its only indicator;
   `e` cycles a selected node's named blocks and `enter` runs the cycled one
   through `eval::session::run_one` (milestone 8) without leaving the graph,
   reporting the outcome on a one-row status line reserved at the bottom of
   the viewport; `?` is a full-screen keybinding reference. See /Terminal
   UI/ above. Letter keybindings are remappable in `[keys]` (decision 18).
   Unix termios first, Windows Console API deferred; `/` (jump to a node by
   title) is the one interaction-table key still unbound.
8. \[DONE\] `eval/` (plan, run, result) + `dankg check`. Scoped to one file at a
   time (decision 19): `plan.rs` walks `doc.blocks` directly rather than
   recursing into lists the way the graph's `named_blocks` does, refuses a
   cycle or an unknown dependency instead of guessing, and refuses a
   dependency in a different language from its target rather than
   concatenating incompatible source into one interpreter. `run.rs` spawns
   once per target's whole chain, capturing stdout/stderr on their own
   threads so a chatty process cannot deadlock the timeout poll, and kills
   the whole process group (not just the direct child) on timeout so an
   orphaned grandchild cannot hold the output pipes open past it -- found by
   timing the real binary, not by the unit tests alone. `result.rs` hashes
   the concatenated chain plus the *template* a language resolved to (never
   the substituted argv, which would embed eval's own ephemeral temp path)
   and writes results back as targeted text splicing over the exact line
   range located via the parsed `Document`, not a second `md/fmt.rs` pass --
   nothing else in the file moves. `dankg check` adds unresolved-link
   detection (reusing the whole-root index every other command already
   builds) alongside per-file staleness, and is the second CI gate next to
   `dankg fmt --check`. `session.rs` holds the interactive plan-confirm-run
   flow and `run_one`, split out from `main.rs` specifically so the TUI can
   call the same function (see *Terminal UI*, *Eval in the TUI*); `--list`
   is `session.rs`'s exploratory mode, printing every named block's name,
   language, line, containing heading and configured-or-not without running
   anything -- how to find a block's name before naming it to `--block`,
   from the CLI alone, and (decision 6) walking a whole directory as a
   corpus when that is what was named, not just the one file `--block`/
   `--all` are scoped to. A named top-level block is also a graph node in
   its own right now (decision 20), drawn distinctly in every format --
   see *Block nodes* under *Data model*.
9. Literate database management, over DuckDB. Depends on 8: it is the same
   plan-run-write-back machinery pointed at a database instead of a process.
10. `dankg serve` -- deferred, opt-in, only if the static path proves insufficient.
11. \[DONE\] `dankg tangle` (`src/tangle.rs`). Block scope reuses
    `eval::plan::top_level_blocks` exactly (decision 23), independent of
    naming: tangle never reads a block's name for anything but display, so
    decision 22's later reversal (whole-file, flat naming and `deps`
    resolution -- see *Code evaluation*) changed nothing here.
    Placement groups by the nearest heading with no parent of its own
    (`plan::root_heading`, a small generalization of "level-1 heading" that
    also handles a document with no level-1 wrapper), in document order,
    never consulting `deps`; `path=` overrides it per block. Identifiers
    reuse `graph::slug::Slugger` so a tangled file's name matches the graph
    node's own anchor for that heading. `[tangle.<lang>]` configures two
    independent, optional commands against the whole assembled tree via a
    new `\{dir\}` substitution -- `glue` (decision 27, structural
    connective tissue, run first) and `command` (build, run second);
    unconfigured, `ext` falls back to the `--lang` value itself. A
    directory (or several paths) walks the corpus (decision 26), nesting
    each contributing file under its own root-relative-path subdirectory
    once more than one is involved, and a per-file `dankg.tangle.public`
    frontmatter hint reaches a `glue` command through an optional sidecar
    manifest (decision 28) rather than DanKG's own code ever branching on
    it. See *Tangle*.

# Open questions

- Do frontmatter tags become graph nodes, or stay node attributes used for
  filtering? Currently attributes.
- Do containment edges count against `--depth`, or only link edges? Currently
  both, which may make depth 2 feel shallow in deeply nested files.
- Cross-root links: refused today. Is a multi-root mode ever wanted?
- Cross-file tangle, an eval-able-but-not-tangled escape hatch, and
  whether a language with no configured `[tangle.*] command` should still
  get a compile-check: see *Tangle*'s own *Open questions*.
