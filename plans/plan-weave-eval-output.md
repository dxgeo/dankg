# `dankg weave`: recorded eval output

## Context

`dankg weave` renders every `Block` in a document, including the
`<!-- dankg:result ... -->` marker `dankg eval` writes after a named
code block. Today that marker is not recognized as anything special.
It falls into the same catch-all arm every other passthrough
construct does, and comes out as literal, escaped comment text in the
woven page -- both in HTML (`render/weave_html.rs`) and in Typst
(`render/typst.rs`). The plain code fence that follows it, holding the
block's actual captured stdout, renders too, but as an ordinary,
unrelated code block with no visual connection to the block above it.

<!-- dankg:depends target=../src/render/weave_html.md#blocks quote="Outside the subset. Escaped, not raw: an unparsed construct" -->

This plan makes weave recognize that marker-plus-output shape and
render it as one paired unit: the source block, then its recorded
output, visually grouped, with a distinct look when the recorded run
failed. It adds a staleness check, since presenting a recorded result
without saying whether it is still trustworthy would be worse than not
showing it specially at all. And it adds `weave=hidden`, an info
string attribute that drops a block (and its paired result, if any)
from woven output entirely -- for a block whose source is useful to
read in an editor or to tangle, but adds nothing to the document a
reader sees.

Decision 41 currently states weave "never consults `deps=`/`name=`."
This plan narrows that claim: showing a result honestly now requires
both.

<!-- dankg:depends target=../architecture.md#decision-41-weave-scope quote="Weave never executes anything and never consults `deps=`/`name=`." -->

## What this reuses

**The marker shape `eval::result` already owns.** `parse_marker`
already turns a `<!-- dankg:result ... -->` line into a name, a hash,
a `failed` flag, and `produces`/`reads`. `locate_existing` already
knows the exact three-block shape -- a named `Code`, a one-line
`Passthrough` matching that marker, then a plain `Code` -- because
`write_back` needs to find it to replace it in place. Weave's own
recognizer is the same three-block lookahead, read instead of
written.

<!-- dankg:depends target=../src/eval/result.md#locate_existing quote="Looks immediately after the top-level code block" -->

**The lazy, file-scoped chain resolution `run_one` already uses.**
Decision 19 keeps eval file-scoped; a cross-file `deps=`/`xdeps=`
chain is resolved by loading only what it actually reaches, through
`eval::files::Files`, and a `Graph` is only ever built if a `table:`
xdep is actually found along the way.

<!-- dankg:depends target=../architecture.md#decision-19-eval-scope quote="One file; `deps`/`--all` never cross files." -->

Weave's staleness check follows the identical shape: one chain, built
lazily, for one block at a time, never a whole-corpus walk the way
`dankg check`'s own loop does.

**The shared-emitter pattern decision 45 already established.** A CSV
table and a JSON table converge on one `TableData` shape rendered by
one emitter, regardless of source. The new source-plus-output pairing
does the same: one small struct, one emitter each backend calls,
rather than duplicating the pairing logic between `weave_html.rs` and
`typst.rs`.

<!-- dankg:depends target=../architecture.md#decision-45-data-tables-from-csvjsontsv-fenced-blocks quote="one shared table emitter regardless of which source produced it" -->

**`InfoString`'s existing attribute machinery.** `path=` is already a
weave/tangle-adjacent, single-purpose attribute living in the same
`KNOWN_ATTRS` list as `name=`/`deps=`/`db=`, read through one generic
`get(key)`. `weave=hidden` is a new entry of the same shape, not a new
mechanism.

<!-- dankg:depends target=../src/md/mod.md#info_string quote="Attribute keys DanKG understands. Anything else warns and is ignored." -->

**Decision 9's own boundary.** Nothing here runs code. Staleness is a
hash comparison against a result already on disk, never a re-run.

<!-- dankg:depends target=../architecture.md#decision-9-eval-trigger quote="Static output stays static." -->

## What's new

### Recorded output rendered as a paired unit (decision 46)

Both block walks (`weave_html::blocks`/`block`, `typst`'s own
equivalent) currently visit `doc.blocks` one at a time with no
lookahead. Each gains a windowed pass: at a `Code{info}` block with
`info.name()` set, peek at the next two blocks. When they match
`locate_existing`'s own shape -- a one-line `Passthrough` that parses
as a marker for that same name, then a plain `Code` -- consume all
three as one unit and skip past them in the outer walk, instead of
falling through to the ordinary per-block match arms.

The marker itself is never rendered as text again. Its `failed` field
picks which of two paired styles gets used. In HTML, a wrapping
`<figure>` groups the source `<pre><code>` and the result `<pre>`,
with a `<figcaption>` reading "Output" (or "Output (failed)" when
`failed`); a failed result gets an additional CSS class driving a
distinct border color. In Typst, the equivalent is a `#block` with a
caption line and a conditional stroke color, following the same
pattern `code_or_data_table` already uses to pick between a plain
code block and a `#table()` call. `produces`/`reads`, when non-empty,
print as a small provenance line under the output -- what the block
writes, what it reads -- since a reader looking at typeset output has
no other way to see what a `db=` block's run touched.

A block with a name but no matching marker (an untried eval block, or
a pure tangle target) renders exactly as it does today: an ordinary
code block, unpaired.

### Staleness, checked lazily and file-scoped (decision 47)

A recorded result can go stale the moment its source block, or
anything it `deps=`/`xdeps=` on, changes underneath it. Showing that
result without saying so would tell a reader something false: that
what they are reading is current. Weave itself still never executes
anything -- decision 9 holds -- but it now does the same freshness
comparison `dankg check` and `dankg eval`'s own `--if-stale` precheck
already make, one already-computed `expected_hash` against the
marker's own stored `hash`.

For each recognized pair, weave builds that one block's own
`BlockRef`s via `plan::top_level_blocks` (weave already parses the
whole file; this is the same accessor `eval` already runs against it),
then `plan::plan_for` for the one target name. `plan_for` already
follows a cross-file `deps=`/`xdeps=` chain by loading exactly what it
reaches through `eval::files::Files`, seeded at the woven file's own
root -- nothing new here, the exact mechanism `run_one`'s own
`--if-stale` precheck already uses. A `Graph` is built, lazily, only
if that chain turns out to carry a `table:` xdep; otherwise passing
no graph at all is correct, the same as it is for `run_one`.

`hash_template_for` plus `xdep_hashes` (through `is_stale`) then
answer one question: does the chain's current hash match what the
marker recorded. Any of three outcomes -- a real mismatch, a
`plan::PlanError` (a dependency renamed or removed since this result
was written), or a missing `[lang.*]`/`[db.*]` config section --
collapses to the same rendered state: "possibly stale," a small
caption under the output, no visual noise beyond that. A confirmed
match renders with no annotation at all. This keeps the reader-facing
surface to two states, not three -- the distinction between "provably
stale" and "cannot confirm" is not one a reader weaving a document
needs to make; both mean the same thing to them: don't trust this
number without rerunning `dankg eval`.

This is the piece that narrows decision 41. Weave now reads `name=`,
`deps=`, and `xdeps=` -- read-only, never to plan a run, only to ask
whether a stored answer still matches its own inputs.

### `weave=hidden`: dropping a block from output (decision 48)

`weave` joins `KNOWN_ATTRS` in `src/md/mod.md`, alongside a
`InfoString::weave_hidden(&self) -> bool` accessor returning `true`
exactly when `get("weave") == Some("hidden")`. Any other value is
inert -- ignored, block still shown -- the same loose handling
`timeout()` already gives an unparseable value.

Both block walks check `weave_hidden()` before rendering a `Code`
block, and before starting the paired-recognition lookahead above:
a hidden source block, matched or not to a result marker, produces no
output at all -- not a placeholder, not a collapsed toggle, nothing.
`weave=hidden` is a weave-only rendering hint. It has no effect on
`dankg tangle` or `dankg eval`; a hidden block still tangles and still
evaluates exactly as before.

### Config

No new `[weave.*]` config key. Paired rendering, staleness, and
`weave=hidden` are all always-on, the same way GFM tables and CSV/JSON
data tables (decision 42, decision 45) needed no opt-in flag once
weave existed at all.

## What this explicitly does not do

- Match `dankg check`'s exact whole-corpus staleness sweep. Weave's
  check is scoped to exactly the chains its own recognized pairs
  reach, lazily, the same file-scoped cost model `run_one` already
  accepts -- not a second corpus-wide index build.
- A collapsible "hidden, but click to reveal" affordance in the HTML
  backend. `weave=hidden` means absent from the rendered document,
  full stop.
- A third visual state distinguishing "definitely stale" from
  "cannot verify." Both read as "possibly stale" to a reader.
- Any change to `dankg tangle` or `dankg eval` themselves. `weave`
  gains a new attribute and a read-only hash comparison; neither
  command changes.
- Tagging a result's output with its own format for a table-shaped
  render (decision 45's own deferred "eval interop" note). A `db=`
  block's captured stdout still renders as an ordinary code block
  inside the new paired unit, not as a table.

## Critical files

- `src/eval/result.md` / `eval/result.rs` -- `parse_marker`,
  `locate_existing`'s own shape (reused, not called, since weave reads
  a live `Document` rather than raw source text), `is_stale`,
  `xdep_hashes`, `hash_template_for`.
- `src/eval/plan.md` / `eval/plan.rs` -- `top_level_blocks`,
  `plan_for`, `BlockRef`, `PlanError`.
- `src/eval/files.md` / `eval/files.rs` -- `Files::new`/`discover`,
  the lazy single-entry loader `plan_for`'s cross-file lookups run
  against.
- `src/md/mod.md` -- `InfoString`, `KNOWN_ATTRS`, the new
  `weave_hidden()` accessor.
- `src/render/weave_html.md` / `weave_html.rs` and
  `src/render/typst.md` / `typst.rs` -- both block walks gain the
  windowed pair recognizer, the hidden-skip check, and a call into
  the one new shared emitter.
- `src/render/assets.md` -- `WEAVE_CSS` gains rules for `.eval-pair`,
  its failed variant, and the stale caption.
- `src/weave.md` / `weave.rs` -- `render_html`/`render_pdf` thread
  `Config` and `root` down into the block walk, which needs both for
  the staleness lookup; today neither reaches past `run`'s own top
  level.
- `architecture.md` -- amend decision 41's own text; add decisions
  46, 47, 48.

## Verification

1. Edit loop per CLAUDE.md: `cargo run --release --bin dankg -- tangle . --lang rust -o src`, `cargo build --release`,
   `cargo test`.
2. `dankg fmt --check` over every changed file; `dankg check .` over
   the whole corpus, this plan's own `dankg:depends` markers included.
3. Unit tests:
   - `md/mod.rs`: `weave_hidden()` true only for the exact value
     `hidden`, false for absent or any other value.
   - `eval/result.rs`: unchanged, but its existing `locate_existing`
     tests double as the recognizer's own fixture shapes.
   - `weave_html.rs`, `typst.rs`: a code block with no marker renders
     unpaired (unchanged behavior); a code block plus a matching
     marker plus output renders as one paired unit with no raw
     comment text anywhere in the output; a `failed` marker gets the
     distinct style; a hand-written stale hash renders the "possibly
     stale" caption; a `weave=hidden` block (marker-paired or not)
     produces no output at all.
   - `weave.rs`: an end-to-end case, a scratch file with a real
     recorded result, confirms the `Report` still comes back
     unchanged in shape while the rendered HTML contains the pairing.
4. A manual smoke test against a scratch file: a real `sh name=a`
   block, run once through `dankg eval` to get a genuine result
   marker, then `dankg weave --format html` and `--format pdf`.
   Confirm the pairing renders instead of raw comment text, in
   both backends. Hand-edit the recorded hash to something wrong and
   re-weave; confirm the stale caption appears. Add `weave=hidden` to
   the source block and re-weave; confirm it, and its output, are
   both gone.
