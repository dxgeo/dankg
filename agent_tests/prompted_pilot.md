---
title: Agent pilot, explicit tool-use instruction
author: Daniel J. Okuniewicz
---
[ctags_pilot.md](ctags_pilot.md) and [lsp_pilot.md](lsp_pilot.md) both
found the same thing on task B: the agent never called its own special
tool, on any task, in any arm.

<!-- dankg:depends target=lsp_pilot.md#findings quote="Not one of the six fresh trials in this run touched its own special tool, on any task." -->

This pilot removes that variable. Fresh trials were run for all three
tool-arms -- `raw+ctags`, `raw+LSP`, and `literate` -- with an explicit
instruction to use the arm's own tool at least once, and for task B,
to check every candidate pair against it before deciding. This tests
whether the gap those two files found was about an agent not knowing
to reach for a tool, or about something the tool itself cannot fix.

# Setup

Three fixtures, rebuilt fresh from current `main`, identical in shape
to `ctags_pilot.md`'s and `lsp_pilot.md`'s own: `raw+ctags` (a `tags`
file over `src/**/*.rs`), `raw+LSP` (the same `.rs` tree plus
`lsp_query.py`, the `rust-analyzer` wrapper), and `literate`
(`src/**/*.md` plus the `dankg` binary and `.dankg` config).

<!-- dankg:depends target=ctags_pilot.md#setup quote="the raw `src/**/*.rs` tree, plus a `tags` file" -->

The same three tasks as `pilot.md`, unchanged, with one addition to
each task's instructions: a REQUIREMENT to call the arm's tool at
least once, and for task B specifically, to check every candidate pair
against it before deciding whether to include or exclude that pair.
Each report was also asked to state explicitly how the tool did or
didn't change its judgment.

# Task

Identical to `pilot.md`'s three tasks (decision citation, reciprocal
references, flat lookup), plus the explicit-use requirement above.

<!-- dankg:depends target=pilot.md#tasks quote="Find every pair of files under `src/` whose documentation references each other both ways." -->

# Grading

Correct or incorrect per task, plus tool calls, tokens, and
wall-clock, the same measures `lsp_pilot.md` already tracked. Each
prompted trial is also compared directly against its own unprompted
counterpart from `ctags_pilot.md`/`lsp_pilot.md` -- a paired
before/after on the identical task and fixture shape.

# Results

| Task | Arm | Correct | Tool calls (unprompted &#8594; prompted) | Tokens (unprompted &#8594; prompted) | Wall-clock (unprompted &#8594; prompted) |
|---|---|---|---|---|---|
| A: decision citation | ctags | yes | 5 &#8594; 6 | 38,627 &#8594; 54,535 | ~19s &#8594; ~51s |
| A: decision citation | LSP | yes | 6 &#8594; 11 | 41,893 &#8594; 43,808 | ~28s &#8594; ~90s |
| A: decision citation | literate | yes | 4 &#8594; 6 | 38,696 &#8594; 40,125 | ~15s &#8594; ~48s |
| B: reciprocal references | ctags | no | 12 pairs/14 calls &#8594; 19 pairs/82 calls | 69,742 &#8594; 234,955 | ~279s &#8594; ~1301s |
| B: reciprocal references | LSP | no | 11 pairs/23 calls &#8594; 16 pairs/46 calls | 120,494 &#8594; 148,901 | ~416s &#8594; ~837s |
| B: reciprocal references | literate | no | 4 pairs/13 calls &#8594; 22 pairs/103 calls | 99,570 &#8594; 192,943 | ~410s &#8594; ~1589s |
| C: flat lookup (control) | ctags | yes | 6 &#8594; 4 | 33,839 &#8594; 35,329 | ~22s &#8594; ~42s |
| C: flat lookup (control) | LSP | yes | 5 &#8594; 6 | 32,945 &#8594; 32,847 | ~23s &#8594; ~69s |
| C: flat lookup (control) | literate | yes | 6 &#8594; 5 | 34,085 &#8594; 38,579 | ~23s &#8594; ~43s |

Ground truth for task B is unchanged: exactly 2 real reciprocal pairs
under `src/` (`assets`/`html`, `dot`/`mermaid`), confirmed against
`dankg graph`'s own `reciprocated` field.

<!-- dankg:depends target=ctags_pilot.md#results quote="Ground truth for task B was reconfirmed directly against this repo's current `main` immediately before the trial, not assumed." -->

# Findings

Task B got worse, in every arm, on every measure, when the tool became
mandatory. `ctags` went from 12 candidate pairs to 19. LSP went from 11
to 16. Literate went from 4 to 22 -- the worst over-count in this
entire series, on the one arm whose tool actually gives an exact
answer. All three still missed `dot`/`mermaid`, or (for literate)
buried it correctly-found among 20 false positives. Cost rose just as
sharply: `ctags` task B went from ~279s to ~1301s, roughly 4.7x.
Literate went from ~410s to ~1589s, roughly 3.9x. Every other task
also cost more under the explicit-use instruction, in tokens and
especially wall-clock, with two minor exceptions (`ctags` and literate
task C each used slightly fewer tool calls, though not less time).

<!-- dankg:depends target=lsp_pilot.md#results quote="`assets`/`html` real, `dot`/`mermaid` missed." -->

`ctags` and LSP's task B failures did not change shape under
prompting. They got more thorough at the same wrong thing. Both tools
were used carefully this time -- 29 `readtags` calls and 14
`lsp_query.py` calls, each one disambiguating a real ambiguity (which
file actually owns a symbol named `run`, or `Graph`, or `sort`) before
counting a pair. That care surfaced more genuine code coupling between
modules than a plain grep pass ever found. Genuine code coupling was
never the right criterion, though. Task B asks about documentation,
not linkage. Both tools answered a question about linkage more
thoroughly instead, which produced more matches for the wrong
yardstick, not fewer. `dot`/`mermaid` stayed invisible to both tools
for the same reason it always has: no code reference exists between
them, by construction. No amount of careful querying can find one.

<!-- dankg:depends target=lsp_pilot.md#findings quote="A tool that answers "what does the code call" cannot substitute for a graph built from an authored, "these two things are linked" relationship, whether an agent is fooled by it or reasons its way past it entirely." -->

Literate's failure is a different shape. It is the sharpest result
in this file. The prompted agent ran `./dankg graph . --format json`,
read its `reciprocated` field, and got the exact right answer: 2
pairs, `assets`/`html` and `dot`/`mermaid`, nothing else. It said so
directly in its own report. It then treated that exact answer as
merely "a starting seed," reasoned that `dankg`'s link-based definition
missed "looser phrasing" like a shared function name in prose, and
went on to manually read all 51 files for two-way `module::function`
mentions -- the same criterion `ctags` and LSP were using -- adding 20
more candidates on top of the correct 2. The tool was used exactly as
instructed and gave the exact right answer. The agent overrode it
anyway. Recall was already perfect without the extra work: the
unprompted literate trial also found both real pairs by hand, just
with 2 false positives instead of 20. Forcing `dankg` into the loop
did not fix a missing-recall problem, because there wasn't one here.
It added confidence in a manual criterion the agent trusted more than
the tool that answers the actual question by construction.

<!-- dankg:depends target=pilot.md#results quote="Ran `dankg graph --format json` and filtered on `reciprocated: true`." -->

Tasks A and C mostly held their prior shape, correct in every cell,
but the explicit-use requirement was not free anywhere. LSP's task A
grew from 6 calls to 11, since the agent first had to locate a
*relevant* symbol to satisfy the requirement before it could query it
\-- a search cost the unprompted trial never paid. Every wall-clock
number roughly doubled to quadrupled even where tool-call counts
barely moved, since `rust-analyzer`'s per-call indexing wait and the
extra reasoning needed to justify each mandatory call both cost real
time the correctness numbers don't show.

# Follow-up: an authoritative instruction

The literate arm's caveat above named a specific next trial: tell the
agent to treat `dankg graph`'s `reciprocated` field as the final
answer, not a starting point. One fresh trial ran exactly that, on
task B only. The instruction added one explicit rule: report only
pairs backed by a `reciprocated: true` edge, and do not supplement
with manual reading, even for a pair that looks plausible.

The result was exact. 2 pairs, `assets`/`html` and `dot`/`mermaid`,
nothing else -- 0 false positives, against the earlier prompted
trial's 20. It was also the cheapest literate task-B trial in this
entire series: 6 tool calls, 31,859 tokens, ~52s. That beats the
original *unprompted* trial (13 calls, 99,570 tokens, ~410s) as well
as the loosely prompted one (103 calls, 192,943 tokens, ~1589s) on
every measure at once -- correctness, tool calls, tokens, and
wall-clock, together, not traded off against each other.

<!-- dankg:depends target=lsp_pilot.md#results quote="Found 4 candidate pairs: `assets`/`html` and `dot`/`mermaid` real, plus two false positives" -->

The agent's own report confirms why: it ran `dankg graph`, filtered
`reciprocated: true` edges in Python, and stopped. It did the one
extra step the task's own scope required -- excluding a `reciprocated`
edge between `architecture.md` and `project.md` because those two
files sit outside `src/`, not because it doubted the tool's field
itself. No prose file was read at any point. The earlier failure was
never about recall or about the tool's own capability. It was about
an instruction that told the agent to *consult* the tool without also
telling it the tool's answer was the whole of the assignment. Closing
that one sentence of ambiguity closed the entire gap, at a fraction
of the earlier cost.

<!-- dankg:depends target=pilot.md#findings quote="The literate corpus's efficiency advantage depends on the agent knowing to query the graph instead of grepping it like source." -->

This does not retest whether the same fix would help `ctags` or LSP.
It should not: those two tools have no field that could ever represent
`dot`/`mermaid`'s relationship, authoritative instruction or not, since
no code reference exists between them for any index to find.

# Caveats and next steps

- N=1 per cell, the same caveat as every pilot in this series -- N=1
  for the follow-up trial too.
- The original instruction told each agent to use its tool "at least
  once" and, for task B, "for every candidate pair." It did not say
  what to do when the tool's own answer conflicts with a manual read
  \-- literate's agent had to decide that on its own, and chose to
  trust its own broader criterion over `dankg`'s exact one. The
  follow-up above tested the fix directly and it held, at N=1.
- `ctags` and LSP's worse over-count came from spending more, carefully
  verified effort on the same flawed criterion -- treating a two-way
  code-symbol mention in prose as equivalent to a designed link. This
  file did not test whether telling those two arms the *correct*
  criterion up front (an explicit link, not a mention) would close the
  gap `readtags`/`lsp_query.py` cannot close on their own; it only
  tested whether more tool use under the original task wording would.
- No real sandbox, the same caveat as every pilot before this one.
