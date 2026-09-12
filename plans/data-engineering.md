# Data engineering with DanKG: the incrementality question

Status: implemented, Option B (`--if-stale`). See architecture.md's
own *Code evaluation*. Touched `eval/result.md` (the new `is_stale`,
shared with `check_cmd`), `eval/session.md` (`run_single`'s own
precheck, and `corpus_graph_if_needed`, factored out of `run_one`),
`cli.md`, and `main.md`'s `check_cmd` call site. The breadth of
"which existing approach solves this" survey below is unaffected;
only *Implementing the recommended approach*'s own gap is closed.

This came out of a conversation about using `dankg` to manage a data
pipeline (census ingestion, national rollups, maps) rather than a
literate codebase. The single weakness worth taking seriously is that
`dankg eval` has no built-in way to skip work that is already up to
date. This document lays out, honestly, which existing data
engineering approaches to that problem could be layered on `dankg` as
it stands today, which would need `dankg` itself to change, and which
turn out not to solve the problem they are usually cited for.

## The problem, precisely

`plan.rs` topologically sorts a target block's transitive `deps=`,
then `run.rs` concatenates those sources in order ahead of the target,
writes one temporary file, and spawns the configured command once,
every time.

<!-- dankg:depends target=../architecture.md#decision-11-execution-model quote="Deps prepended into one process." -->

Dependency side effects re-run on every eval. There is no persistent
state between separate `dankg eval` invocations, and no notion of
"this upstream block is already known-good, do not redo it." That is
a deliberate trade for reproducibility and for never needing a PTY or
a per-language state protocol -- it is not an oversight. But it means
a pipeline shaped like `fetch -> clean -> aggregate -> map` re-fetches
and re-cleans from scratch every time you want to look at a new map,
unless something outside this mechanism intervenes.

## Approaches that need nothing from `dankg`

These live entirely inside the block's own script. `dankg` spawns
whatever command is configured and hands it concatenated source; it
has no opinion about what that source does internally, so all three
of these work today, unmodified.

**Existence-check, Luigi's whole model.** A block's script checks
whether its declared output already exists and exits early if so.
`dankg` still pays the cost of re-invoking the interpreter and
re-concatenating the chain into a temp file every eval -- decision 11
does not change -- but the expensive part (the actual download, the
actual computation) only happens once. The weakness is Luigi's own
weakness: existence is not correctness. A stale-but-present file still
counts as done. `dankg` actually complements this rather than
inheriting the gap, because its own hash-tagged results already tell a
human when a block's *source* changed and the existence-check's
assumption ("nothing upstream moved") is no longer safe to trust.

<!-- dankg:depends target=../architecture.md#decision-12-results quote="Written back into the markdown, hash-tagged." -->

**Incremental logic inside the SQL, dbt's whole model.** A `sql` block
spawns DuckDB as a configured external command, so nothing stops its
own text from reading

<!-- dankg:depends target=../architecture.md#decision-16-database-engine quote="DuckDB, spawned as a configured command." -->

`INSERT INTO agg SELECT ... WHERE ds > (SELECT max(ds) FROM agg)`
instead of a full rebuild. `dankg` still re-runs the whole chain's
process on every invocation, exactly as it always does -- the
redundant work `dankg` itself cannot avoid gets absorbed by the
database engine instead, one layer down. Correctness of the
high-water mark, late-arriving rows, all of it, is entirely the SQL
author's responsibility, the same as it is in real dbt usage.

**Partition pruning, the Spark/Iceberg/Delta model.** Write outputs
partitioned (Parquet by state and year, say) and let the reading tool
filter by partition. `produces=file:PATH` and `reads=file:PATH`
declare the shared artifact path both sides believe in; nothing stops
`PATH` from naming a partitioned directory, and nothing about `dankg`
understands partitions one way or the other.

<!-- dankg:depends target=../architecture.md#file-dependencies quote="declare the artifact both sides believe they share" -->

The incrementality lives entirely in the storage layout and the
reading tool's own pruning, orthogonal to anything `dankg` tracks.

## Approaches that need a thin wrapper around what `dankg` already has

**Hash-chain skip with a lockfile, DVC's actual model.** `dankg check`
already computes, per block, whether the concatenated source hash of
its transitive `deps=`/`xdeps=` chain still matches what was last
recorded, and separately verifies a `produces=`/`reads=` file
contract. That is real staleness detection, not a guess, and it is
already exposed: one line per stale block, in a fixed, greppable
shape.

<!-- dankg:depends target=../src/main.md#check_cmd quote="stale: {rel_path} `{}`" -->

A wrapper -- a Makefile, a `just` recipe, a shell loop -- can run
`dankg check .`, collect the `stale:` lines it prints, one per stale
block, and call `dankg eval FILE --block NAME --yes` only for the
names on that list, leaving everything else untouched. That reuses
two pieces `dankg` already has (a truth-teller and an executor)
without `dankg` itself ever making the skip decision. The gap: there
is no `--only-if-stale` flag on `eval` itself, so the decision has to
live in the wrapper, and it only operates at the granularity `dankg`
already tracks -- a whole named block, never a partition inside one.

**Version-based memoization, Dagster's asset model.** Structurally the
same wrapper as DVC above. Dagster's "code version" is a first-class,
author-declared field; `dankg`'s equivalent already exists implicitly,
as the concatenated source hash a result is tagged with. There is no
separate implementation path here -- the DVC-shaped wrapper already
covers this, under a different name.

## An approach that would need `dankg` itself to change

**Content-addressed build caching, Bazel/Nix's actual model.** Real
skip-and-reuse at this level means hashing the full input closure
(including the toolchain), storing outputs keyed by that hash, and
letting `run.rs` decide, on its own, not to spawn anything. That
crosses a real line: every approach above leaves the *skip* decision
outside `dankg`, in a wrapper a human wrote and can inspect. This one
puts it inside the tool. `dankg eval` is deliberately never automatic
\-- it prints a plan and asks before running anything specifically so
a human is the one deciding what executes.

<!-- dankg:depends target=../architecture.md#decision-9-eval-trigger quote="`dankg eval` only; serve mode deferred." -->

A silent, automatic "decided not to run this" is a materially
different trust surface than a wrong plan a reader can see and
reject. If this were ever built, the plan should keep saying, every
time, exactly which blocks it is skipping and why -- never a fully
invisible skip.

## Approaches that look like caching but solve a different problem

**Manual resume, Metaflow's actual model.** Metaflow does not cache
automatically. Every run re-executes every step from scratch; the
mechanism that exists instead is `resume run-id step-name`, which
restarts a flow from a named step, reusing a *specific prior run's*
artifacts for everything before it. This does not map onto `dankg` at
all, because decision 11 has no notion of one long-lived run with
checkpoints to resume from -- every `dankg eval` is already stateless
from scratch, by design, so there is no "run identity" for a resume
command to point at in the first place.

**Identity-based task state, Airflow's actual model.** Airflow's core
scheduler tracks task-instance success keyed by
`(dag_id, task_id, execution_date)` and will not automatically re-run
something already marked successful for that slot, regardless of
whether the code or the inputs changed since. That is caching by
*schedule identity*, not by content. `dankg`'s own result tag is
already stronger on this exact axis: it is a hash of the block's
actual source, so `dankg check` correctly reports staleness the
moment that source changes, something Airflow's own default behavior
does not do on its own.

## What this means for `dankg`

Of everything surveyed, the DVC-shaped wrapper -- an existence-check
inside expensive scripts, plus a small script that greps
`dankg check`'s existing per-block stale lines and drives conditional
`dankg eval --block NAME --yes` calls -- is achievable today, with
zero changes to `dankg`, and covers most of what a census-style
pipeline would actually need.

The one gap worth building, if this direction is ever pursued for
real: `check_cmd`'s stale-block list is an `eprintln!` text line today,
not a documented, versioned interface. Formalizing that -- a
`--format json` for `check`, or a narrower `dankg eval --if-stale`
flag that reads the same staleness computation `check` already does --
would remove the "parsing stderr text" fragility without touching
decision 11's execution model or decision 9's never-automatic
principle at all. That is a far smaller, safer change than a Bazel/Nix
style cache, and it is where I would start.

## Implementing the recommended approach

### The wrapper itself, no `dankg` changes

Write blocks with `produces=file:PATH`/`reads=file:PATH` so `check`
has a file contract to verify, not just a source hash. A driver
script -- a Makefile, a `just` recipe, plain shell -- runs
`dankg check .`, parses the `stale:` lines out of its stderr into
`(file, block)` pairs, and calls
`dankg eval <file> --block <name> --yes` only for names on that list.

No topological logic is needed in the wrapper, for a reason worth
being precise about: a block's own recorded hash already

<!-- dankg:depends target=../architecture.md#results quote="covers the concatenated source of the target's whole chain" -->

covers its entire chain, not just its own text. If `map` does not show
up in the stale list, that already means nothing anywhere in its
chain -- `fetch`, `clean`, `aggregate`, `map` itself -- has changed
since it last ran. Checking only the leaf blocks a reader actually
cares about is sufficient; there is no need to independently track
per-stage staleness in the wrapper at all. The one real fragility:
that stale-line format is an `eprintln!` string literal in
`check_cmd`, not a documented contract, so a future wording change
in `dankg` silently breaks the grep.

### The gap: two concrete options

**Option A: `--format json` on `check`.** `Command::Check` currently
carries only `paths` and `cache`; this needs a new field, flag
parsing mirroring the `--format` machinery `graph` already has, and,
inside `check_cmd`, the four staleness/failure branches that today
just call `eprintln!` collected into a `Vec` of structured entries
instead, serialized as JSON when the flag is set.

<!-- dankg:depends target=../architecture.md#decision-13-cli-shape quote="JSON is a first-class, testable surface from day one." -->

The bigger cost is everything around that one change: a new
output-format abstraction for a command that has never needed one,
golden JSON fixture tests matching `graph`'s own existing pattern, and
updating every test that constructs `Command::Check { .. }` today for
the new field.

**Option B: `--if-stale` on `eval`.** Smaller and more surgical.
`check_cmd` already calls `plan::plan_for_index` plus
`result::expected_hash`/`xdep_hashes` to decide one block's
staleness; that logic moves out into a function `check_cmd` and a new
`eval` code path both call, rather than staying inlined where it is
today. Given `--if-stale`, `eval` calls that shared check first: not
stale, print "already up to date" and exit `0` before the plan, the
prompt, or any spawn ever happens; stale, proceed exactly as `eval`
already does. No new output format, no new serialization surface --
one function extracted, one early-exit branch added.

### Recommendation

Option B. It touches less surface, does not need a whole format
abstraction built for a single consumer, and it removes the wrapper's
dependency on parsing stderr text entirely. The wrapper collapses to
one line per leaf block, unconditionally:

```
dankg eval census.md --block map --yes --if-stale
```

no `check`, no grep, no parsing step in between at all.
