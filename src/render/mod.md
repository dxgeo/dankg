# Render mod

Seven renderers exist: `render/json.rs`, `dot.rs`, `mermaid.rs`, `html.rs`,
`assets.rs`, `typst.rs`, and `weave_html.rs`. The first five handle one
output format `dankg graph --format` accepts. The last two are
`dankg weave`'s own pair, one per `--format` value (plan-weave.md,
decisions 43-44). This file only declares them.

Nothing here re-exports anything or adds its own logic. Once each sibling
has its own literate source, this file's only job is to list them. That is
exactly what `glue/rust.py` already generates for a directory with no
`path=`-placed `mod.rs` of its own.

It stays hand-placed here anyway, for the same reason `hash.md` needed no
`[tangle.rust] glue` for one file. Turning on glue for the whole corpus is
not worth it just to save five lines in one directory. (`src/lib.md`
explains the `path=`/`-o src` convention this file follows.)

```rust name=render_mod path=render/mod.rs
pub mod assets;
pub mod dot;
pub mod html;
pub mod json;
pub mod mermaid;
pub mod typst;
pub mod weave_html;
```
