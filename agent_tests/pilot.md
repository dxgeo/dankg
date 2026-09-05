---
title: Agent navigation pilot
author: Daniel J. Okuniewicz
---
[project.md](../project.md#key-features)'s third key feature claims DanKG
is agent-compatible. An LLM can run it because it only touches plaintext.
This pilot tests a sharper version of that claim. Does an LLM navigate a
codebase faster and more correctly in DanKG's own literate `src/*.md`
form, compared to the plain `src/*.rs` a normal Rust crate would ship?

[feature_pilot.md](feature_pilot.md) runs a companion test on
implementing a feature instead of navigating one.
[deps_pilot.md](deps_pilot.md) runs a third test, on transitive
dependency tracing and staleness. That is the sharpest version of the
relationship-shaped question task B below first raised.

# Setup

The `literate` branch carries both forms side by side. `dankg tangle`
generates every `src/*.rs` from a matching `src/*.md`. The prose is
identical in both. This isolates one variable: graph-shaped navigation
(headings, resolved links, backlinks, `deps=`) against flat files plus
grep, with content held constant.

Two directories were built from the same commit:

- **raw**: `src/**/*.rs`, `Cargo.toml`, and the shared root docs
  (`architecture.md`, `project.md`, which predate the literate
  conversion and exist on `main` too).
- **literate**: `src/**/*.md`, the same shared root docs, a `.dankg`
  config, and a copy of the `dankg` binary.

Six fresh, non-forked subagents ran the pilot, one per (task, arm) pair.
A fresh agent was required rather than a fork of the working session. A
fork inherits the whole conversation. It would already know the
answers. Each agent was told to work only inside its assigned
directory, to solve one question, and to report back its answer plus
the ordered list of every tool call it made.

**Caveat.** "Only inside its assigned directory" was an instruction,
not a sandbox. A subagent with `Bash` access could, in principle, read
anywhere on the filesystem. None showed signs of doing so. A stricter
version of this test would enforce that boundary for real, with a
container or a scoped tool policy, not a prompt.

# Tasks

Three questions separate a real hypothesis from a placebo. Graph-shaped
questions should favor the literate arm. A flat, single-file question
should be a wash. That question serves as a control.

**A — decision citation (graph-shaped).** Why does the module that
resolves cross-file links wait for the whole corpus to be parsed before
resolving anything? Which numbered architectural decision does that
follow? *Ground truth:* Decision 6 ("Index scope"). Backlinks are only
honest once every file has been seen.

**B — reciprocal references (graph-shaped).** Find every pair of files
under `src/` whose documentation references each other both ways.
*Ground truth:* `render/assets` ↔ `render/html`, and `render/dot` ↔
`render/mermaid`.

**C — flat lookup (control).** Find the function that computes the
initial node ordering before the crossing-reduction sweeps run. State
the determinism guarantee its tests check. *Ground truth:*
`initial_order` in `layout/order`, a depth-first walk seeded in index
order. `ordering_is_the_same_on_every_run` asserts byte-identical
layers across repeated builds of the same graph.

# Results

| Task | Arm | Correct | Tool calls | Notes |
|---|---|---|---|---|
| A: decision citation | raw | yes | 5 | Found decision 6 by grepping `architecture.md`'s decision list directly and matching the rationale by hand. |
| A: decision citation | literate | yes | 6 | Grepped the `.md` files the same way the raw arm grepped `.rs`. Never ran `dankg` itself. One more call than the raw arm, not fewer. |
| B: reciprocal references | raw | no | 12 | Grepped for any filename or `crate::` mention of another module, in either direction, and reported 10 "pairs." Only one (`assets`/`html`) is a real reciprocated link. `dot`/`mermaid` was missed. The other ~9 are ordinary one-way doc comments that name a collaborating module, misread as mutual. |
| B: reciprocal references | literate | yes | 7 | Ran `dankg graph --format json` and filtered on `reciprocated: true`. This was the one run that treated the graph as structured data rather than prose. Found exactly the 2 true pairs, nothing else. Lost two calls to a self-inflicted stdout/stderr mixup. |
| C: flat lookup (control) | raw | yes | 2 | `find` plus one `Read`. |
| C: flat lookup (control) | literate | yes | 2 | `grep` plus one `Read`. Tied with raw, as the control predicts. |

# Findings

The advantage is not automatic. It is not just about speed.

On task A, having the `.md` files and the `dankg` binary available
bought nothing. The literate agent fell back to the exact same
grep-and-read strategy as the raw agent, and used one extra call doing
it. The control task (C) came out tied. That is what a real effect
should look like: no advantage where the graph structure has nothing to
contribute.

Task B is where the two arms actually diverge, and the gap is bigger
than the tool-call count suggests. "Two files reference each other" has
no crisp answer in raw source. A module's doc comments naming a
collaborator by filename is just normal, well-documented code. It is
not evidence of a designed bidirectional relationship. Grep alone
cannot tell the two apart. The raw agent had only one tool: a heuristic
search for cross-mentions in both directions. That heuristic produced
five times more "pairs" than actually exist. It also missed one of the
two true ones. The literate agent did not have to define "reciprocal"
itself. `dankg graph`'s `reciprocated` field already encodes the one
true criterion: an explicit link written both ways. Its answer was
exact.

Here is the implication. The literate corpus's efficiency advantage
depends on the agent knowing to query the graph instead of grepping it
like source. Task A shows that path is not automatic. Its
*correctness* advantage on relationship-shaped questions looks more
structural. Raw source has no ground truth for "these two things are
linked," beyond an LLM's guess at authorial intent. The literate graph
has one by construction.

# Caveats and next steps

- N=1 per cell. Six agent runs support no statistical claim. This is a
  pilot. It checks whether the experiment is well-formed. It does not
  settle the question.
- No real sandbox (see *Setup*).
- Someone who already knew the answer hand-picked every task here. A
  fuller version needs a larger, blinder task bank, several trials per
  cell, and a scripted harness instead of six one-off `Agent` calls.
  The chat history this pilot came out of discusses harness options not
  yet built.
- This pilot only asked read-only questions. `feature_pilot.md` (see
  the link above) runs the same comparison on an actual implementation
  task instead.
