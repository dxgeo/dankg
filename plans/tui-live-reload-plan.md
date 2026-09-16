# TUI live reload: no watcher, no lost expansion

## Context

Today the TUI only reloads on purpose: pressing `r`, returning from the
configured `[editor] command` handoff, or finishing an eval run. Every
case calls the same `App::reload` (*Load, reload, reset*,
`tui/app.md`), which re-runs `build` (a fresh `index::load` +
`resolve::resolve`) and then, on purpose, drops every entry in
`self.expanded` rather than replaying it against the new tree.

<!-- dankg:depends target=../src/tui/app.md#load-reload-reset quote="risks a confusing placement more than starting clean costs a keypress" -->

The ask this plan answers: reflect a file saved from another editor
while the TUI keeps running, with no keypress needed, on a corpus of
thousands of files rather than this repo's own ~70. Two designs were
considered first and both fail at that size, for different reasons.

**Watch only the selected node's file.** Cheap, but wrong regardless of
scale: dependency staleness is corpus-wide by design, not scoped to one
file.

<!-- dankg:depends target=../architecture.md#decision-6-index-scope quote="bidirectional links need the full corpus" -->

Watching one file at a time silently reintroduces the exact gap
`dankg check` exists to close, just at the TUI's own freshness layer:
an edit to a file the reader is not currently looking at, that some
other file's `dankg:depends` or eval `deps=` points at, would never be
noticed until the reader navigates there or presses `r`.

**Watch every indexed file, unconditionally reload on any change.**
Correct, and the detection cost is not the problem -- see below. The
problem is `reload`'s own "drop every expansion" policy, which is
sound when the reader just caused the edit themselves and wants a
fresh frame, but becomes the steady state rather than an edge case
once thousands of files means edits happening constantly, most of them
nowhere near what the reader is currently looking at.

## What this reuses

**Parsing is already incremental.** `graph::cache` keys each file's own
index contribution on `(mtime, len)`, verified by a content hash. An
unchanged file is a cache hit regardless of corpus size; only a file
that actually changed gets re-parsed. A "full reload" already costs
close to nothing for everything the reader didn't touch.

<!-- dankg:depends target=../src/graph/cache.md#graph-cache quote="An entry is keyed on `(mtime, len)` and verified by a content hash." -->

**Resolution stays corpus-wide, on purpose.** `resolve::resolve` still
walks the whole index on every reload, and that is correct, not a cost
to engineer around: backlinks are only honest once every file has been
seen (decision 6, above). Scoping resolution down to "just the changed
file" would make backlinks lie.

**No new file-watching FFI.** `plans/backend-protocol-plan.md` already
worked through this exact question for `dankg serve`'s own change
notifications and picked polling on the existing mtime/hash keying
over hand-rolling `inotify`/`kqueue`/`FSEvents` per platform. The same
reasoning applies here, so this plan does not quietly reopen a decision
already settled elsewhere in the repo.

<!-- dankg:depends target=../plans/backend-protocol-plan.md#phased-build-order quote="this repo has shown no appetite for new per-platform FFI beyond what's already justified" -->

**The event loop already ticks.** `event_loop`'s inner wait loop already
polls `term::stdin_ready(RESIZE_POLL_MS)` on a 100ms cadence purely to
notice a resize with no keypress. That tick is the natural hook for a
file-change sweep -- no second loop, no second timer.

**`self.selected` already survives a reload correctly.** `is_visible_under`
already answers "does this one node still make sense in the new tree,"
and `reload` already uses it as a best-effort check before resetting
selection. The fix below generalizes that same check to `self.expanded`
rather than inventing a second mechanism.

## What's new

### Detection: the existing tick, a slower cadence

Stat-sweeping thousands of files at 100Hz-worth of ticks is wasted work
for a check that only needs to feel prompt, not frame-rate. Gate the
sweep to run every Nth tick of the loop that already exists, using a
counter alongside the existing resize check rather than a second poll
loop. The file list to sweep needs no new field: every file already
appears on some node's `Node.file` in `self.index`, so the sweep reads
that directly instead of tracking a parallel list.

Coalesce every change seen within one sweep into a single `reload`
call, not one per changed file -- `resolve::resolve` runs over the
whole index either way, so reloading twice for two changes noticed at
once buys nothing.

### Expansion: diff against the old tree, don't drop it

Replace `reload`'s blanket reset of `self.expanded` with a structural
diff against the index it is replacing: a previously-expanded `NodeId`
stays expanded if its identity and `Node.parent` are unchanged in the
new index. Only a node whose own shape actually changed falls back to
today's behavior for that one node. `self.selected`'s own handling is
untouched -- it is already correct.

This keeps `reload`'s existing policy exactly as-is for the cases that
already call it on purpose (`r`, the editor handoff, an eval run): the
reader caused the edit, a fresh frame is the right response. The new,
background-triggered call site is the only one that benefits from
diffing instead of dropping, because it is the only one where the
reader may not be looking at what changed at all.

## What this explicitly does not do

- No per-platform file-watching FFI, per the precedent above.
- No change to `graph::build`, `resolve::resolve`, or `graph::cache`
  themselves -- every piece this plan needs from them already exists.
- No new Cargo dependency.
- No in-TUI text editing. The `[editor]` handoff stays the only way to
  change a file; this plan only changes how fast the TUI notices the
  result.

<!-- dankg:depends target=../architecture.md#decision-17-editor-integration quote="No GUI/TUI toolkit; the user's editor is the buffer, always." -->

## Open questions

- **Opt-out.** Whether a `[tui] watch = false` toggle (mirroring
  `[tui] breadcrumb`) is worth adding for a reader who wants reload
  to stay purely manual, versus shipping the diffed reload
  unconditionally once it is safe enough not to need one. Deferred to
  whoever builds this -- cheap to add later, not worth guessing now
  without a reader's actual complaint to react to.
- **Renamed headings.** A renamed heading gets a new slug-derived
  `NodeId`. Today's `reload` already treats that as a brand-new node,
  orphaning any expansion state that referred to the old one. This
  plan inherits that limitation rather than fixing it -- carrying
  expansion across a rename would need title-similarity matching, a
  different and much fuzzier problem, out of scope here.
- **Sweep cadence.** A hardcoded constant alongside `RESIZE_POLL_MS`
  and `ESC_TIMEOUT_MS`, not new configuration -- revisit only if a real
  corpus size shows the default picked is wrong.

## Phased build order

1. **Cadence-gated sweep, `reload`'s current behavior unchanged.**
   Stat every file named in `self.index` every Nth tick; on any mtime
   change, call `reload` exactly as `r` does today. Proves detection
   and coalescing in isolation, before touching expansion at all.
2. **Structural diff for `self.expanded`.** Swap `reload`'s blanket
   reset for the per-node diff above. Testable without a real
   background writer: build two `Graph` values by hand and assert on
   which previously-expanded ids survive.
3. **Wire the two together**, verified against a real pty: edit a file
   from a second terminal while the TUI sits mid-navigation in a large
   synthetic corpus, confirm the tree updates with no keypress and
   without collapsing an unrelated, already-expanded subtree.

## Critical files

- `src/tui/app.md` -- `reload`, `is_visible_under`/`ancestors_of`
  generalized to cover `self.expanded`, `event_loop`'s tick gains the
  sweep counter.
- `src/tui/term.md` -- `stdin_ready`, unchanged, just called at the
  same cadence it already runs at.
- `src/graph/cache.md` -- unchanged; the reason detection cost is
  already close to free.
- `src/graph/index.md` -- unchanged; decision 6 is why resolution stays
  corpus-wide instead of being scoped to the changed file.

## Verification

- Unit tests for the structural diff: a hand-built old/new `Graph`
  pair covering "unchanged node stays expanded," "node's own shape
  changed, falls back," "node no longer exists, drops."
- A real-pty test, per this repo's own pattern for anything touching
  real terminal timing: spawn `dankg tui` against a scratch corpus,
  edit a file on disk from the test process itself, assert the tree
  reflects the change within a bounded number of ticks with no
  keypress sent.
- `cargo test`, `dankg fmt --check`, `dankg check .` -- the usual
  gates, no exception here.
