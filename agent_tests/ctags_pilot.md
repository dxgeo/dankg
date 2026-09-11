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

# Results

| Task | Arm | Correct | Tool calls | Tokens | Wall-clock | Notes |
|---|---|---|---|---|---|---|
| A: decision citation | raw (`pilot.md`) | yes | 5 | -- | -- | Grepped `architecture.md`'s decision list directly. |
| A: decision citation | literate (`pilot.md`) | yes | 6 | -- | -- | Grepped the `.md` files the same way; never ran `dankg`. |
| A: decision citation | raw+ctags | yes | 5 | 38,627 | ~19s | Never called `readtags`. Found `resolve.rs` by `find`/`grep`, read it, then grepped `architecture.md`'s decision list -- the identical method the original raw arm used, at the identical cost. |
| B: reciprocal references | raw (`pilot.md`) | no | 12 | -- | -- | Reported 10 "pairs" by grepping cross-mentions in both directions; only `assets`/`html` was real, `dot`/`mermaid` was missed. |
| B: reciprocal references | literate (`pilot.md`) | yes | 7 | -- | -- | Exact, via `dankg graph --format json` filtered on `reciprocated: true`. |
| B: reciprocal references | raw+ctags | no | 14 | 69,742 | ~279s | Reported 12 "pairs" -- worse over-counting than the original raw arm's 10. Correctly caught `html.rs`/`assets.rs`. Missed `dot`/`mermaid` again. `readtags` was called twice, only to disambiguate two ambiguous names to their defining file -- never to test reciprocity itself. |
| C: flat lookup (control) | raw (`pilot.md`) | yes | 2 | -- | -- | `find` plus one `Read`. |
| C: flat lookup (control) | literate (`pilot.md`) | yes | 2 | -- | -- | `grep` plus one `Read`. |
| C: flat lookup (control) | raw+ctags | yes | 6 | 33,839 | ~22s | Never called `readtags`, despite being told about it and despite the task being an exact-name definition lookup. Used `find`, `ls`, two `grep` passes, and two `Read` calls instead. |

`--` marks a cell `pilot.md` never recorded. Its own Results table
tracked tool calls only, not tokens or wall-clock. The raw and
literate rows above cannot be filled in retroactively. Only the
`raw+ctags` rows have real numbers here.

Ground truth for task B was reconfirmed directly against this
repo's current `main` immediately before the trial, not assumed.
`assets.md`'s and `html.md`'s prose link each other. Only `html.rs`'s
code calls into `assets` (`use super::assets`,
`assets::CSS`/`assets::JS`). `assets.rs`'s code never references
`html`. `dot.md` and `mermaid.md` link each other in prose the same
way. Neither `.rs` file has a real code reference to the other --
`mermaid.rs` names `dot.rs` once, but only inside a doc comment, not
a `use` or a call.

# Findings

Task B is where the hypothesis is confirmed. It is the task this
pilot exists to test. `ctags` closed none of the gap `pilot.md` found.
It reproduced the raw arm's exact failure shape. The one pair with a
real code call (`assets`/`html`) was caught. The one pair with no code
call at all (`dot`/`mermaid`) was missed. The heuristic over-count got
worse, not better -- 12 candidate "pairs" against the original raw
arm's 10. `readtags` was used twice in this trial, both times only to
resolve which file actually defines an ambiguous name. Neither call
touched the question the task actually asks. A definitions index has
no field, command, or query shape that can represent "these two things
cite each other." That was the prediction. Nothing in a real `ctags`
run found a way around it.

<!-- dankg:depends target=pilot.md#findings quote="Raw source has no ground truth for "these two things are linked," beyond an LLM's guess at authorial intent. The literate graph has one by construction." -->

Task A held exactly as predicted: a flat tie with the original raw
arm, at the identical tool-call count, by the identical grep-and-read
method. `ctags` never entered the picture, because the answer lives in
`architecture.md`'s prose, not in a Rust symbol.

Task C did not hold as predicted. The hypothesis expected a direct
`readtags` lookup to match the existing 2-call floor, not beat it. The
`readtags` command was named and demonstrated in the task's own
instructions. The task was exactly the kind of question --
"find the function named X" -- a definitions index answers in one
query. The agent never called it anyway. It defaulted to `find`, two
rounds of `grep`, and two `Read` calls instead. That tripled the cost
of the tie every other arm reached on this task.

`pilot.md`'s own task A already found this same failure shape, on a
different tool: a literate agent with `dankg` available that never
touched it, grepping the `.md` files exactly like the raw arm instead.

<!-- dankg:depends target=pilot.md#findings quote="having the `.md` files and the `dankg` binary available bought nothing" -->

This time the accident fell the other way. It cost the arm nothing in
correctness, only in tool calls. Having a real index available is not
the same as an agent knowing, or choosing, to reach for it.
`pilot.md` already found that true once. This trial found it true
again.

<!-- dankg:depends target=pilot.md#findings quote="The literate corpus's efficiency advantage depends on the agent knowing to query the graph instead of grepping it like source." -->

# Caveats and next steps

- One trial has been run, for the `raw+ctags` arm only, across all
  three tasks. The `raw` and `literate` rows above are `pilot.md`'s
  own numbers, reused as recorded there, not re-run here.
- N=1, the same caveat as every arm in `pilot.md`.
- The `literate` branch `pilot.md` originally ran its two arms on does
  not exist in this repository as checked out for this trial. The
  `raw+ctags` fixture was built directly from current `main` instead,
  after reconfirming the task B ground truth held there (see
  *Results*). The `raw`/`literate` rows are `pilot.md`'s own historical
  numbers from whatever state that branch was in at the time, not
  re-verified against `main` here.
- No real sandbox. Each subagent had unrestricted `Bash` access to its
  assigned directory, the same caveat as every pilot before this one.
- [lsp_pilot.md](lsp_pilot.md) still stands as the sharper follow-up:
  the prediction here was that a *definitions* index cannot represent
  reciprocity. A tool with real find-references might, in principle,
  get closer -- the direct next test, not yet run.
