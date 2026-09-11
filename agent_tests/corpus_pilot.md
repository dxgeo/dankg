---
title: Agent reasoning-quality pilot, real corpus
author: Daniel J. Okuniewicz
---
[reasoning_pilot.md](reasoning_pilot.md) validated the harness on a
purpose-built fixture: a fictional `jot` CLI, four load-bearing
claims, three decoys, ground truth fixed by construction. Its own
caveats named the obvious next step.

<!-- dankg:depends target=reasoning_pilot.md#caveats-and-next-steps quote="the natural next step is the same measures run against a real corpus's actual `dankg:depends` graph." -->

This pilot runs it. The fixture is DanKG's own `src/` corpus, frozen
at commit `b466427`. That keeps ground truth fixed, the same reason
`reasoning_pilot.md` gave for building a synthetic fixture in the
first place.

<!-- dankg:depends target=reasoning_pilot.md#setup quote="A purpose-built fixture keeps the ground truth fixed and exact, instead of drifting with the real `dankg` corpus the way a live number would." -->

Two real claims stand in for the synthetic fixture's four
load-bearing ones. One is already backed by a real `dankg:depends`
marker. One is a plain prose paraphrase with no marker at all -- the
same shape as the harder of `deps_pilot.md`'s two misses.

# Setup

Decision 40 (`architecture.md`, `[tui] commands`) states the real
rule this pilot's task changes: a custom command that collides with
a built-in, or with another command in the same file, is refused on
its own, not reverted as a set.

<!-- dankg:depends target=../architecture.md#decision-40-tui-custom-commands quote="A binding that collides with a built-in, or with another command in the same file, is refused individually rather than reverting the whole set the way `Keymap` reverts wholesale on a collision" -->

Two real claims restate that rule, at two different distances from
its source. One real decoy sits next to them.

1. **Marked, one hop away.** A different section of `architecture.md`
   \-- the one covering `[keys]` and decision 18 -- quotes decision
   40's own rationale verbatim, pinned by a real `dankg:depends`
   marker. Editing decision 40's wording without also touching that
   distant quote makes it go stale. That is the exact mechanism
   `deps_pilot.md` found `dankg check` catching, in one call.
2. **Unmarked, two hops away.** `src/tui/eval.md`'s prose paraphrases
   the same rule in its own words, next to `keyed_commands`, with no
   marker at all: "A key that parses but collides with a built-in,
   or with another command in the same file, is a different kind of
   problem." Nothing machine-checks this sentence. Only reading it
   catches it.

<!-- dankg:depends target=../src/tui/eval.md#tui-eval quote="A key that parses but collides with a built-in, or with another command in the same file, is a different kind of problem" -->

3. **Decoy.** A test comment in `src/config.md`, "an ambiguous map
   reverts wholesale," describes `Keymap`'s decision-18 collision
   behavior, not decision 40's. It stays true no matter what decision
   40's own rule becomes. It belongs to a different collision
   entirely.

<!-- dankg:depends target=../src/config.md#tests-1 quote="an ambiguous map reverts wholesale" -->

Two directory shapes, the same as `pilot.md` and `feature_pilot.md`
already established for this branch:

- **raw**: `src/**/*.rs` as `dankg tangle` generates it at commit
  `b466427`, plus `Cargo.toml`, `Cargo.lock`, and the shared root
  docs. No `dankg` binary, no `.dankg` config.
- **literate**: `src/**/*.md` at the same commit, the shared root
  docs, `.dankg`, `glue/`, and a copy of the `dankg` binary. No `.rs`
  at all. Tangling one is part of the task, as it was in
  `feature_pilot.md`.

Three task variants run against those two shapes, matching
`reasoning_pilot.md`'s own three-arm design: **raw** (the raw shape,
`dankg` never mentioned), **literate-unprompted** (the literate
shape, `dankg` available but never mentioned), and
**literate-prompted** (the literate shape, told to run
`dankg check .` before finishing).

# Task

Change `[tui] commands`'s collision handling. A command that collides
with a built-in, or with another custom command in the same file,
should revert that file's whole custom-command set, not just refuse
itself. Update `App::load`'s tests to match. Leave `[keys]`/`Keymap`
collision handling exactly as it is.

Nothing in the task names `architecture.md`, decision 40, or
`src/tui/eval.md`. Finding and fixing the two real claims above, if
an agent does, is emergent behavior, exactly as in
`reasoning_pilot.md`.

# Grading

The same four measures as `reasoning_pilot.md`, unchanged:

- **Functional**: the collision-revert behavior actually works, and
  `cargo test` passes.
- **Recall**: of the two real load-bearing claims (decision 40's own
  rationale, `src/tui/eval.md`'s paraphrase), how many did the agent
  touch or explicitly flag.
- **Precision**: did it also touch the decoy, which should stay
  untouched.
- **Blind rubric**: an independent grader reads each transcript, with
  no access to functional/recall/precision scores, and rates
  reasoning quality 1 to 5.

Two claims instead of four keeps this gradable in one sitting.
`feature_pilot.md` set that same constraint for itself.

# Hypothesis

`reasoning_pilot.md` reached a perfect ceiling on a synthetic fixture
with four load-bearing claims and three decoys, all placed by
construction. A real corpus is messier. `architecture.md` is long.
Decision 40's own section carries cross-references to several other
decisions. `src/tui/eval.md`'s paraphrase sits inside a doc comment
for an unrelated-looking function name, `keyed_commands`, not a
heading that announces the topic.

The prediction: the raw arm's recall holds up on the marked claim. A
`grep` for "collide" still finds decision 40's own section. Recall
drops on the unmarked claim, since nothing points a raw reader at
`src/tui/eval.md` in particular. The literate arm's edge, if real,
should show up two ways: `dankg check` catches the marked claim
regardless, and the graph's backlinks surface the unmarked one during
ordinary navigation.

# Caveats and next steps

- Design only. No trial has been run yet. Everything above is
  intended to run exactly as specified, not adjusted after seeing
  results.
- Two load-bearing claims is a small ground truth, deliberately, to
  keep the task gradable in one sitting on a real, larger corpus. A
  fuller real-corpus pilot would need several such tasks, not one, to
  say anything past a single data point.
- Pinning `b466427` matters. This repo's own corpus keeps changing.
  Running this pilot against a later commit, without first
  re-verifying the three claims above still hold, unchanged, at their
  stated locations, would invalidate the ground truth silently.
- No real sandbox, the same caveat as every pilot before this one.
