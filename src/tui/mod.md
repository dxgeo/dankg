# TUI mod

`dankg tui`'s six pieces, declared once here and nowhere else redundant
with it: `term` (raw mode, alternate screen, size query), `input` (byte ->
key-event decoding), `draw` (tree/panel rows -> character grid), `editor`
(spawning the reader's editor, via \[`crate::cmd`\]), `eval` (cycling a
node's named blocks and evaluating one via \[`crate::eval::session`\]),
and `app` (the event loop). `app` alone is private. Nothing outside this
module needs `app::*` directly. Only the one function this file
re-exports from it does.

```rust name=tui_mod path=tui/mod.rs
//! Terminal UI. See `architecture.md`, "Terminal UI" (milestone 7).
//!
//! `term` (raw mode, alternate screen, size query), `input` (byte ->
//! key-event decoding), `draw` (tree/panel rows -> character grid),
//! `editor` (spawning the reader's editor, via [`crate::cmd`]), `eval`
//! (cycling a node's named blocks and evaluating one via
//! [`crate::eval::session`]), and `app` (the event loop) are built.
//! `app::run` is the entry point.

mod app;
pub mod draw;
pub mod editor;
pub mod eval;
pub mod input;
pub mod term;

pub use app::run;
```
