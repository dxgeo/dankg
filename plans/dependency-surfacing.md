# Dependency Surfacing

DankG TUI design note. Status: implemented in full -- §2's kind
marker, badges, and panel rows (A-E), and §3's filter. See
architecture.md's own *Dependency surfacing*. Touched `tui/app.md`
and, for six visibility changes, `eval/plan.md`/`eval/result.md`.

Give code-block nodes their own visual identity in the tree, and make
two kinds of block-to-block dependency -- the `deps=`/`xdeps=` eval
chain, and `produces=`/`reads=file:PATH` file artifacts -- visible and
filterable. Prose dependencies (`dankg:depends`) are explicitly out of
scope for this pass.

## 1\. Where things stand today

Three things get called a "dependency" in this codebase, and only one
has any presence in `graph::model` -- the structure the TUI's tree and
panel actually read from. The other two live entirely inside
`eval::plan`, re-parsed from disk on demand, never materialized as a
node or edge.

| Concept          | Declared as                                      | Graph / TUI today |
|-------------------|--------------------------------------------------|--------------------|
| Code block         | fenced block, `name=`                             | `NodeKind::Block` -- a real tree row, drawn identically to a heading |
| Eval chain          | `deps=` / `xdeps=name`                            | none -- `eval::plan::BlockRef` only, resolved fresh per eval run |
| File artifact         | `produces=file:PATH` / `reads=file:PATH`         | none -- decision 33: "never resolved on its own" |
| DB relation touch      | inferred, not authored                         | already shown: `⚭` badge, `produces:`/`reads:` panel rows |

**Correction, kept for the record.** Mid-discussion I first guessed
the existing `⚭` badge already covered "file dependency." It doesn't
\-- `⚭` is decisions 35/36/38's database-relation lineage, a different
subsystem from decision 33's file-path artifacts. The two only collide
on the English word "produces."

## 2\. What this proposes

Three additions, all display-only -- nothing here changes what
`graph::build`, `fmt`, or `check` compute.

### A. Kind marker

A `Block` node gets a `»` prefix folded into its `title` where
`TreeRow`s are built (`push_row`, `tui/app.rs`) -- not a new parameter
on `draw::tree_line`, which stays exactly as graph-agnostic as its own
module doc already insists on. Costs nothing against search: the
glyph never appears in a typed query, so
`n.title.to_lowercase().contains(&needle)` is untouched.

### B. Eval chain -- resolved and navigable

New badge counts, `⇒N` (this block's own resolved
`deps=`/`xdeps=` targets) and `⇐N` (other blocks that name this one),
sitting next to the existing `→N ←N` for links -- double strokes for a
build dependency, single strokes for a written link. Two new panel
rows, `PanelRow::DepOut` / `DepIn`, navigable exactly like
`Outgoing`/`Backlink` today: same `target()`, same `enter`-to-jump
through `reveal_and_select`.

### C. File artifacts -- shown, not navigated

Decision 33 is explicit that `produces=`/`reads=file:PATH` is a check
on top of an existing `deps=`/`xdeps=` edge, not an independent
relationship -- so there's nothing of its own to resolve. One fixed
badge glyph, `▤`, plus a non-navigable panel row
(`PanelRow::FileDep`, the same shape as the existing DB-relation text
row) showing the declared path.

Stretch, not core: reuse `plan::check_file_deps`'s own output to flag
a block whose `reads=` doesn't match any dependency's `produces=` --
an early warning in the tree, before `dankg check` catches it.

### D. Broken deps -- shown, not just counted

`resolve_dep` and `resolve_xdeps` don't just report that a
`deps=`/`xdeps=` entry failed to resolve -- they already know
why: `DepLookup::Escapes` versus `DepLookup::NotFound`,
converted by the existing `dep_error`/`xdep_error` into the
same `PlanError` variants `dankg check` already prints. The
`⇒N`/`⇐N` badges above only count targets that *resolved*,
so a broken entry is invisible in the tree today. This adds
that, reusing the existing error text instead of inventing
new wording.

<!-- dankg:depends target=#b-eval-chain----resolved-and-navigable quote="this block's own resolved `deps=`/`xdeps=` targets" -->

For `xdeps=` this is already free -- `resolve_xdeps` is
already public and already returns
`Result<Vec<usize>, PlanError>`. For plain `deps=`, §4's one
visibility change needs two more beside it: `dep_error` and
`DepLookup` itself, both to `pub(crate)`. **Correction, found
while implementing:** the original draft here leaned toward
leaving `DepLookup` private, reasoning a caller never matches
its variants directly, only passes one straight into
`dep_error`. Rust disagreed -- a `match` arm still needs the
type nameable from its own call site to bind a value of it at
all, regardless of whether the variants themselves ever get
inspected.

<!-- dankg:depends target=#4-where-the-data-comes-from quote="the one visibility change this proposal needs is `pub(crate)`" -->

A broken dep has no `NodeId` to navigate to, unlike
`DepOut`/`DepIn`. It needs a non-navigable row instead --
the same shape as the file-artifact row above: a new
`PanelRow::DepBroken` showing the matching `PlanError`'s own
`Display` text verbatim, so the TUI never invents wording
`dankg check` doesn't already use.

<!-- dankg:depends target=#c-file-artifacts----shown-not-navigated quote="`PanelRow::FileDep`, the same shape as the existing DB-relation text row" -->

One consequence for §3's filter: a block whose `deps=`
failed to resolve still *declares* one, in the sense the
Eval-chain filter cares about. It must not disappear under
that filter -- it's exactly the block a reader turns the
filter on to find.

This matches §4's own staleness policy: browsing already
tolerates a stale snapshot between edits, so the tree is
well placed to show a bad `deps=` first -- before the next
`dankg check` does.

<!-- dankg:depends target=#4-where-the-data-comes-from quote="Browsing tolerates a stale snapshot until restart; only *running* code needs a guaranteed-fresh read." -->

### E. Unresolved-but-correct -- needs to run, not broken

`deps=` and `xdeps=` are not symmetric. A `deps=` target
always gets (re-)run alongside its consumer -- `plan_for`
walks it into the chain. An `xdeps=` target, block- or
`table:`-targeted, never does: it is checked, not run.
"Hasn't run yet" is therefore a real, expected state for
`xdeps=` -- never for `deps=`, which running the consumer
already fixes on its own.

<!-- dankg:depends target=../src/eval/plan.md#eval-plan quote="an `xdeps` target is not walked into the chain" -->

This is already computed, not new.
`eval::result::verified_hash` (private) and its public
wrapper `xdep_hashes` already distinguish never-run,
stale, and `xdeps`-cycle, each with its own existing
wording, and `chain_xdep_hashes` wraps a `table:NAME`
producer failure the same way. `dankg check`'s own
staleness loop and `run_one` both already call this
before trusting or running a block.

<!-- dankg:depends target=../src/eval/result.md#eval-result quote="silently treated as fresh: `xdeps` is a trust boundary on purpose," -->

One shape mismatch: `xdep_hashes` fails fast --
`Result<Vec<u64>, String>`, the *first* problem in a whole
chain, not one reason per entry. §D's one-row-per-declared-
entry model needs `verified_hash` itself, called once per
resolved index, not the aggregating wrapper -- a third
`pub(crate)` bump, beside `resolve_dep` and `dep_error`.

This stays a state of its own, not folded into `DepBroken`:
broken means the reference is wrong; needs-run means the
reference is correct but unexecuted. A new, non-navigable
`PanelRow::DepPending`, `DepBroken`'s shape with its own
glyph -- conflating the two would bury an expected, benign
state under the same signal as an authoring mistake, exactly
the risk decision 32 already names for a noisy check.

<!-- dankg:depends target=#d-broken-deps----shown-not-just-counted quote="so a broken entry is invisible in the tree today" -->

<!-- dankg:depends target=../architecture.md#decision-32-prose-dependencies quote="a weak signal that gates a build trains a reader to silence it rather than read it." -->

Scope stays on `xdeps=`, block- and `table:`-targeted alike.
Plain `deps=` doesn't need this: `plan_for` already runs it
alongside its consumer, so it can never be "correct but
unexecuted" the way an `xdeps=` target can.

### Rendered, roughly

```
▾ Eval in the TUI
    » setup            ▤
    » fetch_rows        ⇒1 ▤
    » render_report      ⇒1 ⇐1  →2 ←1
```

- `»` -- block kind (new)
- `⇒N` -- declares N `deps=`/`xdeps=` targets (new)
- `⇐N` -- named by N other blocks' `deps=`/`xdeps=` (new)
- `▤` -- declares a file artifact (new)
- `→N ←N ⚭` -- existing, links and DB-relation touch, unchanged

## 3\. Filtering

Nothing prunes the tree today -- `visible_rows_with` only ever changes
which nodes are *expanded*, never which ones exist at all. A new
`Filter` state on `App` changes that, picked from among:

```
All, Blocks, Eval-chain, File-artifact
```

Status line shows `filter: eval-chain`, the same way search shows
`/query` and eval-cycling shows `eval: [...]`. `f` is free today --
every fixed key is arrows/enter/tab/esc/`/`/`n`/`N`/`?`, every
remapped default is `k j h l q r e b`.

**Redesigned after shipping: a menu, not a cycle.** `f` first shipped
cycling straight through the four states in a fixed order on every
press. Feedback after using it for real: cycling forward through
states you do not want just to reach the one you do is friction a
picker does not have. `f` now opens a small overlay
(`App::open_filter_menu`) with a cursor over the four names; up/down
move it (`Filter::next`/`prev`, reused from the old cycle order
rather than a second list), `enter` applies whichever it is on
(`App::confirm_filter_menu`), `esc` closes it unapplied
(`App::cancel_filter_menu`). Drawn as a small box floating over the
still-visible tree (`draw::box_grid` + `draw::overlay`, new
primitives, centered by `draw::centered`), not a full-screen
replacement -- and once that compositing existed, `?`'s own help
reference moved onto it too, replacing its earlier full-screen
takeover with the identical kind of box. `box_grid` sizes itself to
its own longest line with no idea what terminal it will land in, so
`render` clips each help line to what actually fits before boxing
them: several are long enough on their own to have pushed an
unclipped box's right border off screen entirely on an ordinary
80-column terminal.

**Added after shipping: each filter remembers its own last row.**
Original ask, verbatim: "if I change the filter from All but I don't
navigate anything, returning to All should return to the remembered
position, but if I do navigate in the new filter, it should stay on
the row I navigated to when returning to All." Generalized to every
filter, not just `All`: `App::filter_history` remembers where
`self.selected` was the last time each `Filter` was active, keyed by
variant, written every time the filter changes. `App::apply_filter`
reads it back on the way into a filter -- force-expanding its
ancestors (`reveal_and_select`) rather than a plain "select if
visible," since a plain check would silently fail on a collapsed
ancestor and reintroduce the exact stuck-cursor bug below, just via
collapse instead of filtering. The one override:
`App::moved_since_filter_change`, set by every real navigation action
(arrows, `/`-search, a panel jump) and cleared on every filter
change. A reader who has explicitly moved since the last filter
change keeps that new position instead of whatever the target filter
remembers -- on every filter, `All` included, since "I just navigated
here on purpose" always outranks "here is where you were three
filters ago."

Semantics: **hide, not dim.** A non-matching row disappears; its
ancestor headings stay, so the reader can still see where a match
lives -- the same mental model `/`-search already trained (jumping
reveals a match's ancestors without touching `self.expanded`), just
as a standing state instead of a one-shot jump. The included set is
*matches ∪ their ancestors*. **Correction, found while
implementing:** rebuilt fresh on every call to `visible_rows_with`
(`App::filter_membership`), not cached once per filter change as
first drafted here -- the same "just rebuild it, cheap at this
corpus's size" reasoning `push_row` itself already runs on every
frame, so a cache would only add invalidation logic for a cost
nothing else here bothers to avoid.

**Settled.** The Eval-chain filter matches a block that *declares*
`deps=`/`xdeps=`, or is only ever a *target* (a shared `setup` block
with nothing of its own to declare) -- **match on either**. A filter
meant to show "the eval-dependency graph" that hid the leaves
everything else depends on would have defeated its own purpose.

**Bug, found by hand after shipping:** changing the filter while a
row that does not survive the new one is selected -- a plain heading,
say, right before switching to Blocks -- left `self.selected` pointed
at a node `visible_rows` no longer produces. Every arrow key uses
`move_tree_cursor`'s own `position` lookup against the *visible* row
list; a `self.selected` missing from it means that lookup never
finds a starting point, so the reader is stuck, unable to move at
all, until `r` resets the session. `App::apply_filter` now calls
`reselect_after_filter_change` as its own last-resort fallback, which
walks `self.selected`'s own ancestor chain to the nearest one the new
filter still keeps, falling back to the first match anywhere
(`index.nodes`' own deterministic order) when nothing in that chain
survives either -- the same "still visible" handling `reload` already
does for a fresh index, applied here to a filter change instead. (The
per-filter memory added afterward, above, now tries a remembered
position first; this fallback is what still runs on a filter's own
first-ever visit, or when its remembered row no longer exists.)

## 4\. Where the data comes from

Computed once at `App::load`, cached alongside `self.index` -- the
same staleness policy as the rest of the tree, not refreshed on every
keypress the way the eval-cycle feature deliberately re-reads from
disk. Browsing tolerates a stale snapshot until restart; only
*running* code needs a guaranteed-fresh read.

- **Source.** `eval::files::Files` already exists for exactly this:
  `load_all` over every path `corpus.files` already lists, then
  `all_blocks()` for a deterministic, corpus-wide `Vec<BlockRef>`. A
  second parse of every file -- accepted, since `ParsedFile` never
  keeps its source `Document` around, and a once-at-load pass is
  strictly cheaper than the eval-cycle feature's existing per-keypress
  re-read.
- **Correlation.** `BlockRef -> NodeId` by `(file, line)`, not
  `(file, name)` -- names collide and get slug-suffixed by `Slugger`;
  lines don't.
- **`deps=` resolution.** Reuse `eval::plan::resolve_dep` rather than
  re-deriving its cross-file-path-and-name lookup. It's private
  today -- the one visibility change this proposal needs is
  `pub(crate)`.
- **`xdeps=` resolution.** Already public: `resolve_xdeps` for a
  block-targeted entry, `table_xdeps` + `graph::query::find_producer`
  for decision 35's relation-targeted `xdeps=table:NAME` form.

## 5\. Effort map

**Mostly wiring:**

- Kind marker
- New badge glyphs
- New panel row variants
- Dependency resolution -- all reuse existing (or soon-`pub`)
  functions

**The real lift:**

- Filter's row-pruning mechanism -- genuinely new: a membership
  computation the tree walk has never needed before

## 6\. Suggested order

1. **Kind marker.** No data plumbing, immediate visual payoff -- good
   first PR to get the glyph taste-tested before anything else builds
   on it.
2. **Load-time correlation.** `Files::all_blocks()` + the
   `(file, line)` match + the `resolve_dep` visibility bump. Plumbing
   only, testable in isolation, no UI yet.
3. **Badges + panel rows.** Makes step 2's plumbing visible for the
   first time.
4. **Filtering.** The standalone, higher-risk piece -- last, once the
   facts it filters on already render correctly.

## 7\. Decisions, as shipped

| Question | Shipped as |
|---|---|
| Block-kind glyph | `»` -- alt considered: `ƒ` |
| Eval-chain glyphs | `⇒N` / `⇐N` -- alt considered: `↳N` / `↰N` |
| File-artifact glyph | `▤` -- alt considered: `⌁` |
| Broken-dep indicator | a separate glyph, `✗N`, not folded into `⇒N` |
| Needs-run indicator | a separate glyph, `↻N` -- first drafted as an hourglass, `⌛N`, which turned out to be a default-emoji-presentation codepoint (renders wide and in color, breaking column alignment); `↻` is text-presentation and fits the existing arrow family (`→ ← ⇒ ⇐`) |
| Filter key | `f`, opens a picker menu (redesigned from cycling -- see §3) |
| Eval-chain filter rule | declares *or* is targeted (see §3) |
| `check_file_deps` mismatch flag | defer past this pass |

***

Everything above is implemented, in the order §6 laid out. The
`check_file_deps` mismatch flag is the one deferred item still
outstanding, exactly as planned.
