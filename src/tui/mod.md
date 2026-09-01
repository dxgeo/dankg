# TUI mod

`dankg tui`'s seven pieces, declared once here and nowhere else redundant
with it: `term` (raw mode, alternate screen, size query), `input` (byte ->
key-event decoding), `draw` (`Layout` -> character grid), `editor`
(spawning the reader's editor, via \[`crate::cmd`\]), `expand` (tab-to-
expand's placement logic), `eval` (cycling a node's named blocks and
running one via \[`crate::eval::session`\]), and `app` (the event loop).
`app` alone is private -- nothing outside this module needs `app::*`
directly, only the one function this file re-exports from it.

```rust name=tui_mod path=tui/mod.rs
//! Terminal UI. See `architecture.md`, "Terminal UI" (milestone 7).
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
```
