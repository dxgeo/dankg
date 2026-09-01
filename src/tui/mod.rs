//! Terminal UI. See `architecture.org`, "Terminal UI" (milestone 7).
//!
//! `term` (raw mode, alternate screen, size query), `input` (byte ->
//! key-event decoding), `draw` (`Layout` -> character grid), `editor`
//! (spawning the reader's editor, via [`crate::cmd`]), `expand`
//! (tab-to-expand's placement logic), `eval` (cycling a node's named
//! blocks and running one via [`crate::eval::session`]), and `app` (the
//! event loop) are built. `app::run` is the entry point.

mod app;
pub mod draw;
pub mod editor;
pub mod eval;
pub mod expand;
pub mod input;
pub mod term;

pub use app::run;
