# Render mod

Five renderers (`render/json.rs`, `dot.rs`, `mermaid.rs`, `html.rs`,
`assets.rs`), one per output format `dankg graph --format` accepts; this
file only declares them. Nothing here re-exports or adds logic of its own,
so once each sibling has its own literate source this file's only job --
listing them -- is exactly what `glue/rust.py` already generates for a
directory with no `path=`-placed `mod.rs` of its own. It stays hand-placed
here anyway, the same reason `hash.md` needed no `[tangle.rust] glue` for
one file: turning glue on for the whole corpus over saving five lines in
one directory is not yet worth the tradeoff (`src/lib.md` explains the
`path=`/`-o src` convention this file follows).

```rust name=render_mod path=render/mod.rs
pub mod assets;
pub mod dot;
pub mod html;
pub mod json;
pub mod mermaid;
```
