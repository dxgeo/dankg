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

**Rationale:** Files stay portable. Wikilinks stay fast to type.

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

**Rationale:** project.md:71 -- never automatic. Static output stays static.

<!-- dankg:depends target=project.md#code-evaluation quote="DanKG never evaluates code automatically." -->

## Decision 10: Block dependencies

`name` and `deps` in the fence info string.

**Rationale:** Explicit DAG; info strings are ignored by other renderers.

## Decision 11: Execution model

Deps prepended into one process.

**Rationale:** No PTY, no per-language state protocol, works for compiled langs.

## Decision 12: Results

Written back into the markdown, hash-tagged.

**Rationale:** File stays the source of truth. Results survive in git.

## Decision 13: CLI shape

`dankg graph --format=<fmt>`.

**Rationale:** JSON is a first-class, testable surface from day one.

## Decision 14: Frontmatter

Flat `key: value` subset only.

**Rationale:** ~80 lines; covers real usage; warns rather than guessing.

## Decision 15: Format safety

Re-parse and compare before writing.

**Rationale:** `fmt` writes in place. A round-trip bug must not reach a note.

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

**Rationale:** Placement is presentational structure. `deps` stays eval's own DAG concern.

## Decision 25: Command name

`dankg tangle`, not `compile`.

**Rationale:** DanKG assembles. A configured command does any actual compiling. Some languages compile nothing at all.

## Decision 26: Tangle corpus scope

One file unchanged. A directory (or several paths) walks the corpus, nesting each contributing file under its own subdirectory once more than one is involved.

**Rationale:** A crate is a whole-corpus artifact. `deps=` has no bearing since tangle never reads it.

## Decision 27: Module glue

An external, per-language, spawned `[tangle.<lang>] glue` command; never DanKG's own code.

**Rationale:** Matches the spawned-command pattern (decision 1). Lets an extension do things DanKG's own code must not (read a block's source).

## Decision 28: Per-file tangle hints

`dankg.tangle.public` frontmatter, surfaced to `glue` only through a sidecar manifest.

**Rationale:** Visibility is a per-file authorial call. Whether the mechanism runs at all stays root-config only (decision 27).

## Decision 29: Cross-file `deps`

`deps=other.md#name` reaches another file's top-level block. `--block`/`--all`/`--each`' own targets stay one file (decision 19 unchanged).

**Rationale:** Concatenation (decision 11) makes "the chain crosses a file" and "the chain crosses a language" the same *kind* of non-problem. Only resolution needed widening, not the execution model.

## Decision 30: Manifest schema v2

Adds a `blocks` array (name, `line`, `end_line`) per file, straight off the same `BlockRef` fields `result.rs` already tracks.

**Rationale:** A richer glue script needs block-level source position to compare against, never a paragraph's own text. DanKG hands over structure. Judgment stays glue's (decision 27).

## Decision 31: Cross-language dependency

`xdeps=name` resolves exactly like `deps=` (decision 29's same-file/cross-file lookup). A target is never concatenated into the chain. It always runs alone, and its staleness hash folds in the referenced block's own hash by verified reference instead.

**Rationale:** Decision 11's refusal to mix languages in one concatenated chain is correct and stays unchanged, but it left no way to track staleness across a boundary it cannot cross. `xdeps=` is a second, narrower mechanism for exactly that gap, not a loosening of decision 11.

## Decision 32: Prose dependencies

`<!-- dankg:depends target=... quote="..." -->` pins one quoted claim against another section, checked by whitespace-normalized substring match, never by hash. `dankg check` reports a miss but never fails its exit code on one.

**Rationale:** A section of prose has no execution to re-derive and compare, the way a code block's output does. Hashing a whole section would flag a typo fix as loudly as a meaning change. A substring match on the specific claim a marker pins is a narrower, weaker, but far less noisy signal, and a weak signal that gates a build trains a reader to silence it rather than read it.

## Decision 33: File artifact dependency

`produces=file:PATH` on a writer block and `reads=file:PATH` on a reader, each paired with an ordinary `deps=`/`xdeps=name` edge naming the other block directly. `dankg check` adds one comparison on top of that edge's own verified hash: the reader's `reads=` path must equal its named dependency's `produces=` path, textually.

**Rationale:** `xdeps=name` (decision 31) already verifies a cross-language producer's source hash, but says nothing about which file that block actually writes. Two blocks can each rename their own path independently and drift apart while the named edge still reports fresh. `produces=`/`reads=` catch exactly that drift, and need no database, no execution, and no resolution logic beyond the edge that already exists.

## Decision 34: Title collision check

`dankg check` reports every heading whose title collides with an earlier heading's in the same file. It classifies each as *referenced* (a written link or `dankg:depends` marker already targets one of the pair's two slugs) or *cosmetic* (nothing does). It also classifies each, separately, as *sibling* (same immediate parent) or *differently-nested*. Neither classification fails the exit code. `graph::build::title_collisions` scopes the underlying collision detection to headings. A named, top-level block sharing its own containing heading's title is excluded.

**Rationale:** A colliding heading still gets a distinct slug from `Slugger` (*Slugs and node identity*, below). The file resolves correctly exactly as written. But that slug is order-dependent. Renaming, reordering, or deleting the earlier same-titled heading silently repoints anything already pinned to the later one's suffix. Only a pair something actually references is at real risk of that. Splitting referenced from cosmetic makes the report actionable. The reader no longer has to verify it by hand. Sibling versus differently-nested is a different question. It asks whether a *human* reading the raw document, not a link resolver, is likely to confuse the two. A block is excluded for a different reason. It cannot precede the heading that contains it. That particular pair can never actually reorder.

## Decision 35: Relation-targeted dependency

`xdeps=table:NAME` names a relation instead of a block, resolved against the whole corpus's own `Produces` edges rather than decision 29's file-scoped lookup. Exactly one block may produce that relation; zero or more than one both refuse, the same as an unknown `xdeps=name` target already does. Once resolved, it behaves exactly like a block-named `xdeps=` (decision 31): never concatenated, its staleness hash folded in by verified reference to the producing block's own hash.

**Rationale:** A `Reads` edge is inferred only from a SQL block's own query text. A block written in another language has no parseable SQL for DanKG to check, so it has no way to declare that dependency today. `Produces` is inferred from a database snapshot diff, not authored, so a fixed block name would go stale the moment a different block started producing the relation. Naming the relation instead keeps the binding live.

## Decision 36: Inferred relation staleness

A SQL block's inferred `Reads` edges are written back with its result, one resolved `table:NAME` per relation the block's own query names but did not create, exactly as if the block had declared `xdeps=table:NAME` itself. `dankg check` verifies each the same way decision 35 already verifies a declared one: recursively, against the producing block's own hash, refusing on a missing or ambiguous producer.

**Rationale:** An edge recorded only for `dankg graph`'s picture of lineage never feeds `check`, so a downstream block can drift stale against its real upstream table with nothing to notice. Recording an inferred `Reads` edge in the same `table:NAME` form decision 35 already resolves and verifies means an inferred dependency costs no second mechanism. Hashing the relation's own content instead was considered and rejected: a source can change while its output happens to look the same, the exact coincidence [agent_tests/deps_pilot.md](agent_tests/deps_pilot.md) found dangerous.

## Decision 37: Live catalog, opt-in only

`dankg graph`/`tui` never touch a database by default: `graph` still only displays stored results. `--live` is the explicit opt-in, spawning each `[db.*]`'s own configured `list` command read-only and reading its stdout as one relation identifier per line -- no protocol assumed, the same allowlist rule as `[lang.*]` (decision 1). The same `list` command is what *Provenance without a driver*'s own before/after diff already runs (decisions 35, 36): one protocol-agnostic primitive, two callers. A `[db.*]` with no configured `list` is reported, not run. Every listed identifier with no `Produces` edge in the current index renders as its own kind: a relation the corpus does not explain. Nothing from `--live` itself is cached or written back.

**Rationale:** A corpus only knows the relations some block's own run happened to produce. A table created by hand, by a tool outside `dankg`, or documented once by a section since deleted, is invisible to it, and that is exactly the gap `--live` closes: the reader asked, `dankg` spawned one read-only command, and anything it found with no explanation gets flagged. Making that command itself protocol-agnostic keeps decision 1 intact for this too: DanKG must not learn a wire protocol or link a client library just to ask "what tables exist," so the reader supplies the one command that answers it, exactly as `[db.*] command` already does for running a block's own SQL. The same reasoning is why the automatic diff runs through `list` too, rather than assuming DuckDB's own catalog functions: DuckDB is only the first engine this gets tested against, not a special case baked into the code.

## Decision 38: Relation nodes cost nothing to enter

An edge landing on a relation node -- either direction of `Produces`/`Reads` -- costs 0 against `--depth`. The same edge kind leaving a relation, onto a block, costs the ordinary 1. A relation therefore renders alongside every block already in view that touches it, at no budget cost, while a further block reached through it still costs exactly the hop a direct edge would.

**Rationale:** Containment already costs a hop (*View selection*, above), and that is flagged there as a problem, not a model to repeat here. A relation reads more like an attribute of the block that produces or reads it than like a fifth heading a reader had to click through to reach -- "a table one hop from its ETL is not really a hop." Zero-cost entry only, though: if leaving a relation were free too, every block that ever touched it would collapse to zero distance from every other, and `--depth` would stop bounding anything once a corpus had one widely-shared table. Lineage stays exactly the traversal *Provenance without a driver* already promises, just one that never double-charges for stopping to look at the table itself. One real hop still buys every other block touching the same relation, sibling producer or downstream reader alike, once a reader spends it. That is not a leak to guard against. A shared table's other writers are exactly the kind of structure the induced-subgraph rule already refuses to hide once it is one hop away, and `--depth 0` is the reader's own filter for not wanting it yet.

## Decision 39: Row count stays out of the hash

A `SELECT` result's row count, or any other captured output, never enters the staleness hash. `dankg check` still only recomputes source: the concatenated chain, its resolved template, and any `xdeps=`/`table:NAME` it verifies (decisions 31, 35, 36). Whether a captured result still matches live data is a different question, answered by `--live` (decision 37), not by `check`.

**Rationale:** Every hash `dankg` computes is a source hash, never a content hash -- the direct lesson of [agent_tests/deps_pilot.md](agent_tests/deps_pilot.md): a source can change while its output happens to look the same, so hashing output instead risks silence on exactly the change that matters. A snapshot's row count adds nothing a source hash does not already cover, since nothing changes it without the SQL re-running. A running pipeline's row count drifts independent of source by definition, so no hash bit can represent it as a single stale/fresh signal without answering a question `check` was never built to ask. The two cases do not actually disagree; they fail for different reasons and land on the same answer.

# Terminology

- root :: The directory defining one knowledge base. Everything under it is in
  the corpus. Everything outside is not. Never called a "vault".
- node :: One heading, at any depth, in one file.
- containment edge :: Parent heading to child heading. Derived from document
  structure, not written by hand.
- link edge :: An explicit reference written by the author.
- entry :: The file or node named on the command line. Sets the view, never the
  index.
- block :: A fenced code block carrying a `name` in its info string.
- prose dependency :: A `dankg:depends` marker (decision 32): a quoted
  claim pinned against another section, checked by substring, never a
  graph edge.
- file dependency :: A `produces=`/`reads=file:PATH` pair (decision
  33\): a declared artifact path, checked for agreement against an
  existing `deps=`/`xdeps=name` edge, never resolved on its own.

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

Extent is a line range rather than a byte range. The parser carries line numbers
throughout. A line range is what diagnostics and editors both want.

A file with no headings, or with links written above its first heading, gets a
synthetic level-0 node named from its frontmatter title or file name, so that
`[text](file.md)` always has something to land on.

## Block nodes

A named, top-level code block is a node too (decision 20). It is scoped to
`eval::plan::top_level_blocks`'s exact definition (decision 19) rather than to
"every fenced block". A selectable node that `dankg eval` cannot run would be
a dead end. So "is this a node" and "is this eval-able" are made the same
question by construction, not by convention. Concretely, a block node must sit
outside a list and carry a name (`name=` present in its info string). An
unnamed block, or one inside a list item, is invisible to the graph exactly as
it already is to eval.

<!-- dankg:depends target=#decision-20-block-nodes quote="Node scope == eval scope, exactly (decision 19)." -->

`graph/build.rs` builds a block's node in the same single pass as headings and
paragraphs. It attaches the node to whichever heading is `current` at that
point in the document, or, before the first heading, to the lazily-created
file-level node (the same fallback a pre-heading paragraph already triggers).
A block never becomes `current` itself: nothing nests inside one, so later
content keeps attaching to whichever heading was already open. Its slug comes
from the same per-file `Slugger` a heading's does, so a block named `index`
colliding with a heading titled "Index" gets the `-1` suffix exactly as two
same-titled headings would.

A block has no heading depth for `level` to carry. There is none. Instead
`level` holds a sentinel (`BLOCK_LEVEL = 7`), picked to sit above every real
heading level (1..=6). That single fact is what keeps `set_extents` correct
without a special case in its search. Scanning past a block while looking for
a heading's own next sibling-or-higher never mistakes one for it, since a
block's level can never satisfy `<= a real heading's`. Overwriting a block's
own end_line the way a heading's is computed would be wrong regardless.
`Block::Code::end_line`, straight from the parser, is already exactly right.
So `set_extents`'s outer loop is scoped to `kind == Heading` nodes, full stop.
By the time it runs, no block node exists yet to touch anyway. Block nodes are
appended to the same pass that produces headings, and `set_extents` runs once,
after.

Edges are stored directed. `reciprocated` is filled in after the whole corpus
is indexed. If `a -> b` and `b -> a` both exist, both are marked, and the
renderer renders one undirected edge instead of two arrows. This is the
mechanism behind "the link goes both ways but renders as unidirectional unless
linked back".

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
  depends.rs         dankg:depends marker: quote-anchored prose staleness
```

# Markdown subset

Implemented: ATX headings, fenced code with info strings, inline and reference
links, wikilinks, unordered and ordered lists including nesting, emphasis and
strong, code spans, paragraphs, thematic breaks, hard breaks.

Passed through as literal text: setext headings, HTML blocks, tables, block
quotes, indented code blocks, entity references, autolinks, images, and link
reference definitions.

Link titles are parsed and kept even though DanKG has no use for them. An AST
that discards input it has already read cannot be rendered faithfully. The
field costs nothing.

The vendored CommonMark `spec.json` lives at `tests/data/commonmark/spec.json`.
It is test *data*, not a dependency. Scoring it requires rendering the AST to
HTML. So `tests/support/html.rs` implements a CommonMark HTML renderer used
*only* as a conformance oracle. DanKG itself never renders markdown to HTML.
It renders a graph.

The harness is a regression gate. Unimplemented sections never fail a build,
but a section that loses ground does.

```
cargo test --test commonmark -- --nocapture   # full per-section table
DANKG_SHOW=Links cargo test --test commonmark -- --nocapture
DANKG_BLESS=1 cargo test --test commonmark    # re-record the baseline
```

Current conformance is 368/652 (56%). The implemented sections score as
intended: emphasis 94%, ATX headings 94%, code spans 90%, fenced code 89%.
Nearly all remaining failures are constructs deliberately outside the subset.
Notably, most list failures are not list bugs. The list structure is correct.
The case fails only because it also contains an indented code block or a block
quote.

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
taken relative to it. Output is the same wherever the binary was run from. The
resolver refuses any link that climbs above it.

Discovery walks up from the named path looking for a `.dankg/` directory. The
directory containing it is the root. Failing that the root is the common
ancestor of the paths given, which for a single file is its own directory. A
`.dankg/` is never created. A directory the user did not mark is not a root.
It is just where a file happens to live.

Naming a second path in a different root is refused. Cross-root graphs would
need a second boundary. One boundary is the point.

The walk covers the whole root, always (decision 6). It yields root-relative
`/`-separated paths in sorted order. Filesystem order is not deterministic and
would leak into the output. Three exclusions are structural rather than
configured:

<!-- dankg:depends target=#decision-6-index-scope quote="Whole root, always." -->

- Dot-entries are skipped outright, which is what keeps `.dankg/` and `.git/`
  out of the corpus without anyone writing a pattern for them.
- Symlinks are never followed. They are the one way a walk could leave the
  root. The root is a hard boundary. Every skipped link is reported.
- Paths are resolved lexically, never with `canonicalize`, for the same reason.
  Resolving a link before the boundary check is what would defeat it.

A path named on the command line is always indexed, even when `.dankgignore`
excludes it, with a warning saying so. Naming a file is an explicit request.
It should not fail silently. But neither should the override be invisible.

## .dankgignore

One pattern per line. `#` comments. `!` un-ignores. A leading `/` anchors to
the root. A trailing `/` matches directories only. `*` and `?` stay within one
path segment. `**` crosses them. Later rules win, so an exception can follow
the rule it excepts. A pattern with no `/` applies at every depth.

Deliberately a subset of gitignore rather than a clone of it. The whole point
of a root is that its contents are predictable. A matcher nobody can predict
would undo that.

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
index is the whole root. The view is what a reader can take in.

Hops are counted in *both* directions. A link pointing at the entry is as much
a neighbour as one the entry points at. That is the entire reason the index is
built over the corpus rather than over the one file. Containment counts as a
hop too. That is the open question below. It can make depth 2 feel shallow in
a deeply nested file. `--all` is the answer until something better is decided.

The result is the *induced* subgraph, not a spanning tree. An edge between two
nodes that both made it in survives even when it was not the edge that brought
either of them there. Dropping it would render a graph missing structure it
can plainly see.

Naming a file selects every heading in it, not just the first. Naming a
directory leaves no entry to start from. The only sensible reading of "graph
this corpus" is all of it. So that is what it does.

# Layout

Standard four-phase Sugiyama over the selected subgraph.

1. *Acyclic*: DFS, reverse back edges, remember them so arrowheads still point
   the original way. Nothing is dropped. A self-link is marked instead, since
   it cannot be layered, and is rendered as a loop.
2. *Rank*: longest-path layering. Edges spanning more than one layer are split
   into one-layer segments joined by virtual nodes, which become the bend
   points of the rendered polyline.
3. *Order*: median heuristic, four alternating sweeps, keeping the best
   ordering seen. Ties broken by NodeId so the result is deterministic.
4. *Coordinate*: priority method for x, fixed layer height for y. Edges through
   virtual nodes become polylines.

Containment edges carry weight 2 and link edges weight 1. The weight is what
the *ordering and coordinate* phases read, not the ranking. Longest-path
layering already puts a child one layer below its parent. What weight 2 buys
is horizontal: a parent is pulled into line with its children rather than with
whatever else happens to link to it. That is the sense in which a heading sits
directly above them.

Priority in phase 4 goes to the virtual nodes, ahead of every real one. A long
edge that zigzags is far harder to follow than a box sitting slightly
off-centre. So the bend points get the position they ask for. The real nodes
move aside. Ordering is never changed here. A node may slide within its layer
but can never overtake a neighbour, so phase 3's crossing count survives.

Coordinates are integers. Whole pixels have no float formatting to disagree
about. The output is meant to be committed.

Determinism is a hard requirement, not a nicety. It is what lets a rendered
graph be committed and diffed. Nothing here iterates a hash map. The starting
order comes from a depth-first walk in index order rather than from whatever
the graph handed over. Tests assert byte-identical output across repeated
runs and across the order the corpus was read in.

# Drawn formats

Both take the same `Layout`. What they can do with it differs. Pretending
otherwise would be the dishonest part.

- *dot*: node positions are emitted as pinned `pos` attributes, so
  `neato -n -Tsvg` reproduces DanKG's layout exactly. Plain `dot -Tsvg` throws
  them away and re-lays the graph out. But the `rank=same` groups and
  `weight=2` on containment mean it still agrees about which node belongs on
  which layer. Edge geometry is deliberately *not* emitted. Graphviz wants
  B-spline control points. DanKG has polylines. Converting between them to
  satisfy a flag most people will not pass is not worth a bug in the
  arithmetic.
- *mermaid*: runs its own layout and will not take coordinates at all. What
  it takes is the ordering. Nodes are emitted rank by rank, left to right.
  Edge direction is DanKG's, including which edge of a cycle was turned
  around. Identifiers are `n0`, `n1`, ... because mermaid's identifier grammar
  does not admit `#` or `/`.

A reciprocated pair is one undirected line in both. The half that gets
rendered is the one the layout ran *down* the page. So a renderer doing its
own layering ends up agreeing with DanKG about which node sits above which.
Unresolved nodes are dashed and muted in both. A block node (decision 20)
gets a tint and a monospace label in `dot` (`fillcolor`/`fontname`, since
`resolved` is always true for one, the two states never fight for the same
box). It also gets its own `classDef` in `mermaid`, collected into a `class`
line the same way dangling nodes already are. This is distinct from an
ordinary heading's, never dashed, since a block is never unresolved.

# HTML renderer

One file. CSS and JS are `const &str` in `assets.rs`, inlined at render time. No
network requests, no build step, no server. The only URL in the page is the SVG
namespace, which is an identifier rather than an address. A test asserts there
is never a second one.

The Rust side emits final SVG coordinates. The JS does three things only: pan
and zoom, click a node to expand its hidden neighbours, and click through to
the source file. Expansion is instant because the full index ships in the page
as a JSON blob alongside the visible subgraph. It is the canonical
`--format json` dump rather than a bespoke shape. So the page and the
scriptable surface cannot disagree about what the graph is.

Unresolved nodes render dashed and muted. A block node (decision 20) renders
with a `.node.block` class: a tint and a monospace label, distinct from an
ordinary heading's. This holds both when the node is server-rendered and when
it is script-grown, since the grown path reads `data.kind` straight out of the
same embedded index that the server-rendered path reads `Node.kind` from.
Reciprocated edges render as a single line with no arrowhead. Stale results
render with a warning badge.

<!-- dankg:depends target=#data-model quote="always true for a block" -->

## Expansion is a placement, not a second layout

Clicking a node reveals its hidden neighbours from the blob. They are dropped
into the nearest free slot on the rank the edge puts them, on the same grid
`layout/` already fixed. They are not laid out again. Running Sugiyama in the
browser would move every box on screen, which is exactly what a reader tracing
one link does not want. It would also need a second layout engine in a second
language.

The price is that the expanded rendering is not the rendering `--depth N+1`
would produce. So grown boxes are rendered as provisional rather than passed
off as authoritative. Re-running DanKG is how you get the real layout of the
larger graph. `reset` returns the page to exactly what Rust rendered.

## One click never means two things

The box is the expand toggle, so opening the source file needs its own target:
a small badge on the box's top-right corner, in the gap the layout already
leaves between boxes, costing the label no characters.

The `href` is the root-relative path, which means the page works where it is
meant to live: at the root. An absolute path would work from anywhere and
could not be committed. Committing the output is the point.

## What the script is told rather than trusted to know

Anything both halves need is sent in the meta blob: the box metrics
(`CHAR_WIDTH` and friends, so an expanded box is sized the way the layout sized
the rendered ones) and the edge-key separator. The rule is that a value used by
both languages lives in Rust and travels, because nothing type-checks the two
against each other. See the implementation note below for what it cost to learn
that.

# Terminal UI

Milestone 7, placed before eval: it depends only on `graph/` and `layout/`
(milestones 2 and 5), not on anything eval or the database milestone add. So
there is no ordering reason to wait.

Selecting a node and handing it to the reader's own editor only works
unconditionally from inside a terminal. A browser click cannot spawn an
arbitrary local process. Only editors that register an OS URI scheme
(`vscode://`) can be reached from HTML at all. `vim`, `nvim`, and `emacs` are
structurally unreachable from a page. A TUI lives in the same terminal as the
editor it hands off to, so any configured `[editor] command` (decision 17)
reaches it, no scheme required.

## What is reused unchanged

`graph/` does not change. The tree's own structure is `Node.parent`,
already computed by `graph::build` for every heading and block; the
named entry file(s) resolve to top-level roots via the identical
`graph::view::entry_nodes` `dankg graph` itself uses to find its own
entries. `graph::query::links_for` (new this pass) follows the same
read-only-lookup convention `find_producer`/`live_orphans` already
established there: a pure function over an already-built `&Graph`,
nothing it computes ever mutates the corpus. `layout/` is *not* reused.
The TUI stopped consuming a `Layout` entirely when it moved from a
Sugiyama graph rendering to a tree -- see *The tree and the
cross-reference panel*, below.

## What is new

```
src/tui/
  term.rs     raw mode, alternate screen, size query; a Drop guard restores
              the terminal on panic so a crash never leaves a broken shell
  input.rs    hand-parsed ANSI escape sequences -> key events
  draw.rs     tree/panel rows -> character grid, two independently
              scrolled panes joined by one divider column; no graph or
              layout knowledge at all; one buffered write per frame
  app.rs      event loop, the tree (children/expanded), the panel, focus
  editor.rs   suspend term.rs, spawn [editor] command with {file}/{line},
              wait, resume, re-index (the file may have just changed)
  eval.rs     find a node's named blocks, run one via eval::session
              (milestone 8) without leaving the tree
```

## Interaction

- `arrows/hjkl`: move. Up/down walk the visible tree rows (or the panel's own rows, while it has focus). Left/right collapse/expand a node, or step onto its parent/first child if it is already collapsed/expanded (the standard file-tree convention).
- `enter`: suspend, spawn the configured editor at the node's line, resume. Or, while cycling a node's blocks, run the cycled one. Or, while the panel has focus, jump to the focused link.
- `tab`: toggle focus between the tree and the cross-reference panel. Its old job -- revealing hidden neighbours onto a 2D graph grid -- has no equivalent once there is no grid.
- `/`: jump to a node by title, anywhere in the corpus (see *Jump and default depth*, below). `enter` confirms, `esc` cancels.
- `n`/`N`: jump to the next/previous match of the last confirmed search, wrapping past either end, reporting the match's own rank and the total match count on the status line -- vim's own binding exactly. An earlier pass bound `p` instead of shift-`N`, since `p` was free once panning retired; that saved nothing an experienced vim user would notice and cost them a keybinding they already knew, so it went back to `N`.
- `e`: cycle the selected node's named blocks. `enter` runs the cycled one, in place, without leaving the tree (see *Eval* below)
- `esc`: cancel an in-progress block cycle or an in-progress search; return focus from the panel to the tree.
- `r`: collapse back to the entry view
- `q`: quit, restoring the terminal
- `b`: toggle the origin breadcrumb on the status line (see *Origin breadcrumb*, below)
- `?`: toggle a full-screen keybinding reference

Movement follows the flattened list of currently-visible tree rows, so
"down" is well-defined without inventing a second notion of adjacency --
the tree-view successor to what `layout/order.rs`'s rank/order structure
once gave the Sugiyama renderer.

Every letter here but the mode-independent bindings above is remappable in
`[keys]` (decision 18). Arrows, enter, tab, esc, `/`, `n`, `N`, and `?` are
not, since they are not tree-navigation letters to begin with. `/` and `?`
specifically are fixed because both are close to universal across terminal
tools (`/` to search in vim/less/htop, `?` for help in the same set); `n`/
`N` are fixed alongside `/` for the identical reason -- they are the other
half of the same search feature, not independent actions a reader would
want rebound on their own.

## Eval in the TUI

Milestone 8 built `eval/` as a library, not a `main.rs` orchestration function
like `graph`/`fmt`, specifically so this could reuse it. `eval::session::run_one`
(rebuild a named block's plan, resolve its language, spawn its chain once,
write the result back) is now the one place "run this block" is
implemented. It is called both by `dankg eval`'s own multi-target loop and by
`tui::eval::run`. The TUI cannot depend on the `main` binary, so a function
two different front ends both need has to live in the library either way.
This is the same reasoning as `cmd.rs` already being shared between the
editor handoff and `eval`'s own command spawning.

`e` (`keys.eval`) looks up the selected node's named blocks (every
top-level block whose own line falls in `node.line..=node.end_line`, exactly
the section `graph/build.rs` already computes that heading to span) and
starts cycling on the first one. A second press advances to the next,
wrapping. `enter` runs whichever is currently cycled, exactly the way `enter`
already runs the editor, except it never leaves the TUI. `eval::run` spawns
and captures output through pipes (`eval/run.rs`), not through inherited
stdio, so nothing about it needs the terminal suspended. No separate confirm
prompt either. Cycling to a block and pressing `enter` to run it already
*is* the confirmation, the same reasoning decision 9's `dankg eval` prompt
does not apply to `enter`'s editor handoff.

A node with no named blocks in its section is a silent no-op on `e`. There is
nowhere to cycle to, the same "nowhere to report to" call `enter`'s
best-effort editor-spawn failure already makes. Navigating away (any
direction key, `tab`, `r`) cancels an in-progress cycle. It was scoped to
whichever node was selected when it started, and moving off that node makes
it stale. `esc` cancels it explicitly, without moving anything.

### The status line

One row, reserved at the bottom of the viewport whenever something
claims it: `app.status`, the `/query` typed so far while `app.search`
is active, or the origin breadcrumb (below). `render` computes
`content_rows = term_rows - status_rows` up front (`status_rows` is 0
or 1, never more, since only one claim is ever shown at a time) and
windows the graph into that, the same fixed-upper-bound reasoning as
everywhere else column/row budgets get clamped in this module. Search
outranks status, which outranks the breadcrumb -- the two the reader
is actively acting on always win the one shared line. It shows the
block-cycle list while cycling
(`eval: [setup] index   enter=run esc=cancel`, the cycled name
bracketed) and the last run's outcome afterward (`index: ok`,
`index: failed`, `index: timed out`, or the error text for something
that could not even be attempted, such as an unconfigured language).
`reload`, which a completed run always triggers since the file just
changed, leaves `status` alone on purpose. The reader just ran the
block. Reloading is not itself a reason to hide what happened.

### Origin breadcrumb

While the panel has focus, hovering a link moves the tree's own
underlined row onto that link's target (*Jump and default depth*,
below), so `self.selected` -- the node the reader tabbed away from to
start exploring links in the first place -- ends up with no marker
anywhere on screen. A reader who hovers a second link has already lost
track of where they started. `keys.breadcrumb` (`b`, default) toggles
a status-line breadcrumb, `self.selected`'s own title, for the running
session. `[tui] breadcrumb` (default `true`) sets whether it starts on
at all; the key still overrides that default for the session either
way. It only ever shows while the panel has focus and only when
neither the search prompt nor an eval outcome already claims the
line -- both are the reader's own immediate action, so neither should
have to compete with a breadcrumb for the one row they share.

### Block nodes and the cycle key, side by side

Decision 20 made a named top-level block a node in its own right, rendered
with a heavy border (`draw_box`'s third glyph set, alongside plain and
dashed: `┏━┓┃┗━┛`) wherever it already sits in the graph. It is reachable by
ordinary arrow/hjkl navigation, not only by `e`. This was added after `e`'s
cycle mode already existed, deliberately left standing rather than replaced.
Cycling answers "what can I run from here" without moving the selection or
the viewport at all, which staying on the current node and pressing `e`
repeatedly still does more directly than navigating to a block node instead.
Both paths end at the same call (`eval::run`, `run_selected_block`). So they
cannot disagree about what running a block does, only about how a reader
gets there.

### Discoverability outside the TUI

`dankg eval <path>... --list` (`eval::session::list_blocks`/`list_corpus_text`)
answers the same question a CI script or a reader without a terminal needs
answered: every top-level named block, its language, source line, containing
heading (the nearest heading at or above the block's own line, a cheap
approximation of the same containment `graph/build.rs` computes properly for
the node itself, without needing the whole graph pipeline just to answer
"what can I run here"), and whether that language is configured at all. It
runs nothing and needs no confirmation, the same as `--format json` being the
graph's own pipeable, "requested output" surface (decision 13's reasoning
applied to eval).

<!-- dankg:depends target=#decision-13-cli-shape quote="JSON is a first-class, testable surface from day one." -->

Unlike `--block`/`--all`, scoped to exactly one file because `deps=` only
resolves within one (decision 19), `--list` has no execution to scope. A
named file lists just its own blocks. A directory (or several paths, or
nothing, defaulting to `.`) walks the whole corpus via `index::load` the
same way `graph`/`index`/`check` already do (decision 6) and lists every
file's, each line still prefixed by its own root-relative path.

## Help screen

`?` is fixed, not remappable (see *Interaction* above), and toggles a
full-screen keybinding reference (`app::help_lines`) that *replaces* the
graph rather than overlaying it. There is no compositing in this module.
Full-screen takeover is exactly what `enter`'s editor handoff already
does for the same reason. It reads `keys` live, so a remapped letter shows up
correctly rather than the reference silently going stale next to a config
that no longer matches it.

Help mode is fully modal in the event loop. Every key but the dismissers
(`?`, esc, `keys.quit`) is swallowed before it reaches the tree/panel's own
match arms, so nothing about the selection, panel focus, or an in-progress
eval cycle can change while help is on screen. `write_frame` (the
buffered-write, no-trailing-`\r\n`-on-the-last-line logic decision-critical
to not scrolling the alternate screen: see the Terminal UI intro) is shared
between the tree/panel frame and the help screen, the only two things this
module ever renders. It deliberately does not clip columns itself, since a
frame line carries ANSI attribute codes that count as characters but not
screen columns. Column-clipping those would cut one off mid-escape-sequence.
Plain-text callers (the status line, help's own lines) clip themselves
before handing `write_frame` anything.

### A byte lost after a standalone Esc

Binding `esc` to something real surfaced a latent bug in `input.rs` that
nothing had ever exercised. \[`decode`\] tells a lone Esc apart from
`ESC [ <letter>` by reading one more byte. When that byte is not `[`, it
correctly reports using only 1 of the 2 bytes it looked at. But
`read_key`'s loop discarded its whole buffer between calls. So that second,
unused byte (the start of whatever the reader actually typed *next*)
simply vanished. In a debug build this tripped a `debug_assert_eq!` the loop
carried for exactly this invariant. In a release build there was no assert
to catch it. So the keystroke right after every standalone Esc was silently
eaten, and the reader had to press it twice. Nothing before this session ever
bound standalone Esc to anything, so the path was real but unreachable by
any interaction table entry. It was a true "documented gap" until eval's
cycle mode gave Esc a job and made it load-bearing.

The fix threads a small `pending: Vec<u8>` through `read_key` across calls,
owned by `event_loop`. A byte `decode` reports as unused is carried into the
next call instead of discarded, decoded first (before any new read, so a
`pending` buffer that already resolves to a full key never blocks trying to
read more), and drained back out to whatever is left over each time. Found by
driving the real binary through a pty, not by the unit tests alone. The
existing suite only ever fed `decode` and `read_key` complete, single
sequences in one shot, never a standalone Esc immediately followed by
another real keystroke in the same read.

## The tree and the cross-reference panel

`dankg tui` rendered a Sugiyama graph layout, boxes and polylines on a
character grid, until this pass replaced it with a nerdtree-style
collapsible tree plus a persistent detail panel. The corpus's own graph
is overwhelmingly a containment hierarchy -- on this repo's own
self-hosted corpus, 496 `Contains` edges against 56 `Link` edges and a
handful of `Produces`/`Reads` -- so a general-DAG layout was spending
its whole visual budget (crossing lines, wide ranks) on structure the
data barely has. A tree matches its actual shape.

The tree always represents the *whole* resolved corpus (`App::index`),
unconditionally -- not a `--depth`-limited view the way `dankg graph`
selects one. Every top-level node (`Node.parent: None`, excluding
`Relation`) is a top-level tree row from the first frame, whichever
file(s) were named on the command line or not. `--depth`/`[tui] depth`/`--all` no longer decide what is *loaded*; they decide only how
many containment levels below the named entry file(s) start
*pre-expanded* (`App::initial_expansion`). Every other top-level file
still appears, collapsed to one line -- the same thing a real file-tree
plugin already does: it shows the whole project, not just whatever file
happens to be open. `right` expands a collapsed node in place, or steps
onto its first child if it is already open; `left` collapses one in
place, or steps onto its parent -- the standard file-tree convention,
replacing rank/order-based movement entirely. `r` re-collapses
everything back to that initial state, not just the cursor.

<!-- dankg:depends target=#decision-7-view-scope quote="Entry + 2 hops, expandable in the browser." -->

`Relation` nodes are deliberately never tree rows: a relation belongs to
a database, not a file (`db:NAME`, a synthetic namespace with no
`Node.parent`), so it has no natural place in a per-file containment
tree. It only ever appears as panel text (below), never something the
tree itself walks into.

The right pane is what makes a corpus's "surprising cross-link" visible
without a drawn graph to see it in: the selected node's own outgoing
links, backlinks, and produced/read relations (`graph::query:: links_for`), refreshed every time the selection moves. Links are
navigable -- `tab` gives the panel keyboard focus, up/down move its own
cursor among the navigable rows, `enter` jumps to one. Relations are
plain text, never navigable: a relation node has no file/line to jump
to. `esc` or `left` while the panel has focus returns focus to the tree
without acting.

Jumping -- from a panel link, or from `/`-search (below) -- means
bringing a node that may not currently be visible on screen, since the
tree only shows what its ancestors' own expansion state allows.
`App::reveal_and_select` force-expands every ancestor along a target's
`Node.parent` chain, even ones the reader never opened, then selects it.
Both callers need the identical operation, so there is exactly one
implementation of it, rather than the panel and search each growing
their own.

`enter` is what makes a panel jump permanent, but the reader does not
have to press it to find out where a link goes first. While the panel
has focus, `render` previews whichever row the cursor is currently on:
the tree shows that link's own target, ancestors opened for that one
frame only (`App::visible_rows_with`, never touching `self.expanded`),
in place of the real selection's underlined row. Moving the panel cursor
updates the preview immediately; leaving without pressing `enter`
(`esc`/left) leaves `self.expanded`/`self.selected` untouched, and the
tree pane's own `scroll_to_show` snaps back onto the real selection on
the very next frame, the same as any other move already does. Only
`enter` calls `reveal_and_select` for real. A panel row with nothing to
preview (a `Relation` line, or an empty panel) simply falls back to
showing the real selection, as if there were no preview at all.

Each tree row's compact badge (`→1 ←2 ⚭`) summarizes the same
`links_for` result the panel shows in full for the *selected* node --
at-a-glance scanning for every *other* row, so a reader does not have to
select something just to learn whether it connects to anything at all.

## Dependency surfacing

A `Block` row's own title gets a `» ` prefix (`App::kind_marker`),
and the badge grows four more glyphs alongside `→N ←N ⚭`: `⇒N`/`⇐N`
for a block's own resolved `deps=`/`xdeps=` targets and the blocks
that name it, `▤` for a declared `produces=file:PATH`/
`reads=file:PATH` (decision 33's file artifact -- still never
resolved on its own, just shown), `✗N` for a `deps=`/`xdeps=` entry
that failed to resolve, and `↻N` for one that resolved but has not
actually run. `xdeps=` alone never triggers a run, so that last case
is a real, expected state, not a corpus error.

<!-- dankg:depends target=dependency-surfacing.md#e-unresolved-but-correct----needs-to-run-not-broken quote="An `xdeps=` target, block- or `table:`-targeted, never does: it is checked, not run." -->

Five matching panel rows (`PanelRow::DepOut`/`DepIn`/`FileDep`/
`DepBroken`/`DepPending`) carry the same facts in full for the
selected node. `DepOut`/`DepIn` are navigable exactly like
`Outgoing`/`Backlink`; the rest are plain text, reusing `PlanError`'s
and `eval::result`'s own existing wording rather than inventing new
copy, so a broken or pending dependency reads the same in the TUI as
it would from `dankg check`. `App::compute_dep_data` computes all of
it once at load time, the same staleness policy as the rest of the
tree: a second parse of the whole corpus through
`eval::files::Files`, correlated back to each row by `(file, line)`
rather than `(file, name)`, since a name can collide and get
`Slugger`-suffixed where a line never does. Six items moved from
private to
`pub(crate)` for this, reused rather than re-derived:
`eval::plan::resolve_dep`/`dep_error`/`xdep_error`/`DepLookup`, and
`eval::result::verified_hash`/`block_index_for`.

`f` cycles a `Filter` state (`App::cycle_filter`) through `All`/
`Blocks`/`Eval-chain`/`File-artifact`, hiding a non-matching row while
keeping its ancestors visible -- the same "hide, not dim" model
`/`-search's own ancestor-reveal already trained, as a standing
choice instead of a one-shot jump. `Eval-chain` settles
dependency-surfacing.md §3's own open question in favor of "declares
*or* is targeted": a block only ever named by another's `deps=`/
`xdeps=`, with nothing of its own to declare, still matches --
otherwise the filter would hide the very leaves a reader turns it on
to find. The full rationale for all of this, including the glyph
choices still open for debate, is dependency-surfacing.md's own
design record.

## Jump and default depth

`dankg tui` selects its initial expansion from its own default depth
(`config::DEFAULT_TUI_DEPTH`, `[tui] depth`), not `[graph] depth`'s
(decision 7's own "Entry + 2 hops"). A terminal tree has no zoom the way
HTML's pan-and-zoom page does. The same width that reads as comfortable
on a scrollable, zoomable page already looks sprawling stamped into one,
and a large, sprawling corpus (this one, self-hosting, is the case that
motivated it) makes that worse, not better. Starting narrower trades
that for more `right`-to-expand along the way, one node at a time.

`/` is the reader's way back out across a corpus a narrower default
otherwise makes harder to explore blind. It opens a typed-query buffer
(`App::start_search`/`search_push`/`search_backspace`) and, on `enter`
(`App::confirm_search`), searches the whole corpus (`App::index`)
directly -- there is no separate drawn view to try first anymore, since
the tree already spans everything -- case-insensitively, and hands off
to `reveal_and_select` exactly like a panel jump does. `esc` cancels
with no jump; no match leaves the selection alone and reports so on the
status line.

Confirming a search is only the first hit, not the only one: `n`/`N`
(vim's own binding exactly) cycle forward/backward through every
remaining match of `last_search`, the pattern kept alive after
`search` itself closes. Both directions, and the initial confirm,
reduce to one search-from-here primitive
(`App::jump_to_search_match`): the next/previous match relative to
the *current selection's* own position in the corpus, not relative to
wherever the last match happened to land, wrapping past either end.
Searching from the cursor rather than from the last hit is what keeps
`n`/`N` behaving sensibly even after the reader has moved around by
hand in between -- the identical reason vim's own `n`/`N` work the
same way. Every jump that lands on a match also reports its own rank
and the total match count on the status line, e.g. `/needle: 2 of 5`
\-- otherwise the reader has no way to tell how many hits `n`/`N` still
have left to cycle through. The rank is always the match's plain
position in corpus order, even right after a wrap; it says nothing
about which direction the jump came from, the same way vim's own
`n`/`N` never mark a wrap either.

## Scope decisions this would actually need

- *Platform*: raw mode is POSIX termios (`ioctl`, no crate) on Linux/macOS.
  Windows needs the separate Console API. Shipping Unix-only first and calling
  Windows a follow-up matches how the markdown subset shipped (decision 3): a
  documented gap beats a blocked release.
- *Redraw strategy*: one full buffered redraw per frame, not a diffed one.
  DanKG's graphs are the size of a knowledge-base neighbourhood, not a video
  frame. Diffing is complexity bought for a problem this doesn't have.
- *Color*: 16-color ANSI, no truecolor assumption. Unlike the drawn formats,
  nothing here is committed or diffed, so determinism does not apply, but
  portability across terminals still does.
- *Additive, not a replacement.* HTML stays the committable, server-free
  artifact (decision 9). The TUI is a live session with no output to commit.
  Losing either would lose what the other is for.

## Scrolling

A large corpus's tree is almost always taller than the terminal, and the
panel can be too. HTML solves this by being a scrollable page; a
terminal is a fixed grid, so `app.rs`'s `render` queries the real size
(`term::size`) every frame and clips each pane to it independently
(`draw::window`), scrolling just far enough to keep that pane's own
current row on screen (`draw::scroll_to_show`). This is "just far
enough" rather than centring: the reader's sense of where things are on
screen should not jump on every keypress that was already visible, only
on one that would otherwise leave the window. The tree's `scroll_row`
and the panel's `scroll_panel` are entirely independent fields, each
following only its own pane's current row -- there is no shared
viewport position the way a single 2D grid once needed one, and nothing
like the old pan mode's "detach the viewport from the selection
entirely" is needed either: each pane is already just a vertical list,
and switching which one has focus (`tab`) is what changes which current
row a reader is looking at, not a second scrolling mode layered on top.

# Code evaluation

Never automatic. `dankg graph` only ever *displays* stored results. It
cannot execute anything.

```
$ dankg eval notes.md --block index
  will run (2 blocks, in order):
    setup [python] notes.md:71   via: uv run python
    index [python] notes.md:88   via: uv run python
  proceed? [y/N]
```

The plan is always printed before anything runs, and every block it
names has its language resolved *before* the prompt too. One
unconfigured `[lang.*]` refuses the whole plan, rather than running
part of what the reader approved and silently skipping the rest.
`--yes` skips the prompt for scripted use. It never becomes the
default. `--no-write` runs everything but prints the captured output
instead of writing it back: a preview of what would change.

`deps` resolves within the file named on the command line by default
(decision 19): `plan.rs` walks `doc.blocks` directly, rather than the
recursive walk `Document::named_blocks` uses for the graph, so a named
block nested inside a list is invisible to eval entirely. Every block
reached from one target must also share its language. Concatenating a
`[sh]` dependency ahead of a `[python]` target and running the result
through one interpreter is far more likely a mistake than an
intentional pipeline, so `plan_for` refuses it before anything is
spawned.

A `deps=` entry may also name another file: `deps=other.md#name`,
mirroring a written link's own `other.md#heading` shape exactly
(decision 29). It resolves relative to the *declaring* block's own
file, via the same `graph::resolve::join_normalize`/`dir_of` a link
already uses, and is refused the same way if it would climb above the
root. `eval::files::Files` is what this actually costs. Given the
target file, it follows only the cross-file `deps=` edges any of its
blocks declare, transitively, loading exactly those files, but never
the whole root (over-inclusive by *block* rather than by *chain*: see
its own doc comment). `plan.rs` itself stays a pure
function of an already-built, multi-file `&[BlockRef]` slice, the same
"boundary logic lives with the code that touches the filesystem"
discipline `resolve.rs`/`index.rs` already split the same way.
Concatenation (decision 11) is what makes this cheap: the chain is
still one flat file handed to one interpreter, so a dependency living
in another file is no different, mechanically, from one living in
another language-compatible heading of the same file. Only *which*
files must be read changes. Two blocks in different files may share a
`name` (decision 22's uniqueness is per-file, not per-corpus). A local
(no `#`) reference inside a cross-file-reached block still resolves
within *that* block's own file, never back against wherever the walk
started. `--block`/`--all`/`--each`'s own targets stay exactly one
file (decision 19's original scope, unchanged). Only a target's
*chain* may now reach outside it, so a pulled-in dependency from
another file is never itself treated as one of `--all`/`--each`'s
targets.

`xdeps=name` (decision 31) resolves the same way. It is never
concatenated. A target with an `xdeps=` entry always runs alone.
Decision 11's own refusal to mix languages in one chain is correct. It
stays exactly as it is. Concatenating a `[sh]` block ahead of a
`[python]` target through one interpreter really is almost always a
mistake. Decision 11 does not offer a way to track staleness across
the boundary it correctly refuses to cross.
[agent_tests/deps_pilot.md](agent_tests/deps_pilot.md) found that gap
costs real correctness. A raw pipeline with no equivalent mechanism
missed a stale upstream stage in the same two ways, in shell, Python,
SQL, and a realistic mixed-tool pipeline alike. `xdeps=`'s own hash is
never a referenced block's stored marker, read blindly.
`eval::result::verified_hash` recomputes that block's own hash from
its current chain and its own `xdeps`, recursively. This is the exact
check `dankg check`'s per-block loop already runs. It only trusts a
stored value once a fresh recomputation still matches it. A mismatch,
a missing result, an `xdeps` cycle, and a now-unconfigured language
are all refused rather than silently trusted. `xdeps` is a trust
boundary on purpose. It holds itself to a stricter bar than decision
11's own chain, where an unconfigured language is merely skipped
rather than failed. A cache, shared across `dankg check`'s whole run,
remembers a block already verified once. A real DAG's diamond shape or
fan-out means the same upstream block is often reachable from many
paths.

**Deferred.** Whether `--each`/`--all` should ever schedule execution
across `xdeps=`, not just track staleness through it, is left open on
purpose. Only `xdeps` edges are a genuine ordering constraint. A
`deps=` member never needs to have run standalone first, since
concatenation always re-derives it from current source. A real fix
would need a second, `xdeps`-only topological sort, layered over
`plan_each`/`plan_all` without disturbing decision 11's chain-building.
Even that would only help ordering within one already-named file or
corpus scope. Decision 19's one-file targeting is untouched by any of
this. A pipeline spanning several files, this document's own
`agent_tests/deps_pilot.md` mixed-tool example among them, still needs
each file run by hand, in order. The alternative is having eval
automatically run a missing upstream `xdeps` target to satisfy a
downstream one. That is not merely undone work. It is a different
feature, refused on purpose. Decision 9 does not run anything
automatically. Cascading execution across files the reader did not
explicitly ask to run in this invocation is exactly the kind of trust
decision 9 exists to withhold until asked. `xdeps=` stays a tracking
primitive, not a scheduling one, unless a concrete pipeline need
forces the question for real.

For a language with no real per-file module system reachable from within
one compiled unit (Rust among them), this is enough to write a genuinely
multi-file literate program with no `mod`/`use` at all: every file eval
reaches contributes raw text to the same one concatenated compile, so
there is exactly one flat item namespace regardless of how many `.md`
files a chain spans. DanKG still never reads what a block's code means
(decision 1). Two blocks defining the same Rust identifier collide
exactly as they would pasted into one file by hand, and `rustc`'s own
"duplicate definition" is the whole error-reporting story here, on
purpose.

`dankg check`'s staleness loop re-verifies every result against
`eval::files::Files::load_all`, loaded with the *whole* corpus up
front, rather than `discover`'s on-demand, chain-only loading. `check`
already visits every file for its unresolved-link pass (decision 6),
so there is no "avoid reading files a target's own chain does not
reach" reason to hold back the way `eval` itself does, and loading
everything is what lets a cross-file `deps=` resolve during a
staleness recheck at all, regardless of which file `check`'s own loop
happens to be iterating at the time. Editing only the *dependency*
file (`main.md` untouched, `lib.md` it reaches via
`deps=lib.md#helper` changed) correctly marks `main.md`'s stored
result stale.

A named block's uniqueness, and `deps` resolution, are both whole-file
and flat (decision 22): any block can depend on any other, in any
heading, without regard for containment. This is a reversal. The first
cut of decision 22 scoped a name's uniqueness to its own heading (two
blocks named `setup`, one under "Parsing" and one under "Rendering",
would not collide) and resolved `deps` lexically, walking from a
block's own heading up through its ancestors only, never sideways to a
sibling. It shipped, and the very first real document written against
it broke. A plain sequence of sibling sections, each one heading
depending on the last, arguably *the* standard shape for a literate
pipeline, has no ancestor relationship between the sections at all, so
a later stage's `deps` could never reach an earlier one. The relaxed
uniqueness that scoping bought was never load-bearing for anything but
a nice symmetry with real module namespacing. Nothing downstream
actually needed two blocks to share a name, since `tangle` (decisions
23-25) groups by heading directly and never consults a block's name to
do it. Flat resolution is what "any block can depend on any other"
actually requires, so that is what shipped instead.
`containing_heading`/`root_heading` (`plan.rs`) survive the reversal
unchanged. They were always `tangle`'s placement helpers, never
eval's, and eval's own naming/resolution code needs neither.

`--each` and `--all` (decision 21) both run every target as its own
spawn, with its own transitive `deps` re-concatenated from scratch
(decision 11's one-process-per-chain model is unchanged either way),
in an order that respects the DAG. If `b` depends on `a`, `a`'s own
run happens first, even though it may be declared later in the file.
They differ only in *which* blocks get that treatment. `--each` is
every named block in the file: the original `--all` behaviour, kept
under its own name because it is still the right tool for
smoke-testing every block in isolation, dependency or not, each with
its own recorded result. `--all` is narrower: only the DAG's *leaves*
(blocks nothing else in the file names in a `deps`), each still
pulling its own full transitive chain exactly as `--each` would. So a
pure dependency like `setup` is no longer *also* given a standalone
spawn merely because it happens to be named. That standalone spawn was
pure waste under the old `--all`. Its result was never anything a
reader asked to see independently, only a side effect of treating
every named block uniformly.

Nothing about decision 11 changes to make this possible. That is also
why it does not fully eliminate repetition: a block depended on by two
different leaves still runs twice in one `--all` invocation, once
inside each leaf's own chain, because nothing here shares process
state across spawns. `--all` removes the *wasted* standalone run, not
the DAG's own real repetition. That repetition is decision 11's
consequence, stated plainly above, not a bug `--all` owes a fix to. A
pure dependency also gets no `<!-- dankg:result -->` of its own under
`--all`, since there is no standalone run to hang one on. `--each` is
what still gives every block a recorded result, dependency or not.

Decision 11's repetition is free when a chain is cheap. It is not free
when a stage in it does real, heavy work. A pipeline processing
hundreds of gigabytes would re-derive that stage from scratch on every
`--block`/`--each`/`--all` invocation that reaches it, with no way to
reuse a result already known to be fresh. `dankg check`'s hash
comparison only ever answers "is this stale," never "skip re-running
this, it already isn't." The [milestone 9 artifact
idea](#open-questions-for-this-milestone) sidesteps this by
construction, not by accident: a cross-language dependency was
designed to fold in a producing block's hash by reference, never by
re-concatenating and re-running it. Whether that same
reference-not-concatenation approach is worth offering inside a single
language too, as an explicit opt-out of decision 11's repetition for a
stage a reader knows is expensive, is open. Nothing about decision 11
itself needs to change either way. A chain that opts out this way
chooses a different, additional mechanism; it does not disable this
one.

`run_one` (`session.rs`), the one place "run this block" is
implemented (see *Terminal UI*'s *Eval in the TUI*), identifies its
target by *position*: where it sits among the file's named top-level
blocks, in document order, rather than re-resolving it by name on
every call. `run_single`'s own multi-target loop
(`--block`/`--all`/`--each` alike) and `tui::eval::run` both already
hold the exact block they mean, from a plan or a cycle list built
moments earlier. Routing that back through a name lookup would be
strictly weaker information for no benefit, a habit worth keeping
regardless of whether names happen to be unique at the moment. That
position stays valid across every write-back `run_single`'s loop makes
against the same file, because a result marker is never itself a named
block and so never changes how many named top-level blocks exist or
their relative order, only their line numbers, which every call
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

Timeout defaults to 30s, overridable per block via `timeout`. The
target's own value governs the one spawn covering its whole chain.
stdout and stderr are captured on their own threads, not read after
the fact: a piped stream fills its OS buffer and blocks the child once
it is full, and a timeout loop that only polled `try_wait` would
deadlock on a chatty process instead of ever reaching its deadline.
stderr is shown on DanKG's own stderr but never stored. stdout is
truncated at 64 KiB with a warning and is what gets written back (or,
under `--no-write`, printed). A non-zero exit stores the output and
marks the result failed.

A timed-out kill has to reach more than the one process `run.rs`
spawned directly: a shell script's `sleep` runs as *its* child, and
killing only the shell leaves `sleep` orphaned and still holding the
output pipes open, so the reader threads block until it exits on its
own regardless of the timeout. This was found by timing the real
binary, not by the unit tests alone, and is now pinned by one that
asserts wall-clock time rather than only the `timed_out` flag. The fix
spawns into a fresh process group (pgid equal to its own pid) and
kills the negated pid on timeout, reaching the whole group in one
signal. This calls `kill(2)` directly, via a hand-declared
`extern "C"` binding -- the same precedent the TUI's termios binding
already set, since decision 1 rules out the `libc` crate, not a raw
syscall declared by hand.

It shelled out to the system `kill` binary first instead, on the
mistaken read that decision 1 ruled out both. That form was also
broken on Linux: CI's `kill -KILL -{pgid}` reported success on every
run, yet a `ps` snapshot taken 50ms later still showed the whole group
alive. `-KILL` immediately followed by another dash-prefixed argument
appears to confuse that binary's own argument parser. Calling the
syscall directly removes the parser, and the external process, from
the picture entirely.

Unix only, the same kind of documented gap as the TUI's termios
(decision 3's precedent). Off Unix, a lone `Child::kill` is used
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

`hash` covers the concatenated source of the target's whole chain and
the *template* string its language resolved to (`[lang.*] command`,
e.g. `uv run python {file}`). It never covers the fully-substituted
argv, which would embed eval's own ephemeral temp file path and make
every result look stale the instant it was checked. Editing `[lang.*] command` is exactly the kind of change that should mark a result
stale. A different temp path on every run is not a change at all. The
hash itself is the fixed-width 16 hex digit form `hash::hex` uses
everywhere else in the codebase (the cache stamp included), not the
shortened form the pipeline sketch above once implied.

On any mismatch the result is stale: it still renders, but `dankg check` reports it. A failed run's marker carries a trailing `failed`
word (`<!-- dankg:result name=index hash=... failed -->`), rather than
a separate attribute. The output is still whatever stdout produced,
staleness is still just the hash, and `failed` only changes what a
reader sees at a glance. Write-back is text splicing over the original
source, not a second pass through `md/fmt.rs`. Locating the exact line
range to touch (reusing the already-parsed `Document` only to find
that range) and replacing nothing else is what keeps every other block
in the file untouched by an eval run. A full AST-to-text rewrite could
not promise that. Blank-line spacing around the result is renormalized
to exactly one blank line on both sides every time, so hand-introduced
drift self-heals rather than accumulating.

## `dankg check`

The CI gate (decision 9's flip side: `eval` never runs anything automatically,
`check` is what confirms nothing needs to). Five checks, all reported.
Three can fail the exit code on their own. Two are advisory only
(*Prose dependencies*, *Title collisions*, both below, explain why):

<!-- dankg:depends target=#decision-9-eval-trigger quote="`dankg eval` only; serve mode deferred." -->

- *Unresolved links*, from the same whole-root index `graph`/`tui` build
  (decision 6: the index is always the whole root, so this needs no
  separate walk).
- *Stale results*, per file: every top-level named block with an
  existing `<!-- dankg:result -->` has its expected hash recomputed
  from the file's *current* source and config, exactly the way `eval`
  would, and compared against the stored one. This is necessarily
  file-scoped, the same as `eval` itself (decision 19): there is no
  cross-file dependency DAG to walk. A block whose language has since
  been removed from config cannot be re-verified and is not counted
  stale on that account alone. A plan error (a dependency renamed or
  removed, a cycle introduced) *is* counted stale, since the result
  can no longer be reproduced from what the file says now.
- *Advisory-stale prose dependencies*, corpus-wide: see *Prose
  dependencies*, below.
- *File dependency mismatches*, corpus-wide: see *File dependencies*,
  below.
- *Advisory duplicate heading titles*, per file: see *Title
  collisions*, below.

# Prose dependencies

A code block's staleness has a clean signal: re-hash the source, compare
it to what last ran (decision 12, `xdeps=`'s own decision 31). Prose has
no such signal. Two sections can say the same words and mean different
things, or different words and mean the same thing. Hashing a whole
section, the way eval's result marker hashes a whole chain, would flag a
typo fix as loudly as a meaning change. Hashing nothing would flag
nothing at all.

Decision 32's `<!-- dankg:depends target=... quote="..." -->` picks a
narrower claim instead of a whole section: the author names the exact
sentence they are relying on, written under (or near) the section that
depends on it. `target` is `other.md#heading` or `#heading` for the
declaring file itself, resolved exactly the way a written link's own
target is (`graph::resolve::join_normalize`/`dir_of`, shared rather than
reimplemented, the same reuse `eval::plan`'s cross-file `deps=` already
gets). `quote` is the claim, whitespace-normalized before comparison so a
rewrapped paragraph -- a line break moved, no word changed -- never
counts as drift. `dankg check` asks one question per marker: is `quote`
still a substring of the target section's current, whitespace-normalized
text? A missing target (a bad file, a bad heading, a path escaping the
root) and a quote that no longer appears are both reported by file, line,
and target, exactly like an unresolved link's own diagnostic.

Unlike an unresolved link or a stale eval result, none of this fails
`check`'s exit code. A substring match is a much weaker signal than a
content hash: it is fooled by an edit that changes meaning while leaving
the exact pinned sentence intact, and it is not fooled by anything that
does not touch those words, but there is no way to be sure to which side
of that line a real report will fall. A mechanism that gates a build on a
signal this weak trains a reader to add `--no-verify`-shaped workarounds,
or to stop reading its output at all, rather than to treat a report as
worth a look. `dankg:depends` stays a `dankg check`-reported hint, one a
reader chooses to act on, not a build gate. Nothing here creates a graph
node or edge, unlike a written link: rendering a dependency in `dot`,
`mermaid`, and `html`, and giving it a cache schema of its own, is a
real, separable feature this decision deliberately leaves for whenever a
real corpus actually asks for it. See *Open questions*.

# Tangle

This is milestone 9's precedent, pointed at a directory instead of a
database: a markdown file's named blocks are, collectively, a real
program, and `dankg tangle` materializes it as one. "Tangle" rather
than "compile" (decision 25), because DanKG's own job is assembly, not
compilation. DanKG never touches a compiler. It writes a source tree
and hands it to a configured command (`rustc`, `cargo`, or nothing at
all for a language with no separate build step), the same way
`[lang.*]` is spawned for `eval`. It is also the term literate
programming itself already uses for exactly this operation: extracting
and reassembling code chunks into compilable source, as opposed to
"weaving" them into typeset documentation. So it costs no new
vocabulary once `project.md`'s own framing is taken seriously.

```
dankg tangle <path>... --lang <lang> [-o DIR]
```

## Scope

A tangled block is exactly an eval-able one (decision 23): named,
top-level, not nested in a list. This is `eval::plan::top_level_blocks`'s
own definition, unchanged. Decision 20 already made "is this a node"
and "is this eval-able" the same question, so that nothing selectable
could be un-runnable. Extending that equivalence to tangle keeps every
block that ships in a compiled program visible to the graph, listable
via `eval --list`, and individually eval-able on its own. A block that
compiled into the real artifact but could not be seen or run any other
way would be exactly the blind spot decision 20 already refused to
create. `Document::named_blocks` (the recursive, list-descending walk
that already exists and is already tested, `tests/parsing.rs`) is
deliberately not what tangle uses, for the same reason `eval` does not
use it either.

<!-- dankg:depends target=#decision-23-tangle-block-scope quote="Exactly decision 20's node scope (named, top-level)." -->

A block not written in `--lang`'s language is skipped. A file with no
blocks in that language tangles to an empty tree, which is reported
rather than treated as an error, on the same "half a plan is still a
real answer" principle as an empty `--list`.

One file tangles just it, unchanged from decisions 23-25. A directory
(or several paths) walks the corpus instead (decision 26), the same
choice `--list` already offers, and via the same distinction
`session::list_corpus_text` already draws: naming only files tangles
exactly those, naming a directory tangles everything under it. Unlike
`--block`/`--all`/`--each`, nothing about `deps=` forces this to stop
at one file. Tangle never reads that attribute (see *Placement*), so
there is no single-file DAG standing in the way, and a real compiled
program is a whole-corpus artifact in a way a disposable eval script
never has to be: a crate is built once, not once per source file. A
single named file still skips the whole-root walk `index::load` would
otherwise do, and the graph build that comes with it, for the same
reason `eval`'s own single-file path already avoids both (decision
19\). Nothing here needs to know about any file but the one named.

## Placement

Structure comes from containment, never from `deps` (decision 24). The
heading with no parent of its own (the top of a block's own ancestor
chain, usually but not necessarily level 1, since a document that
opens straight at level 2 still has a well-defined top) becomes one
file, and a block under a more deeply nested heading folds into it,
attaching in document order. Finding a block's immediate heading
reuses the same approximation `eval::session::list_blocks` already
computes ("the nearest heading at or above" a block's line). Walking
from there to the top reuses the containing heading's own containment,
the same shape `graph/build.rs`'s `current`-heading stack tracks, over
bare line numbers rather than `NodeId`s (`plan::containing_heading`,
`plan::root_heading`). Blocks within one generated file are emitted in
document order, not DAG order: `deps` is eval's own concept for
concatenating a disposable, throwaway script, and has nothing to say
about how a persistent, structured source file should be laid out. A
block with no `deps` at all tangles exactly the same as one with
several. Tangle never reads the attribute in the first place.

<!-- dankg:depends target=#decision-24-tangle-placement quote="Heading containment + document order; `deps` not consulted." -->

Corpus-wide (decision 26), each contributing source file's own
heading placement gets nested under one more directory: its own
root-relative path, extension stripped
(`graph::build::strip_extension`, the same convention a `NodeId`'s own
`key` already uses). So `notes/lexer.md`'s "Tokenize" heading tangles
to `notes/lexer/tokenize.rs` rather than colliding with an unrelated
file's identically-titled heading. That nesting only appears once more
than one file actually contributes a matching block. Naming a single
file, even one sitting inside a larger corpus with nothing else in
`--lang`'s language, tangles exactly as if it had been named alone,
with no directory to explain that was not already there for a reason.

DanKG's own code does not write `use`/`import` statements, or infer
what one block's code means well enough to name a specific item from
another. That stops being placement and starts being a small compiler,
in tension with decision 1 in spirit if not in the letter, and it
would eventually guess wrong about some reference in a way decision
1's zero-crates policy was never at risk of. The author writes those
themselves, the same way they write the rest of a block's source, and
corpus-wide placement (decision 26) is exactly as deterministic as
single-file placement always was. So the author has everything needed
to get a cross-file reference right by hand: `notes/lexer.md`'s
"Tokenize" heading always tangles to `notes/lexer/tokenize.rs`,
corpus-wide or not.

Purely *structural* connective tissue is a different question, and
decision 27 answers it differently. Some languages (Rust) will not
compile a directory tree at all unless something declares `mod x;`
for every file and subdirectory, and that declaration needs no
understanding of what a block's code means, only the shape of the
tree tangle already computed. That is exactly the kind of thing a
tool that owns the whole tree can safely do. But *that*
safely-mechanical property does not hold for every language. Java's
`package` declaration has to sit inside each `.java` file, first line,
and the filename has to match the public class name the block
defines. DanKG cannot know that without reading the code, the same
reading it must not do for a `use` statement either. So this is never
DanKG's own generator, for any language, no matter how mechanical the
case looks. It is `[tangle.<lang>] glue`, a second, independent,
optional command (alongside `command`, which builds; `glue` prepares)
spawned against the already-materialized `\{dir\}`, exactly the way
every other per-language capability in DanKG is a spawned external
program rather than linked-in code (decision 1). An extension is free
to do what DanKG's own code must not: read a block's source to find a
class name, say. It is the reader's own configured, opted-in program,
the same trust model `[lang.*] command` already has (decision 9: the
risk was accepted the moment it was configured). DanKG ships a
documented reference `glue` script for Rust, because the mechanical
case holds there. Python needs none, since a directory of `.py` files
is already an importable namespace package with nothing to generate.
Anyone wanting Java support writes their own external program. No
change to DanKG itself is required, ever, to add a language this way.

`glue/rust.py` only ever writes a `mod.rs` where none exists. It
never touches one that is already there (see the script's own doc
comment). That backoff means porting an *existing*, hand-written
multi-file Rust module into literate form never actually exercises the
generator: a human already wrote that module's `mod.rs` by hand,
tangle reproduces it verbatim like any other block, and `glue` finds a
file already sitting where it would have written one and leaves it
alone. The mechanism is real, but only load-bearing for a module
authored *through* dankg from the start, with no hand-written `mod.rs`
anywhere in its literate source. This is confirmed by a pilot
(`literate/layout-pilot/`) built specifically to withhold that
hand-authored piece and prove the generator fires without it,
including a negative control: deleting `glue` from config and
re-tangling fails the build with rustc's own `E0583`, the same error a
human would get for forgetting the declaration themselves.

A block wants a path outside the heading-derived tree entirely more
often than it wants to be reassigned to a different heading's file (a
`Cargo.toml`, a `pyproject.toml`), so the fence info string grows one
more optional key: `path=`, an explicit output path that overrides the
heading-derived default, the same way `db` already names a target
distinct from a block's language. A block naming `path` still needs a
`name` (decision 23 does not relax for it) and is still a node the
graph and `eval --list` can show. `path` changes only where its
tangled content is written, not whether it is one.

Identifiers are derived from the same per-file `Slugger` a heading's slug
already comes from, then run through one more, language-specific pass
that narrows a slug (already lowercase, already `[a-z0-9_-]`) into a
valid identifier for the target language. Two slug-and-sanitize passes
sharing one `Slugger` is the same "the escaper and the parser must agree"
reasoning that keeps `fmt`'s emphasis-escaping logic calling into
`inline.rs` rather than reimplementing it, applied to a second consumer
of slugs instead of a second consumer of flanking rules.

## Per-file hints and the glue manifest

`dankg.tangle.public` (decision 28) is frontmatter's own way into
this: a per-file hint that this file's tangled output should be
declared publicly visible wherever a `glue` command's generated
declarations would otherwise default to private. Frontmatter has no
scope finer than a whole file, so this is a whole-file decision. A
source file wanting different visibility for different sections is
outside what it can express, and would need those sections split into
separate files instead. Unset, or anything other than exactly `true`,
means private: a typo here must never silently widen visibility, the
same "half-understood is worse than refused" reasoning frontmatter and
config both already apply elsewhere.

Whether *the mechanism* runs at all stays root config's call, never
frontmatter's (decision 27's own principle, applied here): two files
cannot coherently disagree about whether the one shared output tree gets
glue at all, so that decision has to live above any single file. What
frontmatter narrows is how *this file's own* contribution to a
mechanism root config already turned on gets declared. This is the
same "may narrow but never widen" shape `dankg.*` frontmatter keys
already have for `[lang.*]` (see *Config*, below).

A `glue` command only ever sees a plain directory of already-written
files. This is deliberate: the simplest possible glue script needs
nothing more than that to work. A richer one that wants to honour
per-file visibility reads an optional sidecar,
`\{dir\}/.dankg-tangle-manifest.json`, written only when `glue` is
actually configured (a language with none gets no extra file
cluttering its output), and listing every tangled file's own output
path, source `.md` path, `public` flag, and (version 2) its own
contributing blocks. This is hand-rolled JSON, the same reasoning as
`render/json.rs`: DanKG takes no crates, and the shape here is small
and fully under our control. It is small enough that the escaper is
literally shared (`render::json::string`), rather than reimplemented a
second time for one more, equally small, JSON writer.

### Manifest version 2: per-block source position

Decision 30, and the first real instance of "a richer glue
implementation needs more" the schema's own version-1 comment already
anticipated. Each file's manifest entry gained a `blocks` array
(`name`, `line`, `end_line` for every contributing block, in document
order), straight off the same `BlockRef` fields `result.rs` already
carries for write-back positioning (`src/tangle.rs`'s
`ManifestEntry`/`BlockEntry`). Nothing new is computed for the
manifest's sake. Tangle already has these coordinates in hand at the
exact point it was building a `ManifestEntry` anyway, so this is
exposure, not a feature.

What a `line`/`end_line` pair is *for* stays outside DanKG entirely:
which paragraph of source prose "belongs" to a block, and what counts
as a suspicious echo of it, is a glue script's own judgment to make,
the same trust boundary decision 27 already draws. DanKG hands over
structure, never meaning. `glue/rust-doclint.py` is the reference
consumer. It flags a tangled block's own `//!`/`///` comment when it
borrows a clause from the literate source's prose rather than
pointing back to it, windowing its comparison to the prose between
one block and the previous one rather than the whole file. That is
exactly what block-level position makes possible that file-level
position (version 1) could not. A version-1 manifest (no `blocks`
field, an older `dankg`) still parses correctly. The field is purely
additive, and `rust.py`'s own `path`/`source`/`public` reads are
unaffected by its presence or absence.

`glue/rust-doclint.py` has to resolve a manifest entry's `source` back
to a real file, which `rust.py` never needs to do. `source` is not
consistently resolvable from wherever a glue script happens to run.
Tangling one named file makes `source` exactly the path given on the
command line. Tangling a corpus makes it root-relative instead, and
neither `\{dir\}` nor anything else glue is handed carries that root.
The script takes an optional second argument, `\[source-root\]`, for
this reason. This was found only because a corpus-wide pilot
(`literate/layout-pilot/`) tangled clean and silent, which looked like
a passing check until it turned out every `source` lookup was
silently missing the file. Both reference scripts have their own test
suite now (`glue/test_glue.py`, stdlib `unittest`, run directly with
`python3`), covering `rust.py`'s generation rules and
`rust-doclint.py`'s windowing and fallback behaviour. Chaining the two
in one `glue` line (structural generation before the doc-comment
check, via `sh -c '... && ...'`, the same quote-aware split `cmd.rs`
already tests) is what surfaced the `source-root` gap in the first
place.

## Tangle config

```
[tangle.rust]
glue    = dankg-glue-rust {dir}
command = cargo build --manifest-path {dir}/Cargo.toml
ext     = rs

[tangle.python]
ext     = py
```

`command` is optional, spawned once against the whole assembled tree
via a new `\{dir\}` substitution alongside `[lang.*]`'s `\{file\}`.
Python has nothing to compile, so materializing the module tree *is*
the whole operation, and a `[tangle.python]` with no `command` simply
stops there, successfully. `glue` is a second, equally optional
command, using the same `\{dir\}` substitution, spawned first: it
exists to add structural connective tissue (decision 27) before
`command` tries to build the result, so a build command that needs it
never has to ask for it itself. `ext` identifies which fence language
tag the section gathers, the same role it already plays for
`[lang.*]`. The two sections are independent. An `eval`-configured
language needs no `[tangle.*]` entry and vice versa, since a block can
be exercised as a disposable script, tangled into a persistent
program, both, or neither. Unconfigured, `ext` falls back to the
`--lang` value itself, rather than guessing a per-language table. This
is serviceable for a first, config-free invocation, but in practice
anyone naming a real `command` (needing a real compiler to find real
files) sets `ext` alongside it anyway.

## Output

Tangled output goes to `-o DIR`, defaulting to `.dankg/build/<lang>/`,
and is never written back into the markdown. Unlike an eval result, a
tangled tree is wholly derived and disposable, and the markdown stays
the one thing worth committing (decision 12's reasoning, from the
other side of the same principle: the file is the source of truth, so
nothing generated from it gets to also claim that title). The build
directory is excluded from the corpus walk the same structural way
`.dankg/` and `.git/` already are, not via `.dankgignore`, so a
tangled tree sitting under the root never becomes something `dankg graph` tries to read as notes. Every generated file opens with a
banner comment naming the heading and source file it came from and
saying it is generated. `fmt`'s "nothing unverified reaches the disk"
guard has no equivalent here, since the generated tree is not the
thing being trusted. The markdown that produced it is.

<!-- dankg:depends target=#decision-12-results quote="File stays the source of truth." -->

## Trigger

Never automatic, the same principle as decision 9: `graph`/`check`
never invoke tangle, and tangling requires its own explicit command.
Unlike `eval`, there is no interactive plan/confirm prompt. Tangle
does not run the reader's program, only assembles and (optionally)
builds it, and build-time code execution (a `build.rs`, a setup
script, or `glue` itself, decision 27's own external program) is a
risk the reader already accepted the moment they configured that
`command` or `glue`, the same way configuring `[lang.*]` is what
authorizes `eval` to run anything at all. `glue` runs before
`command`, so a build failure and a glue failure are both reported the
same way. Tangle already wrote every file either would need. Only
whichever spawned step comes next can still fail.

<!-- dankg:depends target=#decision-9-eval-trigger quote="never automatic." -->

## Pilots 3 and 4: visibility, and eval/tangle composed over one corpus

Pilots 1 (`literate/hash.md`) and 2 (`literate/layout-pilot/`) each
proved tangle's own assemble-and-build loop. Neither touched `dankg eval` at all, and neither ever set `dankg.tangle.public` (decision
28\): pilot 2's five sibling modules only ever reach each other through
`super::`, which a *private* `mod x;` already permits between siblings
under one parent. Two more pilots close both gaps.

`literate/visibility-pilot/` is the first real exercise of decision 28
end to end: `exposed.md` sets `dankg.tangle.public: true` on its one
heading, and the crate root's `lib.rs` (not a descendant of
`exposed/`, so ordinary sibling privacy cannot save it) calls
`exposed::greeting::hello()` across that boundary. It only compiles
because the flag made `glue/rust.py` write `pub mod greeting;` rather
than `mod greeting;` into the `mod.rs` it generated for `exposed/`,
confirmed by inspecting both the manifest
(`.dankg-tangle-manifest.json`'s `exposed/greeting.rs` entry: `public: true`) and the generated `mod.rs` directly. The negative control (done
by hand, not committed broken, the same reason pilot 2's own
`glue`-disabled check is prose) sets the frontmatter to `false` and
re-tangles. `cargo test` then fails with `error[E0603]: module `greeting` is private`, pointing at `lib.rs`'s own `use` line, proof
the flag is load-bearing rather than cosmetic.

`literate/composition-pilot/` asks a question neither earlier pilot
could: can the same block serve eval's flat, module-free concatenation
(decision 11) and tangle's real multi-file structure at once? Two
corpora under one root answer it oppositely. `same-file.md` puts a
dependency and its target under one heading, so tangle's placement
(containment only, decision 24) and eval's own concatenation
(`deps`-ordered, decision 11) agree by construction. There is no
cross-file boundary for the two mechanisms to disagree about, and
`dankg eval same-file.md --block quadruple` and `dankg tangle same-file.md --lang rust` followed by `rustc --test` on the one file
it produces both pass, unmodified, on the identical block source.
`producer.md`/`consumer.md` puts the dependency in a different file,
and `dankg eval consumer.md --block use_greeting` still passes.
Decision 29's cross-file `deps=` had unit coverage (`src/eval/plan.rs`,
`src/eval/files.rs`), but never a pilot spawning the real binary until
this one. Tangling the same two files apart (`producer/greeting.rs`,
`consumer/use_greeting.rs`) and compiling `consumer/use_greeting.rs`
alone fails with `error[E0425]: cannot find function `make_greeting` in this scope`: its `use super::*` reaches `consumer`'s own module,
not `producer`'s, and nothing here writes the `use crate::producer::greeting::make_greeting;` that would fix it, because
writing it would just as surely break the identical block's use as a
flat, concatenation-eval'd script, which has no `crate::producer` to
resolve at all. This is the finding, not a bug in either mechanism: a
block gets eval-smoke-testing across a file boundary or real
cross-tangle-file linkage at that same boundary, never both
unmodified. The corpus is committed exactly as written, eval-oriented,
with no tangle scaffold wired up, because the tangled pair genuinely
does not build. For a real self-hosting attempt, the actionable shape
is `same-file.md`'s: a module's internal helpers, smoke-tested and
tangled together, stay under the module's own heading. A cross-file
compile-time dependency (`layout-pilot`'s own `super::acyclic::Role`)
already accepts hand-written `use` and should not also be asked to
serve one leg of a cross-file eval chain.

## Pilots 5 and 6: doc-comment injection and cross-reference rewriting

A gap the visibility/composition pilots' own discussion surfaced: a
tangled module's `//!`/`///` comments, by convention, point back at
the literate `.md` (`hash.md`'s own `module_doc` block) rather than
restating it. This is exactly right for a reader with the corpus in
hand and exactly wrong for a reader of a binary-only build, who cannot
open a file that was never shipped. `glue/rust-docinject.py` answers
it without any core DanKG change at all. Manifest v2's `line`/`end_line`
per block already give an external, opt-in program everything needed
to compute a block's own *owned prose* (the paragraphs between the
previous block's own close and this one's own open) and write it in
as a real `///`, backing off exactly the way `rust.py` already backs
off an existing `mod.rs` (decision 27's own idiom, one level deeper):
a block that already carries a hand-written `//!`/`///` is left
untouched entirely. This is deliberately `///` only, never a
synthesized `//!`. A file-level module doc characterizes the whole
file, and nothing here can tell whether a given block's own prose was
ever meant to carry that weight.

A DanKG link inside the copied prose is rewritten too:
`[text](other.md#name)` or `[text](#name)` becomes
`[text](crate::path::to::name)` when `name` names a block the same
tangle run produced, resolved through the exact same manifest
name/path data `glue` already has. A rustdoc *intra-doc link* is not a
bare pointer. `cargo doc` resolves it via the compiler's own name
resolution and fails the build on one that does not, so a
`#![deny(rustdoc::broken_intra_doc_links)]` crate attribute turns "the
rewrite is correct" into a checked build property rather than an
assertion. `literate/docinject-pilot/` (pilot 6) proves it end to end:
`producer.md`'s `make_greeting` is linked from `consumer.md`'s own
prose, tangled apart into `producer/greeting.rs`/`consumer/caller.rs`,
and the generated `fn.shout.html` carries a real
`<a href="../../producer/greeting/fn.make_greeting.html">`, grepped
out of the actual `cargo doc` output, not inferred from the build
merely succeeding. `literate/hash.md` (pilot 1) gained
`rust-docinject.py` in its own `glue` chain as pilot 5. This is proof
the mechanism holds on a purely single-file corpus with no
cross-references at all, and that `module_doc`'s own hand-written
`//!` is correctly left alone while `fnv1a`/`hex`/`parse_hex`/`tests`
each gain a real `///` for the first time.

Doc-comment content turns out to be exempt from the
composition-pilot's own finding, for a reason worth stating plainly:
`rustc` strips comments before anything resembling name resolution
runs, so a `///`'s content, rewritten link or not, has zero effect on
whether a block compiles under `eval`'s flat concatenation or
`tangle`'s real module tree. Only `cargo doc` ever looks inside one.
Code composing across a tangle-file boundary is genuinely constrained
(pilot 4). Documentation doing the same is not, for the exact opposite
reason.

Three real bugs, found only by actually running this against real
corpora, never by reasoning about the design in advance. This is the
same pattern every earlier pilot's own notes already describe:

- A `path=Cargo.toml` block is selected by *fence* language matching
  `--lang rust`, but its own output is TOML, not Rust. The first real
  run against `hash.md` tried to write `///` into it and broke the
  manifest with `error: key with no value, expected =`. Fixed by
  skipping injection for any output whose own path does not end `.rs`,
  while still letting that block's own prose count as spent so a
  *later* real `.rs` block's window does not silently widen to include
  it.
- A leading YAML-style frontmatter block (`---` ... `---`) is not a
  paragraph. The crude, independent paragraph splitter every glue
  script here already has to write for itself (decision 27: no access
  to `md/`) read it as one and swept `dankg.tangle.public: true`
  itself into the very first block's own injected comment on
  `docinject-pilot`'s first real run. Fixed by blanking a detected
  leading frontmatter block in place of its own lines before
  windowing, preserving every line number so nothing downstream needs
  to know it happened.
- A block that opens with a hand-written `use` (exactly the escape
  hatch the composition pilot's own finding requires for a real
  cross-tangle-file reference) is itself a Rust item, and a doc
  comment attaches to whatever comes *directly* after it. Inserting at
  the very start of a block's content put the synthesized `///` on the
  `use` itself. `cargo doc` built successfully regardless, and simply
  rendered no docblock at all on the function it was meant to
  document. That is what made the bug easy to miss without grepping
  the actual generated HTML rather than trusting a green build. Fixed
  by skipping leading blank lines, `use` statements, and attributes
  before choosing where a synthesized comment actually lands.

All three are pinned in `glue/test_glue.py` (46 tests total across the
three reference scripts), each named for the run that found it rather
than the mechanism it now guards.

## Self-hosting: the first real module

Every pilot above proved the mechanics on a throwaway crate under
`literate/`, never on the one `cargo build` actually compiles.
`src/hash.md` is the first module converted for real: `src/hash.rs` is
generated from it by `dankg tangle` and then committed alongside it,
rather than left generated-and-ignored the way `**/.dankg/build/` is
everywhere else. Diverging from that default is deliberate here, since
the crate this repo ships has to keep building with a plain `cargo build` for anyone who does not already have a working `dankg` binary,
which no pilot ever had to consider. Committing the generated file
only stays honest if drift between it and its source gets caught:
`tests/literate.rs`'s `hash_rs_matches_its_literate_source` re-tangles
`src/hash.md` into a scratch directory on every `cargo test` run and
diffs the result against the committed `src/hash.rs` byte for byte.
`src/hash.md`'s own *Literate source* section carries the regenerate
command. No `src/`-scoped `glue` is configured yet: `hash.rs` is one
flat file with no subdirectory needing `mod` declarations, the same
reason pilot 1 needed none either. That arrives once a second module
(one with siblings) converts.

## Open questions (Tangle)

- Should a named, eval-able block be excludable from tangle
  specifically: a demo or scratch snippet that should stay runnable
  and visible without shipping in the built artifact? No such escape
  hatch exists yet. `path` is an override for *where* a block lands,
  not an opt-out.
- Whether to run a target language's own compile-check (`py_compile`,
  `rustc --crate-type` with no output) when no `[tangle.*] command` is
  configured, versus leaving "no command" to mean "materialize only,"
  full stop, as written above.
- The manifest schema (decision 28, widened by decision 30) reached
  `version: 2` once a richer glue implementation
  (`glue/rust-doclint.py`) was actually written and needed block-level
  source position. Declaration order across a directory (rather than
  whatever order tangle happened to write files in) remains
  unaddressed, since no glue consumer has needed it yet.
- `dankg.tangle.public` is the one frontmatter hint that exists. Whether
  other purely-authorial, per-file glue preferences ever justify their own
  `dankg.tangle.*` key, versus staying a `glue` script's own problem to
  solve by reading the block source it is free to read, is open.

# File dependencies

Decision 33. `xdeps=name` (decision 31) already lets a shell block
depend on a Python block, or the reverse, without concatenating them:
exactly the gap [agent_tests/deps_pilot.md](agent_tests/deps_pilot.md)
found in a raw pipeline with no `dankg`-tracked edge at all. That
closes the plain-file half of what used to be an open question under
*Literate database management*, below: a file handoff had no
dependency-tracking primitive short of a database. It does not close a
narrower gap underneath it. `xdeps=name` verifies the *named block's*
source hash. It says nothing about which file that block actually
writes. Two blocks can each rename their own path independently,
`xdeps=name` still matches, and the pipeline is broken anyway.

`produces=file:PATH` on the writer and `reads=file:PATH` on the reader
declare the artifact both sides believe they share, on top of the
`deps=`/`xdeps=name` edge that already names the other block directly.
Neither attribute resolves anything by itself. There is no catalog to
diff, unlike decision 16's DuckDB relations, and no corpus-wide search
for whoever last wrote `PATH`: the existing edge still says *which*
block this is about, exactly as it always did. `produces=`/`reads=` add
one comparison on top of that edge's own verified hash, in `dankg check`: the reader's `reads=` path must equal its named dependency's
`produces=` path, after the same normalization a written link's own
path already gets. A mismatch is a hard failure, not an advisory one,
unlike decision 32's prose dependencies. Two declared strings failing
to agree is a much stronger signal than a substring search over prose,
and checking it costs nothing: no filesystem read, no spawned command,
decision 9 untouched.

<!-- dankg:depends target=#decision-32-prose-dependencies quote="`dankg check` reports a miss but never fails its exit code on one." -->

A block missing `produces=`/`reads=` is unaffected. Neither attribute
is required the way `name` is required to be targeted at all; both are
a second, optional layer for a pipeline that wants its file contract
checked, not just its source hash re-verified. Same-file and cross-file
resolution both work exactly as `xdeps=` already does (decision 29's
lookup, unchanged): `produces=`/`reads=` never invent a second
resolution path of their own to keep in sync with the first.

## Open questions (File dependencies)

- Should `produces=`/`reads=` ever be required together, so a block
  naming one without the other is refused rather than silently
  unchecked? Left permissive for now: an author adding the check to
  one side of an existing pipeline should not be forced to touch the
  other side in the same edit.
- A block can write more than one file, or read more than one. Today
  each attribute takes exactly one `PATH`. Covering more needs either
  repeated declarations or a comma-separated value, whichever a real
  multi-artifact pipeline asks for first.
- This says nothing about a file's own staleness independent of its
  producing block: an artifact edited or deleted by hand, outside
  `dankg` entirely. `xdeps=`'s verified hash already covers "did the
  producing block's source change." Whether the file on disk still
  matches what that block last wrote is a different,
  filesystem-reading question left alone on purpose. Stat-and-hash an
  artifact is close enough to executing something that it deserves its
  own decision, not a rider on this one (decision 9).

# Title collisions

Decision 34. Two headings can legally share a title in one file.
`Slugger::assign` (`graph/slug.rs`) already gives the second one a
distinct, `-1`-suffixed slug. The file resolves correctly exactly as
written. That slug is order-dependent, though. Rename the first
heading, reorder the two, or delete the first one. The suffix shifts to
whatever same-titled heading now comes first. Anything already pinned
to the old suffix -- a written link, a `dankg:depends` marker --
repoints silently, to whatever node the slug now happens to mean.

`graph::build::title_collisions` scopes the check to headings only. A
named, top-level block can share its own containing heading's title
too. That is this corpus' own idiom: every module's `## Tests` heading
wraps a `name=tests` block. This pair is excluded on purpose. A block
cannot precede the heading that contains it. It can never actually
reorder. Two independent headings can.

`dankg check` reports every collision, one line per node after the
first: file, line, title, the slug it actually got, and the slug it
lost. This is advisory only, the same "weak signal, never a build gate"
reasoning as a `dankg:depends` marker. The file is not broken. The fix
is a choice for the author, not something `check` should force.

<!-- dankg:depends target=#decision-32-prose-dependencies quote="`dankg check` reports a miss but never fails its exit code on one." -->

Not every collision carries the same risk, though. Nothing breaks
until some written link or `dankg:depends` marker actually targets one
of the pair's two slugs. `dankg check` cross-references both:
`index_graph`'s own link edges (`Graph::incoming_link_count`) and
every resolved `dankg:depends` target already collected for the
prose-dependency pass, above. A collision with a reference prints as a
live risk. A collision with none prints as cosmetic. It is safe to
leave for whenever the author gets to it.

Every collision also prints as *sibling* (the two headings share the
same immediate parent, or both have none) or *differently-nested*
(they do not). `Node` already carries `parent`. This needs nothing new
to load. A sibling pair looks identical to a reader scanning the one
section they are both under. A differently-nested pair rarely does.
Whichever surrounding section the reader is already in disambiguates
it. This is a second, independent axis, not a replacement for
referenced/cosmetic. A sibling pair can still be cosmetic. A
differently-nested pair can still be referenced.

## Open questions (Title collisions)

- `sibling` compares only the immediate parent, not the full ancestor
  chain. Two headings could share a grandparent under different
  immediate parents. Or they could nest many levels apart under the
  same top-level section. Whether a graded "how many ancestors differ"
  measure would tell a reader more than this binary split is open.

# Literate database management

Milestone 9. Literate programming put the prose and the code that
implements it in one file. This does the same for data. The markdown
file holds the explanation, the ETL that produces a table, and a link
from the table back to both, so "where did this number come from" is
a graph query rather than an archaeology project.

The pieces are already here. A `sql` block is a code block, so it has a `name`
and `deps` and takes part in the same DAG. `dankg eval` already prints a plan,
spawns a configured command, and writes results back into the file. What
milestone 9 adds is a database as the thing being written to, and the relations
in it as graph nodes.

## DuckDB, and why the dependency policy survives

DuckDB is a single-file embedded database that reads CSV, Parquet and JSON in
place and ships as one static CLI binary. That is close to the same set of
constraints DanKG holds itself to.

DanKG does not link it. There is no crate, no FFI, no bindings.
Decision 1 is unchanged. `duckdb` is spawned exactly like `python` or
`sh`, configured in the same file, and subject to the same allowlist
rule: a `sql` block with no configured command is reported and never
run.

<!-- dankg:depends target=#decision-1-dependency-policy quote="Zero crates, std only, forever." -->

```
# .dankg/config
[db.warehouse]
command = duckdb -csv {db} -f {file}
path    = data/warehouse.duckdb
```

`-f`, not shell redirection: decision 11's own "no PTY, no per-language
state protocol" already means every command spawns directly, with no
shell in between to interpret a `<`. `{file}` is always a real path on
disk (`run`'s own temp file), so `-f {file}` reads it exactly the way
`sh {file}` already does for a shell block.

A `[db.*]` section names one database. Other engines fit the same shape; DuckDB
is the first implementation, not a special case in the code.

## Blocks

````
```sql db=warehouse name=load_orders deps=schema
CREATE OR REPLACE TABLE orders AS
SELECT * FROM read_parquet('raw/orders/*.parquet');
```
````

`db` names the target database and is the only new attribute.
Everything else (`name`, `deps`, `timeout`, the plan-then-prompt, the
hash-tagged result block, the stale badge) works as it already does
for code.

## Provenance without a driver

The point of this milestone is the edges, not the execution.

DanKG runs the block's own `[db.*]`'s configured `list` command
(decision 37) before the block runs and again afterwards, and diffs
the two. A name newly present in the second snapshot is the block's
own output. Neither snapshot alone is enough, though. A bare name diff
only ever catches a relation appearing. It cannot catch one an
existing `UPDATE`, `INSERT`, or `CREATE OR REPLACE` changed in place,
since the name already existed before the block ran. The block's own
SQL text has the opposite gap: it can name a write or a read, but it
cannot tell a real relation from a query-local alias or a typo.
`eval::sql` scans the block's own text for both. A name it reports
only counts once one of the two snapshots confirms the relation
actually exists. Both the snapshots and the scan read the same plain,
protocol-agnostic listing `--live` also uses. DuckDB is the first
engine this gets tested against, not an assumption the diff itself
makes: no bindings, no catalog format to track, just `list`'s own
contract of one identifier per line, however it was produced.

<!-- dankg:depends target=#decision-37-live-catalog-opt-in-only quote="reading its stdout as one relation identifier per line" -->

Each relation becomes a node in a synthetic `db:NAME` namespace, never
a real file: `NodeId` already means "the namespace `slug` is unique
within" for every other node kind, and a relation reuses that same
meaning rather than a second identity scheme, since it belongs to a
database, not to whichever file's block happened to name it first.

<!-- dankg:depends target=#slugs-and-node-identity quote="`NodeId` is `<file path relative to root, extension stripped>#<slug>`." -->

Two new edge kinds join `Contains` and `Link`:

```rust
enum EdgeKind { Contains, Link, Produces, Reads }
```

`Produces` runs from the block's node to the relation. `Reads` runs
from the relation to the block that consumed it. A table therefore
edges back to the prose section that explains it, and forward to
everything downstream of it. Lineage is just a traversal, and it
renders in the same graph as everything else.

## Relation-targeted dependency

Decision 35. `Produces` and `Reads` above are inferred, not authored:
DanKG diffs the configured `list` command's own output around a
block's run, and reads a SQL block's own query text for what it named
but did not create. A block written in another language has no query
for DanKG to read, so it has no way to declare a `Reads` dependency at
all today.

<!-- dankg:depends target=#provenance-without-a-driver quote="`eval::sql` scans the block's own text for both." -->

`xdeps=table:NAME` closes that gap by naming the relation, not the
block that writes it. Resolution walks the whole corpus's own
`Produces` edges, since a relation belongs to a database, not to
whichever file happens to declare it. Exactly one block may claim to
have produced `NAME`. Two blocks producing a same-named relation in
different databases, or a relation a later block drops and recreates,
both leave more than one candidate, and DanKG refuses rather than
guess which one a reader meant.

Once resolved, `table:NAME` behaves exactly like a block-named
`xdeps=`: never concatenated into the reading block's own chain, and
its staleness hash folds in the producing block's own hash by verified
reference, recomputed the same recursive way
`eval::result::verified_hash` already handles a named `xdeps=` target.
A missing producing block, an ambiguous one, and a cycle through one
are all refused rather than silently trusted, the same trust boundary
decision 31 already holds `xdeps=name` to.

<!-- dankg:depends target=#code-evaluation quote="are all refused rather than silently trusted" -->

## Open questions (Relation-targeted dependency)

- Same-named relations in two different `[db.*]` targets both refuse
  today. Whether a qualified form, `table:db.NAME`, is worth adding, or
  refusing and asking the author to rename is good enough, has not come
  up against a real corpus yet.

## Inferred relation staleness

Decision 36. Decision 35 gave `table:NAME` a verified-reference
staleness check for the one case a reader writes by hand. An ordinary
SQL block's own `Reads` edges never go through that hand-written form.
They come from diffing the catalog after the block already ran, not
before,

<!-- dankg:depends target=#provenance-without-a-driver quote="before the block runs and again afterwards, and diffs the two" -->

which means `check` -- which never executes anything -- cannot
discover them on its own. Only a run of `eval` can.

<!-- dankg:depends target=#dankg-check quote="`check` is what confirms nothing needs to" -->

The fix writes the discovery back rather than asking `check` to find
something that does not exist yet. `dankg eval` already writes a
block's result into its source, hash-tagged (decision 12); this adds
one `table:NAME` per discovered `Reads` relation to that same
written-back record, indistinguishable afterward from one an author
typed by hand. A later `dankg check` reads it and walks decision 35's
own resolution and verification exactly as written: one relation, one
producing block, refuse on zero or more than one.

<!-- dankg:depends target=#decision-35-relation-targeted-dependency quote="its staleness hash folded in by verified reference to the producing block's own hash" -->

A block's first run has no prior record to compare and nothing to
refuse. It writes the discovery once, the same way any first run
writes a fresh hash with no staleness yet to check against.

## Open questions (Inferred relation staleness)

- A block's own set of relations it reads can change between runs -- a
  query rewritten to join a new table, or to drop one. Whether `check`
  should flag *that* drift on its own, separately from a stale hash on
  an unchanged relation set, is open.

## Live catalog

Decision 37. `Produces` only ever describes what a block's own recorded
run created (*Provenance without a driver*, above). It says nothing
about a relation nobody in the corpus ever ran a block against --
created by hand, by a tool `dankg` never touched, or documented once by
a section since deleted. `dankg graph`'s default stays exactly what it
already is:

<!-- dankg:depends target=#code-evaluation quote="`dankg graph` only ever *displays* stored results" -->

`--live` is the one explicit door across that line. For every `[db.*]`
carrying its own configured `list` command, DanKG spawns it read-only
and reads its stdout as one relation identifier per line. `list` is
new, but the pattern is not: `[db.*] command` already runs a block's
own SQL through whatever engine a reader configured (decision 16),
never asking DanKG to know its protocol. `list` asks the same reader
for the one command that answers "what exists," not "what does this
block's SQL say":

```
# .dankg/config
[db.warehouse]
command = duckdb -csv {db} -f {file}
list    = duckdb -csv -noheader {db} -c "select table_name from duckdb_tables() union select view_name from duckdb_views()"
path    = data/warehouse.duckdb
```

A Postgres warehouse configures the identical shape against
`information_schema`, a REST-fronted catalog against whatever endpoint
it exposes: `list`'s contract is stdout, one identifier per line,
nothing else. DanKG parses that and nothing about how it was produced,
the same indifference decision 1 already holds `command` to. DuckDB's own
`-csv` mode defaults to a header row; `-noheader` above is what keeps
that row from reading as a relation named `table_name`. Getting a
header-free listing out of whichever engine a reader configures is their
own job, the same way writing a correct `command` already is. *Provenance
without a driver*'s own before/after diff (decisions 35, 36) runs
through this exact command too, not DuckDB's own catalog functions:
decision 16 already promised DuckDB would be the first engine tested
against, not a special case baked into the code, and the original
sketch broke that promise before `list` existed to fix it.

<!-- dankg:depends target=#decision-1-dependency-policy quote="Zero crates, std only, forever." -->

<!-- dankg:depends target=#duckdb-and-why-the-dependency-policy-survives quote="DuckDB is the first implementation, not a special case in the code" -->

A `[db.*]` with no configured `list` is reported and skipped, not run
\-- the same allowlist rule an unconfigured language already gets, and
no reason to refuse the rest of a multi-database corpus over one
section that never opted in.

<!-- dankg:depends target=#duckdb-and-why-the-dependency-policy-survives quote="a `sql` block with no configured command is reported and never run" -->

Every listed identifier with no matching `Produces` edge in the current
index renders as its own node kind: a relation the corpus does not
explain. Nothing from `--live` itself is cached or written back: a
second run may draw a different orphan, or none at all, with no
corresponding diff, unlike every other view `dankg` draws.

<!-- dankg:depends target=#milestones quote="so `--no-cache` is byte-identical" -->

## Open questions (Live catalog)

- Typing `--live` is already the ask decision 9 requires, so it runs
  without a blocking prompt of its own. Whether it should still print
  the exact command about to run first, the courtesy `eval`'s own plan
  step gives before its prompt, is a smaller, separate question.
- `list`'s only contract is stdout, one identifier per line. Whether
  that identifier must already match a `Produces` edge's own
  `<db>::<schema>.<table>` shape, or DanKG normalizes it, is
  unresolved.

## Relation depth

Decision 38. *View selection* already counts a hop in both directions,
and even lets containment count as one, flagged there as making a
deeply nested file feel shallow at a low `--depth`.

<!-- dankg:depends target=#view-selection quote="Hops are counted in *both* directions" -->
<!-- dankg:depends target=#view-selection quote="Containment counts as a hop too" -->

A relation risks the same problem, one layer worse: every block that
touches a shared table would need naming individually, or a reader
would need `--all`, just to see what a block sitting right next to its
own ETL means.

The fix is not exempting a relation from the view. It is exempting the
one edge that brings a relation in from the budget that limits
everything else. Stepping onto a relation node -- a `Produces` edge
from a block already in view, or a `Reads` edge likewise -- costs 0.
Stepping off one, onto a block the relation names, costs the ordinary
1, exactly like a `Link` edge. A relation therefore always renders next
to any block already selected that touches it, at any `--depth`,
including 0, but does not itself shorten the distance between two
otherwise-unrelated blocks that happen to share it.

No new machinery draws the result. The *induced subgraph* rule already
renders every edge between two selected nodes, not just the edge that
discovered either one: a relation with two blocks already in view, one
producing and one reading, shows both edges the moment either block
earns its own way in.

<!-- dankg:depends target=#view-selection quote="survives even when it was not the edge that brought" -->

One hop still buys every other block touching that same relation, a
sibling producer into a shared table exactly as much as a downstream
reader. That is deliberate, not a leak to close later. The corpus
already refuses to hide structure it can see once two nodes are both
in view; a relation's other writers are exactly that structure, one
hop away. A reader not wanting it yet already has the filter:
`--depth 0`.

<!-- dankg:depends target=#view-selection quote="Dropping it would render a graph missing structure it can plainly see." -->

## Open questions (Relation depth)

- The layout phase weights containment edges 2 and link edges 1,
  pulling a parent in line with its children (*Layout*, above).
  Whether a `Produces`/`Reads` edge needs its own weight, or inherits
  link's, is undecided; a relation drawn far off to the side of its own
  ETL would undercut the entire point of costing nothing to reach it.

<!-- dankg:depends target=#layout quote="Containment edges carry weight 2 and link edges weight 1." -->

## Row count

Decision 39. Every hash `check` already recomputes is a source hash:
the chain, its resolved template (*Milestones*, above), and now a
verified `xdeps=`/`table:NAME` reference (decisions 31, 35, 36). None
of them ever touch a block's own captured output.

<!-- dankg:depends target=#milestones quote="hashes the concatenated chain plus the *template* a language resolved to" -->

A `SELECT`'s own result is nothing but captured output: a markdown
table, truncated at the same 64 KiB every result already is. Folding
its row count into the hash would be the first exception to a rule the
whole mechanism depends on, and the exact one
[agent_tests/deps_pilot.md](agent_tests/deps_pilot.md) already found
dangerous: a source can change while its output happens to look the
same, so trusting output over source risks missing the change that
mattered while flagging one that never did.

The open bullet framed this as splitting on snapshot versus running
pipeline. It does not split, once pushed on -- both land on "leave it
out," for different reasons. A snapshot cannot drift without its own
SQL re-running, which the source hash already tracks; row count would
only repeat a signal that exists. A running pipeline's row count
drifts by definition, independent of source, so no single stale/fresh
bit can represent it without `check` answering a question it was never
built to ask: not "did the source that would reproduce this change,"
but "has the world moved on since."

The real need underneath -- is a captured result still current against
live data -- is not new. Decision 33's own open questions already
named the identical shape for a file: whether the file on disk still
matches what its block last wrote.

<!-- dankg:depends target=#open-questions-file-dependencies quote="Stat-and-hash an artifact is close enough to executing something that it deserves its own decision, not a rider on this one" -->

That was left for `--live` to answer once it existed, not folded into
`check`'s hash. A `SELECT`'s own currency against live data belongs
there too (decision 37), if it is ever wanted, rather than reopening
the source-hash rule for one block kind.

## Open questions (Row count)

- Should `--live` (decision 37) ever extend to re-running a stored
  `SELECT` and diffing its row count or content against what is on
  disk, the way it already diffs the catalog for orphans? Not decided
  here; today's `--live` only lists relations, it does not re-run a
  block's own query.

## What this buys an agent

project.md's third key feature is that an LLM can run DanKG because it
only touches plaintext. That claim gets much stronger with a database
behind it: an agent reading the corpus sees what each table means,
which block builds it, what that block depends on, and whether the
stored result is stale, without a connection, credentials, or a schema
dump. Answering "what breaks if I change `orders`" becomes reading a
file.

<!-- dankg:depends target=project.md#key-features quote="Agent-compatible" -->

## Open questions for this milestone

- Does `dankg graph` ever touch the database? Decision 37 (*Live
  catalog*, above) resolves this: never by default, `--live` is the
  explicit opt-in.
- Are relation nodes counted against `--depth`? Decision 38 (*Relation
  depth*, above) resolves this: entering one is free, leaving one for
  a further block still costs the ordinary hop.
- Write-back for a `SELECT`: the result block holds the query output
  as a markdown table, truncated at the same 64 KiB. Decision 39 (*Row
  count*, above) resolves whether it belongs in the hash: never, for
  either a snapshot or a running pipeline.
- A `Reads` edge is inferred from a SQL block's own query text. A
  block written in a different language can consume a relation
  without naming it in any parseable SQL. Decision 35
  (*Relation-targeted dependency*, above) resolves this:
  `xdeps=table:NAME` names the relation instead of a block.
- Do `Produces`/`Reads` edges feed `dankg check`'s staleness hash, or
  only `dankg graph`'s picture of lineage? Decision 36 (*Inferred
  relation staleness*, above) resolves this: an inferred `Reads` edge
  writes back and verifies exactly like a declared `table:NAME` one.
- This milestone only covers a database's own relations. The plain-file
  half of this gap -- a shell stage handing a file to a Python stage,
  the exact case
  [agent_tests/deps_pilot.md](agent_tests/deps_pilot.md#caveats-and-next-steps)
  found untracked -- is resolved by decision 33 (*File dependencies*,
  above) and needs no database. What decision 33 does not give a
  relation is the *inferred* half of this milestone's own value: a
  file's producer has no catalog to diff, so `produces=file:PATH` is
  always an explicit declaration, never inferred the free way a
  relation's `Produces` edge is from its database's own configured
  `list` command (decision 37). The relation kind stays the richer, database-requiring layer for
  whoever already has one; decision 33 is what the common,
  database-free case gets instead.

# Config

No serde, so the format is a minimal INI: sections, `key = value`,
`#` comments, no nesting, no arrays.

```
# .dankg/config
[graph]
depth = 2

[tui]
depth = 1

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
```

`[tui] depth` is `dankg tui`'s own default, separate from `[graph] depth` (*Jump and default depth*, under *Terminal UI*): a terminal tree
has no zoom, so the same width that reads fine on a scrollable,
zoomable page already looks sprawling here. Unset, it falls back to
`config::DEFAULT_TUI_DEPTH`, narrower than `[graph] depth`'s own
default. `--depth`/`--all` on the command line still override either
one the same way.

`[keys]` remaps the TUI's letter mnemonics (decision 18). Arrows are
always up/down/left/right regardless of what is written here, since
remapping a positional key to another position is not a meaningful
request. A value that is not exactly one character, or that collides
with another action's binding, warns and the whole map falls back to
the defaults above. A config half-remapped would leave one key doing
two things with no indication which, so this is the same
"half-understood is worse than refused" principle as frontmatter and
section names, not a special case for `[keys]`.

<!-- dankg:depends target=#decision-18-keybindings quote="`[keys]` remaps letters only; arrows fixed." -->

`[editor] command` is the template a future `dankg open <node>` spawns
to jump to a node's source line, substituting `\{file\}` and `\{line\}`
the same way `[lang.*]` substitutes `\{file\}`. Unset means
unconfigured, not "no editor". Falling back to `$EDITOR`/`$VISUAL` is
left to whatever spawns the command, since that is an environment
concern and config.rs only reports what the file said. This is the
answer to "should DanKG grow a GUI or TUI to edit files": it does not.
The editor a reader already has is the buffer. DanKG's job stops at
pointing it at the right line (decision 17).

A `[lang.*]` section is also the allowlist: a fenced block in a
language with no configured command is never executed, only reported.
Per-file overrides go in frontmatter under `dankg.*` keys and may
narrow but never widen what the root config permits. Not every
`dankg.*` key fits that "permission" framing. `dankg.tangle.public`
(decision 28) is a per-file authorial preference, not something to
narrow or widen, but it keeps the same prefix and the same principle
in spirit: it can only ever change what happens to *this* file within
a mechanism root config already turned on, never turn the mechanism on
itself.

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
```

`--format json` always emits the whole index, which is what makes it
the scriptable surface. A consumer that asked for the graph should not
silently get a fragment of it. `--depth` and `--all` shape the drawn
formats only, and passing `--depth` alongside `json` warns rather than
being quietly ignored.

`--no-cache` applies to `graph`, `index`, `check`, `eval --list`, and
`tangle` when it is walking a corpus rather than reading one named
file directly (which never opens a cache to begin with). `index` and
`check` both default to the working directory, because "tell me about
the corpus I am standing in" is the common case for either. `index` is
the command to run when the graph is not what you expected, since that
produces exactly two questions: which root am I in, and which files
did it decide were mine.

`--block`, `--all` and `--each` take exactly one path. `deps=`
resolves within one file only (decision 19), so a second path would
have nothing to mean. `--block`, `--all`, `--each` and `--list` are
mutually exclusive, and one of them is required. There is no default
target to fall back to, on the same "never automatic" principle as
decision 9 itself. `--all` and `--each` differ only in which blocks
get run, not in how (decision 21). `--list` is the odd one out in
every direction at once: it explores rather than runs, needs no
confirm prompt, prints to stdout as the requested output rather than
to stderr the way the plan/confirm/result messages of the others do,
and, since it has no execution to scope the way
`--block`/`--all`/`--each` do, takes any number of paths, defaulting
to `.`, and walks a whole directory as a corpus exactly like
`graph`/`index`/`check` (decision 6).

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

An `Inline::Text` node holds the character the author meant, not the
bytes they typed, so anything that would be re-read as markup has to
be escaped back. `\`, \`\`\`, `[` and `]` are always escaped. `#`,
`>`, `-`, `+`, `*`, `~`, `<` and a leading `1.` are escaped at the
start of a line, where they would open a block. `*` and `_` are
escaped only where they could actually open or close emphasis, which
is what keeps `snake_case_name` intact.

`]` is escaped even though a bare one is inert, because a real link writes an
unescaped `[` and a stray `]` inside its text would close it early.

Thematic breaks are written `***`, never `---`. A leading `---` would be read
back as frontmatter, and `- ---` inside a list item is a thematic break rather
than a bullet.

## Round-tripping is the strongest parser test available

Two properties, both checked over the fixture corpus and every markdown input
in the vendored spec:

- *Idempotence*: formatting twice equals formatting once.
- *Fixed point*: already-formatted input is returned unchanged.

Together these exercise the parser far harder than the conformance suite does,
because every construct must survive a full parse-print cycle rather than
merely producing the right HTML. Writing them found four real bugs: emphasis
delimiters escaped against the wrong neighbour, unescaped `]` truncating link
text, an info string rebuilt in the wrong order, and a leading blank line
inside a list item wrongly making the whole list loose.

648 of the spec's 652 markdown inputs round-trip. The four that do
not are one shape: an indented block that the parser keeps as
`Passthrough` sitting next to a list whose items were indented more
deeply than normal form allows. Normalizing the list narrows its
content column, and the indented block (which the formatter is
obliged to re-emit byte for byte) is then deep enough to be swallowed
as a continuation line. Implementing indented code blocks closes all
four. They are listed by example number in `tests/fmt.rs` rather than
counted, so fixing one fails the test and prompts its removal.

## Nothing unverified reaches the disk

`fmt` re-parses its own output before writing and compares the two
documents, ignoring source line numbers, the one thing formatting is
expected to change. If they differ, or if a second pass is not
byte-identical, the file is left alone and the reason goes to stderr.
The four spec cases above take this path.

<!-- dankg:depends target=#decision-15-format-safety quote="Re-parse and compare before writing." -->

The guard exists because `fmt` is the only command that writes to a
user's notes. A conformance regression costs a number in a table. A
formatter regression costs the note.

## What it does and does not normalize

Normalized: heading style, bullet and delimiter consistency, ordered-list
renumbering from the first item's start, fence length, info-string attribute
order, list indentation, blank-line discipline, trailing whitespace, a single
trailing newline, and hard breaks (a trailing backslash, never two
spaces: trailing whitespace is exactly what normalizing removes).

Not normalized: paragraph line breaks. Reflowing prose to a column limit turns
a one-word edit into a rewritten paragraph in every diff, and where an author
breaks a line is authorial. Soft breaks are preserved exactly.

# Cache

`.dankg/cache/` holds one entry per source file, keyed on `(mtime, len)` and verified by an FNV-1a content hash. The pair is the cheap
rejection. The hash closes the window where a file is rewritten inside
one filesystem timestamp tick. The config hash is stamped into every
entry, so changing `.dankg/config` invalidates all of them at once.
Entries are named by a hash of the path, so a nested source file needs
no nested cache directory.

What is stored is a file's *index contribution* (its nodes,
containment edges, raw links and aliases) rather than the markdown
AST. That is exactly what pipeline steps 4-6 consume and a far smaller
thing to write a codec for. `dankg fmt`, which needs the whole AST,
does not use the cache and does not want to: it reads every file it is
given anyway. A node's row grew a `kind` field when block nodes were
added (decision 20). The row format's own `VERSION` was bumped
alongside it, so every existing entry becomes a miss, rather than
being misread by a decoder now expecting one field more than it has.

An entry also stores the diagnostics the parse raised, and a hit
replays them. Without that, "deleting the cache changes nothing but
runtime" would be false in the one way a user would actually notice:
warnings vanishing on the second run. The claim is tested rather than
asserted: `--no-cache` exists so that a cached run and a cold one can
be compared byte for byte, and `tests/index.rs` does exactly that for
both the graph and the diagnostics.

Nothing in the cache is load-bearing. A missing, truncated, corrupt,
or older-version entry is a miss, not an error. A write failure is
counted and ignored. Writes go to a temporary file and are renamed
into place, so a half-written entry is never readable as a whole one
and two concurrent runs do not interleave. No cache is opened at all
for a root with no `.dankg/`.

# Diagnostics

Every dropped, skipped, or unresolved thing warns on stderr with `file:line`, and
runs end with a one-line summary (`2 unresolved of 31 edges`). stdout carries
only the requested output, so `--format=json` stays pipeable.

Diagnostics are sorted by `(file, line)` before they are emitted, stably, so
that the same corpus reports the same things in the same order however the walk
or the resolver happened to reach them. Determinism is a hard
requirement for output. There is no reason for it to stop at stderr.

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

`indent_info` returns both because a tab advances the column to the
next multiple of four while consuming a single byte. Slicing a line by
a column count silently corrupts the slice on any tab-indented line.
Rule: anything compared against the four-column block rules uses
columns. Anything used to slice uses bytes. The original code used
`&text[indent..]` throughout, which is correct only for space-indented
input. So it passed every test until a tab appeared.

## The root boundary is only as strong as the root

`join_normalize` refuses a link that climbs above the root, but "the root" has to
be a real value. It now comes from `.dankg/`, falling back to the common
ancestor of the paths named on the command line, and every `NodeId` is taken
relative to it.

Having a real one matters for more than tidiness. With no root, the
boundary silently becomes the working directory, so whether
`../../etc/passwd.md` is refused depends on how deep the path you
happened to type was. The unit tests all passed against fixtures at
depth zero, where climbing out is impossible. The leak only appeared
when the binary was run against the real corpus. Any change to path
handling needs an end-to-end test, not just a unit test. Both
`tests/graph.rs` and `tests/index.rs` have a `binary` module for
exactly this.

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
`inline.rs`, so `can_open_close` is shared rather than reimplemented.
Two copies would drift, and the drift shows up as emphasis silently
appearing or vanishing on the first `dankg fmt`, in a file the user
has already committed.

The same rule forces the formatter to track the *semantic* previous and next
character, not the previous output byte. Emphasis is decided by neighbours, so
a child inline has to be told what its parent is about to write on either side
of it: `Writer::run` takes the trailing character for exactly this reason. The
symptom of getting it wrong is `foo _\__` formatting to `foo ___`.

## The renderer and its script cannot be type-checked against each other

`html.rs` stamps `data-key` on every line it draws. `assets.rs`
recomputes that key for lines it adds, and skips any key already
present. If the two spellings differ, nothing fails. The script simply
draws a second copy of every edge Rust already drew, on top of the
first, the moment a reader expands anything.

They did differ. The first `assets.rs` carried literal NUL bytes where the
separator should have been: invisible in the editor, invisible in the diff, and
invisible in every Rust test, because a test comparing `edge_key` to itself
agrees with itself no matter what the JS says. It surfaced only by running the
shipped script against the shipped page.

Two things came out of it. The separator now lives in Rust and is
sent in the meta blob, so there is one spelling rather than two.
`assets::CSS` and `assets::JS` are asserted to contain no control
characters, because a hand-written asset has no compiler looking at
it, and an invisible byte in one is otherwise found by nobody.

The general rule for this file: a constant both languages depend on is Rust's,
and travels. A constant duplicated in the JS is a bug waiting for a reader.

## Most list conformance failures are not list failures

When triaging the conformance table, check what else is in the failing case
before touching `gather_list`. Nearly every failure in "List items" and "Lists"
has correct list structure and fails only because the case also contains an
indented code block or a block quote, both of which are outside the subset and
render as passthrough. Implementing indented code blocks would lift
three sections at once. Changing the list code would lift none.

## A syscall reporting success is not the same as it reporting something useful

`term::size`'s `ioctl(TIOCGWINSZ)` returned `rc == 0` (success) while
handing back an all-zero `Winsize`, observed through a terminal
multiplexer that had not yet told the kernel a real size.
`Result::unwrap_or` only catches an `Err`. A "successful" zero sailed
straight through it and into `draw::window`, which clipped every frame
to a `0x0` rectangle. The symptom was total: not a badly-sized drawing
but no drawing at all, on every single frame. That is what made it
findable. A partial failure would have been read as more scrolling to
do, not a bug. `app::term_dimensions` now filters on the value, not
just the `Result` variant, and is a pure function specifically so this
case has a unit test rather than depending on catching a multiplexer
in the act again.

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
- Layout invariants that a reader notices the moment they break: no
  two boxes in a layer overlap, nothing starts left of the margin,
  every polyline begins at its source and ends at its target, and no
  bend point escapes the ranks its edge spans.
- The drawn formats are checked structurally rather than by shelling out to
  graphviz, which is not installed everywhere: braces balance, and every
  identifier an edge references was declared as a node.
- Golden `--format=html` over the same fixture corpus, which pins the markup,
  the stylesheet and the script in one file: the only honest way to review a
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
  grandchild holding the output pipes open (pinned by wall-clock time,
  not only the `timed_out` flag, since that is exactly the dimension
  the bug it caught broke). Write-back is checked as pure text splicing: inserting after
  a block with nothing following, replacing an existing result in place,
  renormalizing drifted blank-line spacing, and preserving a file's missing
  trailing newline.
- `[keys]` parsing: defaults with no section, individual remaps, a
  multi-character value rejected in favour of its default, and a collision
  between two actions reverting the whole map rather than half of it.
- The tree: every top-level file becomes a root even when not named on
  the command line, only the named entry starts expanded, right
  expands-or-steps-onto-the-first-child and left collapses-or-steps-
  onto-the-parent, up/down never land on a collapsed node's hidden
  children, and `r` re-collapses everything back to the initial state.
  The panel: its rows list outgoing links then backlinks then produces
  then reads, relation rows are excluded from what its own cursor can
  land on, `tab` toggles focus and back, and `enter` on a focused row
  both jumps and force-expands every ancestor of its target -- the
  same `reveal_and_select` `/`-search's own `enter` uses, checked to
  reach a match outside whatever the initial expansion left open.
  `render` is checked to mark the focused pane's current row in reverse
  video and the other pane's own remembered row underlined. The panel's own
  live preview: hovering a row surfaces its target without moving the
  real selection, moving the panel cursor moves the preview with it,
  `render` is checked to actually reveal a previewed target's hidden
  ancestors for that one frame, and leaving the panel without `enter`
  is checked to leave both `expanded` and the selection untouched.
  `/`, `n`, and `N`: confirming lands on the first match at or after the
  current selection rather than always the corpus's first one, `n`/`N`
  cycle forward/backward and wrap at either end, both search from the
  *current* selection rather than the last match (checked by moving
  away by hand between two searches), `n`/`N` are checked as a no-op
  with no prior search, cancelling the search prompt is checked to
  leave the last confirmed pattern alone for `n`/`N` to keep cycling,
  and every successful jump is checked to report its own rank and the
  total match count on the status line, unchanged by a wrap.
- Eval in the TUI: cycling finds only the blocks inside a node's own
  line range, wraps, and is a silent no-op with none to find. Running
  writes back and reports `ok`/`failed` on the status line and clears
  the cycle either way. Navigation and `esc` both cancel a cycle, the
  former via the same `clear_transient` path movement, `tab`, and `r`
  all share. `render` is checked to draw the help screen instead of
  the tree while `help` is on. `read_key` is checked to carry an
  unused byte from a standalone Esc into the next call rather than
  losing it, and to resolve a `pending` buffer that already decodes to
  a full key without blocking on a read that would hang forever
  against an empty reader. These are the two ends of the bug a real
  pty session against the built binary found (see *Terminal UI*, *Help
  screen*), neither reachable from feeding `decode`/`read_key` one
  complete sequence at a time the way the rest of the suite already did.
- Block nodes (decision 20): a named top-level block becomes a node
  contained by its heading with the right `(line, end_line)`. An
  unnamed block or one nested in a list does not. A block before any
  heading attaches to the lazily-created file-level node. A block's
  own end_line survives `set_extents` untouched while a heading's
  still correctly runs past it to the *real* next sibling. A block
  name colliding with a heading slug gets the same `-1` suffix two
  same-titled headings would. Each drawn format is checked to give a
  block node its own look: `dot`'s fillcolor and monospace font,
  `mermaid`'s `classDef block`, the HTML page's `.node.block` class
  (both server-rendered and script-grown), and the TUI's heavy border.
  `eval --list` is checked to list only the named file's blocks when
  given one, and every file's when given a directory.
- `--each`/`--all` (decision 21): `--each` still gives every block its
  own chain, dependency or not, and `--all` reports only DAG leaves,
  still pulling a leaf's full transitive chain. Whole-file, flat
  naming and `deps` resolution (decision 22, after its reversal): a
  dependency across sibling headings, with no ancestor relationship
  between them, still plans. Two blocks anywhere in the file sharing a
  name are refused (`DuplicateName`) rather than allowed to coexist.
  `plan_for_index` runs the block at a given position directly, which
  is what `run_one` (both `run_single`'s loop and `tui::eval::run`)
  and `dankg check`'s staleness loop use rather than a name lookup.
- Tangle (decisions 23-28): a level-one heading (or the topmost
  heading of one with no level-one wrapper) becomes one file. Two
  headings become two files. A nested heading folds into its top-level
  ancestor. A block before any heading goes to `main.<ext>`. A block
  nested in a list or in another language is not tangled. `path=`
  overrides the heading-derived path. The default output directory is
  `.dankg/build/<lang>/`. `ext` falls back to the `--lang` value when
  unconfigured. A configured `[tangle.*] command` runs against the
  assembled directory via `\{dir\}`, and no command configured stops
  at materializing files. A directory walks the whole corpus, nesting
  each contributing file under its own subdirectory. Naming one file
  out of a larger corpus, or several files explicitly, tangles only
  what was named, with no extra nesting for a single contributor.
  `glue` runs before `command` and can see what `command` will (a
  file `glue` created is still there when `command` runs). The
  sidecar manifest is written only when `glue` is configured, never
  otherwise, and carries each tangled file's `dankg.tangle.public`
  frontmatter hint, defaulting to `false` with none.
- Prose dependencies (decision 32): `depends.rs`'s own unit tests cover
  marker parsing (key order, a required `quote` missing, an unrelated
  comment), `resolve_target` (same-file, cross-file relative to the
  declaring file, a path escaping the root), `section_text`'s inclusive
  1-indexed slicing, and `verify` staying fresh across a rewrapped line
  break while going stale once the pinned words are actually gone.
  `tests/depends.rs` drives the real binary against a dedicated fixture
  corpus (`tests/data/depends-corpus/`, its own root, isolated from the
  golden-dump corpus) and checks `dankg check`'s wiring end to end: a
  fresh same-file quote and a fresh cross-file one both stay silent, a
  stale quote, an unresolvable target, and a target escaping the root
  are each reported by file, line, and target, the summary line counts
  every marker once, and none of it fails the exit code.
- File dependencies (decision 33): `eval/plan.rs`'s own unit tests cover
  `check_file_deps` in isolation -- a match via `deps=`, a match via
  `xdeps=` across languages, a mismatch, a dependency declaring no
  `produces=` at all, a `reads=` with no dependency edge to check
  against, a `reads=` escaping the root, an unrecognised artifact kind
  staying silently uncounted, a match found among several candidate
  dependencies, and (the one a raw string compare would get wrong) two
  blocks in different directories naming the same artifact by different
  relative spellings, confirmed to still agree once both are resolved
  through `join_normalize`. `tests/filedeps.rs` drives the real binary
  against `tests/data/filedeps-corpus/`, its own isolated root, and
  checks the same cases end to end, plus the one thing the unit tests
  cannot: unlike a `dankg:depends` marker, a mismatch here does fail
  `dankg check`'s exit code.

# Milestones

1. \[DONE\] `md/` subset + frontmatter + conformance harness. 368/652 (56%).
2. \[DONE\] `graph/` model, slugs, resolution, `--format=json`. The graph is fully
   testable here, before anything is drawn: `tests/data/corpus.golden.json`.
3. \[DONE\] `dankg fmt`. Placed here deliberately: the round-trip properties
   harden the parser before the graph work starts depending on it, and the AST
   additions it needs were cheapest to make then. 648/652 spec inputs round-trip;
   the four that do not are documented above and are refused rather than
   mangled.
4. \[DONE\] Root discovery, corpus walk, cache, diagnostics. `.dankg/`
   marks a root. The walk covers all of it and honours `.dankgignore`.
   The cache stores each file's index contribution and its
   diagnostics, so `--no-cache` is byte-identical. `dankg index`
   reports what the walk found.
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
7. \[DONE\] Terminal UI: `src/tui/` (term, input, draw, app, editor,
   eval). No change to `graph/`. Originally rendered the same
   Sugiyama layout `layout/`/the drawn formats do, over `graph::view:: select_view`'s own depth-limited subgraph, with `tab` growing that
   view in place and `p` detaching the scroll into a pan mode. Later
   rewritten to a nerdtree-style collapsible tree plus a persistent
   cross-reference panel, since the corpus's own graph is
   overwhelmingly a containment hierarchy: far more `Contains` edges
   than `Link` edges on this repo's own self-hosted corpus (see
   [Self-hosted corpus stats](#self-hosted-corpus-stats), below, for
   the live count). A general-DAG layout was spending its visual
   budget on structure the data barely has. `layout/` is no longer consulted by the TUI at
   all; the tree is built straight from `Node.parent` over the whole
   resolved corpus (`App::index`), unconditionally -- `--depth`/`[tui] depth`/`--all` now decide only how many levels start pre-expanded
   under the named entry file(s), not what is loaded. `tab` now
   toggles focus between the tree and the panel; `p`/pan is retired,
   since two independently-scrolled vertical panes need no 2D
   viewport to detach. `graph::query::links_for` (new) is what the
   panel reads: a selected node's outgoing links, backlinks, and
   produced/read relations, navigable via `enter` once the panel has
   focus, which force-expands a jump target's whole ancestor chain
   (`App::reveal_and_select`) -- the same operation `/` (jump to a
   node by title, shipped after this milestone's initial cut) already
   needed for its own jump, so the two share one implementation.
   Selecting a node still hands off to the configured `[editor] command` (decision 17) directly, which a browser click structurally
   cannot do for a terminal editor. `e` cycles a selected node's named
   blocks and `enter` runs the cycled one through
   `eval::session::run_one` (milestone 8) without leaving the tree,
   reporting the outcome on a one-row status line reserved at the
   bottom of the viewport. `?` is a full-screen keybinding reference.
   See *Terminal UI* above. Letter keybindings are remappable in
   `[keys]` (decision 18). Unix termios first, Windows Console API
   deferred.
8. \[DONE\] `eval/` (plan, run, result) + `dankg check`. Scoped to
   one file at a time (decision 19): `plan.rs` walks `doc.blocks`
   directly rather than recursing into lists the way the graph's
   `named_blocks` does, refuses a cycle or an unknown dependency
   instead of guessing, and refuses a dependency in a different
   language from its target rather than concatenating incompatible
   source into one interpreter. `run.rs` spawns once per target's
   whole chain, capturing stdout/stderr on their own threads so a
   chatty process cannot deadlock the timeout poll, and kills the
   whole process group (not just the direct child) on timeout so an
   orphaned grandchild cannot hold the output pipes open past it.
   This was found by timing the real binary, not by the unit tests
   alone. `result.rs` hashes the concatenated chain plus the
   *template* a language resolved to (never the substituted argv,
   which would embed eval's own ephemeral temp path) and writes
   results back as targeted text splicing over the exact line range
   located via the parsed `Document`, not a second `md/fmt.rs` pass.
   Nothing else in the file moves. `dankg check` adds unresolved-link
   detection (reusing the whole-root index every other command
   already builds) alongside per-file staleness, and is the second CI
   gate next to `dankg fmt --check`. `session.rs` holds the
   interactive plan-confirm-run flow and `run_one`, split out from
   `main.rs` specifically so the TUI can call the same function (see
   *Terminal UI*, *Eval in the TUI*). `--list` is `session.rs`'s
   exploratory mode, printing every named block's name, language,
   line, containing heading and configured-or-not without running
   anything: how to find a block's name before naming it to
   `--block`, from the CLI alone, and (decision 6) walking a whole
   directory as a corpus when that is what was named, not just the
   one file `--block`/`--all` are scoped to. A named top-level block
   is also a graph node in its own right now (decision 20), drawn
   distinctly in every format. See *Block nodes* under *Data model*.
9. \[DONE\] Literate database management, over DuckDB. Depends on 8: it is
   the same plan-run-write-back machinery pointed at a database instead of
   a process. Decision 33's `produces=`/`reads=file:PATH` ships first, as
   the database-free half of the same gap: see *File dependencies*. `db=`
   blocks resolve and spawn through `[db.*]` (decision 16,
   `eval::run::db_command_for`/`run_db`). *Provenance without a driver*'s
   own snapshot-diff-plus-SQL-scan inference ships, writing `produces=`/
   `reads=` back onto the result marker (decisions 35-36), which
   `graph::build` reads into real `Produces`/`Reads` edges and relation
   nodes. `xdeps=table:NAME` resolves against those edges corpus-wide
   (`graph::query::find_producer`) and verifies a relation's producing
   block recursively, the same way a named `xdeps=` already does. Two
   real bugs turned up building this, both fixed: the milestone's own
   `duckdb -csv {db} < {file}` example assumed shell redirection that
   never existed (decision 11's own "no shell" design; `-f {file}` is
   what actually works), and `dankg check`'s staleness loop, along with
   `verified_hash` itself, computed a db-targeted chain's hash against
   the wrong template twice over -- both now call one shared
   `eval::result::hash_template_for` so they cannot drift apart again.
   `--live` (decision 37) ships too: `dankg graph --live` spawns every
   `[db.*]`'s own `list` and adds a node (`graph::query::live_orphans`)
   for whatever it reports that the corpus does not already explain. A
   `[db.*]` with no `list`, or whose `list` fails to run, is reported and
   skipped rather than aborting the rest of the corpus's own databases.
   Relation depth cost (decision 38) ships too: `graph::view::select`
   folds every relation an already-chosen block touches into the view
   before spending any of the `depth` budget, so a relation renders next
   to its own block at `--depth 0`; leaving one, onto a further block,
   still costs the ordinary hop, unchanged from any other edge.
10. `dankg serve`, deferred, opt-in, only if the static path proves insufficient.
11. \[DONE\] `dankg tangle` (`src/tangle.rs`). Block scope reuses
    `eval::plan::top_level_blocks` exactly (decision 23), independent
    of naming: tangle never reads a block's name for anything but
    display, so decision 22's later reversal (whole-file, flat naming
    and `deps` resolution: see *Code evaluation*) changed nothing
    here. Placement groups by the nearest heading with no parent of
    its own (`plan::root_heading`, a small generalization of "level-1
    heading" that also handles a document with no level-1 wrapper),
    in document order, never consulting `deps`. `path=` overrides it
    per block. Identifiers reuse `graph::slug::Slugger` so a tangled
    file's name matches the graph node's own anchor for that heading.
    `[tangle.<lang>]` configures two independent, optional commands
    against the whole assembled tree via a new `\{dir\}` substitution:
    `glue` (decision 27, structural connective tissue, run first) and
    `command` (build, run second). Unconfigured, `ext` falls back to
    the `--lang` value itself. A directory (or several paths) walks
    the corpus (decision 26), nesting each contributing file under
    its own root-relative-path subdirectory once more than one is
    involved, and a per-file `dankg.tangle.public` frontmatter hint
    reaches a `glue` command through an optional sidecar manifest
    (decision 28) rather than DanKG's own code ever branching on it.
    See *Tangle*.

# Self-hosted corpus stats

Milestone 7 (above) rewrote the TUI because this repo's own corpus is
overwhelmingly a containment hierarchy, not a general DAG. This block
checks that claim instead of restating a hand count. `dankg eval architecture.md --block corpus-edge-counts --yes` re-runs it and
writes the current count back below.

```sh name=corpus-edge-counts
json=$(cargo run --release --quiet -- graph . --format json 2>/dev/null)
contains=$(printf '%s\n' "$json" | grep -c '"kind": "contains"')
link=$(printf '%s\n' "$json" | grep -c '"kind": "link"')
echo "$contains Contains edges against $link Link edges"
```

<!-- dankg:result name=corpus-edge-counts hash=ae8fcf0de8ab4635 -->

```
497 Contains edges against 55 Link edges
```

# Open questions

- Do frontmatter tags become graph nodes, or stay node attributes used for
  filtering? Currently attributes.
- Do containment edges count against `--depth`, or only link edges? Currently
  both, which may make depth 2 feel shallow in deeply nested files.
- Cross-root links: refused today. Is a multi-root mode ever wanted?
- Cross-file tangle, an eval-able-but-not-tangled escape hatch, and
  whether a language with no configured `[tangle.*] command` should still
  get a compile-check: see *Tangle*'s own *Open questions*.
- Should `dankg:depends` (decision 32) ever become a graph edge -- drawn
  in `dot`/`mermaid`/`html`, given its own cache schema bump -- rather
  than a `check`-only report? Deferred on purpose until a real corpus
  wants to *see* a prose dependency, not just be warned about one. A
  marker that never resolves would need a placeholder-node treatment
  much like a dangling link's own (decision 8), which is one more reason
  this was left for a real need rather than spun up speculatively.
- Should a marker ever be allowed more than one `quote=`, for a section
  that leans on several claims from the same target at once? Today a
  section wanting that writes several markers. Whether that is a real
  cost or just unwritten sugar has not come up against a real document
  yet.
