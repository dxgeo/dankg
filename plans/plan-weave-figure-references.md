# `dankg weave`: figure references

## Context

A woven document can already build a real, captioned figure. A
recognized pair (decision 46) whose source block declares
`produces=file:` and carries a caption wraps its artifact in a real
`#figure(...)` in Typst and a real nested `<figure>` in HTML
(decision 54), placed inside or outside the pair's own box
(decision 56). Both backends number those figures, and neither
number is counted by dankg: Typst's own `#figure` counter does it in
the PDF, a stylesheet's own CSS counter does it in HTML.

What no reader can write today is a reference to one. There is no way
to say "see Figure 3" in prose and have either backend resolve it. A
figure carries a caption, but it carries no name, so there is nothing
for a reference to point at.

This plan gives a figure a label, and gives prose a way to reference
that label in both backends.

<!-- dankg:depends target=../architecture.md#decision-54-a-produced-artifact-renders-as-a-real-captioned-figure quote="gives a reader genuine, automatic" -->

## What three probes established first

The syntax already parses. `:` is inside decision 57's own key
charset, `[A-Za-z0-9_:.-]+`, so `[@fig:chart]` and a bare
`@fig:chart` both parse today as a real `Inline::Citation` holding
the one key `fig:chart`. A probe against the release binary warns that the
citation key `fig:chart` is not in the bibliography, and renders
`[?]`. The whole inline path is already built: the bracketed trigger,
decision 57b's own bare-`@` lookbehind, `md/fmt.rs`'s own escape
rule, and decision 59's own literal-source fallback. No parser change
is in scope here.

Typst resolves the same syntax natively. A real compile against the
targeted 0.15.1 confirms that `#figure(...)[body] <fig:chart>`
followed by `@fig:chart` compiles, and that a document holding both a
figure label and a `#bibliography(...)` call resolves both through
the one `@` namespace. Typst's citation syntax and its reference
syntax are the same syntax.

Typst counts each figure kind separately. The same real compile,
read back with `pdftotext`, renders two tables and two images
interleaved as `Table 1`, `Figure 1`, `Table 2`, `Figure 2`, and
renders the four references as exactly those four strings. A single
sequential counter would say 1, 2, 3, 4 and disagree with the PDF on
every document mixing a table with an image. This is the one finding
that shapes decision 65 below.

## What this reuses

**Decision 57's own parse path, unchanged.** A figure reference is
not a new inline construct. It is the construct decision 57 already
built, carrying a key that happens to name a figure rather than a
bibliography entry. Nothing in `md/inline.rs` or `md/fmt.rs` changes.

<!-- dankg:depends target=../architecture.md#decision-57-key-inline-citation-syntax-and-inlinecitation quote="A key matches `[A-Za-z0-9_:.-]+`" -->

**Decision 59's own fallback.** A citation with nothing to resolve it
renders its literal source text, escaped exactly like ordinary prose,
in both backends. A figure reference that resolves to no figure takes
the identical path, so an unresolvable `@fig:chart` reaches neither
compiler as real reference syntax with nothing behind it.

<!-- dankg:depends target=../architecture.md#decision-59-citations-and-a-references-list-render-in-both-weave-backends quote="its own literal source text, escaped exactly like ordinary prose." -->

**Decision 54's own real figure element.** Both backends already emit
something a label can attach to. Typst emits `#figure(...)`, which
takes a trailing `<label>`. HTML emits a nested `<figure>`, which
takes an `id`. A heading in HTML already carries an `id` built from
`slugs`, so the plumbing pattern for the second one exists.

**Decision 52's own override shape.** A caption is synthesized by
default and overridden by a reader-written `caption=`. A label works
the same way: derived by default from the block's own `name=`,
overridden by a reader-written `label=`.

<!-- dankg:depends target=../architecture.md#decision-52-caption-overrides-a-pairs-own-synthesized-caption quote="It replaces exactly one of the two, never both" -->

**Decision 61's own "drop once, in `weave::run`" ordering.** The
repeated title heading is dropped in `weave::run` rather than in each
renderer, because both backends want the identical document. A
numbering pre-pass has the same requirement for the same reason, and
lands on the same line of that function's own hand-ordered sequence.

<!-- dankg:depends target=../architecture.md#decision-61-a-frontmatter-cover-switch-for-the-woven-title quote="Both backends want the identical document." -->

## What's new

### Decision 63: A figure's label comes from its block's own `name=`, or from `label=`

Every eval-able block already carries a unique `name=` (decision 19).
It is unique within its own file by construction, and it is already
that block's own graph node slug. A captioned `produces=file:`
artifact takes its label from that name, so a block written
`name=chart produces=file:chart.png caption="Revenue"` is referenced
as `@fig:chart` with no new attribute written at all.

A new `label` attribute joins `KNOWN_ATTRS` beside `caption` and
`figure`, and overrides that default for one block. It exists because
a block's own `name=` is also its eval identity and its tangle
identity, and renaming a block for a code reason should not have to
break every reference in the prose.

Only a captioned artifact is a figure at all (decision 54). A block
with a name but no caption renders unwrapped, has no figure to label,
and is not referenceable. A reference naming one warns by line and
falls back, the same as a reference naming nothing.

A `label=` colliding with another block's own label warns by line and
the second one is dropped, matching decision 58's own second-fence
rule. Suffixing the collision the way `Slugger` suffixes a duplicate
heading is wrong here: a silently suffixed label is a reference that
silently points at the wrong figure.

### Decision 64: `@fig:` is a namespace, resolved before the bibliography and never through it

A key whose text begins with `fig:` is a figure reference. It is
resolved against the document's own figure labels alone, and it never
falls through to the bibliography. A `fig:`-prefixed key that matches
no figure warns that no figure carries that label, and renders its
literal source text. It never warns that the key is missing from the
bibliography, and it never renders `[?]`.

Dispatching on the prefix rather than trying each source in turn is
what makes the diagnostic exact. A reader who writes `@fig:chart` and
misspells the label wants to be told that no figure carries it, not
that no bibliography does.

Typst receives `@fig:chart` and resolves it itself against the
`<fig:chart>` label decision 63 emits. This is byte-for-byte what
`typst.rs` already writes for a non-narrative citation, so the Typst
arm changes only in what gates it. That gate is decision 59's own
`bibliography.is_none()` guard, which today escapes every `@` to
literal text when no bibliography is configured. It widens to "a
bibliography or a figure label exists", so a document with figures
and no bibliography still emits real reference syntax.

HTML receives a link to the figure's own `id`.

### Decision 65: One numbering pre-pass feeds HTML, and Typst still counts for itself

CSS cannot put a counter value from one element into an `<a>`
somewhere else in the document. `target-counter()` is the feature
that does exactly this, and no browser implements it -- only print
processors such as Prince and WeasyPrint. JavaScript could count at
load, at the cost of a page that is no longer static. Neither is a
real option for a self-contained HTML page, so HTML cannot number a
reference without dankg counting.

`weave::run` gains a pre-pass that numbers every figure in document
order and hands both renderers the same map. HTML writes the number
as literal text. Typst still receives `@fig:chart` and still counts
for itself, so the PDF keeps Typst's own outline machinery, its own
supplement text, and everything a `[weave.pdf] template` can already
restyle. Both backends count the same figure sequence, so both land
on the same number.

The pre-pass counts per kind, not sequentially. Typst numbers tables
and images on separate counters, confirmed by a real compile above.
A single counter would disagree with the PDF on every mixed
document, so the pre-pass keys on the same `kind: table` and
inferred-image split `typst.rs` already makes.

Where the pre-pass sits inside `weave::run` is not free, for the same
reason decision 61's own heading drop is not. It must count exactly
the figures each backend will actually emit. So it runs after
`drop_repeated_title_heading`, and it skips a block hidden by
`weave=hidden` or `weave=output-hidden`, and it skips an uncaptioned
artifact, which decision 54 says is not a figure at all.

HTML's own figcaption number comes from the same pre-pass, replacing
decision 54's own CSS counter. This is an amendment to that decision
rather than an addition to it. Two numbering sources that have to
agree by coincidence is worse than one that cannot disagree.

## Open questions, for discussion before anything is built

1. **Is "Figure 3" worth the pre-pass?** The alternative is a link
   whose text is the caption -- `<a href="#fig-chart">Revenue</a>` --
   which costs nothing, needs no counting, and never disagrees with
   anything. Decision 65 exists only if the reference has to read as
   a number. Everything hard in this plan is downstream of that one
   answer.

2. **Is amending decision 54's CSS counter acceptable?** Decision 65
   moves HTML's figure numbering out of the stylesheet and into
   dankg. That is a real reduction in what a stylesheet alone can
   restyle, traded for HTML and PDF that cannot disagree.

3. **What does the bracketed form mean for a figure?** Pandoc's own
   crossref convention reads `@fig:x` as `fig. 1` and `[@fig:x]` as
   `(fig. 1)`. Typst draws no such distinction: `@label` renders
   `Figure 1` either way. Three options, and they do not have to
   match the citation forms: render both the same, adopt Pandoc's
   parenthesised split, or refuse the bracketed form outright.

4. **Does `Inline::Citation` get renamed?** The variant would now
   carry a construct that is not a citation. Renaming it to
   `Inline::Ref` touches `md/mod.rs`, `md/inline.rs`, `md/fmt.rs`,
   and both backends, and is otherwise mechanical. Keeping the name
   costs one honest doc comment and leaves the type lying about
   itself.

5. **Do `sec:` and `tbl:` ship now?** The resolver that finds a
   figure finds a heading with no new machinery -- headings already
   carry an `id` in HTML and take a label in Typst. Shipping `fig:`
   alone is smaller. Building the namespace lookup generically and
   ship one namespace is the middle path.

6. **What does a reference to a hidden figure do?** A block hidden
   by `weave=hidden` emits no figure. A reference to it resolves to
   nothing. It should warn, but the wording differs from a
   misspelled label -- the label is right and the figure is gone.

## What this explicitly does not do

- Any parser change. The syntax already parses (see the probes
  above), and `md/inline.rs` and `md/fmt.rs` are untouched.
- A reference to anything that is not a captioned `produces=file:`
  artifact, unless question 5 is answered otherwise. A plain fenced
  block, a heading, a table written as markdown, and an equation are
  all out of scope here.
- Configurable supplement text. Typst's own `Figure`/`Table` wording
  stands in the PDF, and HTML matches it literally. A reader wanting
  `fig.` instead of `Figure` restyles Typst through a
  `[weave.pdf] template` today, and gets nothing in HTML.
- A references list, an index, or a list of figures. Typst's own
  `#outline(target: figure)` already builds the last one for a reader
  who asks for it in a template.
- Cross-file references. Weave renders one file (decision 41), so a
  label from another file has nothing to resolve against.

## Critical files

- `src/md/mod.md` / `md/mod.rs` -- `KNOWN_ATTRS` gains `label`;
  `InfoString` gains `label()`. No `Inline` change unless question 4
  is answered by renaming.
- `src/weave.md` / `weave.rs` -- the new numbering pre-pass, placed
  after `drop_repeated_title_heading`, and the label map both
  renderers receive.
- `src/render/typst.md` / `typst.rs` -- a `<label>` appended to
  `#figure(...)` in both the inside and outside placements
  (decision 56); the `Citation` arm's own `bibliography.is_none()`
  gate widened to account for a figure label.
- `src/render/weave_html.md` / `weave_html.rs` -- an `id` on the
  nested `<figure>`; the `Citation` arm's own figure branch; the
  figcaption's own number, if question 2 is answered by amending
  decision 54.
- `architecture.md` -- add decisions 63, 64, 65, and amend decision
  54's own HTML numbering sentence.
- `README.md` -- document `label=` and the `@fig:` namespace.

## Verification

1. Edit loop per CLAUDE.md, then `dankg fmt --check` and
   `dankg check .`.

2. Unit tests, both backends, matching the existing pair-per-case
   style:

   - A captioned artifact emits `<fig:NAME>` in Typst and a matching
     `id` in HTML, in both the inside and outside placements.
   - `label=` overrides `name=`; a colliding `label=` warns by line
     and drops the second.
   - An uncaptioned artifact emits no label, and a reference to it
     warns and falls back to literal text.
   - A `fig:`-prefixed key matching no figure warns about a missing
     figure, never about a bibliography.
   - A document with figures and no bibliography still emits real
     Typst reference syntax, confirming the widened gate.
   - A reference to a figure hidden by `weave=hidden` warns and falls
     back.
   - The pre-pass numbers a table and an image on separate counters,
     and skips hidden and uncaptioned blocks.

3. A real-compile assertion in `tests/typst.rs`, read back with
   `pdftotext`, that the number HTML wrote matches the number Typst
   typeset for the same document. This is the only test that catches
   a per-kind counter drift, and it is the reason the per-kind rule
   is written down rather than assumed.

4. A manual smoke test against `dankg_weave_example`: a document with
   one produced table and one produced chart, a reference to each in
   prose, rendered both ways, confirming the HTML links jump to the
   right figure and the PDF numbers agree with the HTML.
