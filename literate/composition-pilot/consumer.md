# Use greeting

`deps=producer.md#make_greeting` reaches into the other file the same way
a written link would (decision 29: resolved relative to *this* block's own
file, via `graph::resolve::join_normalize`/`dir_of`), which is what lets
`dankg eval consumer.md --block use_greeting` concatenate `make_greeting`'s
source directly ahead of this block's own -- one flat file, no crate, no
`producer` module to speak of, so `make_greeting` is just a sibling
function `use super::*` below can see.

```rust name=use_greeting deps=producer.md#make_greeting
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets_by_name() {
        assert_eq!(make_greeting("world"), "Hello, world!");
    }
}
```

## Pilot notes

This is pilot 4 of "can dankg build itself, dankg-style" (`../hash.md`,
`../layout-pilot/`, `../visibility-pilot/` are pilots 1 through 3).
Reproduce with the repo's own `dankg` binary, from the repo root:

```
cargo run --bin dankg -- eval literate/composition-pilot/consumer.md --block use_greeting --yes --no-write
cargo run --bin dankg -- eval literate/composition-pilot/same-file.md --block quadruple --yes --no-write
```

Both print `test result: ok. 1 passed`, from `rustc --test` run directly
against the concatenated temp file -- decision 29's cross-file `deps=` had
shipped with unit coverage in `src/eval/plan.rs` and `src/eval/files.rs`
(their own `#[cfg(test)]` modules) but never before a real end-to-end
pilot spawning the actual `dankg` binary against real files on disk; this
is that pilot.

**The question this pilot exists to answer.** `hash.md` and
`layout-pilot` each proved tangle's own loop in isolation. Neither asked
whether a block written for tangle's real, multi-file crate can *also* be
the thing `dankg eval` smoke-tests through a cross-file `deps=` chain --
architecture.md's own *Code evaluation* section already states the
mechanical reason to expect friction ("For a language with no real
per-file module system reachable from within one compiled unit ... this
is enough to write a genuinely multi-file literate program with no
`mod`/`use` at all"), but that claim had never been checked against
tangle's *own* multi-file output, which very much does have real modules.
This pilot checks it, with two corpora that share one root and answer the
question oppositely.

**`same-file.md`: composes cleanly, and the reason is structural, not
luck.** `double` and `quadruple` share one heading, so tangle's placement
(decision 24, containment only, `deps=` never consulted) and eval's own
concatenation (decision 11, `deps=` order) agree on the same thing for a
reason that has nothing to do with either mechanism trying to agree: a
heading's blocks land in one tangled file in document order regardless of
`deps=`, and document order already put `double` before `quadruple` here.
Reproduce the tangle side:

```
cargo run --bin dankg -- tangle literate/composition-pilot/same-file.md --lang rust
rustc --test -o /tmp/utils.bin literate/composition-pilot/.dankg/build/rust/utils.rs && /tmp/utils.bin
```

(or, simpler: `cat literate/composition-pilot/.dankg/build/rust/utils.rs`
-- `double` then `quadruple`, byte-for-byte the same two blocks eval
concatenated, in the same order.) No `[tangle.rust] command` is even
configured for this pilot; the point was always the file tangle produces,
not a crate around it, so this repo verifies that file compiles by hand
rather than shipping a scaffold whose only job would be to restate what
`same-file.md` alone already proves.

**`producer.md`/`consumer.md`: the same convention breaks the moment a
dependency crosses a tangle file boundary, and it breaks loudly, not
silently.** Naming both files tangles them into *separate* files,
`producer/greeting.rs` and `consumer/use_greeting.rs` (decision 26's
nesting, triggered because more than one file contributes a matching
block):

```
cargo run --bin dankg -- tangle literate/composition-pilot/producer.md literate/composition-pilot/consumer.md --lang rust
rustc --test literate/composition-pilot/.dankg/build/rust/consumer/use_greeting.rs
```

`consumer/use_greeting.rs`'s `use super::*` reaches `consumer`'s own
module, not `producer`'s -- there is nothing named `make_greeting` in
scope at all, and rustc says exactly that:
`error[E0425]: cannot find function `make_greeting` in this scope`. This
is not a bug in tangle, eval, or either pilot's own source: it is exactly
what decision 24 already promises ("tangle never reads that attribute")
collapsed onto exactly what decision 11 already promises ("no `mod`/`use`
at all") -- the two promises are compatible only when nothing forces a
choice between them, which is precisely `same-file.md`'s shape and
precisely not this one. A real cross-tangle-file dependency needs the
author to write `use crate::producer::greeting::make_greeting;` by hand
in the tangled output's own file (architecture.md, *Tangle*: "The author
writes those themselves") -- which would in turn break the identical
block's use as a flat, concatenation-eval'd script, since there is no
`crate::producer` module for a lone temporary file passed straight to
`rustc` to resolve. **The two mechanisms are not simultaneously
satisfiable on one unmodified block once the dependency and its target
land in different tangled files**; an author gets to pick, per block,
whether it stays eval-smoke-testable across a file boundary or becomes
part of the real cross-file-linked program at that boundary, not both at
once. This corpus is committed exactly as `producer.md`/`consumer.md`
above -- eval-oriented, and *not* wired to a working tangle scaffold --
because the tangled, unmodified pair genuinely does not build; the
failure is the finding, reproduced by hand rather than checked in broken
(the same reason `visibility-pilot`'s own negative control is prose, not
a tracked-broken build).

**What this rules in for a real self-hosting attempt.** A literate
`src/` module whose internal helpers all live under the module's own
top-level heading -- exactly `same-file.md`'s shape, and exactly what
`hash.md`'s single-file pilot already was -- gets eval-smoke-testing and
real tangled compilation for free, with no author-written glue of its
own. A module that genuinely needs another file's item at compile time
(`layout-pilot`'s own `super::acyclic::Role`, `super::types::{Dag, ...}`)
already accepts writing that `use` by hand; what this pilot adds is the
converse fact that such a block should *not* also be expected to serve as
one leg of an eval `deps=` chain reaching across the same file boundary --
if it needs smoke-testing that crosses files, the smoke test itself
belongs in the *dependency's own* file (own heading, own block), reaching
nothing beyond it, exactly `same-file.md`'s pattern turned into a
convention rather than a coincidence.
