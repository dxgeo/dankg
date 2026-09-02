---
title: Agent feature-implementation pilot
author: Daniel J. Okuniewicz
---
[pilot.md](pilot.md) tested whether an LLM navigates DanKG's own
literate `src/*.md` form more effectively than the plain `src/*.rs` a
normal Rust crate would ship. That pilot only asked read-only
questions. This pilot asks a sharper, more practical version of the
same claim. Does the same advantage show up when an agent has to
*implement a feature*, not just find something?

# Setup

Same branch, same two forms. This time each agent has to produce a
build that actually compiles and passes its own tests. The
environments are heavier than `pilot.md`'s as a result:

- **raw**: a full buildable crate — `src/**/*.rs`, `Cargo.toml`,
  `Cargo.lock`, plus the shared root docs.
- **literate**: `src/**/*.md`, `Cargo.toml`, `Cargo.lock`, the shared
  root docs, a `.dankg` config, `glue/`, and a copy of the `dankg`
  binary, but *no* `.rs` at all. Producing one is part of the task.

One task, one fresh subagent per arm. `pilot.md` ran three questions
with repeated trials; this task is expensive enough per run that a
single pair was the right amount of pilot for now. The same constraint
and the same caveat as `pilot.md` apply: "only work inside this
directory" was an instruction, not an enforced sandbox.

# Task

Add glob-style pattern filtering to the CLI:

1. Implement `pub fn glob_match(pattern: &str, text: &str) -> bool`
   wherever CLI argument parsing already lives — `*` matches any run
   of characters, `?` matches exactly one, matching is
   case-insensitive.
2. Add a `--filter <pattern>` option to the `graph` subcommand,
   stored as an `Option<String>`. Wiring it into rendering was
   explicitly out of scope, to keep the task bounded.
3. Add unit tests for `glob_match` covering an exact match, a leading
   `*`, a trailing `*`, and a `?`.
4. Leave `cargo build` and `cargo test` passing.

The task was chosen on purpose to sit next to [decision
1](../architecture.md#decision-1-dependency-policy): pure Rust, no
dependencies. A glob matcher is exactly the kind of thing a competent
Rust engineer instinctively reaches for a crate (`glob`, `wildmatch`)
to do. The task tests whether the constraint that forbids that gets
*found*, not just followed once known.

# Results

| | Raw | Literate |
|---|---|---|
| Correct implementation | yes | yes |
| New dependencies added | 0 | 0 |
| Passes an independent held-out test vector | yes | yes |
| `cargo build` / `cargo test` | pass, 475 tests | pass, 475 tests |
| Tool calls | 22 | 28 |
| Wall-clock time | ~145s | ~265s |

Both results were checked independently rather than taken on the
agent's word. `Cargo.toml` was diffed against the original: no new
dependency in either. The actual `glob_match` source was read. A
held-out test vector (`*eval*`, `d?nkg`, `graph*`, an empty pattern,
and more, never shown to either agent) was run against both. Both
pass. The raw arm wrote a DP table. The literate arm wrote a
two-pointer backtracking matcher. Different algorithms, both correct.

# Findings

Both arms respected decision 1 equally. That was not because of the
graph. Both agents independently chose to read `architecture.md` and
`project.md` in full before writing any code, unprompted. Both named
"decision 1" as the reason for hand-rolling the matcher. `pilot.md`
found the in-context citation valuable for *locating* a decision, in
task A there. That advantage never got exercised here. A careful agent
looks for top-level constraints on its own before touching an
unfamiliar codebase. It does not need a hyperlink pointing it there.

The arms actually diverged on overhead. It ran the wrong way for the
literate side. The literate agent paid a real, visible cost discovering
something it would not have needed otherwise. It read `src/tangle.md`,
`src/lib.md`'s regenerate section, and `.dankg/config` to work out that
`.rs` is generated. It ran `dankg tangle . --lang rust -o src` before it
could edit anything meaningfully. It built and tested the regenerated
tree on top of that. The result is roughly 27% more tool calls and 83%
more wall-clock time than the raw arm, for byte-for-byte equivalent
correctness. The `.rs`/`.md` split cost nothing to read in `pilot.md`.
It bought nothing to write here. It is a mandatory generate-and-verify
round trip a plain Rust crate never pays.

Read together with `pilot.md`, the claim narrows further. The literate
graph's edge is specific to *relationship-shaped* questions with no
ground truth in raw prose: reciprocation, or a decision a piece of code
is required to follow. For "does this respect a constraint a careful
agent would check anyway," and for routine, self-contained
implementation work, the literate form adds a real tax and no
offsetting benefit.

# Caveats and next steps

- N=1. A single task, a single run per arm. This rules nothing in or
  out. It is one more data point next to `pilot.md`'s six.
- The task avoided `deps=`-chain and eval-block territory on purpose,
  to keep it gradable in one sitting. A feature that actually needed to
  trace a cross-file `deps=` chain, the way task B in `pilot.md` needed
  `reciprocated`, might tell a different story. That is the natural
  next task for this file, not a new pilot.
- No real sandbox, same as `pilot.md`.
