---
title: Agent dependency-staleness pilot, against a competing tool
author: Daniel J. Okuniewicz
---
[deps_pilot.md](deps_pilot.md) compared DanKG's literate form against
a raw arm with no tooling at all: plain paragraphs, no `name=`,
`deps=`, or hash. Every raw arm reached the same wrong verdict on the
same two stages, across four languages. Its own findings named what
the raw arm actually lacked.

<!-- dankg:depends target=deps_pilot.md#findings quote="It is the lack of any conservative, dependency-based invalidation primitive, the kind of thing `make`, `bazel`, or `dbt` provide and a bare interpreter or database client does not." -->

That is a sharper, fairer comparison than "no tooling." `make` is
free, older than any agent, and exactly the primitive named. This
pilot gives the raw arm `make` instead of nothing, on the identical
fixture. It asks whether a real dependency-invalidation tool -- not
DanKG's specifically -- closes the gap.

# Setup

The shell pipeline from `deps_pilot.md`, unchanged: the same 8
scripts, the same diamond DAG, the same `| rev` edit to `normalize`,
the same "reversal preserves row count and summed length" coincidence
trap.

<!-- dankg:depends target=deps_pilot.md#setup quote="Two copies of the same pipeline, same edit applied to both" -->

A third copy is built alongside the existing raw and literate ones:

- **make**: the same 8 shell scripts, wired into a Makefile with a
  real target per stage and real file-based prerequisites. `counts`
  and `ratios` depend on `normalize`'s output file. `normalize`
  depends on `dedupe`'s, and so on down the DAG, matching what the
  scripts' own file reads already imply. `train` and `summary` get
  declared the same way a real team would declare them: from the
  scripts' own file reads, and nothing more. Neither script reads a
  file. Neither gets a declared dependency. Adding one neither script
  actually has would defeat the point of the comparison.

This is not a handicapped Makefile. It is the Makefile a competent
engineer writes from reading these exact 8 scripts, nothing more and
nothing less.

One fresh subagent for the `make` arm, alongside the existing raw and
literate results already recorded for this pipeline.

# Task

Identical to `deps_pilot.md`'s: classify all 8 stages into "recorded
output no longer trustworthy" or "safe to leave alone," with
reasoning. The `make` arm is explicitly told it can run `make` -- a
dry run (`make -n`), a real run, or both -- to check.

<!-- dankg:depends target=deps_pilot.md#task quote="classify all 8 stages into" -->

# Grading

Identical to `deps_pilot.md`: correct or incorrect against the same
5-stale/3-safe ground truth, plus tool calls and wall-clock.

# Hypothesis

`deps_pilot.md`'s two failures had different causes. `make` should
not fix them the same way.

1. **`counts`/`ratios`** are genuine file dependents of `normalize`.
   A Makefile target with `normalize`'s output as a prerequisite
   invalidates them correctly, by construction, the moment
   `normalize`'s recipe or inputs change. `make`'s mtime model should
   match `dankg check`'s answer here exactly.
2. **`train`/`summary`** have no file dependency to declare, in
   `make` or anywhere else. The missing link lives in a sentence of
   prose, not a file read.

<!-- dankg:depends target=deps_pilot.md#findings quote="The only place that dependency exists is a sentence of prose two files away." -->

A Makefile has no way to represent a dependency that isn't a file.
The prediction: the `make` arm matches the literate arm on
`counts`/`ratios`, closing half the gap. It reproduces the raw arm's
exact miss on `train`/`summary` instead. The tool it was handed
cannot express that edge at all. The agent's own reasoning is not the
variable under test here.

# Caveats and next steps

- Design only. No trial has been run yet.
- N=1, same as every arm in `deps_pilot.md`.
- The Makefile omits any comment flagging `train`/`summary` as
  unusually risky. A real engineering team would likely leave one by
  hand -- the same role `deps=` plus a prose link plays for the
  literate arm. Leaving it out isolates one question, whether `make`
  can express this dependency at all, from a separate one, whether a
  team remembered to write a comment. Disclose that asymmetry
  alongside any result; don't treat it as a neutral setup.
- Only the shell pipeline is rebuilt. `deps_pilot.md` found the same
  result in Python, SQL, and a mixed-tool pipeline. A fuller version
  of this pilot would need `make`, or a language-appropriate
  equivalent (`dbt` for the SQL pipeline), rebuilt for each. Only
  then could the hypothesis above hold generally, rather than for
  shell specifically.
- No real sandbox, the same caveat as every pilot before this one.
