---
title: Hash
---
# Hash

DanKG needs a content hash for two things: keying the per-file index
cache (`graph/cache.rs`) and stamping the config into that cache. This
way, an edited `.dankg/config` invalidates every entry rather than
leaving a stale one behind. Neither use is security-bearing. A collision
costs a stale parse, which the next edit corrects. This is why the
choice is FNV-1a: eight lines, no tables, no dependency, exactly the
standard-library-only constraint
[decision 1](../architecture.md#decision-1-dependency-policy) holds
everywhere else.

This file is the literate source. `src/hash.rs` is generated from it by
`dankg tangle` and then committed alongside it. This way, `cargo build`
never needs a working `dankg` binary just to compile the crate. Only
editing this module does. See *Literate source* below for the regenerate
command and how staleness is caught.

## The hash

Rust's inner-doc-comment (`//!`) has to be the first thing in the file it
documents. This is why it gets its own small leading block, rather than
living inside `fnv1a`'s. Every block below shares one explicit
`path=hash.rs` (`src/lib.md` explains the convention), rather than the
heading-derived default this file used at first. That default only stays
flat once `hash.md` is the *only* source file contributing Rust. It
stopped being true the moment a second module converted. An un-pathed
block groups by its file-prefixed heading once tangle sees more than one
contributor
([decision 26](../architecture.md#decision-26-tangle-corpus-scope)). That
would nest this file under `hash/hash.rs` in a corpus-wide run. An
explicit `path=` sidesteps that decision entirely. It lands at exactly
`hash.rs` regardless of how many other files tangle alongside it.

This is also the file's one deliberately duplicated sentence, not a
habit to repeat. `//!` is what a `cargo doc` reader sees with no markdown
in front of them, so it needs *something*. It stays a pointer, though,
rather than a second copy of the paragraph above.

```rust name=module_doc path=hash.rs
//! A 64-bit content hash (FNV-1a). See `src/hash.md#the-hash` for why.
```

The mix step is the entire algorithm: XOR a byte into the state, then
multiply by the FNV prime, for every byte in the input. `OFFSET` and
`PRIME` are FNV-1a's published constants, not tunable.

```rust name=fnv1a path=hash.rs
pub fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut h = OFFSET;
    for byte in bytes {
        h ^= *byte as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}
```

Cache file names all need the same shape. This is why the hex form is
fixed-width, rather than however many digits the value happens to need.

```rust name=hex path=hash.rs
/// Fixed-width so cache file names all have the same shape.
pub fn hex(value: u64) -> String {
    format!("{value:016x}")
}
```

The inverse, used to compare a stored hash against a freshly computed one
without keeping either side's numeric form around longer than it has to
be.

```rust name=parse_hex path=hash.rs
pub fn parse_hex(text: &str) -> Option<u64> {
    u64::from_str_radix(text, 16).ok()
}
```

## Tests

Three cases: published FNV-1a reference vectors (so a transcription
mistake in the constants above cannot pass silently), the hex round-trip
at its fixed width, and a sanity check that a one-byte change actually
changes the hash.

```rust name=tests path=hash.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_reference_vectors() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn hex_round_trips_at_fixed_width() {
        let h = fnv1a(b"notes/ideas.md");
        assert_eq!(hex(h).len(), 16);
        assert_eq!(parse_hex(&hex(h)), Some(h));
    }

    #[test]
    fn differs_on_a_one_byte_change() {
        assert_ne!(fnv1a(b"# One\n"), fnv1a(b"# Two\n"));
    }
}
```

## Literate source

This is the first real (non-pilot) module converted to literate form.
See `literate/hash.md` for the standalone pilot that proved the
mechanics first, on a throwaway crate rather than the one `cargo build`
actually compiles. That directory sits outside this corpus:
`.dankgignore` excludes `/literate/` as its own, already self-contained
concern.

Regenerate every converted module at once, from the repo root, including
this file. There is no narrower single-file command any more, now that
more than one source contributes (`src/lib.md`, *Literate source*):

```
cargo run --bin dankg -- tangle . --lang rust -o src
```

That overwrites `src/hash.rs` in place. No `[tangle.rust] glue`/`command`
is even configured for `rust` at the repo root yet. Tangle never runs on
its own regardless (architecture.md,
*[Trigger](../architecture.md#trigger)*, the same principle as
[decision 9](../architecture.md#decision-9-eval-trigger)). Run `cargo test` yourself afterward to confirm the regenerated tree still builds
before committing it.

**Staleness is a checked property, not an assumption.**
`tests/literate.rs` re-tangles the whole corpus into a scratch directory
on every `cargo test` run. It diffs every file it produced against the
committed one at the same path under `src/`, failing loudly the moment
any of them drift. The generated tree is trusted only as far as that
check, never on its own. The markdown that produced it is what's
actually trusted ([Trigger](../architecture.md#trigger)).
