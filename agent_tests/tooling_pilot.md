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

# Results

| | Raw (`deps_pilot.md`) | Literate (`deps_pilot.md`) | Make |
|---|---|---|---|
| Correct (5 stale / 3 safe) | no -- found only 1 stale, 7 safe | yes, exact | no -- found 3 stale, 5 safe |
| Tool calls | 7 | 9 | 7 |
| Wall-clock | ~69s | ~42s | ~146s |
| Method | Read all 5 files, then executed all 8 scripts to empirically diff current vs. recorded output | Traced `deps=` by hand, then ran `dankg check` to cross-verify | Read all 8 scripts and recorded outputs, checked mtimes, ran `make -n`, then a real `make` run, diffed before and after |

`make`'s own dry run named the mechanism directly. `make -n normalize.txt` proposed a rebuild. So did `counts.txt` and
`ratios.txt`, each cascading through `normalize.txt`'s own listed
prerequisite. `make -n train.txt`, `make -n summary.txt`, and every
upstream target reported "up to date." That is `make`'s complete,
correct answer under its own model: 3 stale, 5 safe. It is also
exactly wrong on two of those five, for exactly the reason predicted:
`train.sh` and `summary.sh` read no file at all. No Makefile edge can
reach them.

The agent's own classification matched `make`'s verdict exactly,
stage for stage. `normalize`, `counts`, `ratios` were marked
untrustworthy. `raw`, `dedupe`, `train`, `summary`, `audit` were
marked safe. 6 of 8 correct. Both misses are `train` and `summary`,
the identical two stages every raw arm in `deps_pilot.md` missed, on
all four of its pipelines.

# Findings

One part of the hypothesis needs a real correction, not just a
confirmation. `train.sh` and `summary.sh` carry an inline comment,
seen and quoted directly in the agent's own report, describing
exactly the dependency `make` cannot express: a real model would take
the row count and total length as features, but this stand-in never
reads either file. That is closer to a full disclosure than
`deps_pilot.md`'s own "sentence of prose two files away." The agent
read it and discounted it anyway: "despite the 'conceptual'
dependency noted in its comment." This was not a case of the
information being unavailable. It was a case of the agent treating
"no file read" as the controlling fact, and a same-file comment
describing a real dependency as decoration.

<!-- dankg:depends target=deps_pilot.md#findings quote="Nothing in a plain read-and-run approach surfaces that gap." -->

That sharpens the pre-registered hypothesis rather than overturning
it. The prediction was that `make`'s own model has no representation
for a non-file dependency. An agent trusting `make` alone would miss
it as a result. The actual failure sits one level up. Even a comment
stating the dependency directly, in the exact file whose staleness is
in question, did not change the verdict. `make`'s silence carried
more weight than the prose sitting right next to it. That is a
stronger, worse failure than the tool-limitation story alone
predicts. It is not just that `make` had nothing to say about
`train`/`summary`. It is that having something to say, right there in
the same file, still lost to `make`'s answer.

The other half of the hypothesis held exactly as predicted. `counts`
and `ratios` were caught correctly, by the same mtime cascade
`dankg check`'s `deps=` graph uses, at the same tool-call cost (7,
tied with the original raw arm, well under literate's 9). `make`
closed exactly half the gap `deps_pilot.md` found. It did so for the
reason predicted: a prerequisite that is a real file, declared
correctly, propagates staleness correctly. A prerequisite that was
never a file at all does not exist for `make` to propagate. That held
even with a human-readable comment sitting two lines above the
command that ignored it.

# Caveats and next steps

- One trial has been run, for the `make` arm only. The `raw` and
  `literate` rows above are `deps_pilot.md`'s own numbers, reused as
  recorded there, not re-run here.
- N=1 for the `make` arm, the same as every arm in `deps_pilot.md`.
- The fixture built for this trial diverged from the plan above in
  one way. `train.sh` and `summary.sh` carry an inline comment
  describing the conceptual dependency directly, not the
  comment-free Makefile this file originally proposed. That turned
  out to matter for the finding, not just the setup -- see Findings.
  A stricter replication should either remove that comment, or run
  both versions and compare.
- Only the shell pipeline was rebuilt for this trial. `deps_pilot.md`
  found the same result in Python, SQL, and a mixed-tool pipeline. A
  fuller version of this pilot would need `make`, or a
  language-appropriate equivalent (`dbt` for the SQL pipeline),
  rebuilt for each. Only then could the finding above hold generally,
  rather than for shell specifically.
- No real sandbox. The agent had unrestricted `Bash` access to the
  fixture directory and, as explicitly permitted, performed a real
  `make` run that regenerated `normalize.txt`, `counts.txt`, and
  `ratios.txt` in place. The fixture's original "last recorded
  output" state no longer exists on disk after this trial.
