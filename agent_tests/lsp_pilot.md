---
title: Agent navigation pilot, against an LSP
author: Daniel J. Okuniewicz
---
[ctags_pilot.md](ctags_pilot.md) gave the raw arm a definition index.
It predicted that would not close `pilot.md`'s task-B gap, since a
definition index cannot even ask whether two files point at each
other. An LSP can. `rust-analyzer` gives real find-all-references,
not just go-to-definition. This pilot asks whether that stronger tool
changes the answer.

<!-- dankg:depends target=ctags_pilot.md#hypothesis quote="`ctags` has no find-references command, so it cannot even ask "what points at `html.rs`," let alone "does that pointer go both ways."" -->

# Setup

The same `literate` branch, the same commit, the same two directories
`pilot.md` already built. Also carried over: the two verified code
facts `ctags_pilot.md` established. `html.rs` really does call into
`assets.rs` (`use super::assets`, then `assets::CSS`/`assets::JS`).
`assets.rs` never calls into `html` at all. `dot.rs` and `mermaid.rs`
share no code reference in either direction, despite both files'
prose linking to each other.

A third copy is added:

- **raw+LSP**: the raw `src/**/*.rs` tree, a real `Cargo.toml` so
  `rust-analyzer` can build a genuine semantic index, and a small CLI
  wrapper exposing two commands over `rust-analyzer`'s own stdio
  protocol: `goto-definition <file>:<line>:<col>` and
  `find-references <file>:<line>:<col>`. `rust-analyzer` needs to
  finish indexing the crate before either command returns a real
  answer -- a real, disclosed startup cost this arm pays that neither
  `raw` nor `raw+ctags` do.

The same three tasks as `pilot.md`, unchanged.

# Grading

Identical to `pilot.md`: correct or incorrect per task, plus tool
calls and, given the indexing cost above, wall-clock alongside them.

# Hypothesis

Task A and task C should land exactly where `ctags_pilot.md`
predicted for its own arm. No change on A: `architecture.md` isn't
code either tool indexes. A tie at the existing floor on C: a direct
definition lookup was already available.

Task B is the sharp case. The two verified code facts above point
two different ways. `find-references` run on `assets`'s public items,
from inside `html.rs`, returns a real hit: `html.rs` genuinely calls
`assets::CSS` and `assets::JS`. An agent could reasonably read that
hit as "these two are related." It could land on `assets`/`html` as
a pair, for a reason that has nothing to do with the actual criterion
\-- an explicit link written both ways in prose. The same query for
`dot`/`mermaid` returns nothing at all, in either direction, because
no code reference exists between them. The prediction: an LSP-backed
agent likely guesses `assets`/`html` right, by luck. It misses
`dot`/`mermaid` for certain, since `find-references` has no signal to
offer there at all. That reproduces `pilot.md`'s raw arm's exact
miss, on the one pair a stronger tool could not have caught by
construction.

<!-- dankg:depends target=pilot.md#findings quote="Raw source has no ground truth for "these two things are linked," beyond an LLM's guess at authorial intent. The literate graph has one by construction." -->

The deeper point either result would support: `find-references`
answers "what does the code call," not "what do the docs say points
at what." `dankg`'s `reciprocated` field is built from the second
graph, an authored one, not derived from the first. No amount of
semantic code intelligence recovers a relationship that was never a
code relationship to begin with.

# Caveats and next steps

- Design only. No trial has been run yet.
- N=1, the same caveat as every arm in `pilot.md` and
  `ctags_pilot.md`.
- `rust-analyzer`'s indexing cost is real and should be measured, not
  waved away -- a slow first query is a genuine tax on this arm, the
  same way `feature_pilot.md` counted the literate arm's own tool
  tax rather than ignoring it.
- The `assets`/`html` and `dot`/`mermaid` code facts were verified
  directly against this repo's current `main`, not re-checked against
  the `literate` branch `pilot.md` itself ran on, the same caveat
  `ctags_pilot.md` already flagged for itself.
- No real sandbox, the same caveat as every pilot before this one.
