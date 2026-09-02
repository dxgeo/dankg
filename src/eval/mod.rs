//! Code evaluation: never automatic (decision 9). `dankg eval` only ever
//! displays or writes stored results; nothing here is reachable from
//! `dankg graph`.
//!
//! - `plan`    the dependency DAG and topological order -- "what will run."
//! - `files`   loads the (usually zero) extra files a cross-file `deps=`
//!   chain reaches -- and only those, never a corpus walk.
//! - `run`     process spawn, timeout, output capture.
//! - `result`  hashing, the `<!-- dankg:result ... -->` marker, write-back.
//! - `session` the interactive `dankg eval` flow (plan, confirm, run, write
//!   back) and `run_one`, shared with `tui::eval`'s in-grid cycle-and-run.

pub mod files;
pub mod plan;
pub mod result;
pub mod run;
pub mod session;

pub use session::EvalTarget;
