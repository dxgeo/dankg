# Data mod

One reader so far: `data/table.rs`. This file only declares it, the
same hand-placed shape `render/mod.md` already uses for its own
sibling list -- not worth turning on `[tangle.rust] glue` for the
whole corpus just to save one line in one directory.

```rust name=data_mod path=data/mod.rs
pub mod table;
```
