---
title: Agent dependency-staleness pilot
author: Daniel J. Okuniewicz
---
[pilot.md](pilot.md) found the literate graph's edge was narrow. It
only helped on questions with no ground truth in raw prose, like
whether two files reference each other both ways.
[feature_pilot.md](feature_pilot.md) found that implementation work
doesn't reliably trigger that edge at all. This pilot asks the
sharpest version of a relationship-shaped question: transitive
dependency tracing and staleness. That is the thing `deps=` and
`dankg check` exist for. The same fixture was rebuilt four times, in
shell, Python, SQL, and a realistic mixed-tool pipeline, to check
whether the result depends on language at all.

# Setup

DanKG's own self-hosted `src/` corpus has neither `deps=` nor
hash-tracked results. It tangles Rust; it doesn't `eval` it. This
pilot needed a purpose-built fixture instead of the branch's own tree.
An 8-stage pipeline was built across 5 markdown files (`raw.md`,
`clean.md`, `features.md`, `model.md`, `report.md`), forming a real
diamond-shaped DAG:

```
load_raw -> dedupe -> normalize -+-> counts  -+-> train -> summary
                        |         `-> ratios  -'
                        `-> (report.md's audit depends on dedupe only)
```

Two copies of the same pipeline, same edit applied to both:

- **literate**: real `dankg` named blocks (`name=`/`deps=`).
  `dankg eval --each --yes` ran once for real, to get genuine
  hash-tagged results. One script was edited afterward (see *The
  change*). The corpus is authentically stale as a result, not
  artificially so.
- **raw**: the same prose and the same underlying logic, with no
  `name=`/`deps=`/hash metadata. Plain paragraphs describe what each
  stage does, and, where a script's own file or table reads make it
  obvious, what it reads from. Each stage carries a plain "last
  recorded output" block, the real output from before the edit,
  instead of a hash.

**The change:** the `normalize` stage's logic gained one step that
reverses each output value's characters. Nothing was re-run afterward
in either copy.

**Ground truth**, from `dankg check` run directly on each literate
copy, not hand-derived: 5 of the 8 recorded results are stale.
`normalize` was edited directly. `counts` and `ratios` both read
`normalize`'s output. `train` depends on both `counts` and `ratios`.
`summary` depends on `train`. The other 3 are untouched: `load_raw`,
`dedupe`, and `audit`. `audit` depends on `dedupe`'s output, a
different branch of the DAG than the one that changed.

One fresh subagent per arm per pipeline, with the same constraints and
the same sandboxing caveat as `pilot.md` and `feature_pilot.md`.

# Task

Given the pipeline and the already-applied edit, classify all 8 stages
into "recorded output no longer trustworthy" or "safe to leave alone,"
with reasoning. Each raw arm was explicitly told it could execute the
scripts or queries itself to check.

# The four pipelines

The shell version ran first: temp files under `/tmp`, `sh` as the
`[lang.sh]` command, `| rev` as the edit. Two more were then built to
the identical DAG shape and the identical "numeric coincidence" trap.
The edit changes every value's content but preserves both the row
count and the summed length. Only the language changes between runs:

- **Python**: each stage reads and writes plain text files under
  `/tmp`, run via `python3 {file}`. The edit adds `[::-1]` to
  `normalize`'s list comprehension.
- **SQL**: each stage is a query against one shared DuckDB file, run
  via `duckdb {db} -f {file}`. The edit wraps `normalize`'s `upper()`
  call in `reverse()`.

A fourth, genuinely mixed pipeline was meant to come next: one
`deps=` chain crossing from a shell block to a Python block to a SQL
block. It could not be built that way. `dankg`'s `eval` planner
refuses a `deps=` chain that spans languages, on purpose. Its own
source states the reason directly:

> Every block in the result shares the language of `target`'s own
> occurrence, if it has one at all: mixing an unstated dependency in a
> different language into the one interpreter that will run the whole
> concatenated file is far more likely a mistake than an intentional
> multi-language pipeline, so it is refused rather than run.
> — `src/eval/plan.md`

A literal shell-to-Python-to-SQL `deps=` chain does not exist in this
tool. That is a deliberate design choice, not an oversight, and it
directly overturns a claim made earlier in this investigation, before
this fixture was attempted: that `deps=` "does not care what language
sits on either side of an edge." It does care. It refuses to cross that
line at all.

The realistic form a mixed-tool pipeline takes instead, and the form
most real pipelines already use, is a shell layer that calls other
tools internally. The fourth pipeline was built this way: every stage
declared `sh`, with three of the eight stages internally invoking
`python3` or `duckdb`. This satisfies `deps=`'s same-language rule
while still crossing real tool boundaries underneath, which is the
closest available proxy to the pipeline originally proposed.

In every pipeline, `train` and `summary` still have no file or table
reference at all, only hardcoded output and a sentence of prose
describing their place in the pipeline. This preserves the exact
failure mode the shell run was built to expose.

# Results

## Shell

| | Raw | Literate |
|---|---|---|
| Correct (5 stale / 3 safe) | **no — found only 1 stale, 7 safe** | **yes, exact** |
| Tool calls | 7 | 9 |
| Wall-clock | ~69s | ~42s |
| Method | Read all 5 files, then *executed* all 8 scripts to empirically diff current vs. recorded output | Traced `deps=` by hand across the 5 files, then ran `dankg check` to cross-verify |

## Python, SQL, and the mixed-tool pipeline

| Pipeline | Arm | Correct | Tool calls | Wall-clock |
|---|---|---|---|---|
| Python | raw | no — same 1-stale verdict | 7 | ~59s |
| Python | literate | yes, exact | 8 | ~36s |
| SQL | raw | no — same 1-stale verdict | 8 | ~115s |
| SQL | literate | yes, exact | 7 | ~37s |
| Mixed (`sh` calling `python3`/`duckdb`) | raw | no — same 1-stale verdict | 12 | ~80s |
| Mixed (`sh` calling `python3`/`duckdb`) | literate | yes, exact | 9 | ~44s |

Every raw arm reached the identical wrong conclusion by the identical
reasoning, across all four pipelines. Every literate arm reached the
exact ground truth, cross-verified against `dankg check`, in 7 to 9
calls each.

# Findings

This is the sharpest gap across all the pilots so far, and it holds
across four pipelines without changing shape. It is not about speed.
Three of the four raw arms used *fewer* tool calls than their literate
counterpart. It is about being wrong, in the dangerous direction, on
half the task, every time.

Every raw agent was genuinely thorough. Each one ran or queried the
pipeline itself rather than eyeballing a diff. Each one correctly
noticed that the edit doesn't change a row count or a summed length,
so `counts` and `ratios` still report the exact values they reported
before the edit. That is a true, non-obvious observation, and every
agent concluded from it that both stages were safe. That conclusion is
only true by numeric coincidence: reversal happens to preserve length.
`dankg`'s model doesn't try to prove semantic equivalence between old
and new output, the same reason `make` and `bazel` don't. It
conservatively invalidates anything whose source or transitive
dependency changed. "The number still happens to match" is not
something a pipeline can safely rely on holding under the *next* edit.
Every raw agent's standard for "safe" was strictly weaker than the one
that matters for a real pipeline. Nothing in a plain read-and-run
approach surfaces that gap. It looks like due diligence right up until
the coincidence breaks.

The second error is starker, and it repeated identically four times.
`train` and `summary` were marked "always accurate" because their
scripts or queries have no file or table reference at all. Neither a
reader nor a script-runner can find a dependency trail in the text.
The only place that dependency exists is a sentence of prose two files
away. Every raw agent read that prose and still missed the implication
two hops downstream. That is exactly what `deps=` is for. It makes a
dependency load-bearing and machine-checkable, instead of a claim
sitting in a paragraph, waiting for whoever reads it next to remember
it correctly.

The cross-language work also revises a theory from the navigation and
implementation pilots, twice over. Rust showed a weaker literate
advantage there, and the working guess was that Rust's own native
tooling, a compiler-checked module graph and rustdoc's intra-doc
links, was already covering some of the same ground. SQL is at least
as sophisticated a language as Rust in its own domain: strongly typed,
schema-enforced, with a real query planner. It still failed exactly as
badly as bare shell scripts. What plain shell, ad hoc Python, and raw
`duckdb` CLI usage share is not a lack of sophistication. It is the
lack of any conservative, dependency-based invalidation primitive, the
kind of thing `make`, `bazel`, or `dbt` provide and a bare interpreter
or database client does not. That, not general language maturity,
looks like the actual variable.

The second revision is about `deps=` itself. A mixed-language pipeline
was expected to be dankg's best case, on the theory that no single
native tool can see across a language boundary while `deps=` supposedly
does not care what language sits on either side of an edge. That theory
was wrong on its own terms: `deps=` does care, and refuses to cross a
language boundary at all. The mixed-tool pipeline actually tested here
worked around that by keeping every stage nominally `sh`. It is not
evidence about crossing a real `deps=` language boundary, because no
such boundary can exist to cross. What it does show is that shelling
out to different real tools underneath a uniform `sh` declaration
costs the raw arm even more manual verification than a single-language
pipeline did (12 tool calls, the most of any raw arm here) for the same
outcome: the identical wrong answer. That is one more data point in
the same direction, not a test of the sharper claim originally
proposed.

Net result across all four pipelines: 16 of 32 classifications were
wrong on the raw side, every one a false negative in the same two
places, every time. `dankg check` gave the exact answer every time, in
one call. Read next to `pilot.md` and `feature_pilot.md`, this is the
strongest evidence yet for the theory those two proposed. DanKG's
structural edge is specific to questions with no ground truth in raw
text. Transitive staleness is the sharpest version of that question
tested so far, and it does not depend on which language, or how many
languages, the pipeline happens to be written in underneath a single
`deps=` chain.

# Caveats and next steps

- N=1 per pipeline, one purpose-built fixture each, same as the other
  two files' caveats.
- Each fixture was designed to expose exactly this failure mode: a
  no-file-I/O logical dependency two hops downstream, plus a
  numeric-coincidence trap. It is a fair test of a real failure class,
  not a neutral random sample of "typical" pipeline questions.
- No real sandbox, same as `pilot.md` and `feature_pilot.md`.
- The raw arms' "numeric coincidence" reasoning is worth a second,
  adversarial fixture, one where the coincidence *doesn't* hold. That
  would check whether the same kind of agent catches its own
  overconfidence, or whether the false-safe verdict here was luck
  rather than a stable pattern.
- A genuine cross-language `deps=` chain cannot be built at all, given
  the constraint this pilot found in `src/eval/plan.md`. Anyone
  extending this file should not try to route around that constraint
  again; the mixed-tool pipeline here is the closest honest proxy
  available, and its result should be read as one more single-language
  data point, not as a test of a real language boundary.
- This finding fed directly into
  [architecture.md's open questions for the literate-database
  milestone](../architecture.md#open-questions-for-this-milestone): a
  relation's `Produces`/`Reads` edges are one scoped way to get a real
  cross-language dependency, without touching `deps=`'s own
  same-language rule at all.
