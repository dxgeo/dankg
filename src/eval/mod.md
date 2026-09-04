# Eval mod

`dankg eval`'s six pieces. Nothing under this module is reachable from
`dankg graph`. This command evaluates code only when invoked directly. It
never evaluates code as a side effect of rendering or checking a corpus
([decision 9](../../architecture.md#decision-9-eval-trigger)).

```rust name=eval_mod path=eval/mod.rs
//! Code evaluation: never automatic (decision 9). `dankg eval` only ever
//! renders or writes stored results. Nothing here is reachable from
//! `dankg graph`.
//!
//! - `plan`    the dependency DAG and topological order -- "what will run."
//! - `files`   loads the (usually zero) extra files a cross-file `deps=`
//!   chain reaches -- and only those, never a corpus walk.
//! - `run`     process spawn, timeout, output capture.
//! - `result`  hashing, the `<!-- dankg:result ... -->` marker, write-back.
//! - `session` the interactive `dankg eval` flow (plan, confirm, evaluate,
//!   write back) and `run_one`, shared with `tui::eval`'s in-grid
//!   cycle-and-run.
//! - `sql`     a hand-rolled scanner for what a `db=` block's own SQL
//!   writes to and reads from (decision: *Provenance without a driver*).

pub mod files;
pub mod plan;
pub mod result;
pub mod run;
pub mod session;
pub mod sql;

pub use session::EvalTarget;
```
