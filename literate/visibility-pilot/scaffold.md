# Scaffold

The crate root, exactly the same `path=` shape `hash.md` and
`layout-pilot/scaffold.md` both already use (architecture.org, *Tangle >
Placement*): `Cargo.toml` and `lib.rs` land outside the heading-derived
tree entirely, since neither is content `exposed.md`'s own headings should
own.

```rust name=cargo_manifest path=Cargo.toml
[package]
name = "dankg-visibility-pilot"
version = "0.1.0"
edition = "2024"

[lib]
name = "dankg_visibility_pilot"
path = "lib.rs"

[dependencies]
```

`lib.rs` reaches all the way into `exposed::greeting::hello` from the crate
root -- a module outside `exposed/` entirely, not one of its own
descendants. `[tangle.rust] glue` only ever writes `mod.rs` for a
directory it did not find one in already (`glue/rust.py`'s own doc
comment); the crate root is never one of those directories, so this file
declares `pub mod exposed;` itself, by hand, the same way `layout-pilot`'s
own `lib.rs` declares `pub mod layout;`. What it does *not* write by hand
is `exposed/mod.rs` itself -- that one is `glue`'s to generate, and
whether it comes out `pub mod greeting;` or `mod greeting;` is exactly
what `exposed.md`'s own frontmatter decides.

```rust name=crate_root path=lib.rs
pub mod exposed;

#[cfg(test)]
mod tests {
    use super::exposed::greeting::hello;

    #[test]
    fn reaches_the_public_submodule_from_the_crate_root() {
        assert_eq!(hello(), "hi");
    }
}
```

## Pilot notes

This is pilot 3 of "can dankg build itself, dankg-style" (see
`../hash.md` and `../layout-pilot/layout.md` for pilots 1 and 2, and
architecture.org's *Tangle* section for the question itself). Reproduce
with the repo's own `dankg` binary, from the repo root:

```
cargo run --bin dankg -- tangle literate/visibility-pilot --lang rust
```

Naming the directory (not the two files individually) matters here: with
two files each contributing a matching block, decision 26's nesting
applies, which is what actually puts `exposed.md`'s one heading under its
own `exposed/` subdirectory rather than flat at the tree's root -- and a
subdirectory `mod.rs` is the whole point of this pilot, since `path=`'s
own two blocks above never go through one at all.

**What this pilot proved that neither earlier one could.** `hash.md` has
no directory boundary to cross. `layout-pilot` has one (`layout/`), but
every reference into it is either `layout::` itself (declared `pub mod
layout;` by the hand-written crate root, same as this pilot's `exposed`)
or `super::acyclic`/`super::types` from a *sibling* inside the same
directory -- and a sibling is always a descendant of that directory's own
module, so Rust's ordinary privacy rule already lets it through no matter
what `glue` writes. Nothing in either earlier pilot ever needed a
`pub mod` one level *inside* a generated `mod.rs`, so `dankg.tangle.public`
(decision 28) sat in the code, unit-tested (`src/md/frontmatter.rs`,
`src/tangle.rs`, `glue/test_glue.py`) but never proven against a real
`dankg tangle` + real `glue` + real `cargo test` run. This pilot's whole
shape exists to force exactly that path: `lib.rs` is not a descendant of
`exposed`, so `exposed::greeting::hello()` only resolves because
`exposed.md`'s `dankg.tangle.public: true` made `glue/rust.py` write
`pub mod greeting;` into the `mod.rs` it generated for `exposed/`.

**The manifest, confirmed.** `.dankg-tangle-manifest.json`'s entry for
`exposed/greeting.rs` carries `"public": true`, `"source": "exposed.md"`
-- read straight off `exposed.md`'s own frontmatter by `tangle.rs`
(`src/md/frontmatter.rs::tangle_public`), never re-derived from anything
`glue` itself inspects.

**The negative control, done by hand rather than committed broken** (the
same reason `layout-pilot`'s own `glue`-disabled check is prose, not a
tracked-broken build): edit `exposed.md`'s frontmatter to
`dankg.tangle.public: false` (or delete the line -- decision 28 says
anything but exactly `true` means private), re-run the `tangle` command
above, and `cargo test` fails with
`error[E0603]: module `greeting` is private`, pointing at `lib.rs`'s own
`use super::exposed::greeting::hello;` -- the same class of error
`layout-pilot`'s missing-`glue` check produces (`rustc` refusing to build
what an un-configured or unset mechanism left out), and proof the flag is
load-bearing rather than cosmetic: unset it, and the exact line that
compiled a moment ago stops resolving.
