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

**Design decision: guarding the convention against accidental
collisions.** Scanning every `[tui] commands` block's stdout for
`select:`/`status:` would misread ordinary output that happens to start a
line the same way — `git status`-style tools, or a script's own
`print(f"status: {x}")` — as a control line instead of showing it,
silently discarding exactly the raw output this convention exists to
preserve. Two decisions close that gap:

1. **Opt-in per block, not automatic for every `key=` binding.** A new
   `protocol=lines` attribute (`KNOWN_ATTRS`, `src/md/mod.md`) marks a
   block's stdout as this convention's own wire format. Only a block
   carrying it gets scanned; every other `[tui] commands` block keeps
   today's behavior — raw text on the status line — unchanged. This is
   the same "explicit opt-in, not silent scanning" rule `key=` itself
   already follows, applied one level deeper.
2. **`select:` also needs semantic validation, not just a recognized
   prefix.** A `select: target` line only moves the tree cursor when
   `target` resolves to a real node already in the graph —
   `depends::resolve_target`, reused rather than reimplemented, but
   called with an empty declaring file so `target` resolves
   root-relative, exactly like a bare `dankg graph --format json` node
   id. A running command has no "current file" the way a
   `dankg:depends` marker embedded in one specific corpus file does, so
   there is nothing a bare `#slug` could resolve relative *to* — a
   target naming no file at all just fails to resolve, same as any
   other unresolved one. An unresolved target is silently skipped, not
   shown as raw text: the block already opted into the protocol, so a
   stale or misspelled target is the protocol misfiring, not a case to
   fall back to raw display for. `status:` gets no equivalent check —
   almost any string is a "valid" status message — so its only defense
   is decision 1's opt-in gate.

**Collision policy.** One unified "every character already spoken for"
list — current `Keymap` field values plus the hardcoded `Char` literals in
`event_loop` (`'?'`, `'/'`, `'f'`, `'n'`, `'N'`) plus every other already-
registered `key=` block — checked per registration. A colliding `key=`
block is refused with a named diagnostic; every other binding stays
intact. (`Keymap`'s own precedent — any collision reverts the *whole* map
— doesn't transfer here, since there's no single "whole map" moment the
way `[keys]` parsing has one.)

**Node classification/display, same primitive.** Settled on dedicated
`tag: target kind=... icon=...` lines, scanned under the exact same
`protocol=lines` gate as `select:`/`status:` above — not a separate
`classify=` attribute, since a `classify=` block would need its own
opt-in, its own collision story, and its own trigger, all of which
`protocol=lines` already has by the time this needs it. `target`
resolves exactly like `select:`'s own target, and an unresolved one is
skipped the same way.

**Design decision, revised after the first cut shipped: durable, not
session-only.** The version that actually shipped first kept a
`tag:` line's own effect only in memory, on `App`, gone the moment the
TUI quit. That contradicts a principle DanKG already commits to
elsewhere — decision 12: a result is "written back into the
markdown, hash-tagged... file stays the source of truth." A reader
who tags a node, quits, and reopens `dankg tui` a day later should not
find the tag gone; nothing else DanKG computes behaves that way.

The fix: `tag:`'s own target gets a real, durable
`<!-- dankg:tag kind=... -->` marker written into the *target's own
file* (`tag.md`, new) — `eval::result`'s "write it into the file,
don't just remember it" policy, applied to this convention instead of
a block's own captured output. The marker sits immediately *before*
the node it classifies, not after (the direction `eval::result` uses
for its own marker): a classification describes a node the way a
caption describes a photo rather than following it, and "before" also
can never collide with `eval::result`'s own claim on the line right
after a block's closing fence.

Scope: a marker can attach to a heading or a block (`Block::line()`
already reports either's own anchor line uniformly), but not a
`Relation` node, which has no real file or line to attach one to
(decision: *Provenance without a driver*).

Reading a marker back is a corpus-wide pass, `tui::app::compute_tags`,
computed once per load/reload exactly alongside `deps`
(dependency-surfacing.md) rather than tracked as separate app state —
**not** a change to `graph::model::NodeKind`, which stays exactly as
it is (`Heading`/`Block`/`Relation`) since it's load-bearing structure
`graph::view`'s zero-cost-relation-edge rule and `BLOCK_LEVEL` depend
on. This is purely additive metadata, read into the same
`Annotation`/`annotations` shape the first cut already had; only how
it gets populated changed. `push_row` still gains the one new read
point that consults it when building a row's own badge — the same
shape of hook `dependency-surfacing.md`'s existing badge work already
added, just keyed by user data instead of built-in facts. `Filter`
(previously a fixed `All`/`Blocks`/`EvalChain`/`FileArtifact` enum)
still gains a `Tag(String)` variant, so a kind some marker has set
becomes a selectable entry in the existing filter-menu overlay,
alongside the built-ins — the menu's own option list is no longer a
fixed constant, since it now depends on what the corpus itself
actually has written into it, not merely what has run this session.

**Design decision, revised once more: the icon moves to config, and
`kind=` becomes a checked vocabulary.** The marker above, as it first
shipped, still carried its own `icon=` alongside `kind=`. Two problems
with that: a reader tagging fifty nodes `kind=task` was repeating the
same icon fifty times, with no single place to change it later; and
`kind=` itself was an unchecked free string, so a typo (`kind=tsak`)
just silently failed to classify anything, with nothing to catch it.

Both are fixed by moving the icon out of the marker and into a new
`[kind.<name>]` config family (`config.rs`, the exact
`langs()`/`dbs()` template), keyed by the same name a `tag:` line's
own `kind=` already names:

```
[kind.task]
icon = ☐
```

The marker itself shrinks to `<!-- dankg:tag kind=value -->` --
nothing else survives to write, so there is no longer anything to
merge when a node is retagged; a fresh `kind=` always replaces
whatever the marker said before. `App::compute_tags` resolves a
node's own icon by looking `kind` up in `[kind.*]` at the same
load/reload pass that reads the marker back, so a config edit takes
effect on the next reload exactly like every other config-driven
behavior already does. A `[kind.*]` section with no `icon` at all is
still a real, declared name -- there is just nothing to draw for it.

Centralizing the vocabulary is what makes it checkable at all: `dankg check` gained a sixth pass, over every `dankg:tag` marker in the
corpus, failing the exit code when a `kind=` names no `[kind.*]`
section -- the same "two declared things disagree" severity a
`produces=`/`reads=file:PATH` mismatch (decision 33) already gets,
not the weaker advisory treatment a `dankg:depends` substring miss
gets. A typo in `kind=` is now caught the same build a typo in
`produces=` already is.

**One more check, added because centralizing icons made it possible
for the first time**: a seventh, advisory-only pass over every
configured `[kind.*]`'s own `icon`, `tag::icon_may_break_alignment`,
flags a glyph likely to render wider than one terminal column or in
color. This codebase has already hit the exact bug once by hand --
`⌛`, first chosen as the needs-run badge glyph, rejected once it
turned out to default to emoji presentation, in favor of `↻` (§7's own
decisions table). With every icon now declared in one place instead
of scattered across however many `tag:` lines happened to set one,
`dankg check` finally has one fixed place to catch the same mistake
before it ships, rather than leaving every future kind's author to
rediscover it by hand. Deliberately narrow rather than a full Unicode
`Emoji_Presentation` table (`tag.md`'s own module doc has the detail):
a block-range guess is provably wrong even for a glyph already shipped
here (`✗`, in the same Dingbats block as several genuinely
emoji-default characters, is itself text-presentation).

**Design decision: `target=`, once positional placement turned out to
have a real drift problem.** The marker sits immediately *before* the
node it classifies (this file's own placement design above), and
that tie was purely positional -- whichever heading or block happened
to sit right after the marker. Insert a new heading between an
existing marker and the node it was meant for -- an entirely ordinary
edit, nothing that looks wrong -- and the marker silently reattaches
to the wrong node. Nothing noticed, because nothing had ever recorded
what the marker actually meant.

Considered and rejected first: tightening the marker's *own* adjacency
instead, prompted by the question "should this sit with no blank line
at all, directly before or after the node?" Tested directly rather
than assumed: a marker with no blank line *before* the node it
describes makes the node disappear from the graph entirely --
`starts_html_block`'s own `gather_passthrough` swallows every
following non-blank line as raw comment text, heading syntax included,
until it hits a blank line. Putting the marker *on the heading's own
line* instead (`## Todo <!-- dankg:tag kind=task -->`) is worse, not
better: the comment becomes part of the heading's own inline content,
corrupting both its displayed title and its slug (confirmed by
running it: `## Todo <!-- dankg:tag kind=task -->` slugifies to
`todo----dankgtag-kindtask---`), and the equivalent for a block
corrupts its fence's own info-string parsing into four separate
warnings. Neither tightens anything usefully; both trade a real
problem for a worse one.

The fix that actually holds: give the marker an explicit `target=`,
`depends::resolve_target`'s own fragment shape (`#slug`,
`dankg:depends`'s own sibling convention), naming the node it is
*for* rather than leaving that implicit. `write_back` always supplies
one -- the one caller, `App::write_tag`, already has the resolved
`NodeId` in hand, nothing to omit it for -- but parsing tolerates its
absence, so a hand-written marker without one still works exactly as
before, purely positional, unverified. `dankg check`'s sixth pass
gained the actual verification: resolve the marker's own `target=`
and confirm it points back at the exact node the marker physically
sits on; a mismatch fails the build, the same severity as an unknown
`kind=`. Confirmed by hand: tag a node, then hand-edit the file to
insert a new heading between the marker and its target -- `dankg check` reports `target=#two -- no longer points back at this marker's own node` and fails, exactly the drift the purely-positional version
would have missed.

**A correctness detail, found while implementing.** A single run
emitting two `tag:` lines for the *same* target needs the second
write to see what the first one just wrote, including the target
node's own anchor line shifting by the two lines the first write
inserted. Resolving both against the graph as it stood before either
ran would stamp the second marker at a now-stale line. The fix:
`App::apply_protocol_output` reloads immediately after *every*
successful write, not once at the end — the same "just rebuild it,
cheap at this corpus's size" tolerance the rest of this codebase
already has for `reload` itself.

Trigger cadence for classification under mechanism 1 alone: on-demand
only, bound to a command's own `key=` — the same cadence `select:`/
`status:` already run under. Automatically re-classifying on every
`reload()` stays a real option for later, not attempted in this pass:
it would need its own config (naming which command is "the
classifier"), and on-demand already covers "let me redefine my own
node types" without it. Live, per-navigation reclassification is not
this mechanism's job at all — that's what mechanism 2 is for.

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

**Status**: mechanism 1 is done in full -- phase 0, all of phase 1, phase
2, and phase 3's collision handling (pulled forward into phase 0's own
commit), plus one addition not originally scoped as its own phase — see
below for each. Only mechanism 2 (phases 4-6, the persistent renderer
process) is still ahead. `architecture.md` (Decision 40, and the new
*`[tui] commands`* subsection under *Eval in the TUI*) and `README.md`
document what's actually shipped; this file stays the plan for what
comes next.

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

**The rest of phase 1 — done.** `select:`/`status:` are real now, both
gated by the `protocol=lines` attribute this file's own design decision
added (`InfoString::protocol_lines`, `src/md/mod.md`;
`BlockRef::protocol_lines`, `src/eval/plan.md`). `tui::eval:: scan_protocol_lines` recognizes both prefixes (plus `tag:`, phase 2's
own convention, scanned in the same pass since all three are one wire
format); `App::run_command` only calls it for a `protocol_lines`
command, otherwise keeping today's raw-text behavior byte for byte.
`App::apply_protocol_output` resolves `select:`'s target root-relative
(`depends::resolve_target` with an empty declaring file, per the design
decision above) and moves the tree cursor only when it resolves to a
real node; `status:`, when present, replaces `outcome_status`'s own
fallback. **Ordering detail, found while implementing**: `reload()`
now runs *before* the outcome is interpreted, not after -- `select:`
only makes sense against the graph the command's own edits actually
produced, and `reload()` used to run last.

**Phase 2 — done, then revised once more after shipping.** Node
classification, via the same `tag:` line `scan_protocol_lines` already
recognizes. The first cut kept a `tag:` line's own effect only on
`App`'s own `annotations: HashMap<NodeId, Annotation>` field, gone the
moment the TUI quit -- session-only, contradicting decision 12's own
"file stays the source of truth." Revised to write a durable
`<!-- dankg:tag -->` marker into the target's own file instead (new
module, `src/tag.md`: `render_marker`/`parse_marker`,
`locate_existing`/`markers_in` for reading one back, `write_back` for
writing one, all mirroring `eval::result`'s own shape but placing the
marker *before* the node rather than after). `App::write_tag` resolves
a `tag:` line's target and calls into it; `App::compute_tags` is the
read side, run once per load/reload alongside `deps` and folded into
the very same `annotations` map the first cut had -- everything
downstream of that map (`badge_for`'s new `annotations` parameter,
appending a node's own `icon=` to its badge; `Filter`'s new
`Tag(String)` variant, losing `Copy` in the process; `App::filter_options`
replacing the fixed `Filter::ALL` constant the menu used to cycle
through) did not need to change at all -- only how the map gets
populated did. `panel_rows` still did not turn out to need its own read
point. Auto-classification on `reload()`, the cadence this phase's own
original sketch left open, still was not built -- on-demand (bound to a
command's own `key=`) is what shipped, matching `select:`/`status:`'s
own cadence exactly.

**Phase 2, revised a second time: the icon moves to `[kind.*]`
config, and `kind=` becomes checked.** See this file's own design
decision above (*the icon moves to config, and `kind=` becomes a
checked vocabulary*) for the reasoning. Shipped: `config::Kind`/
`Config::kinds`/`Config::kind` (`src/config.md`, a new `"kind."`
`FAMILIES` entry); `tag::render_marker`/`parse_marker`/
`locate_existing`/`markers_in`/`write_back` all shrank to carry just
`kind`, no `icon`; `App::compute_tags` now takes `&Config` and
resolves each marker's icon through it; `App::write_tag` no longer
merges anything, since a fresh `kind=` is the only thing left to
write. `main::check_cmd` gained the sixth pass (unknown `kind=`, hard
failure) and the seventh (`tag::icon_may_break_alignment`, advisory),
plus `tag.md`'s own new heuristic function and its dedicated
integration coverage, `tests/tags.rs`, over two small fixture
corpora.

**Phase 2, revised a third time: `target=`, once positional placement's
own drift problem turned out to be the sharper issue.** See this
file's own design decision above for the reasoning, including the two
placement-tightening options tried and rejected first (no blank line
before the node; the marker inline on the node's own line), both
confirmed broken by actually running them, not just reasoned about.
Shipped: `tag::render_marker`/`parse_marker`/`locate_existing`/
`markers_in`/`write_back` all gained `target`, optional on the parse
side (a hand-written marker without one still works, purely
positional); `App::write_tag` always supplies one, from the already-
resolved `NodeId`'s own slug, never copying the `tag:` line's own
spelling of the target verbatim. `main::check_cmd`'s sixth pass
gained the second half of its own job: a `target=` that no longer
resolves back to the node the marker physically sits on fails the
build, the same severity as an unknown `kind=`. `tests/tags.rs` grew
a third fixture case (`tags-bad-corpus/mismatch.md`) exercising
exactly that drift.

**Not part of this plan's own scope, but built directly on top of
it: `keys.tag` (default `t`).** Everything this plan's mechanism 1
covers is reachable only through a written, bound `protocol=lines`
command. `keys.tag` is a built-in keybinding instead -- the same
category as `keys.eval`/`keys.reset`, not another `[tui] commands`
binding -- that opens a picker over `Config::kinds()` and calls the
exact same `App::write_tag_for`/`App::create_kind` a `tag:` output
line already goes through. Documented in `architecture.md`'s own
*Terminal UI* section (*`keys.tag`: classifying a node without
writing a command*), not here, since it isn't an extension point
this plan added -- it's a second door onto the same mechanism.

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

- `src/md/mod.md` — `InfoString`/`KNOWN_ATTRS`: `key=` and `protocol=`,
  both shipped. No separate classification attribute was added --
  `tag:` rides the same `protocol=lines` gate as `select:`/`status:`.
- `src/eval/plan.md` — `top_level_blocks`, the discovery pattern keyed-
  block scanning mirrors; `BlockRef::protocol_lines`, shipped.
- `src/eval/run.md` — the spawn/timeout/kill-tree machinery mechanism 1
  reuses unchanged; mechanism 2 still needs the reader-thread idiom
  adapted from one-shot to long-lived.
- `src/eval/session.md` / `src/tui/eval.md` — `run_one`, the single shared
  execution entry point every command run goes through, and
  `scan_protocol_lines`, shipped.
- `src/tui/app.md` — `event_loop`'s match arms, `App::load`,
  `run_command`/`apply_protocol_output`/`write_tag`, the `annotations`
  field and `compute_tags`, and `badge_for`'s new read point, all
  shipped. Mechanism 2 still needs its own `renderer` field.
- `src/tag.md` — new module: the `<!-- dankg:tag -->` marker's own
  render/parse (`kind` and an optional `target`, no icon),
  `locate_existing`/`markers_in`/`write_back` for reading one back and
  writing one (mirroring `eval::result`'s shape), and
  `icon_may_break_alignment`, the advisory width/color heuristic
  `main::check_cmd`'s seventh pass uses. Shipped.
- `src/config.md` — `[tui] commands = ...`, shipped, following the exact
  `langs()`/`dbs()`/`editor()` template. `Kind`/`Config::kinds`/
  `Config::kind` and a `"kind."` `FAMILIES` entry, shipped on the same
  template. Mechanism 2 still needs a new `[renderer.*]` family on it
  too.
- `src/main.md` — `check_cmd`'s sixth pass, shipped in two parts: an
  unknown `kind=` and a `target=` that no longer resolves back to its
  own marker's node, both hard failures. Seventh pass (wide/colored
  icon, advisory), shipped.
- `src/graph/model.md` — confirmed untouched; classification is a
  separate structure on `App`, read from its own marker, not a
  `NodeKind` variant.

## Verification

- Unit tests for `key=`/`protocol=` attribute parsing, alongside the
  existing `InfoString` attribute tests -- done.
- Corpus-based integration tests (`App::load` against a real `write_corpus`
  fixture, `src/tui/app.md`'s own established pattern rather than a
  separate file modeled on `tests/db.rs`) for keyed-block discovery and
  execution, including `protocol=lines`'s `select:`/`status:`/`tag:`
  handling and the collision it guards against -- done.
- Real pty-driven verification (per `CLAUDE.md`'s documented pattern),
  by hand rather than checked into the suite: a `protocol=lines` command
  bound to `key=g`, run in a live `dankg tui` session, moved the tree
  cursor (`select:`), set the status line (`status:`), added a badge
  glyph to the target node (`tag:`), and surfaced a `tag:task` entry in
  the filter menu -- confirmed working end to end, including the
  revision: a second, brand-new `dankg tui` process, launched against
  the same corpus with the command never run again, showed the same
  badge immediately, read straight out of the file; and the second
  revision after that: with `[kind.task] icon = X` configured and a
  `tag:` line carrying only `kind=task`, the badge still showed `X`,
  and `a.md` on disk carried only `<!-- dankg:tag kind=task target=#two -->`,
  no `icon=` anywhere. Also confirmed live: `dankg check` on a marker
  whose `kind=` was hand-edited to an undeclared name printed the
  `bad-tag` line and exited non-zero. And confirmed the drift
  `target=` exists to catch, live: tagged a node, then hand-edited the
  file to insert a new heading between the marker and its target --
  `dankg check` reported `target=#two -- no longer points back at this marker's own node` and exited non-zero, exactly the silent
  misattribution a purely positional marker would have missed
  entirely. Mechanism 2 still needs its own pty coverage once it
  exists: a configured `[renderer.*]` process updating node styling as
  selection changes, and a renderer-process crash degrading gracefully
  mid-session.
- `tests/tags.rs`, a new dedicated integration-test file over two
  fixture corpora (`tests/data/tags-corpus/`, everything valid, one
  marker with `target=`, one without;
  `tests/data/tags-bad-corpus/`, an undeclared `kind=` in one file and
  a `target=` mismatch in another) -- the same "give each concern its
  own root" precedent `tests/data/filedeps-corpus/` already set.
  Covers what `tag.rs`'s own unit tests cannot: that `dankg check`, run
  as the real binary over a real `[kind.*]` config, actually fails the
  exit code on an unknown `kind=` or a `target=` mismatch, and never
  fails it on a wide icon.
- `dankg fmt --check` / `dankg check .` after re-tangling every touched
  `.md` file via the standard edit loop -- clean.
- The one clean advantage of this design worth explicitly checking:
  `Cargo.lock` and `cargo tree` show zero change through every phase of
  this plan — nothing here should ever add a dependency. Confirmed
  through mechanism 1: this pass touched no `Cargo.toml`.
