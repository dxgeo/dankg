---
title: Agent reasoning-quality pilot
author: Daniel J. Okuniewicz
---
[pilot.md](pilot.md) and [deps_pilot.md](deps_pilot.md) found a real
literate-graph advantage on relationship-shaped questions: whether two
files cite each other both ways, and whether an edit leaves a recorded
result stale. [feature_pilot.md](feature_pilot.md) ran the same
comparison on a self-contained implementation task instead, and found
the literate form adds a real tax and no offsetting benefit.

<!-- dankg:depends target=feature_pilot.md#findings quote="the literate form adds a real tax and no offsetting benefit." -->

That result leaves two things untested. First, `feature_pilot.md`'s
task was chosen to sit next to one named decision, an entry in a list
every careful agent already checks on its own. A real feature more
often touches a fact scattered across several files, with no such list
to check. Second, having the graph available and knowing to query it
are not the same thing.

<!-- dankg:depends target=pilot.md#findings quote="The literate corpus's efficiency advantage depends on the agent knowing to query the graph instead of grepping it like source." -->

This pilot tests both gaps on one feature-implementation task, and
grades reasoning quality directly instead of inferring it from
pass/fail. `feature_pilot.md`'s own caveats named a `deps=`-chain
feature, added to that same file, as the natural next task.

<!-- dankg:depends target=feature_pilot.md#caveats-and-next-steps quote="That is the natural next task for this file, not a new pilot." -->

This pilot varies a second factor that file did not: whether the agent
is told to query the graph before finishing. It also adds blind rubric
grading, which neither prior pilot attempted. Both extensions change
the shape of the experiment enough to stand on their own instead.

# Setup

A purpose-built fixture keeps the ground truth fixed and exact,
instead of drifting with the real `dankg` corpus the way a live number
would. It describes a small, fictional note-taking CLI called `jot`,
across five files:

- `store.md` -- how notes are laid out on disk, and how `list` scans
  the store directory.
- `tag.md` -- how `--tag` filtering reads a note's first line.
- `init.md` -- how `init` creates a fresh store directory.
- `readme.md` -- a short usage guide.
- `notes.md` -- scattered design commentary, the fixture's stand-in
  for `architecture.md`.

Seven prose claims sit across those five files. Four are
*load-bearing*: each states a fact that becomes false once the task's
feature lands. Three are *decoys*: each mentions the changed behavior,
or something next to it, without asserting anything the change
touches. This is the same distinction `pilot.md`'s task B drew between
a real reciprocal link and an ordinary one-way mention, applied to
implementation instead of navigation.

**Load-bearing claims:**

1. `store.md`: "`list` scans only the files directly inside the store
   directory; it does not descend into subdirectories." The one claim
   describing the exact behavior the task changes. Any agent editing
   `list` correctly has to touch this sentence, or a near-identical
   replacement, directly.
2. `notes.md`: "Because `list` only scans the store's top level,
   moving a note into a subdirectory is a deliberate way to archive it
   out of `list`'s output without deleting it." False the moment `list`
   also scans one level of subdirectories, since archiving no longer
   works that way.
3. `notes.md`: "A note count reported by `stats` equals the number of
   files `list` would enumerate, since both walk the same top-level-only
   directory." False unless `stats` is changed to match `list`, which
   the task does not ask for. This is a second-hop consequence, the
   same shape as `deps_pilot.md`'s `train` and `summary` misses.
   `stats` must count independently of `list` in the fixture's
   implementation for this claim to be a real second-hop test; an
   implementation where `stats` simply calls `list` and counts its
   output makes the equivalence hold by construction, no matter how
   `list`'s scan depth changes, and the claim only needs a wording fix,
   not a real check. Trial 1 of the raw arm caught this empirically and
   was discarded and re-run once `stats` was made independent.
4. `tag.md`: "Because `list` never descends into subdirectories, a
   tag-filtered listing and an unfiltered one always report counts
   drawn from the same flat set of files." False for the same reason
   as claim 3, in a second file.

**Decoy claims:**

5. `tag.md`: "Tag matching reads only a note's first line, so a
   `#tags:` header written later in the file is ignored." Untouched by
   the change.
6. `init.md`: "`init` writes its config file into the store directory
   alongside future notes." Untouched.
7. `readme.md`: "Every note lives in exactly one file; `jot` never
   merges two notes together." Untouched, and unrelated enough to work
   as a control the way `pilot.md`'s task C did.

Two copies of the fixture, same prose in both:

- **raw**: plain `.md`/`.sh` files, no `dankg:depends` markers, no
  `dankg` binary, no `.dankg` config.
- **literate**: the same five files, each load-bearing claim (1
  through 4) carrying a real `dankg:depends` marker that targets
  `store.md`'s scan-depth sentence, plus a `.dankg` config and a copy
  of the `dankg` binary. Decoys carry no marker, correctly, since
  nothing depends on them.

Three arms, one fresh subagent per trial, three trials per arm (nine
runs total):

| Arm | Environment | Instruction |
|---|---|---|
| raw | raw fixture | task only |
| literate-unprompted | literate fixture | task only |
| literate-prompted | literate fixture | task, plus: run `dankg check .` before finishing and address anything it reports |

The same sandboxing caveat as every prior pilot applies: "only work
inside this directory" is an instruction, not an enforced boundary.

# Task

Every arm receives the same functional request:

1. Change `list` so it also scans notes one level of subdirectories
   deep under the store directory, in addition to the top level it
   already scans.
2. Leave `tag` filtering behavior otherwise unchanged.
3. Add a test covering a note placed one level down.
4. Leave the fixture's existing tests passing.

Nothing in the task mentions `notes.md`, `stats`, or any claim above.
Finding and addressing them, if an agent does, is emergent behavior,
not a followed instruction.

# Grading

Three separate measures, computed independently:

**Functional correctness.** A held-out directory layout, never shown
to any agent, checks `list` and `tag` against a known-correct output.
Pass or fail, the same method `feature_pilot.md` used.

**Reconciliation precision and recall.** Diff each trial's final state
against the seven-claim key above. A claim counts as *touched* if the
agent edits its sentence, or names and describes that specific claim
as needing review. A generic line naming the file but not the claim
does not count; trial 1 of the raw arm needed this distinction to be
made explicit, and it is stated here in the strict form that trial's
own results support. Recall is touched load-bearing claims over four.
Precision is touched load-bearing claims over all touched claims. This
step is mechanical, checked against a pre-registered key, and needs no
blinding.

**Blind process rubric.** A fresh grading agent reads each trial's
tool-call transcript and final diff, with the arm label stripped, and
answers one question on a 1-to-5 scale: before concluding the task was
complete, did the agent look for consequences of its change outside
the file it edited, and how systematically? The grader sees the
fixture but never the arm label or the reconciliation score.

Alongside the three graded measures, each trial also records its tool
call count and total token usage, the same overhead accounting
`feature_pilot.md` did with tool calls and wall-clock time. This is
cost, not a grade. A cheap wrong answer is still wrong, and an
expensive right one is still right, but the tax either arm pays is
part of the question this pilot exists to answer.

Comparing the blind rubric score against the mechanical reconciliation
score, after grading, checks the thing this pilot exists to check:
whether an independent read of a transcript notices better reasoning
in the same places the precision-and-recall numbers do, or whether it
tracks something else -- confidence, verbosity, tool-call count -- that
correlates with the literate arm without being caused by it.

# Results

**Calibration.** The first raw-arm run predates the `stats` fix. It
edited all four load-bearing claims. Every functional check passed.
Claim 3 only needed a wording fix, though, not real reasoning:
`stats` inherited `list`'s new scan depth for free, since it called
`list` internally at the time. That run is discarded. The fixture
description above already reflects the fix. Every count below is
against the corrected fixture.

| Arm | Trial | Functional | Recall | Precision | Blind rubric | Tool calls | Tokens |
|---|---|---|---|---|---|---|---|
| raw | 1 | pass | 2/4 | 2/2 | 3/5 | 10 | 35,703 |
| raw | 2 | pass | 1/4 | 1/1 | 2/5 | 12 | 36,324 |
| raw | 3 | pass | 4/4 | 4/4 | 4/5 | 14 | 37,647 |
| literate-unprompted | 1 | pass | 4/4 | 4/4 | 5/5 | 24 | 57,040 |
| literate-unprompted | 2 | pass | 4/4 | 4/4 | 4/5 | 17 | 40,018 |
| literate-unprompted | 3 | pass | 4/4 | 4/4 | 3/5 | 18 | 44,149 |
| literate-prompted | 1 | pass | 4/4 | 4/4 | 4/5 | 19 | 42,980 |
| literate-prompted | 2 | pass | 4/4 | 4/4 | 5/5 | 16 | 43,233 |
| literate-prompted | 3 | pass | 4/4 | 4/4 | 4/5 | 19 | 41,498 |

Trial 1's raw-arm agent read all five docs before editing any code. It
then edited only `jot.sh` and `test_jot.sh`. It left every doc
untouched. Its final report, though, explicitly quoted and correctly
flagged two of the four load-bearing claims: `store.md`'s scan-depth
sentence and `tag.md`'s flat-set claim. It also read `notes.md`, which
holds the other two claims. Its report added one generic line:
`notes.md` "also describes old behavior," with neither claim named. A
direct check confirmed the miss is real, not just an unquoted claim:
on a store with one top-level note and one note one level down, `list`
now reports 2. `stats` still reports 1.

That generic line is what forced the stricter reading of *touched*
now written into the Grading section above. "Explicitly flagged it as
needing review" was first written to allow either a specific claim or
a general one. A lenient reading would credit `notes.md`'s generic
mention against both of its claims, putting this trial's recall at
4/4 instead of 2/4. The strict reading tracks what actually got
caught: the agent's own words, and a direct functional check, agree
that the `stats`/`list` divergence went unnoticed.

Trial 2 caught less. Its agent also read all five docs before editing.
It also left every doc untouched. Its closing remark named three files
at once -- `store.md`, `tag.md`, and `notes.md` -- as now describing
`list` as strictly top-level-only. That description matches
`store.md`'s own claim exactly. Claim 1 counts as touched on that
basis. It does not match `tag.md`'s or `notes.md`'s claims. Those are
consequences of the same fact, not restatements of it. `tag.md`'s
claim is about `tag` and `list` no longer sharing a file set.
`notes.md`'s claims are about archiving depth and the `stats` count.
Neither got named specifically. Recall is 1/4. The same `stats`/`list`
divergence trial 1 missed went unnoticed again, unread and untested.

Two trials, two different failure shapes on the same task and the same
fixture. Trial 1 named two specific claims correctly and missed two.
Trial 2 named one. Both agents read every doc. Neither ran or noticed
the `stats` count. Two trials is not enough to call this a pattern. It
is enough to say the raw arm's reconciliation quality is not a fixed
number. It moved by half the load-bearing claims between two runs of
the identical prompt against the identical fixture.

Trial 3 caught everything. Its agent read all five docs before
editing, like the other two. This time it edited `store.md`, `tag.md`,
and `notes.md` directly, naming each file's actual claim rather than
gesturing at it. `notes.md`'s rewrite is the first to get the
`stats`/`list` divergence right. `stats` still counts only the top
level. Its count can now run lower than `list`'s. All four
load-bearing claims were edited correctly. No decoy was touched.
Recall and precision are both 4/4.

**Raw arm summary.** Three trials complete the raw arm. Recall moved
2/4, 1/4, 4/4 across them, on the identical task and fixture.
Precision held at a perfect 2/2, 1/1, 4/4 every time -- whatever the
raw arm missed, it never invented a false consequence or edited a
decoy. The blind rubric moved with recall: 3, 2, 4. Every trial read
all five docs before touching code. Reading was not the variable. What
varied was whether that reading turned into a named, specific claim in
the final report or code, versus a gesture at a whole file. Tool calls
ranged 10 to 14. Tokens ranged from about 35,700 to 37,600, a narrow
band next to the wide swing in recall.

Trial 1 of the literate-unprompted arm read every doc, then found the
`dankg` binary on its own and ran `./dankg --help` and `./dankg check .` before making any edit. It edited all four load-bearing claims
correctly, including both `notes.md` claims and the `tag.md` claim,
and kept every `dankg:depends` quote in sync with `store.md`'s new
wording. Partway through, it misapplied `dankg fmt`, a markdown
formatter, directly against `jot.sh` and `test_jot.sh`. That corrupted
a real line of shell logic. It caught the corruption itself, diffed
against a saved copy, and restored both files exactly before
finishing. Recall and precision are 4/4.

Trial 2 read every doc, then ran `./dankg check .` as a baseline
before editing, the same as trial 1. It edited all four load-bearing
claims and kept every marker quote in sync. No decoy was touched.
Recall and precision are 4/4. It made no misstep with `dankg fmt`.

Trial 3 followed the same pattern: read every doc, ran `./dankg check .` before editing, edited all four load-bearing claims, and kept every
marker quote in sync. It also added a test past the task's own
minimum, confirming a note two levels down still does not appear in
`list`'s output. Recall and precision are 4/4.

**Literate-unprompted arm summary.** All three trials scored a clean
4/4 on both recall and precision. All three found the `dankg` binary
and ran `./dankg check .` before editing, without being told it
existed. `pilot.md`'s task A found a literate agent that stuck to
grep and never touched the graph, on a read-only question. This
pilot's own intro named the open question that result left: is
querying the graph automatic, or does it depend on the agent knowing
to reach for it? On this feature-implementation task, with this
fixture, it was automatic. Three agents, three unprompted `dankg check` calls, zero prompting.

The blind rubric did not track that uniformity. It scored 5, 4, 3
across three trials whose reconciliation outcome was identical. That
is the sharpest instance yet of the split the Grading section set out
to find. An independent read of the same kind of evidence, on the
same underlying result, produced three different impressions.

Cost also moved. Tool calls ran 17 to 24, against the raw arm's 10 to
14\. Tokens ran about 40,000 to 57,000, against the raw arm's roughly
35,700 to 37,600. The literate arm paid a real tax here too, the same
shape `feature_pilot.md` found on a self-contained task. The
difference is what the tax bought. On `feature_pilot.md`'s task, it
bought nothing. Here, it bought a perfect reconciliation score three
times in a row, against a raw arm that ranged from 25% to 100% recall
on the identical prompt.

Trial 1 of the literate-prompted arm read every doc, edited `jot.sh`,
then ran `./dankg --help` and `./dankg check .` before touching any
doc -- the explicit instruction this arm adds. It edited all four
load-bearing claims correctly, re-ran `dankg check .` clean, and also
ran `dankg fmt --check` on all five docs as an extra pass. Recall and
precision are 4/4.

Trial 2 read every doc, edited `jot.sh` and all three doc files in one
pass, then ran `dankg check .` once and got a clean result
immediately, with no iteration needed. It closed by reading back all
five changed files as a final review. Recall and precision are 4/4.

Trial 3 ran `dankg check .` first, as a baseline, before editing
anything. It edited `store.md`, `tag.md`, and `notes.md`, caught a
second stale paragraph in `notes.md` on a re-read, fixed it, then ran
`dankg check .` again clean. Recall and precision are 4/4.

**Literate-prompted arm summary.** All three trials scored 4/4 on
recall and precision, matching the literate-unprompted arm exactly.
The explicit instruction did not raise the ceiling. The unprompted arm
was already at it. What moved was cost. Tool calls averaged 18.0,
against the unprompted arm's 19.67. Tokens averaged about 42,600,
against the unprompted arm's about 47,100. Telling the agent which
tool to run removed the unprompted arm's discovery overhead: no
`--help` call, no `dankg fmt` misfire and recovery, no time spent
finding the binary at all. The blind rubric scored 4, 5, 4, close to
the unprompted arm's 5, 4, 3. Both ranges sit around the same middle,
on six trials whose mechanical outcome never moved from a perfect
4/4.

# Findings

Nine trials complete the pre-registered design: three raw, three
literate-unprompted, three literate-prompted. Every trial passed its
functional check. The feature itself was never the hard part, in any
arm.

Reconciliation was the hard part. Only the raw arm found it hard. Its
recall ranged from 25% to 100% across three identical runs. Its
precision was perfect every time. When a raw agent reconciled a
claim, it reconciled the right one. It just sometimes reconciled
fewer of them. Both literate arms scored a perfect 4/4 on both
measures, in all six trials, no exceptions.

Every literate trial found and ran `dankg check .`, whether told to
or not. `pilot.md`'s task A left open whether an agent reaches for the
graph on its own. On this feature-implementation task, with this
fixture, six agents in a row reached for it without being told once.

The explicit instruction to run `dankg check .` did not raise the
literate arm's ceiling -- it was already there unprompted. It lowered
cost instead: fewer tool calls, fewer tokens, no discovery overhead,
no `dankg fmt` misfire and recovery of the kind trial 1 of the
unprompted arm paid for.

Both literate arms cost more than the raw arm regardless: about 18 to
20 tool calls and 42,000 to 47,000 tokens, against the raw arm's 12
calls and about 36,600 tokens. `feature_pilot.md` found this same tax
on a self-contained task. There, it bought nothing. Here it bought a
perfect reconciliation score six times running, against a raw arm
that ranged from 25% to 100% on the identical prompt.

The blind rubric is the finding this pilot exists to report. In the
raw arm, it tracked the real result: scores of 3, 2, 4 matched recall
of 2/4, 1/4, 4/4 in the same order. In both literate arms, the real
result never moved: 4/4, six times. The blind rubric still swung from
3 to 5. An independent read of a transcript can notice a real
difference when one exists. It can also manufacture an apparent one
when none does. Noticing that an agent reasoned better is not, on its
own, evidence that it did. It has to be checked against a measure the
noticing can't move, the way recall and precision were checked here.

# Caveats and next steps

- Nine runs is still a pilot, not a powered study, for the same reason
  `pilot.md` flagged its own six.
- The fixture is hand-built. A real corpus may scatter load-bearing
  claims and decoys in a different ratio, at a different distance from
  the code they depend on, than this 4-to-3 split does. Once this
  design validates the harness, the natural next step is the same
  measures run against a real corpus's actual `dankg:depends` graph.
- The blind grader is itself an LLM judging another LLM's transcript.
  Its score is a proxy for reasoning quality, not reasoning quality
  itself. The literate-unprompted arm already shows why that matters:
  three trials with an identical 4/4 reconciliation outcome drew blind
  scores of 5, 4, and 3.
- Having a tool available is not free of risk. Trial 1 of the
  literate-unprompted arm misapplied `dankg fmt`, meant for markdown,
  directly against two shell files, and briefly corrupted real logic
  before catching and reverting it. The raw arm cannot make this exact
  mistake, since it has no `dankg` binary to misapply. A fuller
  accounting of the literate arm's cost should count near-misses like
  this one, not just tool calls and tokens.
- No sandbox enforcement, the same caveat as every pilot before this
  one.
