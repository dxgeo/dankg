# `dankg weave`: figure references

## Context

A woven document can already build a real, captioned figure. A
recognized pair (decision 46) whose source block declares
`produces=file:` and carries a caption wraps its artifact in a real
`#figure(...)` in Typst and a real nested `<figure>` in HTML
(decision 54), placed inside or outside the pair's own box
(decision 56). Typst numbers those figures in the PDF, on its own
`#figure` counter, with nothing counted by dankg. HTML does not
number them at all. Decision 54 offered a stylesheet a real element
to key a counter off, and no stylesheet took the offer: `WEAVE_CSS`
has no `counter-increment` in it, and carries no rule at all for the
`table-figure` and `image-figure` classes.

What no reader can write today is a reference to one. There is no way
to say "see Figure 3" in prose and have either backend resolve it. A
figure carries a caption, but it carries no name, so there is nothing
for a reference to point at.

This plan gives a figure a label, and gives prose a way to reference
that label in both backends.

<!-- dankg:depends target=../architecture.md#decision-54-a-produced-artifact-renders-as-a-real-captioned-figure quote="gives a reader genuine, automatic" -->

## Why this is not built on the citation syntax

An earlier draft built the reference on decision 57's own `@key`
citation syntax, written `@fig:chart`. That draft is abandoned. The
reason is worth recording, because the argument for it was
superficially strong.

`:` is inside decision 57's own key charset, so `@fig:chart` parses
today with no parser change at all. That is a real saving. It was
also the whole case for the syntax. The draft optimized for parser
cost and paid for it twice in meaning.

It paid once in `#cite`. `typst.rs` writes `@key` for a bracketed
`[@key]`, which parses as a non-narrative citation. It writes
`#cite(<key>, form: "prose")` for a bare `@key`, which parses as a
narrative one. `#cite` resolves against a bibliography, never against
a figure label. The bare form is the one that draft advertised in
every example, so every example would have reached Typst as the
wrong construct.

It paid again in the bibliography pre-pass. `weave::bibliography`
walks every `Inline::Citation`'s own keys. It warns on each key
missing from the bibliography. It pushes each key into the `order`
vector that *is* the citation numbering. A `fig:` key arriving there
emits a warning the design explicitly promised never to emit. It also
consumes a citation number, which shifts every later citation in the
document by one.

Neither of those is a hard problem. Both are self-inflicted. Both
disappear when a figure reference stops being a citation. The syntax
below borrows nothing from decision 57, and `Inline::Citation` is
untouched by this plan.

## What the probes established

Five probes ran against the real binary and against Typst 0.15.1
directly. Each one removed work from this plan or changed its shape.

**A same-file wikilink already resolves to the block.** `[[#chart]]`
parses as `Inline::WikiLink` with an empty name and the fragment
`chart`. `graph::resolve::find_slug` searches `file.nodes`, which
holds block nodes as well as headings. A two-file probe corpus
confirms the edge. A document containing `[[#chart]]` beside a block
written `name=chart` produces the real link edge
`doc2#doc-two -> doc2#chart` in `dankg graph --format json`. The
reference is therefore already meaningful to `dankg graph`, to
`dankg check`, and to the TUI's own cross-reference panel. An
`@fig:chart` key would have been invisible to all three.

**Weave renders that wikilink as plain text today.** The same probe
run through `dankg weave` produces the literal `#chart` in HTML and
the escaped `\#chart` in Typst. Decision 41's own single-file scope
is the stated reason: there is no corpus to resolve a target
against. That reasoning covers a cross-file target. It does not cover
a same-file fragment, which needs no corpus at all.

**The markdown link form already works in HTML.** The same probe
renders `[text](#chart)` as `<a href="#chart">text</a>` with no new
code whatsoever. Typst is the half that is wrong: it emits
`#link("#chart")[text]`, a URL link to the literal string, which is
meaningless in a PDF.

**Typst resolves a label two ways, and both produce real links.** A
real compile built four interleaved figures. Two are `kind: table`
and two are explicit `kind: image`, and each one carries a label.
Read back with `pdftotext`, they render `Table 1`, `Figure 1`,
`Table 2`, `Figure 2`. `@fig:revenue` and `#ref(<fig:costs>)` both
render the numbered string. `#link(<fig:chart>)[the revenue chart]`
compiles and renders the custom text. The PDF carries 12 `/Link`
entries against 6 `/Dest`, so every one of them is a real internal
link annotation rather than typeset text. Writing `kind: image` explicitly, rather than
leaving Typst to infer it, changes none of that -- the two counters
stay separate.

**An unresolvable reference is a hard build failure.** `@nope` and
`#link(<nope>)[text]` both fail to compile with
`label <nope> does not exist in the document`, exit 1, and no PDF is
written. Typst is right to refuse. Its message is useless to the
author, though, because it points into the generated `.typ`, a build
artifact nobody wrote by hand. So dankg resolves every reference
itself and fails first, on the markdown line the author can actually
find. Decision 64 sets that rule.

<!-- dankg:depends target=../src/render/typst.md#inline-text quote="single-file (decision 41), and there is no corpus to" -->

## What this reuses

**`Inline::WikiLink`, already parsed and already resolved.** A figure
reference is not a new inline construct, and it needs no parser
change. `[[#chart]]` and `[[#chart|the revenue chart]]` both parse
today into `WikiLink { target, label }`, whose two fields carry
exactly the two things a reference needs: what to point at, and what
text to show.

**Decision 54's own real figure element.** Both backends already emit
something a label can attach to. Typst emits `#figure(...)`, which
takes a trailing `<label>`. HTML emits a nested `<figure>`, which
takes an `id`. A heading in HTML already carries an `id` built from
`Slugger` via `heading_slugs`, threaded through `blocks()` as its own
parameter, so a figure id map rides a shape that is already there.

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
as `[[#chart]]` with no new attribute written at all.

A new `label` attribute joins `KNOWN_ATTRS` beside `caption` and
`figure`, and overrides that default for one block. It exists because
a block's own `name=` is also its eval identity and its tangle
identity, and renaming a block for a code reason should not have to
break every reference in the prose.

Only a captioned artifact is a figure at all (decision 54). A block
with a name but no caption renders unwrapped, has no figure to label,
and is not referenceable. A reference naming one fails the weave, the
same as a reference naming nothing.

A `label=` colliding with another block's own label warns by line and
the second one is dropped, matching decision 58's own second-fence
rule. Suffixing the collision the way `Slugger` suffixes a duplicate
heading is wrong here: a silently suffixed label is a reference that
silently points at the wrong figure. It warns rather than refuses,
which is where it parts from the `name=` collision it takes its
default from: `check_unique_names` returns a hard
`PlanError::DuplicateName`. Refusing to render a whole document over
one label typo is a harsher trade than refusing to run one eval
chain.

Each backend derives its own identifier from that one label. Typst
gets `<fig:chart>`, keeping the `fig:` prefix so a figure label can
never collide with a bibliography key in Typst's own single `@`
namespace. HTML gets `id="fig-chart"`, with a hyphen rather than a
colon, since a `:` in an id has to be escaped in every CSS selector
that wants to reach it. The prefix also keeps a figure id clear of
the heading ids `Slugger` already mints.

### Decision 64: A same-file wikilink resolves against whatever the document holds

Decision 41 narrows. Weave resolves a wikilink whose name half is
empty, because a same-file fragment needs no corpus. A wikilink
naming another file still renders as its own plain text, unchanged,
for the reason decision 41 already gives.

Every same-file fragment that finds a target resolves, not figure
labels alone. `find_slug` searches `file.nodes`, which holds headings
and blocks alike, so a rule admitting only figures would have to
carve an exception out of machinery that does not want one. A
wikilink that resolves becomes a link. That is the honest reading,
and it is the one this plan takes.

It is also strictly larger than the figure case. A
`[[#some-heading]]` that renders as plain text in an already-woven
document starts rendering as a link. Nothing renders as *less* than
it does today, so the change adds links rather than removing text.

`[[#chart]]` with no label renders as a reference whose text is the
figure's own number. `[[#chart|the revenue chart]]` renders as a
reference whose text is what the author wrote. That maps onto
`WikiLink`'s own two fields directly, with no new syntax invented and
nothing to disambiguate.

| written | Typst | HTML |
| --- | --- | --- |
| `[[#chart]]` | `@fig:chart` | `<a href="#fig-chart">Figure 3</a>` |
| `[[#chart\|text]]` | `#link(<fig:chart>)[text]` | `<a href="#fig-chart">text</a>` |

The markdown link form `[text](#chart)` resolves through the same
lookup and emits the same two things. HTML already renders it
correctly and needs no change at all. Typst's own `Inline::Link` arm
gains a branch: a fragment destination becomes a label reference
rather than the `#link("#chart")` URL link it writes today, which is
a bug on its own terms.

An unresolved reference fails the weave. It does not render as its
own literal source text, and it does not render as anything else.
`weave::run` reports it through `Diags::error` at the reference's own
line, and returns `Err`. No `.typ` is written, no HTML is written,
and no PDF is built.

Falling back was the earlier design here, and it was wrong. A
document that renders with a broken reference in it is a document an
author ships. The warning scrolls past, the page looks like prose,
and the reader is the one who finds the hole. That is the silent
staleness decision 47's own stale badge exists to prevent. This plan
arrives at the same rule from a different direction.

The run fails after the whole document is walked, never on the first
bad reference. An author who mistyped three labels wants all three
lines in one run, not three runs. Each one carries its own message,
because the next action differs in each: no block carries that name,
the block exists but has no caption and so is not a figure, and the
block is a figure but is hidden by `weave=hidden`.

Both backends are therefore handed references that already resolve.
Neither one needs a failure path of its own, and neither one can
disagree with the other about what resolved.

The third message is free. `weave=hidden` is applied inside each
backend -- `typst.rs` guards with `if !info.weave_hidden()` and
returns `None` for a hidden pair -- rather than by filtering the
document, so the block is still in `doc.blocks` when the pre-pass
walks it. The pre-pass records why it skipped each candidate, and the
resolver reads that back.

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
document.

That split is only half dankg's own today. `typst.rs` writes
`kind: table` for a table, and writes no kind at all for an image,
leaving Typst to infer one from a lone `#image(...)` body. A
pre-pass keyed on that would be predicting Typst's own inference
rather than mirroring a declaration dankg made, which holds only
while there are exactly two artifact shapes. So `typst.rs` starts
writing `kind: image` explicitly -- confirmed by probe to leave the
numbering unchanged -- and both backends count off the one
declaration. That lands before the pre-pass, not beside it.

Where the pre-pass sits inside `weave::run` is not free, for the same
reason decision 61's own heading drop is not. It must count exactly
the figures each backend will actually emit. So it runs after
`drop_repeated_title_heading`, and it skips a block hidden by
`weave=hidden` or `weave=output-hidden`, and it skips an uncaptioned
artifact, which decision 54 says is not a figure at all.

HTML's own figcaption number comes from the same pre-pass. That adds
to decision 54 rather than amending it. The decision offered a
stylesheet an element to key a counter off, no shipped stylesheet
took the offer, and the pre-pass therefore displaces nothing that
renders today.

### Decision 66: An unresolved citation fails the weave too

Decision 59 is amended. A citation key that a configured bibliography
does not carry fails the weave. `weave::bibliography` already finds
every such key and already warns on it by line. That warning becomes
an error, and `weave::run` returns `Err` once the walk is done.

This ends a disagreement the two backends have today, and a probe
confirms both halves of it. With a bibliography configured, Typst
refuses to compile a missing key and writes no PDF. That is decision
44's own "let the compiler's own error surface" stance working as
written. HTML renders the same key as an unlinked `[?]` for a
bracketed citation, or as literal `@key` text for a narrative one,
and writes a finished-looking page. So one document fails one way and
builds the other. Failing both ways is the only answer that keeps
them honest.

The failure is dankg's own, raised before either backend runs, for
the reason decision 64 already gives. Typst's message points into
generated `.typ`, and the author never wrote that file.

A document configuring no bibliography is untouched. There is nothing
to resolve a key against there, so there is no unresolved citation to
fail on, and decision 59's literal-text fallback stands for that case
exactly as written. This is not a softening. A bare `@` in ordinary
prose parses as a citation under decision 57b, which a probe
confirms: `Ping me @dan on the forum` warns that the citation key
`dan` is not in the bibliography. Failing a document that configures
no bibliography would fail every document that mentions a handle in
prose.

### Decision 67: Heading numbering belongs to the template, not to dankg

A bare `[[#some-heading]]` emits `@sec:...` in Typst, the same way a
bare `[[#chart]]` emits `@fig:...`. dankg does not turn on heading
numbering to make that work, and does not suppress the reference to
avoid needing it.

A probe shows what happens with no numbering set. Typst answers
`@sec:intro` against an unnumbered heading with
`cannot reference heading without numbering`, and writes no PDF. The
fix is one line in the author's own `[weave.pdf] template`:

```typst
#set heading(numbering: "1.")
```

That template is prepended to the generated `.typ`, pinned by
`a_configured_template_is_prepended_to_the_typ`, so the setting
applies document-wide. A probe confirms the reference then renders
`Section 1`.

This is not the case decision 64 and decision 66 fail on, and the
line between them is worth stating. An unresolved reference and an
unresolved citation are both errors in the markdown, which is the
document dankg owns. dankg can see them, so dankg reports them. An
unnumbered heading is not an error in the markdown at all. The
reference is correct and the target exists. What is missing is a
typesetting setting living in the template, which is the author's own
file. dankg cannot fix it and should not guess at it. Decision 44's
own "let a real incompatibility surface as the compiler's own error"
stance already covers this exactly.

The labelled form needs none of it. `[[#some-heading|the design]]`
becomes `#link(<sec:...>)[the design]`, which a probe confirms
compiles against an unnumbered heading. An author who wants a heading
reference without touching a template writes the label.

### Decision 68: A bare heading reference takes the heading's own title in HTML

A bare `[[#intro]]` renders in HTML as a link carrying the heading's
own title. `weave_html`'s own table of contents already emits exactly
that for the same target, so this is a rule the renderer follows
already, applied to one more construct.

HTML numbers no heading, which is why a number would be wrong here. A
woven page carries no counter rule at all, and every heading renders
bare. A reference reading `Section 1` would point at a heading with
no visible 1 anywhere near it. The reader would click `Section 1` and
land on `Intro`.

Decision 65's own "both backends must agree" rule does not reach this
case. That rule exists because a figure is numbered twice, by Typst's
own counter and by dankg's own pre-pass, and two counts must not
drift apart. A heading is numbered once, in the PDF alone. One source
and none is not two in conflict.

Counting headings inside dankg is refused for the reason decision 67
already gives. The template owns the numbering format, so `1.`, `A.`
and `I.` are all possible and dankg never sees which one is set. A
number dankg invented would match nothing on the HTML page, and
nothing in the PDF either.

A bare figure reference still takes its number in HTML, unchanged by
this (decision 65). That stays coherent, because the pre-pass puts
real numbers in the figcaptions a reference points at. The asymmetry
between the two is principled rather than an exception: HTML numbers
its figures, and does not number its headings.

An author wanting one string in both backends writes the label.
`[[#intro|the introduction]]` renders that same text in the PDF and
in HTML alike.

## What this breaks for documents that build today

One shape stops building. A document that configures a bibliography
and writes a bare `@` in ordinary prose now fails, where today it
warns and renders. `Ping me @dan on the forum` is that shape.

The fix is decision 57b's own escape, written `\@dan`. A probe
confirms it does the job. The escaped form raises no warning, renders
as `@dan` in the page, and survives `dankg fmt` with its backslash
intact.

That break is worth taking. The alternative on offer is a document
whose PDF failed to build for one reason, and whose HTML page renders
`[?]` beside it and looks finished.

## Settled while reviewing this plan

**The reference reads as a number.** The alternative was a link whose
text is the caption, which costs no counting and cannot disagree with
anything. It lost on two counts. The anchor plumbing `heading_slugs`
already threads through `blocks()` makes the HTML half of either
answer cost about the same, and a caption-text link cannot match what
Typst typesets in the PDF, so it trades an internal disagreement for
a cross-backend one. `[[#chart|the revenue chart]]` gives an author
who wants caption text a way to ask for it explicitly.

**`Inline::Citation` is not renamed.** The earlier draft raised this,
because riding the citation variant would have left it carrying a
construct that is not a citation. Nothing rides it now, so the
question is moot rather than deferred.

**An unresolved citation fails too.** This was an open question
while the reference rule was being written, on the grounds that a
bibliography is often incomplete mid-draft in a way a figure label is
not. Decision 66 answers it: the two rules match. The mid-draft case
is served by configuring no bibliography yet, which decision 66
leaves alone.

**Table references do not ship.** A markdown table is not a figure.
`table_block` emits a bare `#table(...)`, and the only
`#figure(kind: table, ...)` in `typst.rs` is the produced-artifact
path. A markdown table has no caption, no figure wrapper, and no info
string to write `caption=` or `label=` on, so labelling one means
extending decision 52 and decision 54 to a construct with nowhere to
put the attribute. Deferred rather than refused. Worth revisiting
once a document actually wants it.

**Heading references ship.** Decision 64 makes a heading resolve,
decision 67 hands PDF numbering to the template, and decision 68
gives HTML the heading's own title. The labelled form
`[[#some-heading|the design]]` needs no numbering in either backend,
which a probe confirms.

## Open questions

None. Every question this plan raised was settled in review, and each
answer is recorded above. The plan is ready to build.

## What this explicitly does not do

- Any parser change. `[[#chart]]` and `[text](#chart)` both parse
  today. `md/inline.rs` and `md/fmt.rs` are untouched.
- Anything to `Inline::Citation`, the bibliography pre-pass, or
  decision 57's own `@key` syntax. Citations and figure references
  are separate constructs after this plan, sharing no variant, no
  key space, and no numbering.
- A reference to anything that is not a captioned `produces=file:`
  artifact, unless question 2 or 3 is answered otherwise.
- Configurable supplement text. Typst's own `Figure`/`Table` wording
  stands in the PDF, and HTML matches it literally. A reader wanting
  `fig.` instead of `Figure` restyles Typst through a
  `[weave.pdf] template` today, and gets nothing in HTML.
- A references list, an index, or a list of figures. Typst's own
  `#outline(target: figure)` already builds the last one for a reader
  who asks for it in a template.
- Cross-file references. Weave renders one file (decision 41), and
  decision 64 narrows that only for a fragment with no file name in
  it.

<!-- dankg:depends target=../architecture.md#decision-41-weave-scope quote="Single-file only -- no corpus-wide walk exists yet." -->

## Critical files

- `src/md/mod.md` / `md/mod.rs` -- `KNOWN_ATTRS` gains `label`;
  `InfoString` gains `label()`. No `Inline` change at all.
- `src/weave.md` / `weave.rs` -- the numbering pre-pass, placed after
  `drop_repeated_title_heading`; the label map and the skip-reason
  map both renderers receive; the resolve pass that walks every
  reference, reports each unresolved one through `Diags::error`, and
  returns `Err` once the walk is done. `weave::bibliography` raises
  its existing missing-key warning to an error (decision 66), and is
  otherwise unchanged.
- `src/render/typst.md` / `typst.rs` -- a `<label>` appended to
  `#figure(...)` in both the inside and outside placements
  (decision 56); an explicit `kind: image` on an image figure, in
  place of Typst's own inference; a resolving branch in the
  `WikiLink` arm, which returns plain text today; a fragment branch
  in the `Link` arm, which writes a URL link today. The `Citation`
  arm keeps its two branches, since decision 66 makes the unresolved
  key unreachable rather than rendering it differently.
- `src/render/weave_html.md` / `weave_html.rs` -- an `id` on the
  nested `<figure>`; a resolving branch in the `WikiLink` arm; the
  figcaption's own number from the pre-pass. The `Link` arm already
  emits the right thing and is not touched. `citation_html`'s own
  `[?]` rendering becomes dead code under decision 66 and goes. A
  bare heading reference reuses the title text `toc` already writes
  (decision 68).
- `architecture.md` -- add decisions 63, 64, 65, 66, 67, 68; narrow
  decision 41's own wikilink sentence; amend decision 59's own
  fallback sentence to cover the no-bibliography case alone.
- `README.md` -- document `label=` and the `[[#name]]` reference
  form alongside the existing link table, plus the one
  `#set heading(numbering: ...)` line a template needs before a bare
  heading reference will compile. Document the one deliberate
  difference between the backends too: a bare heading reference reads
  as a number in the PDF and as the heading's own title in HTML
  (decision 68), and a label makes the two identical.

## Verification

1. Edit loop per CLAUDE.md, then `dankg fmt --check` and
   `dankg check .`.

2. Unit tests, both backends, matching the existing pair-per-case
   style:

   - A captioned artifact emits `<fig:NAME>` in Typst and a matching
     `id` in HTML, in both the inside and outside placements.
   - `label=` overrides `name=`; a colliding `label=` warns by line
     and drops the second.
   - `[[#chart]]` emits `@fig:chart` in Typst and a numbered link in
     HTML; `[[#chart|text]]` emits `#link(<fig:chart>)[text]` and a
     link carrying that text.
   - `[text](#chart)` resolves identically in both backends.
   - A wikilink naming another file still renders as plain text, so
     decision 41 is narrowed and not repealed.
   - The three unresolved cases -- no such block, block with no
     caption, figure hidden by `weave=hidden` -- each fail the weave
     with their own message, on the reference's own line.
   - A document with three bad references reports all three, and
     writes no output file of any kind.
   - The pre-pass numbers a table and an image on separate counters,
     and skips hidden and uncaptioned blocks.
   - An image figure carries an explicit `kind: image`, so neither
     backend relies on Typst's own inference to agree.
   - A citation key missing from a configured bibliography fails the
     weave in both backends, on its own line, and writes no output.
   - The same key in a document configuring no bibliography still
     renders as literal text and still builds.
   - An escaped `\@key` raises nothing and renders as `@key`, so the
     documented way out of decision 66 is pinned by a test.
   - `[[#some-heading|text]]` emits `#link(<sec:...>)[text]`, which
     needs no heading numbering to compile.
   - A bare `[[#some-heading]]` emits `@sec:...` unchanged, with no
     numbering set by dankg and no suppression of the reference.
   - The same reference in HTML carries the heading's own title, and
     matches byte for byte what `toc` writes for that heading.

3. A real-compile assertion in `tests/typst.rs`, read back with
   `pdftotext`, that the number HTML wrote matches the number Typst
   typeset for the same document. This is the only test that catches
   a per-kind counter drift, and it is the reason the per-kind rule
   is written down rather than assumed.

4. A test that no unresolved reference ever reaches Typst. The probes
   show one is a hard build failure there, with a message pointing
   into generated `.typ`. dankg failing first is what keeps the
   author's own error readable, so this pins the ordering rather
   than trusting it.

5. A manual smoke test against `dankg_weave_example`: a document with
   one produced table and one produced chart, a reference to each in
   prose, rendered both ways, confirming the HTML links jump to the
   right figure and the PDF numbers agree with the HTML.
