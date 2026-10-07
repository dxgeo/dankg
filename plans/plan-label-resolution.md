# A produced artifact is a node, and it carries the slug

Status: implemented. Decisions 70 and 72 shipped in 0.9.0. Decisions
69, 71, 73, 74 and 75 followed, and `architecture.md` is the record of
each as built. Two of them are narrower there than below, and the prose
there says why. Decision 71 refuses a *bare* reference to a pair when
the output is a PDF, because Typst cannot reference a plain block at
all. Decision 69 keeps the artifact's slug in its file's own namespace
rather than under the synthetic `file:` prefix proposed below, because
a synthetic namespace would have left `[[#quarterly]]` unresolvable.

Two things followed it. Decision 76 added the `reads=file:` edge this
plan deferred. Decision 77 closed the last piece, which was wider than
this plan framed it: *What this reuses* below asserts that an artifact
shares one namespace with every heading and block name in its file, and
that was true of `graph::build` alone. Weave derived its own heading
slugs, figure labels and pair anchors from three separate `Slugger`s, so
the two tools could disagree about one document. Weave now takes all
three from `graph::build`. Nothing is left open.

## Context

Decision 63 gave a figure a label, defaulting to its block's own
`name=` and overridden by a reader-written `label=`. Decision 64 made
a same-file wikilink resolve against it. Both shipped in 0.9.0. Both
work inside `dankg weave`.

The graph never learned about `label=`. `graph/build.rs` builds a
block's node from `info.name()` alone. A reference written against a
label resolves in weave and reads as a dead link to `dankg graph`, to
`dankg check`, and to the TUI's own cross-reference panel. `check` is
the CI gate. A corpus adopting `label=` therefore fails its own gate
over a reference that is correct.

The first draft of this plan proposed recording the label as an alias
on the block's own node. That draft is abandoned. A label names the
*figure*, not the block that produced it. Aliasing the block gives one
node two names for two different things. The right shape is two nodes,
each with its own name.

<!-- dankg:depends target=../architecture.md#decision-63-a-figures-label-comes-from-its-blocks-own-name-or-from-label quote="A `label` attribute joins `KNOWN_ATTRS` beside `caption` and `figure`" -->

## What the probes established

**A produced file has no node. A produced relation does.**
`graph/build.rs` gates node creation on `if let Some(db) = info.db()`.
A `[db.*]` block's own `produces=orders` becomes a `Relation` node
under a `db:warehouse` namespace, joined by a real `Produces` edge.
`produces=file:chart.png` on any other block becomes nothing at all.
`a_block_with_no_db_never_gets_a_relation_even_with_a_produces_marker`
pins that deliberately. So the graph already models one kind of
produced thing and not the other, which is a gap with or without
labels.

**The two tools resolve disjoint sets.** A block written
`name=chart label=trend` answers to `trend` in weave and to `chart` in
the graph. Neither fragment works in both. `weave::figures` computes
`info.label().or_else(|| info.name())`. A label replaces the name
rather than adding to it. `graph/build.rs` reads the name and nothing
else.

| written | weave | `dankg graph` and `check` |
| --- | --- | --- |
| `[[#chart]]` | fails the weave | resolves |
| `[[#trend]]` | resolves | dead link |

**The failure message on the first row is wrong.** A real run says the
artifact could not be read. The artifact read fine.
`unresolved_message` falls through to its artifact branch because the
block *is* a figure, just not under the fragment asked for. Nothing
distinguishes "no such figure" from "that name belongs to something
else here".

**The cache stores what resolution reads.** `graph/cache.rs` is a
line-oriented format, and it stood at `VERSION = 3` when this plan was
written. Anything new that resolution depends on has to be encoded,
decoded, and version-bumped. A warm cache otherwise serves pre-change
nodes that still hash as fresh. Decision 69 took the bump to 4, and
decision 76 took it to 5.

<!-- dankg:depends target=../src/graph/cache.md#graph-cache quote="5: a row for each `reads=file:` a block declares (decision 76)" -->

## What this reuses

**`push_relation_node`, `relation_node`, and `EdgeKind::Produces`.** A
produced relation is already a node the producing block points at
through a `Produces` edge. A produced file is the same shape with a
different namespace. Nothing about the edge, the dedupe, or the
rendering needs inventing.

**`Slugger`, and the per-file namespace it already owns.** An
artifact's own name shares one namespace with every heading and block
name in its file. One walk assigns all three, which is what makes a
collision visible where it happens.

**`weave::figures`, which already owns the name.** It resolves an
override against a default in one walk and already warns on a
collision by line. What changes is where the default comes from.

<!-- dankg:depends target=../architecture.md#decision-63-a-figures-label-comes-from-its-blocks-own-name-or-from-label quote="Two figures claiming one label warns at the second one's own line." -->

## What's new

### Decision 69: A `produces=file:` artifact is a node

A block declaring `produces=file:PATH` gets a second node for the file
it writes, joined to the block by the `Produces` edge a relation
already uses. The block node stays exactly what it is.

This closes a gap that predates labels. `dankg graph` already draws a
produced relation, because a `[db.*]` block's own recorded
`produces=orders` builds one. A produced file was left out. A corpus
whose blocks write CSVs and charts shows the code and never what the
code made.

The artifact is what a figure reference points at. A reference names
the thing on the page. The thing on the page is the artifact, not the
source block above it. Two nodes give the two fragments two meanings
rather than making one node answer to two names.

The node lives under a `file:` namespace, the way a relation lives
under `db:NAME`. An artifact is addressable inside its own file
through its name alone. A same-file `[[#trend]]` needs no namespace
written out.

### Decision 70: An artifact's slug comes from its path, or from `artifact=`

The default is the path's own stem: `produces=file:data/quarterly.csv`
gets the slug `quarterly`. That mirrors a heading, whose slug is
derived from the text the author already wrote rather than declared
separately. Most artifacts need no attribute at all.

`artifact=` overrides it for one block, and replaces `label=`.
Decision 63's reason stands unchanged: a block's `name=` is its eval
identity and its tangle identity. A path is a filesystem detail.
Neither should have to change because the prose wants a better word.

`label=` is too generic to keep. One attribute became a Typst label,
an HTML id, and a graph slug at once. `artifact=` names what it
names. The rename is mechanical. `dankg fmt` canonicalizes attribute
order. 0.9.0 has no external users.

Both backends derive their own identifier from that one slug, exactly
as they do now: `<fig:SLUG>` in Typst, `id="fig-SLUG"` in HTML.

A declared slug colliding with a heading slug, a block name, or
another artifact warns at its own line and is dropped. That artifact
falls back to its path-derived default. A derived slug that collides
is suffixed by `Slugger` the way a repeated heading already is,
because a derived collision is not the author's mistake to fix.
Suffixing a *declared* slug is refused for decision 63's own reason:
a silently suffixed slug is a reference that silently points at the
wrong figure.

### Decision 71: A recognized pair is addressable in both backends

`[[#chart]]`, naming the block itself, resolves to the pair's own
rendered box. HTML puts an `id` on the `<figure class="eval-pair">` it
already emits. Typst puts a label on the `#block(stroke: ...)` it
already emits.

Without this, decision 69 moves the disagreement rather than ending
it: the graph would resolve `[[#chart]]` to the block node while weave
still had nothing to point at. A block and the artifact it produces
are two targets. Both are addressable. Each means what it says.

An unpaired block gets no anchor, since decision 46's box is what a
pair renders and there is nothing to point at without one.

### Decision 72: A reference naming something that exists under another name says so

`unresolved_message` gains one answer before its artifact branch. A
fragment naming a block whose artifact is addressable under a
different name reports that, rather than reporting whichever artifact
condition happens to match.

Decisions 69 and 71 make the common case unreachable. The message
stays because it is wrong today, and because a fragment can still miss
in ways the other decisions do not cover.

### Decision 73: An artifact node draws, marked by a file icon

An artifact node is drawn by `dankg graph` and listed in the TUI tree,
the way a heading and a block already are. Nothing gates it behind a
flag.

`--live` is the precedent for gating. It does not apply. A live
catalog spawns a process per `[db.*]` to ask an external system what
exists. An artifact is read straight out of an info string the corpus
already holds, at no cost. A flag would hide a node the corpus states
outright.

`kind_marker` gains a file icon for the new variant, beside a block's
own marker. That match is exhaustive by design. The new variant fails
to compile until its icon is chosen.

### Decision 74: An artifact's title is its path, and its slug is not

A node carries a title and a slug. The two answer different
questions. A title is what a reader sees in the TUI tree and in a
drawn graph's own label. A slug is what a reference resolves against.

An artifact's title is its path as written: `data/quarterly.csv`. A
path is what identifies the file on disk. A stem alone loses the
directory. Two artifacts whose paths share one stem would otherwise
read as one row in the tree.

Its slug stays decision 70's own: the stem, or `artifact=`. A slug is
typed by hand inside a reference. `[[#quarterly]]` is the point of
this plan. `[[#data/quarterly.csv]]` would not be.

`Relation` is the precedent. `relation_node(id, title)` already takes
the two separately. A relation's own title is display text that
nothing resolves against.

### Decision 75: An artifact node borrows its block's line, and owns none of it

An artifact node reports the line of the block that produces it. That
line is what `enter` hands the reader's editor. It is also what a dot
or an HTML label prints after the file name. There is no other source
to look at. A relation node's own `line: 0` would instead print
`report.md:0`, on a node decision 73 now draws.

Borrowing a line is not owning it. An artifact node joins
`NodeKind::Relation` in every guard that writes to a line, or reads a
span:

- `write_tag_for` and its removal counterpart. A `dankg:tag` marker
  anchored on a borrowed line would land on the producing block's own
  fence and read as that block's tag.
- `blocks_in_section`, which the TUI's eval picker calls with a
  node's own span. A borrowed span hands back the producing block,
  which the block node already offers.

`end_line` matches `line`. A span the node does not own has nothing
to measure.

The guard those call sites already carry was written for a real bug.
Its reasoning survives this change with one word moved. A node whose
line was `0` reached `tag::write_back`'s own `anchor_line - 1` on a
`u32`, underflowed, and panicked on the resulting near-`u32::MAX`
splice index. The post-mortem reads that such a node has no real line
for a marker to attach to at all. That stays true of an artifact.
What changes is the test. A line of `0` no longer detects it. The
kind does.

## Why not the alternatives

**Alias the block's own node with the label.** The first draft of this
plan. It loses because a label names the figure. The block node would
answer to a name that is not its own. It also leaves a produced file
invisible to the graph, which is the larger gap underneath.

**Make the block node's slug the label.** One line in `build.rs`, with
no change to weave at all. It loses on decision 20, which fused node
scope and eval scope deliberately: a block `dankg eval` runs as
`chart` would appear in the graph as `trend`. Adding a label would
also move an existing node id silently.

**Scope the attribute to weave and rename it `weave_label`.** It loses
because a name cannot fix the gate. `dankg check` resolves prose links
generically and cannot tell a weave-only reference from any other. A
weave-scoped name appearing in a `[[#...]]` still reports a dead link.
Only deciding whether the graph knows the name fixes that. Naming it
for the command that reads it also sits badly once the graph reads it
too.

**Drop the attribute.** The gap disappears with the feature. It loses
because the default would then be a path. A path is a filesystem
detail no prose should have to quote.

**Fix only the message.** Decision 72 alone. It loses because `check`
would keep reporting a correct reference as a dead link, which is a CI
gate failing over a document that is right.

## Open questions

None. This plan raised three. Each was settled in review.
Decisions 70, 73, 74 and 75 record the answers. The plan was built from
there. A fourth question surfaced during the build rather than before
it: whether Typst can reference the element decision 71 labels. It
cannot, and decision 71 carries the answer.

## What this explicitly does not do

- Any change to `Inline::WikiLink`, the markdown parser, or the
  reference syntax. `[[#chart]]` and `[text](#chart)` both parse
  today.
- A node for `reads=file:`. The reverse edge can follow once the
  forward one exists. It did: decision 76 joins a `reads=file:` to the
  artifact its producer already built, corpus-wide.
- Cross-file references. Weave renders one file (decision 41).
  Decision 64 narrows that for a fragment with no file name in it
  alone.
- Aliases for headings. A heading has one slug. Nothing has asked for
  a second.

## Critical files

- `src/graph/build.rs` / `build.md` -- a `produces=file:` artifact
  becomes a node under a `file:` namespace, joined by `Produces`; its
  slug is assigned from the same per-file `Slugger` a heading and a
  block name come from. Its title is its own path (decision 74). Its
  line is the producing block's (decision 75).
- `src/graph/model.md` -- a node kind for an artifact, beside
  `Relation`.
- `src/graph/resolve.md` -- nothing, if the artifact's slug is a real
  slug in its own file. That is the point of making it a node rather
  than an alias.
- `src/graph/cache.md` -- the artifact node encoded and decoded, and
  `VERSION` 3 to 4. A stamp answers "is this the file I indexed",
  which stays true when build's reading of an unchanged file changes.
  Not bumping would serve stale nodes that still hash as fresh -- the
  same trap decision 62 hit.
- `src/md/mod.md` -- `artifact=` joins `KNOWN_ATTRS` in place of
  `label=`. The accessor is renamed with it.
- `src/weave.md` -- `figures` takes its default from the artifact's
  path rather than the block's `name=`; `unresolved_message` gains
  decision 72's own answer.
- `src/render/weave_html.md` / `typst.md` -- an anchor on the pair's
  own box (decision 71).
- `src/tui/app.md` -- `kind_marker` gains the file icon (decision 73),
  and the artifact kind joins `NodeKind::Relation` in the guards
  decision 75 names.
- `architecture.md` -- decisions 69 through 75; decision 63 narrowed
  to say the slug belongs to the artifact.
- `README.md` and `example/weave_example/report.md` -- the renamed
  attribute. The "A gap worth knowing about" section goes with the
  gap.

## Verification

1. Edit loop per CLAUDE.md, then `dankg fmt --check` and
   `dankg check .`.

2. Unit tests:

   - A block with `produces=file:data/quarterly.csv` builds an
     artifact node slugged `quarterly`, titled `data/quarterly.csv`,
     and a `Produces` edge from the block to it.
   - A `db=` block's own relation node is unchanged, which is what
     makes decision 69 an added namespace rather than an altered one.
   - An `artifact=` override reslugs the artifact node and leaves the
     block node alone. The title stays the path.
   - A declared slug colliding with a heading slug warns by line. The
     artifact falls back to its path-derived default. The heading
     keeps its own unsuffixed slug.
   - Two artifacts whose paths derive one slug are suffixed by
     `Slugger`, never warned about.
   - `[[#quarterly]]` resolves to the artifact node and `[[#chart]]`
     to the block node, in one document.
   - A recognized pair carries an anchor in both backends. An unpaired
     block carries none.
   - An artifact node reports its producing block's own line, and
     `kind_marker` draws the file icon for it.
   - `write_tag_for` refuses an artifact node, the way it already
     refuses a relation. That is decision 75's own guard. It keeps a
     borrowed line from taking a marker.

3. A cache round-trip test: a file with an artifact node encodes and
   decodes intact. A `VERSION` 3 entry is rejected rather than
   decoded. `tests/index.rs`'s own
   `graph_output_does_not_depend_on_the_cache` is what catches a
   missed bump.

4. The four corpus goldens are re-blessed only if the test corpus
   gains a produced artifact. It has none today. An unchanged golden
   is therefore itself evidence that decision 69 adds nodes rather
   than moving them.

5. `dankg check example/weave_example` reports zero unresolved nodes
   beyond the deliberate `[[SomeOtherFile]]`, which is the symptom
   that started this plan.
