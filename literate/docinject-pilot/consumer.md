---
dankg.tangle.public: true
---

# Caller

Wraps [`make_greeting`](producer.md#make_greeting) and shouts the result.
The link is written exactly like any other DanKG cross-file reference; the
only thing special about it is what it becomes once this paragraph is
copied into `call_greeting`'s own injected doc comment.

```rust name=call_greeting
use crate::producer::greeting::make_greeting;

pub fn shout(name: &str) -> String {
    make_greeting(name).to_uppercase()
}
```

## Pilot notes

This is pilot 6 of "can dankg build itself, dankg-style" (`../hash.md`,
`../layout-pilot/`, `../visibility-pilot/`, `../composition-pilot/` are
pilots 1, 2, 3 and 4; hash.md's own config now doubles as pilot 5, having
had `rust-docinject.py` added to its glue chain after this pilot's own
script was written). Reproduce with the repo's own `dankg` binary, from
the repo root:

```
cargo run --bin dankg -- tangle literate/docinject-pilot --lang rust
```

**What the injected doc comment actually contains.** `call_greeting` has
no doc comment of its own, so the backoff does not fire, and its owned
prose -- the one paragraph above -- becomes its `///`. Read straight out
of the tangled `consumer/caller.rs`:

```
/// Wraps [`make_greeting`](crate::producer::greeting::make_greeting) and shouts the result. The link is written exactly like any other DanKG cross-file reference; the only thing special about it is what it becomes once this paragraph is copied into `call_greeting`'s own injected doc comment.
```

The link's *destination* changed from `producer.md#make_greeting` to
`crate::producer::greeting::make_greeting`; its *label* (`` `make_greeting` ``)
did not. `#![deny(rustdoc::broken_intra_doc_links)]` in `scaffold.md`'s
own `lib.rs` is what makes this a checked claim rather than an assertion:
`cargo doc` fails outright on an intra-doc link that does not resolve, so
the pilot's own `command` succeeding *is* the proof the rewritten link
lands on a real item -- confirmed a second way by grepping the generated
HTML for the resolved `href`, rather than trusting the exit code alone.

**Why this doesn't run into the composition-pilot's own finding.**
Pilot 4 found that a block cannot simultaneously be eval-smoke-testable
across a file boundary and real cross-tangle-file-linked at that same
boundary, because a `use` is *code* -- eval's flat concatenation has no
crate for it to resolve against, so writing one breaks eval even though
it is exactly what tangle's own separate-file output needs. A doc
comment is not code. `rustc` strips comments before anything resembling
name resolution runs, so `///`'s content -- link, rewritten or not -- has
zero effect on whether `call_greeting` compiles, under `eval` or under
`tangle`, in either direction. Only `cargo doc` ever looks inside a doc
comment at all, and nothing about *that* pass touches eval's own flat,
crate-less compile. Documentation composes across a tangle-file boundary
for free, precisely because it carries no compile-time weight; code does
not, for precisely the opposite reason.

**A real limitation, not fixed here.** `rewrite_links` matches a target
against the manifest's own block *names*, not against what a block's
Rust code actually defines -- decision 1 forbids reading that. This
corpus only works because `make_greeting` (the DanKG name) and
`make_greeting` (the function `producer.md`'s block defines) happen to be
spelled the same, which is an authoring convention every pilot so far has
followed but nothing mechanically enforces. Name a block one thing and
define a differently-named item inside it, and the rewritten link would
point at a `crate::` path that resolves to nothing (a build failure, at
least, thanks to the `deny` above) or -- worse, and unverified here --
to a real but wrong item if some other block happens to share that name.

**The bug this pilot's own reasoning caught before it was ever run**
(recorded in `glue/rust-docinject.py`'s own module doc, and pinned by
`glue/test_glue.py`'s `DocinjectSkipsNonRustOutput`): the very first real
run of `rust-docinject.py`, against `hash.md`, tried to inject a `///`
onto a `path=Cargo.toml` block -- selected by *fence* language matching
`--lang rust`, but its own output is TOML, not Rust -- and broke the
manifest with `error: key with no value, expected =`. The fix skips
injection for any output whose own path does not end `.rs`, while still
letting that block's own prose count as spent so a *later* real `.rs`
block's window does not silently widen to include it.
