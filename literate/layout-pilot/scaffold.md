# Scaffold

The crate root for pilot 2. Two `path=` blocks, exactly like `hash.md`'s
own scaffold section -- `Cargo.toml` and `lib.rs` land outside the
heading-derived tree entirely, ignoring `file_prefix` (architecture.org,
*Tangle > Placement*), which matters more here than it did for `hash.md`:
this corpus has two contributing files (this one and `layout.md`), so
`file_prefix` nesting is genuinely active, and without `path=` these two
blocks would land under `scaffold/` instead of at the tree's root.

```rust name=cargo_manifest path=Cargo.toml
[package]
name = "dankg-layout-pilot"
version = "0.1.0"
edition = "2024"

[lib]
name = "dankg_layout_pilot"
path = "lib.rs"

[dependencies]
```

`layout` is declared here and nowhere else. Nothing in this pilot writes
`layout/mod.rs` -- see [layout.md](layout.md#types) for why that absence
is the entire point.

```rust name=crate_root path=lib.rs
pub mod layout;
```
