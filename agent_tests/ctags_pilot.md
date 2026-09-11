---
title: Agent navigation pilot, against ctags
author: Daniel J. Okuniewicz
---
[pilot.md](pilot.md) gave its raw arm nothing but `grep` and `find`.
`ctags` is free, older than any agent, and exactly the tool a real
Rust developer without `rust-analyzer` reaches for first. This pilot
gives the raw arm `ctags` instead of nothing, on the identical
fixture and the identical three tasks. It asks whether a real
definition index closes the gap `pilot.md` found on task B.

# Setup

The same `literate` branch, the same commit, the same two
directories `pilot.md` already built.

<!-- dankg:depends target=pilot.md#setup quote="Two directories were built from the same commit" -->

A third copy is added:

- **raw+ctags**: the raw `src/**/*.rs` tree, plus a `tags` file
  generated once, up front, by `ctags -R --languages=Rust`. The agent
  can query it with `readtags -t tags <name>`, which returns a
  definition's file and line. `ctags` gives definitions only. It has
  no find-references command at all.

Two verified facts about this specific corpus fix the ground truth
for task B before any trial runs. `src/render/assets.md`'s prose
links to `html.md`. `src/render/html.md`'s prose links back to
`assets.md`. Only `html.rs`'s code actually calls into `assets`
(`use super::assets`, then `assets::CSS`/`assets::JS`). `assets.rs`'s
code never references `html` at all. `src/render/dot.md` and
`src/render/mermaid.md` link to each other in prose the same way.
Neither has a code reference to the other.

The same three tasks as `pilot.md`, unchanged.

<!-- dankg:depends target=pilot.md#tasks quote="Find every pair of files under `src/` whose documentation references each other both ways." -->

# Grading

Identical to `pilot.md`: correct or incorrect per task, plus tool
calls.

# Hypothesis

Task A asks for an architectural rationale recorded in
`architecture.md`. That file is prose, not Rust. `ctags` indexes
Rust symbols. It has nothing to offer here, the same way having
`dankg` available bought the literate arm nothing on this task.

<!-- dankg:depends target=pilot.md#findings quote="having the `.md` files and the `dankg` binary available bought nothing" -->

Task C already ties at 2 tool calls apiece. A direct definition
lookup for `initial_order` should match that floor, not beat it.
There is no room left to improve on a tie.

Task B is where `ctags` should fail exactly as badly as plain `grep`
did, for a different reason than speed. `ctags` has no
find-references command. It cannot even ask "what points at
`html.rs`," let alone "does that pointer go both ways." Its
definition index has no entry for a prose sentence at all. The
`assets`/`html` pair has one real code call, `html` into `assets`.
Nothing in `ctags`'s vocabulary surfaces "who calls this
definition." That edge stays invisible to it regardless. The
prediction: raw+ctags reproduces the raw arm's exact failure mode --
the same heuristic over-count, the same miss on `dot`/`mermaid` --
because a definition index was never the missing piece.

<!-- dankg:depends target=pilot.md#findings quote="Raw source has no ground truth for "these two things are linked," beyond an LLM's guess at authorial intent. The literate graph has one by construction." -->

[lsp_pilot.md](lsp_pilot.md) runs the sharper version of this same
question: a tool with real find-references, not just definitions.

# Caveats and next steps

- Design only. No trial has been run yet.
- N=1, the same caveat as every arm in `pilot.md`.
- The `assets`/`html` and `dot`/`mermaid` facts above were verified
  directly against this repo's current `main`, not re-checked against
  the `literate` branch `pilot.md` itself ran on. The two branches
  should agree, since both tangle from the same prose, but that
  agreement was assumed here, not confirmed.
- No real sandbox, the same caveat as every pilot before this one.
