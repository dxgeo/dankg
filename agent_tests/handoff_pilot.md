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

# Caveats and next steps

- Design only. No trial has been run yet.
- The two-commit history is hand-built to be believably terse. A
  real stalled PR or agent run may leave more, or less, context than
  this -- a partial comment, an open question in a PR description, a
  half-written test. This fixture tests the sparsest realistic case.
- N=1 per arm, the same caveat as every pilot before this one.
- No real sandbox, the same caveat as every pilot before this one.
