# `dankg serve`: an embeddable backend, not a competing editor

## Context

This supersedes two earlier directions — a sandboxed WASM plugin engine
and an embedded Emacs-style Lisp core — both of which were about making
`dankg tui` itself deeply extensible. The problem with that whole line of
work: DanKG competing to become its own extensible editor means competing
with tools (Emacs, Neovim, VS Code) that have already spent decades
solving exactly that problem, and solving it better than a from-scratch
attempt reasonably could. The better fit, once that's named plainly: DanKG
becomes a well-defined, embeddable **backend** that already-extensible
editors build a frontend on top of, the same relationship a language
server has to the editors that consume it. All the "let a user redefine
keybindings, add commands, customize behavior" ambition moves to whichever
editor the user already lives in, where it already has a mature answer;
DanKG's job becomes serving graph and eval capability well, over a small,
stable protocol, and staying out of the UI business.

**The precedent is LSP, and it's worth naming directly.** Language Server
Protocol exists to solve precisely this shape of problem — one backend,
many independently-extensible frontends, so intelligence isn't
reimplemented N times for N editors. `dankg serve` is DanKG's version of
that: a persistent process an editor-side plugin (`dankg.el`, a Neovim
plugin, a VS Code extension) talks to, instead of shelling out to
`dankg graph --format json` and re-parsing the whole corpus on every
interaction, or reimplementing markdown parsing and graph resolution
itself.

This does **not** replace `eval-custom-plan.md`'s small, cheap `key=`-block
command mechanism for the bare `dankg tui` — that's still worthwhile for
someone using the standalone terminal TUI without any editor integration.
This plan is the parallel, and honestly more important, avenue for anyone
who wants deep customization and already lives inside an editor with its
own extension system.

## What this reuses — the same core surface every prior plan's audit found

Nothing here is new graph or eval capability; it's DanKG's existing power
kept warm and made incremental, over a protocol instead of only a CLI:

- `graph::query`/`graph::view` — the pure, already read-only API
  (`links_for`, `entry_nodes`, `select`) every previous plan's audit
  landed on as the natural basis for anything wanting to query the graph.
- `eval::session::run_one` — the single shared execution entry point,
  already trusted and scoped by Decision 9 and `[lang.*]` config; exposing
  it over the protocol changes nothing about its trust model.
- `graph::cache` — content-hash-based invalidation, already built for a
  different reason (skip re-parsing unchanged files), reused here to
  detect and announce when a connected client's view has gone stale.
- The hand-rolled JSON encoder already backing `dankg graph --format json`
  — reused for protocol message bodies, so no serde/JSON crate is needed.
- `graph::index::load` — the whole corpus-loading pipeline, completely
  unchanged; `dankg serve` just keeps the result resident instead of
  discarding it after one CLI invocation.

## Protocol design

**Framing: LSP-style, deliberately, not DanKG's usual hand-rolled binary
framing.** A small `Content-Length: N\r\n\r\n` header followed by a JSON
body, over stdio. This is the one place in this whole exploration where
matching the ecosystem's already-expected shape beats this codebase's
usual instinct to hand-roll something bespoke — the entire point is
external, third-party adoption, and every editor's plugin ecosystem
already knows how to speak something LSP-shaped, which meaningfully lowers
the bar for someone else to write the client side.

**Requests** — read/query operations mirroring capability DanKG's CLI
already has, not new functionality:

- `node`: `NodeId -> Node`
- `links`: `NodeId -> NodeLinks` (outgoing / backlinks / produces / reads)
- `view`: entry files + depth -> induced subgraph, mirroring `dankg graph`'s
  own core operation
- `eval`: a block target -> run via `run_one`, return captured output and
  status, same execution model and trust boundary as today, exposed
  through one more entry point

**Notifications (server → client, unsolicited)**: `changed`, fired when
watched corpus files change on disk, reusing the existing hash/cache
invalidation machinery rather than requiring the client to poll.

**Lifecycle**: an `initialize`/`shutdown`/`exit` handshake in the LSP
shape, with `initialize` negotiating the corpus root the same way DanKG's
other commands already discover it (walking up for `.dankg/`), so
`dankg serve` behaves consistently with `dankg graph`/`dankg tui` pointed
at the same corpus.

## What this explicitly does not need — the contrast with everything explored before it

- **No sandboxing.** No untrusted code ever executes inside `dankg serve`;
  `eval` keeps exactly the trust model it already has.
- **No embedded interpreter or VM of any kind.**
- **No new Cargo dependency.** JSON encoding reuses existing hand-rolled
  code; the framing is a two-line header format, not a library.
- **No command/keybinding registration surface, no ABI to design or
  version against plugin authors** — the editor's own native plugin
  system owns 100% of "what does pressing a key do." DanKG never needs an
  opinion about keybindings again.
- **No live-render/hot-path problem.** An editor's own buffer-redraw loop
  stays entirely on the editor's side, in-process, using its own mature
  extension system. `dankg serve` only ever answers discrete,
  one-request-at-a-time queries reasonably fast — it never needs to be
  fast enough for per-frame rendering, because it's never in that loop.

## Phased build order

**Phase 0 — skeleton.** `dankg serve [path]` spawns, loads the corpus once
via `graph::index::load`, holds it resident, speaks the LSP-style framing
over stdio, implements only `initialize`/`shutdown`. Proves the process
model and framing in isolation before any real query logic.

**Phase 1 — read queries.** `node`, `links`, `view`, wrapping
`graph::query`/`graph::view` directly. Tested with a scripted client
sending framed JSON requests and asserting on responses — a new
`tests/serve.rs`, modeled on `tests/db.rs`'s real-process pattern.

**Phase 2 — `eval` requests.** Wraps `run_one`, unchanged execution model,
now reachable over the protocol as well as the CLI/TUI.

**Phase 3 — change notifications.** Detecting file changes needs a
decision: `std` has no portable file-watching primitive, and this repo has
shown no appetite for new per-platform FFI beyond what's already justified
(`term.md`'s `poll`/termios). Poll on an interval using the same
mtime+content-hash keying `graph::cache` already computes, rather than
hand-rolling `inotify`/`FSEvents`/`ReadDirectoryChangesW`. Push a
`changed` notification to connected clients when it fires.

**Phase 4 — a minimal reference client, to prove real-world usability.** A
small scripted example (not a full editor package) driving the protocol
end-to-end. A real `dankg.el` or Neovim plugin is worth treating as a
stretch goal outside this plan's own deliverable — likely its own
separate repository once the protocol is stable, not something this repo
needs to own.

**Phase 5 — harden and document.** Capability negotiation in `initialize`
so future protocol changes don't break existing clients; an
`architecture.md` note. This needs less justification than even the
DuckDB precedent — `dankg serve` is DanKG's own existing code, unchanged
in what it does, just given a persistent-process entry point alongside the
CLI and TUI it already has.

## Critical files

- `src/cli.md` / `src/main.md` — new `serve` subcommand.
- `src/graph/index.md`, `src/graph/cache.md` — corpus loading and the
  hash-based invalidation reused for both keeping state warm and
  detecting change.
- `src/graph/query.md`, `src/graph/view.md` — the read API the protocol
  wraps, unchanged.
- `src/eval/session.md` — `run_one`, unchanged, now reachable over the
  protocol.
- wherever the `--format json` encoder lives (`src/render/json.md`) —
  reused for protocol message bodies rather than writing a second one.
- New, and deliberately small: a protocol module (framing, request/
  response types) — the only genuinely new code in this plan, kept thin
  since its entire job is wrapping functionality that already exists.

## Verification

- Integration tests spawning the real `dankg serve` binary and driving it
  with a scripted client (modeled on `tests/db.rs`), asserting on framed
  JSON responses for each request type.
- A change-notification test: modify a file in a scratch corpus while
  `dankg serve` is running, assert a `changed` notification arrives.
- `dankg fmt --check` / `dankg check .` after re-tangling touched `.md`
  files — no changes needed to any existing module's behavior, only new
  CLI wiring and a new protocol module.
- Same clean-dependency bar as the eval-based plan: `Cargo.lock` and
  `cargo tree` should show zero change.
- Manual end-to-end check: a small scripted client (even a shell script
  piping framed JSON over stdio) exercising the full request set against a
  real scratch corpus.
