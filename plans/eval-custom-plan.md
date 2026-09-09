# Dependency-free TUI extensibility: literate eval commands + a persistent live-render process

## Context

This is the dependency-free alternative to a sandboxed WASM plugin engine.
Where a WASM approach gets real sandboxing and true polyglot authoring at
the cost of a new dependency (`wasmi`), a sibling crate, a wire protocol,
and a new `architecture.md` decision qualifying Decision 1, this design
gets new keybindings, new commands, and live-feeling display customization
**without touching Decision 1 at all** — zero new crates, zero workspace changes, `Cargo.lock`
untouched. The cost is the same trust model DanKG's `eval`/`[lang.*]`
already has: full trust, no sandbox (Decision 9 — "the risk was accepted
the moment it was configured"). A survey of comparable single-user,
local-first tools (Vim, Emacs, Obsidian) found this is the industry norm
for this category of software, not a step down — genuine sandboxing shows
up almost exclusively in products with a different threat model (browser
extensions, multi-tenant infra).

This design was arrived at by two refinements on top of "just extend
`eval`":

1. Don't invent a bespoke `[command.*]` config format — let commands *be*
   literate documents, discovered and run through DanKG's own existing
   `eval` machinery, the same way everything else in this codebase
   dogfoods itself.
2. `eval::run`'s one-shot spawn model is structurally too slow for styling
   that needs to track live interactive state (selection, filter) — a
   fresh process per keystroke isn't viable. That specific gap needs a
   **persistent** spawned process, event-triggered rather than per-frame,
   with the render loop reading from a cache it never blocks on.

Two mechanisms follow from that: literate eval-native commands (one-shot,
covers keybindings/commands/on-demand classification) and a persistent
renderer process (covers the live-styling case specifically). Most users
would only need the first; the second is an opt-in upgrade for the subset
of customization that has to feel live.

## Mechanism 1: literate eval-native commands

**A command is just an eval block with a `key=` attribute — not a new
config concept.** Add `key` to `InfoString`'s `KNOWN_ATTRS` in
`src/md/mod.md` (today: `db`, `name`, `deps`, `xdeps`, `produces`, `reads`,
`timeout`, `path`):

````
```python name=reindex key=g
...
```
````

Discovery mirrors `eval::plan::top_level_blocks` exactly — no new scanning
logic, just a new attribute recognized on blocks already being walked.
Execution is `eval::session::run_one`, unchanged — same spawn, same
`[lang.*]`-configured interpreter, same timeout/process-group kill, same
64KB output cap. No new execution engine anywhere in this mechanism.

**Explicit opt-in, not silent corpus-wide scanning.** DanKG only treats
`key=` blocks as live bindings within a location the user has named —
e.g. `.dankg/config`'s `[tui] commands = commands.md` (a specific file or
directory). Scanning the whole corpus for any block anywhere carrying
`key=` would be a much larger, quieter trust surface than every other
opt-in mechanism in this codebase already uses.

**Output convention, layered on the existing capture step.** A run's
stdout is scanned for a small set of recognized line prefixes —
`select: file.md#node-id`, `status: ...` — applied on top of the output
`eval::run` already captures. Unrecognized output falls back to "dump raw
text to `app.status`," matching the warn-and-drop-gracefully philosophy
the config and frontmatter parsers already use elsewhere.

**Collision policy.** One unified "every character already spoken for"
list — current `Keymap` field values plus the hardcoded `Char` literals in
`event_loop` (`'?'`, `'/'`, `'f'`, `'n'`, `'N'`) plus every other already-
registered `key=` block — checked per registration. A colliding `key=`
block is refused with a named diagnostic; every other binding stays
intact. (`Keymap`'s own precedent — any collision reverts the *whole* map
— doesn't transfer here, since there's no single "whole map" moment the
way `[keys]` parsing has one.)

**Node classification/display, same primitive.** A block convention
(e.g. a `classify=` attribute, or dedicated blocks emitting
`tag: <nodeid> kind=task icon=☐` lines) populates a new annotation table
living alongside `App`'s existing `tags`/`deps` data — **not** a change to
`graph::model::NodeKind`, which stays exactly as it is (`Heading`/`Block`/
`Relation`) since it's load-bearing structure `graph::view`'s zero-cost-
relation-edge rule and `BLOCK_LEVEL` depend on. This is purely additive
metadata. `visible_rows`/`panel_rows` gain new read points that consult the
table when building row badges/labels — the same shape of hook
`dependency-surfacing.md`'s existing kind-marker/badge work already added,
just keyed by user data instead of built-in facts. `Filter` (today a fixed
`All`/`Blocks`/`EvalChain`/`FileArtifact` enum) gains a dynamic dimension
so a user tag becomes a selectable entry in the existing filter-menu
overlay, alongside the built-ins.

Trigger cadence for classification under mechanism 1 alone: on-demand
(bound to its own `key=`) or automatically on `reload()` — batch, not
per-frame. This is enough for "let me redefine my own node types," but not
for styling that needs to react live as the user navigates — that's what
mechanism 2 is for.

## Mechanism 2: a persistent process for live-state-dependent rendering

**Why mechanism 1 can't do this.** A one-shot spawn per navigation event
is too slow to feel live — interpreter/process startup cost (single digits
to tens of milliseconds depending on language) times every keystroke
produces a perceptible flash of stale styling even with memoization and
debouncing. Rendering itself can never block on a fresh spawn.

**The fix: a persistent, long-lived subprocess, spawned once, event-
triggered rather than per-frame.** At TUI startup, if configured, DanKG
spawns the plugin author's own native program — not compiled-in Rust, not
a VM, no `wasmi`, no sibling crate — and keeps it alive for the whole
session, exactly like an LSP server. Communication is a hand-rolled,
length-prefixed binary framing protocol over its stdin/stdout pipes — no
serde, matching this codebase's consistent instinct to hand-roll parsers
(`cmd.rs`'s argv splitter, `md/mod.rs`'s parser).

**Config**: a new `[renderer.<name>] command = ...` family, matching the
exact shape of `[lang.*]`/`[db.*]`/`[editor]`'s existing `command` template
pattern — deliberately **not** a literate block, since this names a
standalone long-running program the user brings, not source text DanKG
evaluates. (Mechanism 1's blocks are code DanKG runs; mechanism 2's
renderer is an external program DanKG talks to — different shape of thing,
so a different, but equally established, config mechanism.)

**The load-bearing architectural rule: the render loop never calls the
process directly.** It only ever reads the same annotation table mechanism
1 already populates. The persistent process is asked to refresh that table
only on a defined, bounded set of "interesting" transitions — selection
change, filter change, expand/collapse, confirmed search — never on every
keystroke while typing, and coalesced through a short debounce window
(~100-150ms) so rapid navigation (holding an arrow key) doesn't fire a
flood of requests. When a response arrives, the cache updates and a redraw
is triggered; until then, `render()` uses whatever's already cached. This
is exactly the same "block the event loop briefly, that's fine" tolerance
this codebase already has for `eval` (which already accepts blocking up to
30 seconds on a keypress) — just applied far more gently, and to a
background refresh rather than a foreground wait.

**I/O plumbing**: reuse `eval/run.rs`'s reader-thread + `mpsc::channel`
idiom, adapted from one-shot (spawned and read once) to long-lived
(spawned once, read continuously across the whole session) — the same
adaptation that was scoped for the WASM plan's pipe I/O, just talking
directly to the plugin's own binary instead of a `wasmi`-hosting
intermediary.

**Trust model**: identical to `eval`/`[lang.*]` today — full trust, no
sandbox, Decision 9 applies exactly as it already does. This mechanism
adds no new dependency, no new binary DanKG itself builds, no workspace
member — the "plugin" is entirely the user's own externally-built program.

**Lifecycle and crash isolation**: `App` gains a new field (e.g.
`renderer: Option<RendererHost>`), constructed in `App::load` alongside
`config`/`keys`/`plugins`-equivalent state, torn down on the quit path.
Spawn failure degrades to `None` plus a `diags.warn`, never a hard load
failure — same "opt-in, zero risk if unconfigured" posture as everywhere
else. If the process dies or the pipe hits EOF mid-session, the extension
disables itself for the rest of the session, `app.status` reports it, and
rendering falls back to whatever the annotation table last held — never a
TUI crash.

## Where the two mechanisms meet

The annotation table is the single interface the renderer reads from.
Mechanism 1 populates it via one-shot batch runs (on-demand or on reload).
Mechanism 2, when additionally configured, populates the *same* table via
a persistent process reacting to interactive transitions. A user who
doesn't need live-feeling updates uses mechanism 1 alone — no persistent
process, simplest possible setup, literally just markdown with a `key=`
attribute. A user who wants live styling adds a `[renderer.*]` section on
top. Nothing about mechanism 1 needs to anticipate mechanism 2 architecturally
beyond "the annotation table is the shared surface both write to."

## Phased build order

**Status**: phase 0 is done, plus collision handling pulled forward from
phase 3, the smaller/more urgent half of phase 1 (showing a command's
actual captured output, not just "ok"/"failed"), and one addition not
originally scoped as its own phase — see below for each. The rest of
phase 1, and phases 2, 4, 5, 6, are still ahead. `architecture.md`
(Decision 40, and the new *`[tui] commands`* subsection under *Eval in
the TUI*) and `README.md` document what's actually shipped; this file
stays the plan for what comes next.

**Phase 0 — done.** `key=` support: added to `KNOWN_ATTRS` in
`src/md/mod.md`, `BlockRef.key` carries it through
(`src/eval/plan.md`), `[tui] commands = <path>` in `src/config.md`
opts a file in, `tui::eval::keyed_commands` scans it, one new
`event_loop` match arm (`src/tui/app.md`) resolves a pressed key to a
specific block and runs it via `eval::run` directly — bypassing the
cycle-then-Enter ritual entirely, exactly as scoped. **Went further
than originally scoped in one way, worth recording**: `key=` isn't
limited to a bare character. `input::Key` gained a text parser and a
matching `Display` impl (`src/tui/input.md`) recognizing `ctrl+<char>`
and the named keys (`enter`/`tab`/`backspace`/`esc`/arrows) too, not
just `Char`, since a real keybinding vocabulary needs more than letters
to be useful.

**The smaller, more urgent half of phase 1 — done.** A command's
outcome used to be just `"name: ok"`/`"name: failed"`, discarding
`session::run_one`'s already-captured `stdout`/`stderr` entirely.
`tui::eval::Outcome::Ok`/`Failed` now carry that text (`Failed` prefers
`stderr`, falling back to `stdout` when `stderr` is empty --
`src/tui/eval.md`); `App`'s new `outcome_status` helper
(`src/tui/app.md`) shows its first non-blank line on the status line,
falling back to `"ok"`/`"failed"` when there's nothing to show.
**Differs from the original sketch in one way**: no explicit
`draw::clip_with_ellipsis` call was needed -- `render`'s existing
`app.status.chars().take(term_cols)` already width-clips whatever the
status line holds, from any source, so the only new work was picking
*which* line to hand it (the first non-blank one; embedded newlines,
not width, were the actual problem to solve).

**The rest of phase 1 — not started.** The `select:`/`status:`
output-line convention is still the plan for *richer* output (moving
selection, a deliberate custom message, distinct from a command's raw
captured text) -- the natural next step now that raw output is visible
at all.

**Phase 2** — node classification: the annotation table, the
`tag:`-line convention, new read points in `visible_rows`/`panel_rows`,
`Filter` extended with a dynamic tag dimension surfaced in the filter-menu
overlay.

**Phase 3's collision handling — done, pulled forward into phase 0's own
commit rather than deferred.** `App::reserved_keys`/`load_commands`
(`src/tui/app.md`) check every parsed `key=` binding against `Keymap`'s
eight fields, the hardcoded literals (`?`/`/`/`f`/`n`/`N`), and the
structural keys (`Enter`/`Tab`/`Esc`/arrows) that are always fixed
regardless of `Keymap` — the last group matters more than it might look:
without it, a `key=enter` binding would parse cleanly and simply never
fire, since `Enter`'s own hardcoded `event_loop` arm always claims it
first, which is a worse failure than a refusal. Per-binding refusal, not
a whole-table revert, exactly as scoped: one bad binding is warned about
and skipped, every other command in the file still loads.

**Not originally its own phase, added because it was a real gap**: the
`?` help overlay now lists every loaded `[tui] commands` binding by name
and key, sorted alphabetically, alongside the built-ins
(`app::help_lines`, `src/tui/app.md`). Discoverability was implicit in
the plan's intent but never called out as a deliverable; it's cheap
enough, and directly load-bearing for the collision story above, that it
shipped alongside phase 0 rather than waiting.

**Phase 4** — renderer process scaffolding: `[renderer.<name>] command`
config parsing (mirroring `Config::langs()`/`dbs()`/`editor()`), the new
`App` field, spawn-on-load with graceful degrade, teardown on quit — no
live updates triggered yet, this phase only proves the process survives a
real session without leaking or hanging.

**Phase 5** — the live loop: the framing protocol, the long-lived
reader thread/channel, the defined "interesting transition" event set,
the debounce window, wiring transitions to refresh requests and refresh
responses back into the annotation table plus a triggered redraw. This is
the first phase a human watching the TUI sees styling update live as they
navigate.

**Phase 6** — harden and document: crash/EOF detection and session-wide
disable, `architecture.md` note. This note is much smaller than the WASM
plan's decision entry — nothing here amends or qualifies Decision 1, so it
can be a short addition to the existing "extension points are spawned
external programs" prose rather than a new numbered decision.

## Critical files

- `src/md/mod.md` — `InfoString`/`KNOWN_ATTRS`, where `key=` (and the
  classification attribute) get added.
- `src/eval/plan.md` — `top_level_blocks`, the discovery pattern keyed-
  block scanning mirrors.
- `src/eval/run.md` — the spawn/timeout/kill-tree machinery mechanism 1
  reuses unchanged, and the reader-thread idiom mechanism 2 adapts from
  one-shot to long-lived.
- `src/eval/session.md` / `src/tui/eval.md` — `run_one`, the single shared
  execution entry point every command run goes through.
- `src/tui/app.md` — `event_loop`'s match arms, `App::load`, the new
  `renderer` field, and the `visible_rows`/`panel_rows` read points for
  the annotation table.
- `src/config.md` — new `[renderer.*]` family and `[tui] commands = ...`
  scope setting, following the exact `langs()`/`dbs()`/`editor()` template.
- `src/graph/model.md` — confirm `NodeKind` stays untouched; the
  annotation table is intentionally a separate structure, not a variant
  addition here.

## Verification

- Unit tests for `key=`/classification attribute parsing, alongside the
  existing `InfoString` attribute tests.
- Corpus-based integration tests (modeled on `tests/db.rs`'s real-process
  pattern) for keyed-block discovery and execution against a scratch
  corpus.
- Real pty-driven tests (per `CLAUDE.md`'s documented pattern) covering: a
  keybinding triggering a specific block and updating status/selection; a
  configured `[renderer.*]` process updating node styling as selection
  changes; a renderer-process crash degrading gracefully mid-session.
- `dankg fmt --check` / `dankg check .` after re-tangling every touched
  `.md` file via the standard edit loop.
- The one clean advantage of this design worth explicitly checking:
  `Cargo.lock` and `cargo tree` show zero change through every phase of
  this plan — nothing here should ever add a dependency.
