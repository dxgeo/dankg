# Data mod

Three readers so far: `data/table.rs`, `data/yaml.rs`, and
`data/bib.rs`. This file only declares them, the same hand-placed
shape `render/mod.md` already uses for its own sibling list -- not
worth turning on `[tangle.rust] glue` for the whole corpus just to
save a few lines in one directory.

```rust name=data_mod path=data/mod.rs
pub mod bib;
pub mod table;
pub mod yaml;
```
