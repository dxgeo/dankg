# The graph shows a fifth of what the corpus already declares

## Context

This plan comes out of a review of the whole design record rather than
a bug. The question asked was whether dankg is the best implementation
of what it is for. Most of the answer is yes. One part is not. That part
is measurable.

`dankg graph .` on this repo reports 927 containment edges against 72
link edges. Containment is the document outline, which any markdown
parser derives for free. The 72 link edges are the whole of what the
graph knows about authored cross-reference. The marker below pins that
count against the block that measures it. The next drift therefore
trips `dankg check` rather than sitting here unnoticed.

<!-- dankg:depends target=../architecture.md#self-hosted-corpus-stats quote="927 Contains edges against 72 Link edges" -->

The same corpus carries over 250 `dankg:depends` markers. `dankg check`
prints the exact count on every run. No exact number is typed here for
a reason: adding one marker to this very plan changed it while the plan
was being written. Each one names a target section and a quoted claim. Each one
resolves. None of them is an edge.

So roughly a fifth of this corpus's authored cross-references reach the
graph. The mechanism carrying the other four fifths is the one the
corpus actually adopted.

## What the probes established

**The sparsity is real and already recorded.** Milestone 7 rewrote the
TUI from a Sugiyama layout to a collapsible tree precisely because this
corpus "is overwhelmingly a containment hierarchy, not a general DAG".
That decision cites a live eval block rather than a hand count. The
finding is not new. What is new is that the richest source of authored
edges was identified, built, adopted hundreds of times, and left
outside the graph.

**The deferral has a stated trigger. It has fired.** *Open
questions* asks whether `dankg:depends` should become a graph edge, and
defers it "until a real corpus wants to *see* a prose dependency, not
just be warned about one". Over 250 markers is a real corpus.

**A second, smaller source is computed twice.** `deps=`/`xdeps=` are
resolved corpus-wide by `App::compute_dep_data`, which parses the whole
corpus a second time through `eval::files::Files` for the TUI's own
panel rows. `EdgeKind` has no variant for either. The information
exists, is resolved, is displayed, and is not in the graph. This corpus
has only 25 of them. The gain here is small. The duplicated parse is
the better reason.

**The self-measurement had gone stale. `check` was silent by design.** *Self-hosted corpus stats* recorded 596 containment edges
against 56 link edges when this review started. The corpus had 906 and
72\. `dankg check` called that fresh, and was right to. Decision 39
hashes a block's source, never its output. The source had not changed.
What changed was the corpus it measures. Re-running the block fixed the
number.

This is worth recording because the mechanism was used *correctly* and
still failed. The figure was deliberately not a hand count. It was an
eval block, written so prose could not drift from fact. It drifted
anyway, by a third. Milestone 7's own rationale cites it.

The gap is unchanged by re-running it. Nothing would have caught that
drift. Nothing will catch the next one. Every other eval block in the
corpus is a pure function of its own source. This one reads the corpus.
The corpus is not in its hash.

<!-- dankg:depends target=../architecture.md#decision-39-row-count-stays-out-of-the-hash quote="Every hash `dankg` computes is a source hash, never a content hash" -->

## What this reuses

**`EdgeKind`, and the two inferred kinds already beside it.**
`Produces` and `Reads` are inferred, written back, and verified
recursively by `check` (decisions 35 through 37). An edge dankg derives
rather than reads is therefore an established shape, not a new one. A
prose dependency is the same shape with a different source.

**`depends.rs`'s own resolution.** `resolve_target` already turns a
marker's `target` into a file and a heading, through the same
`join_normalize`/`dir_of` a written link resolves through. Nothing
about finding the far end needs inventing.

**Decision 8's placeholder treatment.** A marker whose target does not
resolve needs exactly what a dangling link already gets: a placeholder
node and a warning. *Open questions* names this as a reason the work
was deferred. It is equally a reason it is cheap.

## What's new

### A prose dependency is an edge

A `dankg:depends` marker becomes an edge from the section that declares
it to the section it targets. The marker stays advisory for `check`'s
exit code. Decision 32 is unchanged. Its reasoning about weak signals
is untouched by whether the thing is drawn.

Its own edge kind, not `Link`. The two are not the same relation. A
link is a reference a reader follows. A prose dependency is a claim one
section makes about another, carrying the quote it relies on.
Collapsing them would lose the distinction `check` already acts on.

### `deps=`/`xdeps=` are edges

One kind for both, since resolution is already shared. This retires
`compute_dep_data`'s second corpus parse: the panel reads the graph the
tree is already built from.

### A corpus-derived measurement can declare its input

The narrow fix for the stale stat is a CI step: run the block and fail
on a diff. The general fix is a way for a block to declare the corpus
as its input. `check` could then answer a question it currently
cannot.

This is deliberately last and deliberately vague. It is a real gap and
it is not obviously worth a mechanism. The CI step costs two lines.

## Why not the alternatives

**Leave it and document the ratio.** The ratio is already documented,
and it is already stale. Documenting a number nothing verifies is the
failure this project exists to prevent.

**Count markers in the stats block without drawing them.** It would
make the recorded figure honest and leave the graph exactly as thin.
The point is not the number.

**Fold prose dependencies into `Link`.** Cheaper, and it destroys the
one distinction that makes them useful: `check` treats a dependency as
advisory and an unresolved link as fatal. One edge kind cannot carry
both policies.

**Draw them behind a flag, the way `--live` gates a spawned catalog.**
`--live` is gated because it spawns processes against an external
system. A marker is already parsed on every run, at no cost. There is
nothing to gate.

## Scope: weave is not the problem, and its boundary is

Weave is tangle's own corollary. The record said so before this review
did. Decision 25 names it: tangle is the literate-programming
term for assembling source, "as opposed to *weaving* them into typeset
documentation". Weave belongs here.

What it lacks is a boundary. Decisions 41 through 68 are almost all
weave, which is 29 of 69 recorded decisions. Weave appears nowhere in
*Milestones*. Every other subsystem is a numbered phase that closed.
Weave has grown as a continuous stream with no point at which it was
declared done.

*Open questions (Weave)* already reads like a scope fence: corpus-wide
weave out of scope, no list of figures, no HTML syntax highlighting,
one artifact per block. Giving weave a milestone number, marked `DONE`
at its current feature set, turns that list from a backlog into a
boundary. Nothing about the code changes. What changes is that the next
feature has to argue for reopening a closed phase.

## Open questions

1. Whether a prose-dependency edge costs a hop against `--depth`, or
   enters free the way a relation does (decision 38). A marker is more
   like an attribute of the section declaring it than a hop a reader
   clicks through, which argues for free. Decision 38's own warning
   applies too: free in both directions collapses distance.
2. Whether the quote itself renders on the edge, or only in `check`'s
   report. A drawn graph has no room for a sentence. A JSON dump has
   room for all of it.
3. Whether a marker that resolves to a *block* rather than a heading is
   legal. `resolve_target` finds a heading. `find_slug` searches blocks
   too.
4. Which cache version this lands on. `graph/cache.rs` is at `VERSION`
   3 and `plans/plan-label-resolution.md` already needs 4. Whichever
   ships second takes 5. Neither should assume it went first.

## What this explicitly does not do

- Change decision 32. A prose dependency stays advisory for `check`'s
  exit code, for the reason decision 32 gives.
- Change what a marker looks like, or add a second `quote=`.
- Make weave corpus-wide. That stays decision 41's own scope question.
- Remove anything from weave, or slow its remaining open questions.

## Critical files

- `src/graph/model.md` -- one edge kind for a prose dependency, one for
  `deps=`/`xdeps=`, beside `Produces` and `Reads`.
- `src/graph/build.md` -- markers read during the same single pass that
  already produces headings, blocks and raw links.
- `src/depends.md` -- `resolve_target` reused rather than a second
  resolution path.
- `src/graph/resolve.md` -- an unresolvable target gets decision 8's
  own placeholder.
- `src/graph/cache.md` -- the new edges encoded, and a `VERSION` bump
  coordinated with the label-resolution plan's own.
- `src/tui/app.md` -- `compute_dep_data`'s second corpus parse retired
  in favour of the graph.
- `src/render/dot.md` / `mermaid.md` / `html.md` -- a look for each new
  kind, the way a block node already has one.
- `architecture.md` -- the decisions, *Open questions* updated where it
  defers this, and a milestone number for weave.

## Verification

1. Edit loop per CLAUDE.md, then `dankg fmt --check` and
   `dankg check .`.

2. Unit tests: a marker becomes an edge with the declaring section as
   `from`; an unresolvable target becomes a placeholder and warns; a
   `deps=` and an `xdeps=` each become an edge; a marker and a written
   link between the same two sections stay two distinguishable edges.

3. A cache round-trip test, and a rejected older-version entry.
   `tests/index.rs`'s own `graph_output_does_not_depend_on_the_cache`
   is what catches a missed bump.

4. The four corpus goldens are re-blessed. Unlike the label-resolution
   plan, this one genuinely adds edges to the fixture corpus. An
   unchanged golden would be the bug.

5. `dankg graph . --format json` reports link-kind edges in the low
   hundreds rather than 72. The *Self-hosted corpus stats* block is
   re-run so the recorded figure matches the corpus again.
