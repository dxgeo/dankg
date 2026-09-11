---
title: Agent complexity-scaling pilot
author: Daniel J. Okuniewicz
---
[pilot.md](pilot.md)'s task B and [deps_pilot.md](deps_pilot.md)'s
staleness trap both show a literate-graph advantage on
relationship-shaped questions with no ground truth in raw prose. Does
that advantage, and the tax that comes with it, grow or shrink as the
codebase gets bigger? Every pilot so far picked one fixed size and
answered the question at that size. This pilot varies size itself,
along two axes, holding the trap mechanism each prior pilot already
validated fixed.

# Setup

Two independent axes, each reusing a mechanism a prior pilot already
validated at one fixed point, now parameterized by size.

## Depth axis: chain length

[deps_pilot.md](deps_pilot.md)'s shell pipeline, unchanged in every
particular except one. The gap between the edited stage (`normalize`)
and the final stage whose staleness has no file trail at all --
`train`/`summary`'s exact shape -- grows from a fixed 2 stages to a
parameter N.

<!-- dankg:depends target=deps_pilot.md#the-four-pipelines quote="The shell version ran first: temp files under `/tmp`, `sh` as the `[lang.sh]` command, `| rev` as the edit." -->

N stages sit between `normalize` and the final hidden-dependency
stage, for N in {2, 4, 8, 16}. N=2 is `deps_pilot.md`'s own shell
run, already recorded -- reused as a data point here, not rerun.
Every inserted stage between N=2's own two positions is a trivial,
fully-file-traceable pass-through (a one-line copy or transform of
the previous stage's output file). It adds distance without adding a
fresh judgment call of its own. The final stage keeps the exact
property `deps_pilot.md` built into `train`/`summary`: no file read
anywhere in its own script, its dependency stated only in a sentence
of prose elsewhere.

<!-- dankg:depends target=deps_pilot.md#findings quote="The only place that dependency exists is a sentence of prose two files away." -->

The same `| rev` edit to `normalize`, and the same coincidence trap on
the stages immediately downstream of it, stay fixed at every N. Only
the distance to the hidden-dependency stage changes.

Two copies per N, the same split `deps_pilot.md` used: **raw** (plain
prose, no `name=`/`deps=`/hash) and **literate** (real `dankg` named
blocks, `dankg eval --each --yes` run once for real, then the same
edit applied afterward, nothing re-run).

## Breadth axis: candidate pool size

This pilot rebuilds [pilot.md](pilot.md)'s task B as a purpose-built
fixture, not the real `src/` tree. That keeps the decoy count a
deliberate choice, not whatever the real corpus happened to contain.

<!-- dankg:depends target=reasoning_pilot.md#setup quote="A purpose-built fixture keeps the ground truth fixed and exact, instead of drifting with the real `dankg` corpus the way a live number would." -->

A pool of M files, for M in {5, 20, 50}. Exactly 2 true reciprocal
pairs (4 files) sit in every pool, unchanged across M. The remaining
M-4 files each carry exactly one one-way mention of another file in
the pool -- a real doc comment naming a collaborator, never returned.
Decoy count scales 1:1 with M: 1 decoy at M=5, 16 at M=20, 46 at M=50.

Two copies per M: **raw** (plain `.md` files, no `dankg` config) and
**literate** (the same files, a `.dankg` config, and a copy of the
`dankg` binary).

# Task

**Depth axis.** Given a chain-length variant with the edit already
applied, classify every stage as "recorded output no longer
trustworthy" or "safe to leave alone," with reasoning. Told it can
execute the scripts to check, the same instruction `deps_pilot.md`
gave.

<!-- dankg:depends target=deps_pilot.md#task quote="classify all 8 stages into" -->

**Breadth axis.** Given a pool-size variant, find every pair of files
whose documentation references each other both ways.

<!-- dankg:depends target=pilot.md#tasks quote="Find every pair of files under `src/` whose documentation references each other both ways." -->

# Grading

Identical to the originals in method, not in fixed numbers. Depth
axis ground truth comes from each variant's own `dankg check` output,
not hand-derived -- the same method `deps_pilot.md` used.

<!-- dankg:depends target=deps_pilot.md#setup quote="from `dankg check` run directly on each literate copy, not hand-derived" -->

Stale count is not fixed at 5 of 8. Every inserted pass-through stage
downstream of `normalize` cascades stale too. Total stale count grows
with N as a result. Breadth axis ground truth stays fixed at exactly 2 true
pairs by construction, verified with `dankg graph --format json`'s
`reciprocated` field, the same method `pilot.md`'s literate arm used.

<!-- dankg:depends target=pilot.md#results quote="Ran `dankg graph --format json` and filtered on `reciprocated: true`." -->

Both axes report tool calls and wall-clock or tokens, the same as
their source pilots. The one addition: every measure is reported as a
function of N or M, not a single number, since the trend across sizes
is what this pilot exists to check, not a point estimate at one size.

# Hypothesis

Two predictions, one per axis, both following from what the earlier
pilots already found rather than a new mechanism.

**Depth.** A raw agent has to hold a longer chain of file reads in
mind to notice the final stage has none. The prediction: raw's recall
degrades further as N grows past 2, the same failure `deps_pilot.md`
found at N=2 getting worse, not different in kind. `dankg check`
stays a single call and an exact answer at every N, the same way it
answered `deps_pilot.md`'s four language variants identically. Tool
calls for the literate arm should stay roughly flat across N. Tool
calls for the raw arm should grow with N, since a longer chain means
more scripts to read and run by hand.

<!-- dankg:depends target=handoff_pilot.md#caveats-and-next-steps quote="need a fact truly absent from every file's current text" -->

**Breadth.** `pilot.md` already found the raw arm's grep-both-
directions heuristic produces roughly 5 times more false "pairs" than
exist, at whatever pool size the real corpus happened to offer.

<!-- dankg:depends target=pilot.md#findings quote="Raw source has no ground truth for "these two things are linked," beyond an LLM's guess at authorial intent. The literate graph has one by construction." -->

The prediction: that false-positive rate grows with the decoy count,
since every one-way mention is one more candidate the heuristic has to
judge by hand. `dankg graph`'s `reciprocated` filter should stay exact
and near-flat in tool-call cost at every M, since it is one query
regardless of pool size.

Neither prediction is about the literate arm's own tax shrinking in
absolute terms. `feature_pilot.md` found a real, fixed cost with no
offsetting benefit on a task this pilot's mechanism does not touch.

<!-- dankg:depends target=feature_pilot.md#findings quote="the literate form adds a real tax and no offsetting benefit." -->

The claim under test here is relative: does the raw arm's cost and
error rate grow faster than the literate arm's as size grows, on a
question the literate arm was already shown to answer exactly at one
fixed size.

# Caveats and next steps

- Design only. No trial has been run yet.
- N=1 per cell, the same caveat as every pilot in this series. Four
  sizes times two arms is 8 trials for the depth axis alone before any
  repeats; a trend read off single points at each size is weaker
  evidence than a single comparison at one size.
- Cost. This is the most expensive pilot in the series to run in
  full. A first pass should run the depth axis alone, since its N=2
  point is already banked for free from `deps_pilot.md`, and defer
  the breadth axis to a follow-up once the depth trend is read.
- The inserted pass-through stages are meant to be judgment-free by
  construction. If a build finds an agent treating one as suspicious
  on its own, that spends some of the "distance" signal on a confound
  this design did not intend -- the same kind of late-discovered
  fixture detail `tooling_pilot.md`'s own caveats flagged for a
  different fixture.

<!-- dankg:depends target=tooling_pilot.md#caveats-and-next-steps quote="The fixture built for this trial diverged from the plan above in one way." -->

- The blind rubric `reasoning_pilot.md` used is deliberately not
  reused here. Its own finding was that a blind score can move
  without the underlying result moving.

<!-- dankg:depends target=reasoning_pilot.md#findings quote="It can also manufacture an apparent one when none does." -->

- No real sandbox, the same caveat as every pilot before this one.

</content>
