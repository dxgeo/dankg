# Crate root

`src/lib.rs` is never tangled by heading-derived placement. Like every
pilot's own crate root, it is hand-placed with `path=`, so it lands
exactly at `src/lib.rs` regardless of what this file's own heading would
slug to (`literate/visibility-pilot/scaffold.md`, *Crate scaffold*: "the
crate root is never one of those directories" a glue-generated `mod.rs`
would manage). All `path=` targets in this file, and in every other
literate module under `src/`, are relative to `src` itself. That is the
`-o src` this repo always tangles with (`src/hash.md`, *Literate
source*).

Every `pub mod` line here names a module that either already has its own
literate source (`src/hash.md`) or is still a hand-maintained `.rs` file
waiting its turn. The module list itself does not change shape as that
conversion proceeds. Only what backs each name does.

```rust name=crate_root path=lib.rs
//! DanKG: a plaintext knowledge grapher.
//!
//! Standard library only, by design. See architecture.md.

pub mod cli;
pub mod cmd;
pub mod config;
pub mod depends;
pub mod diag;
pub mod eval;
pub mod graph;
pub mod hash;
pub mod init;
pub mod layout;
pub mod md;
pub mod render;
pub mod tag;
pub mod tangle;
pub mod tui;
```

## Literate source

Regenerate every converted module at once, from the repo root:

```
cargo run --bin dankg -- tangle . --lang rust -o src
```

`tests/literate.rs` runs the same corpus-wide tangle into a scratch
directory. It diffs every file it produced against the committed one at
the same relative path under `src/`. This is one test, not one per
module. This way, it scales as more of `src/` converts without needing
to grow.
