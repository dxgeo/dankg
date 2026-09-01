---
dankg.tangle.public: true
---

# Greeting

The one heading in this file, and the one thing this whole pilot is
about: `dankg.tangle.public: true` above is what turns this heading's
tangled file, `exposed/greeting.rs`, from `mod greeting;` into
`pub mod greeting;` in the `mod.rs` `[tangle.rust] glue` writes for
`exposed/` -- see `scaffold.md`'s own notes for why that distinction is
exactly what this pilot needed and neither earlier one exercised.

```rust name=hello
pub fn hello() -> &'static str {
    "hi"
}
```
