# Every hash is a source hash, and the data is what moves

## Context

This plan comes out of a question asked of the design record rather
than a bug: what does dankg lack to be a knowledge grapher for data
analysis, not only for a literate program. The answer named five
gaps. One of them already has a plan of its own.

`plans/plan-graph-edge-coverage.md` owns that one. The execution DAG
lives in `eval::plan`. `EdgeKind` has no variant for it. The graph
therefore cannot answer what feeds a table. That plan is a
prerequisite for this one rather than a part of it.

<!-- dankg:depends target=plan-graph-edge-coverage.md#the-graph-shows-a-fifth-of-what-the-corpus-already-declares quote="The information exists, is resolved, is displayed, and is not in the graph." -->

Four items are left. One is a real gap: nothing dankg hashes has
ever been data. The others are ceilings. Eval cannot plan a corpus.
Nothing records a run. A block may name only one artifact. Chains
that share no block still run one after the other.

## What the review established

**The hash is right. It is also blind.** Decision 39 keeps a captured
row count out of the staleness hash. Its rationale still holds.

<!-- dankg:depends target=../architecture.md#decision-39-row-count-stays-out-of-the-hash quote="Every hash `dankg` computes is a source hash, never a content hash" -->

The consequence for a pipeline is that the thing changing most often
is the thing dankg never looks at. A monthly extract lands in a CSV.
No block's source moved. `check` reports every result fresh,
correctly by its own rules, and wrongly about the pipeline.

**A declared input is not an artifact.** `reads=file:PATH` is checked
as path equality against a dependency's own `produces=`, plus that
dependency's own verified hash. Both halves are source. A file
nothing in the corpus produces falls outside the check entirely.
That file is the normal case for analysis work: a vendor drop, a
database export, an extract someone downloaded by hand. A pipeline's
own root input is exactly what `reads=` cannot describe.

**The record already names the artifact half, and defers it.**

<!-- dankg:depends target=../architecture.md#open-questions-file-dependencies quote="Stat-and-hash an artifact is close enough to executing something that it deserves its own decision" -->

That deferral is about a file dankg itself wrote. Whether an artifact
still matches what its block last wrote needs a stat of dankg's own
output. An input is the other side of the same sentence. Nothing
dankg wrote is involved. Nothing is executed.

**Decision 19's rationale has expired.** It reads "No cross-file DAG
to walk".

<!-- dankg:depends target=../architecture.md#decision-19-eval-scope quote="No cross-file DAG to walk" -->

`deps=other.md#name` resolves across files today. `xdeps=table:NAME`
resolves corpus-wide through `graph::query::find_producer`. The data
model crosses files. `plan_all` and `plan_each` are still scoped to
one file, which is the scope `--all` and `--each` inherit from them.

**A run leaves a hash and a transcript, and nothing else.**
`render_marker` records the result hash, a failed flag, and the
`produces=`/`reads=` lists a `db=` run inferred. When the run
happened, how long it took, what it exited with: none of it is
recorded anywhere. Decision 11 makes every eval stateless from
scratch, which is a property worth keeping. It does not follow that
a run should leave no trace.

**Execution is serial.** The only `thread::spawn` pair in `run.rs`
drains stdout and stderr, so that a chatty process cannot deadlock
the timeout poll. Two chains sharing no block still run in sequence.

## What this reuses

**`expected_hash`'s own byte sequence.** It already folds each
verified dependency hash in beside the concatenated chain source and
the resolved command template. An input hash is one more field in
that sequence, not a second staleness mechanism.

**`xdep_hashes`'s verify-then-trust rule.** A referenced block's
stored hash is trusted only once `verified_hash` recomputes it. That
rule is what carries staleness across a boundary rather than
stopping at it. An input needs the simpler half of it. There is no
stored value to distrust, only bytes to hash.

**`is_stale`, the one comparison.** `main::check_cmd`'s per-block
loop and `session::run_single`'s `--if-stale` precheck both call it.
An input hash folded into `expected_hash` reaches both callers with
no second code path to keep in step.

**`plan_all`'s merged topological pass.** It already orders one
file's targets as a subsequence of one topological order. Widening
what it reads from one file's blocks to the corpus's changes its
input, never its computation.

**`render_marker`/`parse_marker`'s optional fields.** `produces` and
`reads` are each omitted entirely when empty, the same optionality
`failed` already has. A run's own recorded fields take that shape.

**The cache's own version rule.** An entry from another `VERSION` is
a miss rather than an error. A bump needs no migration path.

**Decision 8's placeholder treatment.** An `inputs=` path that is not
there gets what a dangling link already gets.

## What's new

### A declared input is hashed

`inputs=file:PATH` names a file the block reads and nothing in the
corpus produces. Its content hash folds into `expected_hash` beside
the verified dependency hashes. A block whose input moved is stale.
`check` names which input moved.

This is a new attribute rather than a widened `reads=file:`.
`reads=` names a handoff between two blocks in the corpus. Its whole
check is that the two sides agree. An input has no producer in the
corpus by definition. Collapsing the two would make a missing
producer indistinguishable from a root input, which is the one
distinction a pipeline's own boundary rests on.

None of this crosses decision 9.

<!-- dankg:depends target=../architecture.md#decision-9-eval-trigger quote="`dankg eval` only; serve mode deferred." -->

Reading a declared file's bytes is what `index::load`, `fmt` and
`check` already do on every run, over every file in the corpus.
Hashing one more named file executes nothing. What decision 9 refuses
is dankg deciding on its own to spawn something.

**The corpus is an input too.** `plan-graph-edge-coverage.md` wants a
way for a corpus-derived measurement to declare its own input and
calls that mechanism deliberately vague.

<!-- dankg:depends target=plan-graph-edge-coverage.md#a-corpus-derived-measurement-can-declare-its-input quote="The general fix is a way for a block to declare the corpus as its input" -->

`inputs=file:` is the narrow half of it, for a block reading one
file. A directory-valued `inputs=dir:` is the rest. That is the same
mechanism pointed at a walk instead of a file. It stays out of scope
here on purpose, since the file case is what a pipeline needs first.

### Eval plans a corpus

`plan_all` and `plan_each` take the corpus's blocks rather than one
file's. `dankg eval . --all` becomes legal and runs one merged
topological pass over every file's targets. Resolution is unchanged.
`deps=other.md#name` and `xdeps=table:NAME` already reach across
files. Nothing new is invented for them to reach through.

`visit`'s own `stack` catches a cycle spanning two files exactly as
it catches one inside a single file. The detection is unchanged too.

The prompt still prints every block it will run, in order, and still
asks before running any of them. A corpus-wide run makes that plan
longer. It does not make it automatic.

### A block may produce more than one artifact

`produces()` and `reads()` hand back their whole raw value unsplit.
`deps()` and `xdeps()` comma-split theirs. Making the first pair
match the second is the whole of the change in `md/mod.md`. Each
value then becomes its own artifact node, which is what the cache
version bump pays for.

### A run records what it did

The result marker grows two optional fields: when the run started,
and how long it took. Both are omitted when absent, the same way
`failed` is omitted.

The start time is recorded as seconds since the Unix epoch, straight
from `SystemTime`. Formatting a calendar date from that by hand is a
routine this plan does not need to own. Decision 1 rules out the
crate that would do it instead.

The duration is what `run`'s own timeout poll already tracks.
`Output` carries it out rather than dropping it on the floor.

Neither field enters `expected_hash`. Decision 39's rule is
untouched: a recorded fact about a run is not source.

### Independent chains run at once

`plan_all` already returns a sequence of chains. Two chains sharing
no block can run at the same time. `[eval] jobs` bounds how many,
defaulting to 1. Nothing changes for a reader who never asks.

Results write back in plan order whatever order they complete in.
That is the one property this cannot trade away. A corpus whose
written results depended on scheduling would make `check` report
differently from one run to the next.

## Build order

1. `inputs=file:`. The one new primitive, and the only item here
   that changes what `check` means.

2. Multi-valued `produces=`/`reads=`. It shares item 1's cache
   version bump. Landing the two together spends one bump rather
   than two.

3. The corpus-wide plan. It needs nothing from items 1 and 2. It is
   what makes item 1 worth having beyond one file's scope.

4. Run metadata. Independent of everything above it.

5. Parallel chains. Last on purpose. Its failure mode is making a
   wrong plan wrong faster.

## Why not the alternatives

**Content-addressed caching, Bazel's model.** `data-engineering.md`
surveyed it and declined it.

<!-- dankg:depends target=data-engineering.md#an-approach-that-would-need-dankg-itself-to-change quote="letting `run.rs` decide, on its own, not to spawn anything" -->

That survey's reason applies here unchanged. This plan leaves the
skip decision where the survey left it. `--if-stale` already asks
`check`'s own question before spawning. A reader sees the answer it
got. An input hash makes that answer correct about data. It never
moves the decision inside the tool.

**An mtime instead of a content hash.** Cheaper, and wrong in both
directions. A touched file with identical bytes would rerun the
pipeline. A file restored from a backup with an older timestamp
would not.

**Schedule identity, Airflow's model.** Declined in
`data-engineering.md` already, on the grounds that a source hash is
stronger on exactly that axis. Nothing here weakens it.

**A wrapper outside dankg.** A driver script can hash an input file
on its own. It cannot fold that hash into a chain's recorded hash.
`check` therefore keeps calling a block fresh whose input moved, and
what `check` says is the one thing a wrapper cannot fix.

**Hashing every file a block happens to open.** That needs a trace of
the running process, which means running it. Declaration is the
cheaper contract. It is also the contract `produces=`/`reads=`
already chose.

## What this explicitly does not do

- Change decision 39. Captured output, row counts and run metadata
  all stay out of the hash.
- Change decision 9. Nothing here spawns what it was not asked to
  spawn.
- Stat or hash a produced artifact. That stays the open question it
  is today.
- Add a scheduler, a trigger, or a partition dimension.
- Add a dependency, per decision 1.

## Open questions

1. Whether a missing `inputs=file:` path fails `check`'s exit code
   or stays advisory. An unresolved link is fatal and a prose
   dependency is not. A missing root input is closer to the link,
   since the pipeline cannot run at all without it. That argues
   fatal. It is left open because a half-written note naming an
   input nobody has fetched yet is a real drafting state.

2. What `inputs=dir:PATH` would hash. Every file's bytes is the
   obvious answer and the expensive one. The walk's own node set is
   cheap and misses an edit inside a file.

3. Whether a run's recorded time belongs on the result marker or
   under `.dankg/`. The marker sits in the document, which is where
   every other recorded fact lives. A per-run history is not a
   document fact, though. The marker has no room to grow one.

4. Whether `[eval] jobs` ever defaults above 1, once results are
   proven to write back in plan order.

5. The cache version this lands on. `plans/plan-graph-edge-coverage.md`
   was expected to take 6, leaving 7 for this plan. It took both: 6 for
   the `quote` field and the two `EdgeKind` variants, then 7 for the
   rows that actually produce them, once a clean CI checkout showed that
   a version-6 entry decodes as fresh while carrying no marker rows at
   all. This plan therefore takes 8.

   The marker below is what caught the change, which is the mechanism
   working rather than a number going stale unnoticed. It tracks the
   live constant. A bump landing before this plan does will therefore
   trip `dankg check` again rather than leave 8 here to rot.

<!-- dankg:depends target=../src/graph/cache.md#graph-cache quote="const VERSION: u32 = 7;" -->

## Critical files

- `src/md/mod.md` -- `inputs` in `KNOWN_ATTRS`, an `inputs()`
  accessor beside `produces()`/`reads()`, and the comma-split both
  of those two grow.
- `src/eval/result.md` -- `expected_hash` folds the input hashes in;
  `render_marker`/`parse_marker` grow the run's own two fields.
- `src/eval/plan.md` -- `plan_all`/`plan_each` over a corpus rather
  than one file, and `check_file_deps` learning the root-input case.
- `src/eval/session.md` -- `run`'s own target scope, and the bounded
  parallel runner.
- `src/eval/run.md` -- `Output` carries out the elapsed time the
  timeout poll already measures.
- `src/graph/build.md` -- an input is a node, the way decision 69's
  artifact already is.
- `src/graph/cache.md` -- the new rows, and the version bump.
- `src/config.md` -- `[eval] jobs`.
- `src/main.md` -- `check_cmd` grows one more pass, for an input
  whose file is gone.
- `architecture.md` -- the decisions, *Open questions (File
  dependencies)* updated where it defers the input half, and
  decision 19's own rationale corrected rather than left standing.

## Verification

1. The edit loop in `CLAUDE.md`, then `dankg fmt --check` and
   `dankg check .`.

2. Unit tests: an input's changed bytes make its block stale; the
   same bytes rewritten do not; a missing input is reported; a
   two-valued `produces=` becomes two artifact nodes; a corpus plan
   orders two files' targets correctly; a cycle spanning two files
   is refused the way one inside a single file already is.

3. A cache round-trip test, and a rejected older-version entry.
   `tests/index.rs`'s own
   `graph_output_does_not_depend_on_the_cache`
   is what catches a missed bump.

4. The corpus goldens re-blessed wherever the fixture corpus grows
   an input node of its own.

5. One check no unit test covers, driven by hand: a two-file corpus
   whose first block declares `inputs=file:` on a CSV. Rewrite the
   CSV. Confirm `check` reports the downstream block stale. Confirm
   `eval --if-stale` runs it. Confirm a second `eval --if-stale`
   skips it.

6. `Cargo.lock` and `cargo tree` unchanged, the same bar
   `plans/eval-custom-plan.md` holds itself to.
