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

Two arms were run fresh, on current `main`: `raw+LSP`, and a new
`literate` trial built the same way, to get a real token/wall-clock
comparison against `dankg` instead of reusing `pilot.md`'s historical
tool-call-only numbers. The `raw` row below is still `pilot.md`'s own
historical run, unchanged, the same reuse `ctags_pilot.md` made.

# Results

| Task | Arm | Correct | Tool calls | Tokens | Wall-clock | Notes |
|---|---|---|---|---|---|---|
| A: decision citation | raw (`pilot.md`, historical) | yes | 5 | -- | -- | Grepped `architecture.md`'s decision list directly. |
| A: decision citation | literate (fresh, this trial) | yes | 4 | 38,696 | ~15s | Grepped `src/graph/resolve.md` and `architecture.md`. Never ran `dankg`. |
| A: decision citation | raw+LSP (fresh, this trial) | yes | 6 | 41,893 | ~28s | Grepped `resolve.rs` and `architecture.md`. Never called `lsp_query.py`. |
| B: reciprocal references | raw (`pilot.md`, historical) | no | 12 | -- | -- | 10 false "pairs" by cross-mention grep; `assets`/`html` real, `dot`/`mermaid` missed. |
| B: reciprocal references | literate (fresh, this trial) | no | 13 | 99,570 | ~410s | Found 4 candidate pairs: `assets`/`html` and `dot`/`mermaid` real, plus two false positives (`hash.md`/`lib.md`, `main.md`/`depends.md`) -- backtick filename citations, not real links, confirmed against `dankg graph`'s own `reciprocated` field. Never ran `dankg graph` itself. |
| B: reciprocal references | raw+LSP (fresh, this trial) | no | 23 | 120,494 | ~416s | 11 false "pairs." `assets`/`html` real, `dot`/`mermaid` missed. Never called `lsp_query.py` -- the agent reasoned explicitly that find-references answers a different question than this task asks, and skipped it. |
| C: flat lookup (control) | raw (`pilot.md`, historical) | yes | 2 | -- | -- | `find` plus one `Read`. |
| C: flat lookup (control) | literate (fresh, this trial) | yes | 6 | 34,085 | ~23s | Grep/Read only. Never ran `dankg`. |
| C: flat lookup (control) | raw+LSP (fresh, this trial) | yes | 5 | 32,945 | ~23s | Grep/Read only. Never called `lsp_query.py`. |

`--` marks a cell `pilot.md`'s original run never recorded, the same
gap `ctags_pilot.md` already flagged. Both fresh arms above ran on the
identical current-`main` commit. Their own numbers are directly
comparable to each other, even though neither is directly comparable
to `raw`'s historical row.

# Findings

Not one of the six fresh trials in this run touched its own special
tool, on any task. Zero `lsp_query.py` calls. Zero `dankg` calls. This
is the starkest instance yet of a pattern `pilot.md`'s own task A
already found once: having a tool available is not the same as an
agent reaching for it.

<!-- dankg:depends target=pilot.md#findings quote="The literate corpus's efficiency advantage depends on the agent knowing to query the graph instead of grepping it like source." -->

Task B is where that absence actually costs something. `pilot.md`'s
original literate run answered task B exactly, 2 pairs and nothing
else, by running `dankg graph --format json` and filtering on
`reciprocated: true`.

<!-- dankg:depends target=pilot.md#results quote="Ran `dankg graph --format json` and filtered on `reciprocated: true`." -->

This trial's fresh literate agent never ran that command. It hand-grepped
the `.md` files instead, the same method the raw arm always uses, and
landed on 4 candidate pairs, not 2. Two were real. Two were backtick
filename citations mistaken for designed links -- the identical
category of error `pilot.md`'s raw arm made on this exact task, just
smaller in degree: 2 false positives here, against 10 for raw, 12 for
`ctags`, 11 for the LSP arm. The literate `.md` prose still carries a
real, independent advantage over raw `.rs` even under pure hand-grepping
\-- markdown's own `[text](file.md)` link syntax reads more legibly
than a Rust doc comment naming a collaborator. That advantage is not
the same as the *exact* result `dankg graph` gives by construction. It
degrades under exactly the same failure mode as every other arm,
just from a smaller base rate.

<!-- dankg:depends target=pilot.md#findings quote="Raw source has no ground truth for "these two things are linked," beyond an LLM's guess at authorial intent. The literate graph has one by construction." -->

The LSP arm's task B result is the sharpest reversal in the series.
The hypothesis predicted an LSP-backed agent might land on `assets`/
`html` "right, by luck," through a real `find-references` hit its own
logic would misread as evidence of reciprocity. That mechanism never
got a chance to run. The agent read the task, correctly reasoned that
`find-references` answers "what does the code call," not "what does
the documentation say points at what," and chose not to call
`lsp_query.py` at all. It reproduced the raw arm's exact failure shape
by the raw arm's exact method -- cross-mention grep -- for a different
reason than predicted: not because the tool gave a misleading signal,
but because the agent judged, correctly, that the tool had nothing
useful to say and skipped it. The deeper point in the original
hypothesis holds regardless of which path got there. A tool that
answers "what does the code call" cannot substitute for a graph
built from an authored, "these two things are linked" relationship,
whether an agent is fooled by it or reasons its way past it entirely.

Tasks A and C held close to their predicted shape. Task A tied across
all three arms in tool-call count. `architecture.md`'s prose settled
it every time -- neither `dankg` nor `rust-analyzer` had anything
Rust-specific or graph-specific to add. Task C also tied on
correctness, though not on tool-call count: `raw+LSP` needed 5 calls,
fresh literate needed 6, and the original raw arm needed only 2,
since neither fresh trial used its available shortcut (`readtags` on
`ctags_pilot.md`'s task C showed the identical pattern once already).

<!-- dankg:depends target=ctags_pilot.md#findings quote="The `readtags` command was named and demonstrated in the task's own instructions." -->

Cost is the other place this run adds something new. Task B is far
more expensive than A or C in every arm that has real numbers: the
literate trial spent roughly 2.5x the tokens of task A or C and about
20x the wall-clock; `raw+LSP` spent almost 3x the tokens and roughly
15 to 20x the wall-clock. A relationship-shaped question with no
ground truth in raw text is not just more error-prone by hand. It is
dramatically more expensive to chase by hand, on every arm, regardless
of which tool sits next to the agent unused.

# Caveats and next steps

- N=1 per arm per task, the same caveat as every pilot in this series.
- `rust-analyzer`'s indexing cost turned out small in practice --
  roughly 1 to 2 seconds per fresh process against this crate's actual
  size -- not the multi-minute tax the design anticipated. That
  specific prediction did not hold. It is a fact about this crate's
  size, not a general claim about `rust-analyzer` on larger codebases.
- The LSP arm's sharpest predicted mechanism -- an agent misreading a
  real `find-references` hit on `assets`/`html` as evidence of
  reciprocity -- was never exercised, because the agent chose not to
  call `lsp_query.py` on task B at all. A trial where the agent is
  told to use `lsp_query.py` specifically, the way `reasoning_pilot.md`
  added a prompted arm after its own unprompted one, is the natural
  way to actually test that mechanism.
- The `hash.md`/`lib.md` and `main.md`/`depends.md` false positives
  were confirmed against this repo's current `main`, not against
  whatever state the `literate` branch `pilot.md` originally ran on
  was in -- the same branch-drift caveat `ctags_pilot.md` already
  flagged for itself.
- No real sandbox. Each subagent had unrestricted `Bash` access to its
  assigned directory, the same caveat as every pilot before this one.
