---
title: Agent cold-start handoff pilot
author: Daniel J. Okuniewicz
---
[deps_pilot.md](deps_pilot.md) already hands its agent someone else's
finished edit, cold, with no narrative.

<!-- dankg:depends target=deps_pilot.md#task quote="Given the pipeline and the already-applied edit" -->

That edit is a single, clean change, though, applied in full. Real
handoffs are messier: a previous session, human or agent, stops
partway through a multi-step change, with nothing but its own commit
history to explain where it got to. This pilot tests that shape
instead: an unfinished, multi-commit refactor, and whether
`dankg check` beats `git log` at finding what's left.

# Setup

The `jot` fixture from `reasoning_pilot.md`, at the point a previous
session left it: `list` already scans one level of subdirectories
deep, and `store.md` already says so. Two commits record that much
work, with realistically terse messages:

<!-- dankg:depends target=reasoning_pilot.md#setup quote="A purpose-built fixture keeps the ground truth fixed and exact, instead of drifting with the real `dankg` corpus the way a live number would." -->

```
1. "scan subdirs in list"   -- changes list's scan depth
2. "update store.md"        -- updates store.md's claim to match
```

Nothing else changed. `stats` still counts top-level files only.
`notes.md`'s two claims and `tag.md`'s claim, the same three from
`reasoning_pilot.md`'s four, are still false. No comment, commit
message, or TODO names any of the three. The previous session simply
stopped.

Three arms:

- **raw**: the fixture at this commit, plus its two-commit history,
  no `dankg` anything. Told to finish the work; not told to look at
  the history.
- **raw-prompted**: identical, but told explicitly to review the git
  history before starting.
- **literate-unprompted**: the same WIP state, with real
  `dankg:depends` markers wiring `notes.md`'s and `tag.md`'s claims
  back to `store.md`'s sentence, plus the `dankg` binary. Not told to
  run `dankg check`.

No literate-prompted arm. `reasoning_pilot.md` already found that
telling the literate arm to run `dankg check .` changes its cost, not
its ceiling.

<!-- dankg:depends target=reasoning_pilot.md#findings quote="did not raise the literate arm's ceiling -- it was already there unprompted" -->

# Task

Every arm gets the same instruction: finish this work, and make sure
the codebase and its docs are fully consistent. Nothing more
specific.

# Grading

The same three measures `reasoning_pilot.md` used -- functional,
recall (of the three still-false claims), precision -- plus one new
measure specific to a handoff: **orientation**. Before touching
anything, does the agent state an accurate picture of what the
previous session already finished, and what it didn't? Score
orientation as accurate, inaccurate, or absent, from the transcript
alone.

# Hypothesis

`git diff` between the two commits shows exactly what changed:
`list`'s code, and one sentence in `store.md`. It does not show what
that change implies two files away, the same gap `deps_pilot.md`
found in a single already-applied edit.

<!-- dankg:depends target=deps_pilot.md#findings quote="The only place that dependency exists is a sentence of prose two files away." -->

The prediction: raw, unprompted, often skips the git history
entirely, reads current file state, and treats passing tests as
"done" -- missing the incomplete doc reconciliation altogether.
Raw-prompted reads the diff, correctly avoids redoing `store.md`'s
already-finished work, but still misses `notes.md`'s and `tag.md`'s
claims, because the diff never touches those files. Literate-
unprompted reaches the same ceiling `reasoning_pilot.md` already
found, by the same mechanism: `dankg check` names the three stale
claims directly, with no git archaeology at all.

# Results

| | raw | raw-prompted | literate-unprompted |
|---|---|---|---|
| Checked git history | yes, unprompted | yes, as instructed | n/a -- no git in this copy |
| Ran `dankg check` | n/a -- no `dankg` here | n/a | yes, unprompted |
| `stats`/`list` divergence | documented as intentional | unified (extended the fix) | unified (extended the fix) |
| Recall (of 3 remaining claims) | 3/3 | 3/3 | 3/3 |
| Precision | 3/3, no wrong touches | 3/3, no wrong touches | 3/3, plus an unrequested `readme.md` addition |
| Orientation | accurate | accurate | accurate |
| Tool calls | 20 | 16 | 36 |
| Wall-clock | ~189s | ~160s | ~370s |

All three arms found and correctly edited all three remaining stale
claims. All three stated an accurate, detailed picture of what the
previous session had and hadn't finished, before making a single
edit. Recall, precision, and orientation do not separate these arms
at all here, unlike every prior pilot in this series.

The raw arm was not supposed to check git history. It did anyway.
Its second tool call, unprompted, was `git log --oneline --all && git status`, followed later by two `git show` calls against the
specific WIP commits. Telling an agent "a teammate worked on this
recently" was cue enough on its own. The raw/raw-prompted split this
pilot was built to test did not actually happen: both arms had the
identical git history in front of them, one because it was told to
look, one because it looked anyway.

# Findings

None of the three predicted mechanisms held as stated. Both raw arms
found and read the git history. All three arms, including both raw
ones, reached perfect 3/3 recall by reading all five docs against
the current code directly, not by diffing two commits. The mechanism
this pilot's own hypothesis named -- `git diff` shows the change but
not its two-file-away implication -- never got a chance to matter,
because nothing here needed a diff to see. `store.md`'s current
sentence and `notes.md`'s current sentence sit in two short files a
normal orientation pass reads back to back. Recall separated the
arms cleanly in `reasoning_pilot.md` and `deps_pilot.md`, where the
missing link genuinely had no visible trail in raw text. It does not
separate them here, because this fixture's remaining claims do have
one, in plain current-state prose, with or without git.

<!-- dankg:depends target=deps_pilot.md#findings quote="The only place that dependency exists is a sentence of prose two files away." -->

The one real split -- what to do about `stats` -- deserves a more
careful claim than "raw got it wrong." `notes.md`'s claim describes
a fact that was true by coincidence, not by declared contract:
`stats` and `list` agreed only because both happened to scan the
same depth. Once `list`'s depth changed, two different edits both
make the doc true again. One rewrites the doc to describe the new,
unequal reality. One changes `stats` to restore equality. This
pilot's own task text -- "finish this work, and make sure the
codebase and its docs are fully consistent" -- does not say which
direction consistency should run. The functional ground truth used
above assumes the second reading, inherited from `reasoning_pilot.md`'s
original design intent for this exact claim, not from anything
stated explicitly in this pilot's own task. Raw's choice was a
defensible reading of an underspecified instruction, not a proven
reasoning failure. It is reported here as a real difference in the
resulting program, not as a verdict.

What is not ambiguous: this split tracks neither git access nor
`dankg` access. Both raw agents had the identical commit history.
Only one of the two used it to read "scan subdirs in list" as an
unfinished half of a change, rather than a closed one. Neither `git log` nor `dankg check` settles that reading either way -- both only
surface facts (a diff, a stale quote), never which of two consistent
resolutions was intended.

The literate-unprompted arm did replicate `reasoning_pilot.md`'s
sharpest finding: it found and ran `dankg check .` without being
told to, and finished at a genuinely verified 0-stale state, not a
self-reported one.

<!-- dankg:depends target=reasoning_pilot.md#findings quote="Every literate trial found and ran `dankg check .`, whether told to or not." -->

That is a real, structural difference from both raw arms' final
state, which rests on each agent's own say-so with nothing to check
it against. It came at the largest cost recorded in this whole
series: 36 tool calls and roughly 370 seconds, more than double
either raw arm. Part of that cost was self-inflicted and new. The
agent's first attempt at rewording a `dankg:depends` quote didn't
match verbatim. It spent several calls reading `src/depends.md`'s
own `normalize_ws` logic to learn the checker only collapses
whitespace. It does not strip markdown. Authoring a correct
`dankg:depends` marker is its own small skill, with its own failure
mode, distinct from the discovery-and-formatting costs
`reasoning_pilot.md` and `feature_pilot.md` already counted.

This pilot set out to ask whether `dankg check` beats `git log` at
finding what's left. The honest answer, from this one trial: `git log` was never the deciding factor for either raw arm. The real
deciding factor -- how to read an underspecified task -- is
something neither tool resolves by itself. `dankg check`'s
demonstrated edge here was narrower and different from the one
predicted: not finding more, but finishing certain, at a real and
now well-documented price.

# Caveats and next steps

- One trial has been run, for all three arms. N=1 per arm, the same
  caveat as every pilot before this one -- and a sharper one here
  than usual, since the raw/raw-prompted split collapsed into two
  runs of nearly the same starting information, not a controlled
  pair.
- The fixture, as built, did not test its own stated hypothesis. All
  three remaining claims turned out readable from current-state
  prose alone, with no git archaeology needed. A fixture that
  actually isolates "needs history" from "needs a careful read" would
  need a fact truly absent from every file's current text -- closer
  to `deps_pilot.md`'s `train`/`summary` shape than to this one.
- The functional ground truth (`stats` must match `list`) is an
  assumption carried over from `reasoning_pilot.md`'s original design
  for this claim, not a requirement stated in this pilot's own task
  text. A stricter version should either state that requirement
  explicitly in the task, or grade the `stats` question as "which
  resolution, with what stated reasoning" instead of pass/fail.
- The two-commit history is hand-built to be believably terse. A
  real stalled PR or agent run may leave more, or less, context than
  this -- a partial comment, an open question in a PR description, a
  half-written test. This fixture tests the sparsest realistic case.
- No real sandbox, the same caveat as every pilot before this one.
