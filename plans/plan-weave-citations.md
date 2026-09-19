# `dankg weave`: citations and a Hayagriva bibliography

## Context

`dankg weave` turns one markdown file into a readable document, HTML
or PDF via Typst (decision 41). Neither backend today has any notion
of a citation or a bibliography: `@` is already escaped as ordinary
prose in the Typst backend (`escape_typst`'s own `#` `*` `_` `` ` `` `<`
`@` `$` `\` list) precisely because Typst reserves it for citation
syntax that nothing in dankg has ever emitted on purpose.

This plan gives a woven document real in-text citations and a real,
numbered references list in both backends, sourced from a bibliography
written in Typst's own native format -- Hayagriva YAML -- either
inline, in a fenced block, or as an external file named from
frontmatter. Confirmed with the user: Hayagriva YAML first (not
BibTeX, a later decision if it comes up); both an inline fenced block
*and* an external frontmatter-named file are in scope together, not
one instead of the other.

Confirmed with the user, and revised from this plan's own first
draft: the `hayagriva` fence gets no rendering behavior change at all.
It renders exactly like any other fenced block -- raw YAML text, shown
by default, hidden only if the author writes `weave=hidden` themselves
(decision 48, unmodified). The tag matters only to a new data
pre-pass that reads the block's text for its bibliography content;
that pre-pass runs whether or not the block itself ends up visible on
the rendered page. Typst owns real bibliography placement and
formatting once `#bibliography(...)` is emitted; dankg's only
remaining choice is *where in the document* that generated call goes,
and both backends make the identical choice -- always at the very
end, regardless of where the `hayagriva` fence sits, or whether the
bibliography came from frontmatter alone. A reader-chosen position for
that generated references list is a real future extension (mirroring
decision 56's own `--figures-inside`/`--figures-outside` precedent)
but out of scope here.

<!-- dankg:depends target=../architecture.md#decision-41-weave-scope quote="Weave never executes anything." -->

## What this reuses

**Decision 45's tag-driven, content-never-sniffed dispatch, and its
own existing fallback.** A `csv`/`tsv`/`json`-tagged fence becomes a
table purely because of its own language tag. Every other tag already
falls through to `code_block`'s own literal rendering, unchanged since
before this plan. A `hayagriva`-tagged fence takes that identical
existing fallback path in both backends -- no renderer change at all
is needed for the fence's own default visibility. The tag matters only
to weave's own new bibliography pre-pass (decision 59), which reads
the block's text for data, independent of whatever the renderer later
does with that same block.

<!-- dankg:depends target=../architecture.md#decision-45-data-tables-from-csvjsontsv-fenced-blocks quote="The language tag is the only signal; content is never sniffed." -->

**Decision 48's `weave=hidden`, unmodified.** An author who wants a
`hayagriva` fence out of the rendered page entirely already has the
identical lever any other block does -- `weave=hidden`, no new
attribute, no special-cased behavior for this one tag.

<!-- dankg:depends target=../architecture.md#decision-48-weavehidden-drops-a-block-from-woven-output quote="A block worth keeping in the source" -->

**Decision 50's fence-or-external-file duality.** A table's content
can come from a fence's own text or from a `produces=file:` artifact
read off disk, both converging on the identical `TableData` before
rendering. A bibliography's content works the same way here: a fenced
block's own text, or an external file named by a new frontmatter
`bibliography:` key, both converge on the identical parsed entry list
before either renderer sees it. Path resolution reuses
`plan::resolve_artifact` directly -- the same root-relative
resolution a `produces=file:PATH` artifact's own path already goes
through, called with the frontmatter value in place of a
`produces=file:`-stripped one.

<!-- dankg:depends target=../architecture.md#decision-50-a-producesfile-csvtsvjson-artifact-renders-as-a-table quote="never a parsed `TableData`" -->

**Decision 51's asset-copy-before-compile shape.** `render::typst`
never touches a filesystem (decision 51's own constraint, restated
below); `render_pdf` already copies image bytes into
`.dankg/build/weave/assets/` before spawning `typst compile`, since
Typst needs a real file at a real relative path. The resolved
bibliography's raw Hayagriva text gets the identical treatment:
written to `.dankg/build/weave/bibliography.yml` by `weave.rs`, never
by `render::typst`, before compilation.

<!-- dankg:depends target=../architecture.md#decision-51-a-producesfile-image-renders-as-a-real-image quote="`render::typst` only ever emits markup, never touches a filesystem" -->

**`frontmatter::Frontmatter::scalar`/`md/frontmatter.md`'s own "flat,
never guessed at" bar.** The new `bibliography:` frontmatter key is
read through the identical `scalar()` accessor `title` already goes
through -- no frontmatter parser change at all. The new bibliography
*file*'s own parser, `data::bib`, is a new module, not an extension of
`md/frontmatter.rs`: Hayagriva's real shape needs one level of nested
mapping (`parent:`) frontmatter's parser explicitly refuses at any
depth, so this is a sibling parser under the same discipline, not a
loosening of that one's own documented limit.

<!-- dankg:depends target=../src/md/frontmatter.md#markdown-frontmatter quote="Unsupported input warns with its line number and is skipped, never guessed at." -->

**`data/table.md`'s hand-rolled-reader precedent (decision 1: zero
crates).** `data::bib::parse` is a third hand-rolled reader alongside
`from_delimited`/`from_json`, not a YAML crate. It is deliberately not
a general YAML parser, the same way `md/frontmatter.rs` is not one
either -- a bounded, line-oriented grammar over a fixed, documented
subset of what Hayagriva files actually contain, detailed under
Decision 58 below.

<!-- dankg:depends target=../src/data/table.md#data-table quote="Two hand-rolled readers, no parsing crate for either" -->

**Decision 42/54's "free" numbering precedent.** Typst's `#figure`
counter already gives a woven PDF real "Table N"/"Figure N" numbering
that nothing in dankg counts by hand. HTML's own references list reuses
the identical idea one level up: an `<ol>` of references, numbered by
the browser itself, in citation order -- not a manually formatted "1."
prefix `weave_html` would otherwise have to keep in sync with the
in-text bracket numbers by hand.

<!-- dankg:depends target=../architecture.md#decision-54-a-produced-artifact-renders-as-a-real-captioned-figure quote="gives a reader genuine, automatic" -->

## What's new

### Decision 57: `[@key]` inline citation syntax and `Inline::Citation`

A new inline construct, recognized the same way `[[wikilink]]` is: a
`[` immediately followed by `@` opens it, one or more citation keys
separated by `; `, closed by `]` -- `[@netwok2019]` or
`[@netwok2019; @smith2020]`. A key matches
`[A-Za-z0-9_:.-]+`; anything else inside the brackets, or a bracket
that never closes on the same line, is not a citation and falls
through to ordinary bracket-as-literal-text handling, the same
fallback `close_bracket` already gives a bracket with no destination
(`bracket_without_destination_is_literal`). A bare, unbracketed `@key`
in running prose -- Pandoc's own narrative-citation form -- is
explicitly out of scope (see below): every citation here is bracketed.

A new `Inline::Citation { keys: Vec<String> }` variant joins `Inline`
in `md/mod.md`, parsed in `md/inline.md` by a `citation()` function
sitting alongside `wiki_link()`, triggered from `open_bracket` the
same way. `Inline::write_plain` renders it as `[key, key]`, joined by
`, ` -- heading titles and slugs have no reason to contain a citation,
but the fallback has to produce *something* rather than panic if one
ever does.

**Rationale:** `@key` is already Typst's own native citation shorthand
\-- adopting `[@key]` (Pandoc's own bracketed form, one character
different) costs a reader nothing new to learn who already knows
either tool, and needs no bracket-vs-bare disambiguation the way a
bare `@key` would against ordinary prose containing an email address
or a `@`-mention convention some other tool already uses in the same
file. Multiple keys separated by `;` matches both Pandoc's own
existing convention and Typst's own adjacent-citation merging
(`@a @b` renders as one bracketed group), so both backends read the
identical author intent off one syntax.

<!-- dankg:depends target=../src/md/mod.md#markdown quote="`[[target]]` or `[[target|label]]`. Not standard markdown." -->

### Decision 58: A `hayagriva`-tagged fence, or a frontmatter `bibliography:` path, as a document's bibliography source

A fenced code block tagged `hayagriva` is parsed by a new
`data::bib::parse(text, diags) -> Vec<BibEntry>`, read for its data
only. Rendering is untouched: like any lang tag `code_or_data_table`
does not specifically recognize, it falls through to `code_block`'s
own existing literal rendering in both backends, shown by default,
hidden only if the author writes `weave=hidden` themselves (see *What
this reuses*, above) -- no behavior change from how every other fenced
block already works. A document's frontmatter may additionally (or
instead) name `bibliography: PATH`,
a root-relative or file-relative path resolved through
`plan::resolve_artifact` and read off disk the same way a
`produces=file:` table already is (decision 50) -- an external,
shareable bibliography with no in-document fence at all. `Frontmatter`
gains `bibliography() -> Option<&str>`, reading the new key through
the existing `scalar()` accessor.

Both sources converge on one `Vec<BibEntry>` before either renderer
sees it, merged by `weave.rs`'s new bibliography pre-pass: the
external file's entries load first, the inline fence's entries load
second and win on a duplicate key -- the same "last wins" convention
`md/frontmatter.rs`'s own duplicate-key handling already established
\-- each duplicate warned about by line. At most one `hayagriva` fence
is recognized per document; a second one warns and is ignored, since
nothing here has a use case yet for more than one anchor point.

`BibEntry` is a fixed, named-field struct, not a generic key/value
map -- the exact fields Decision 59's two renderers need, nothing
more:

```
BibEntry {
    key: String,
    kind: Option<String>,          // `type:`
    title: Option<String>,
    authors: Vec<String>,          // scalar or list-of-scalar `author:`
    date: Option<String>,          // raw text, year extracted at render time
    container_title: Option<String>, // `parent.title`
    publisher: Option<String>,     // top-level, or `parent.publisher`
    volume: Option<String>,        // top-level, or `parent.volume`
    issue: Option<String>,         // top-level, or `parent.issue`
    page_range: Option<String>,
    url: Option<String>,
}
```

**The supported grammar, precisely, since this is not a general YAML
parser (decision 1) any more than `md/frontmatter.rs` is:**

- Column 0: `<key>:` opens an entry. A value on the same line is an
  error (warn, ignore the entry) -- an entry's fields are always
  indented.
- Column 2: `<field>: <scalar>`, `<field>: [<a>, <b>]` (an inline
  list, reusing frontmatter's own bracket-splitting convention), or a
  bare `<field>:` opening a block sequence at column 4.
- Column 4 under a bare column-2 `<field>:`: `- <scalar>` lines, one
  list item per line. A list item that is itself a mapping
  (`- given-name: ...`, Hayagriva's own structured-name form) is
  unsupported -- warned and dropped, the rest of the list kept.
- Column 4 under `parent:` specifically: `<subfield>: <value>` lines,
  the identical column-2 grammar one level down -- `type`, `title`,
  `publisher`, `location`, `volume`, `issue` recognized; nothing
  deeper (`parent.parent`) is read at all.
- Any recognized field name outside this exact shape (`serial-number`,
  `translator`, a `parent` more than one level deep, an indent that is
  not a multiple of 2) warns by line and is dropped -- the entry keeps
  every field that *did* parse, the same "keep what parsed, warn about
  what didn't" stance `md/frontmatter.rs` already holds line by line.

**Rationale:** Real Hayagriva files lean on `parent:` constantly --
it's how an article names its own journal, a chapter its own book --
so a flat-only subset would silently produce citations with no
container title for the ordinary case, not an edge case. One bounded
level of `parent:` is the smallest grammar that keeps that case
working without becoming a real recursive-descent YAML parser. A
structured author name (`- given-name: ... family-name: ...`) is
common enough in real Hayagriva files to be worth naming explicitly as
unsupported here, rather than silently mis-parsed -- an author who
needs it writes `author: Family, Given` instead, same as every other
unsupported YAML construct in this codebase gets a documented,
by-hand workaround rather than a guess.

<!-- dankg:depends target=../architecture.md#decision-50-a-producesfile-csvtsvjson-artifact-renders-as-a-table quote="warns on stderr and adds nothing to the page" -->

### Decision 59: Citations and a references list render in both weave backends

`weave.rs` gains a pre-pass, `bibliography(doc, entry_rel, root, diags) -> Option<Bibliography>`, run once before either renderer:
locate the (at most one) `hayagriva` fence and/or the frontmatter
`bibliography:` file, parse and merge them (decision 58), then walk
every inline in the whole document -- headings, paragraphs, list
items, table cells, the same full-document reach decision 41 already
gives weave -- collecting each `Inline::Citation`'s own keys in
first-appearance order. `Bibliography` holds the merged
`HashMap<String, BibEntry>`, the `Vec<String>` of cited keys in that
first-appearance order (this *is* the numbering, `Table`/`Figure`'s
own free-counter precedent one level up), and the raw Hayagriva text
to write to disk for Typst. A cited key absent from every parsed entry
warns by the citation's own line; an uncited entry is simply never
placed in the ordered list, matching Typst's own default
`#bibliography(...)` behavior (never `full: true`) exactly.

**Typst.** `render::typst::render` gains a `bibliography: Option<&BibliographySummary>` parameter carrying only what it needs --
the set of valid keys (to warn early, redundantly with but ahead of
Typst's own compile error) and the resolved asset path. `inline_text`
gains an `Inline::Citation` arm emitting the keys space-separated,
`@netwok2019 @smith2020` -- Typst's own citation shorthand, relying on
its own adjacent-citation merging for the multi-key case, no
`#cite(...)` call needed. `render::typst::render` appends one
`#bibliography("bibliography.yml")` call after every body block, the
document's own last line -- unconditionally at the end, never at the
`hayagriva` fence's own position (which still renders there as an
ordinary code block, decision 58, unless the author hid it), and
identically whether the bibliography came from that fence, from a
frontmatter path, or both. `render_pdf` (`weave.rs`) writes the merged
raw Hayagriva text to `.dankg/build/weave/bibliography.yml` before
compiling, the identical copy-before-compile shape `images` already
gets (decision 51). No bibliography configured at all: a `[@key]` in
the source renders as its own literal escaped bracket text,
`\[@key\]`, never speculative `@key` markup handed to a compiler with
nothing to resolve it against.

**HTML.** `weave_html::render` gains the full `Bibliography`. An
`Inline::Citation` renders `<span class="citation">[<a href="#ref-KEY">N</a>, ...]</span>`, one link per key, `N` its
resolved position in the cited-order list -- a key absent from every
parsed entry renders `[?]` instead, warned, rather than a link to
nowhere. `weave_html::render` appends an `<ol class="reference-list">`
after every body block, the document's own last element -- the same
unconditional end-of-document placement Typst gets, one `<li id="ref-KEY">` per cited key in citation order. The browser's own
list numbering *is* the citation numbering, matching every in-text
`N` with no number kept or recomputed by hand anywhere in
`weave_html.rs`. Each entry's own text is one fixed, hand-built format
\-- author list, `(year)`, title, container, volume/issue, pages,
publisher, a trailing link if `url` is present -- documented plainly
as exactly that: one simple, non-configurable, non-CSL format, not an
attempt at APA/MLA/Chicago accuracy. No bibliography configured: the
identical literal-bracket-text fallback Typst gets, `[@key]` shown
verbatim, warned.

**Rationale:** Typst already owns real citation resolution and
formatting once it has a file to read; dankg's own job there is
narrow -- pass `@key` through untouched, and get a real file onto
disk at the path referenced. HTML has no such engine to defer to
(decision 1 rules out linking a CSL/citeproc crate, and there is no
natural external-command escape hatch here the way `typst compile`/`duckdb` have, since this has to run inline during HTML
rendering rather than as a spawnable post-process) -- so HTML gets
dankg's own minimal formatter instead of feature parity with Typst's.
Numeric, `<ol>`-numbered, one fixed entry format is the smallest thing
that is still genuinely useful: a reader can find "\[3\]" in the
references list from the in-text link, which is the one property an
un-styled reference list absolutely has to have.

<!-- dankg:depends target=../architecture.md#decision-44-weave-pdf-via-typst quote="let a real incompatibility surface as the compiler's own error." -->
<!-- dankg:depends target=../architecture.md#decision-50-a-producesfile-csvtsvjson-artifact-renders-as-a-table quote="misconfigured is reported, not fatal" -->

## What this explicitly does not do

- BibTeX. Confirmed with the user: Hayagriva YAML first, a `.bib`
  reader is a later decision if it comes up, not bundled here.
- A bare, unbracketed `@key` narrative citation (Pandoc's own
  "as shown by @author" form, which conventionally drops the
  parentheses and shows the author's own name inline). Every citation
  here is the bracketed `[@key]` form; the narrative form would need
  dankg to reach into a resolved author's own name and splice it into
  running prose, real additional machinery with no clear use case
  raised yet.
- Real CSL styling for HTML: author-year vs. numeric choice, a
  journal-specific reference format, locale-aware name ordering. One
  fixed numeric format, described above, is the whole of it for now.
- `serial-number` (DOI/ISBN/ISSN), `translator`, `editor`, `genre`, or
  any Hayagriva field beyond the fixed set Decision 58 names, and any
  `parent` nested more than one level. Warned and dropped per entry,
  never guessed at, the same as every other construct outside a
  documented dankg markdown/frontmatter subset.
- More than one `hayagriva` fence per document, or a way to choose
  which of two fences' entries wins beyond the fixed
  "external-file-then-fence, fence wins a duplicate key" order. A
  second fence warns and is ignored.
- Any way to choose where the references list renders. It always
  appends at the document's own end in both backends, matching Typst's
  own conventional placement, the same fixed structural positioning
  Typst's own cover page and outline already get. A `--bibliography-inside`/`--bibliography-outside`-style flag, mirroring decision 56's own
  `--figures-inside`/`--figures-outside` precedent, is a real later
  decision if a reader actually wants the list somewhere else -- not
  bundled here.

## Critical files

- `src/md/mod.md` / `md/mod.rs` -- `Inline` gains `Citation { keys: Vec<String> }`; `Inline::write_plain` gains its fallback arm.
- `src/md/inline.md` / `md/inline.rs` -- a new `citation()` function
  beside `wiki_link()`, triggered from `open_bracket` the same way.
- `src/md/frontmatter.md` / `md/frontmatter.rs` -- `Frontmatter` gains
  `bibliography() -> Option<&str>` reading the new `bibliography` key
  through the existing `scalar()`.
- `src/data/bib.md` / `data/bib.rs` -- new module: `BibEntry`, the
  bounded Hayagriva-subset parser (`parse`), registered in
  `src/data/mod.md`.
- `src/weave.md` / `weave.rs` -- the new `bibliography()` pre-pass
  (locates/merges the two sources, walks every inline for citation
  order); `render_pdf` writes the merged raw text to
  `.dankg/build/weave/bibliography.yml` before compiling.
- `src/render/typst.md` / `typst.rs` -- `render` gains the
  `bibliography` parameter; `inline_text` gains the `Citation` arm;
  `#bibliography(...)` appended once, unconditionally, after all body
  blocks. No change to `block`'s own dispatch -- a `hayagriva` fence
  already falls through to `code_block` today, same as any other
  unrecognized lang tag.
- `src/render/weave_html.md` / `weave_html.rs` -- the identical shape:
  the `Citation` arm, the `<ol class="reference-list">` appended once
  after all body blocks, built from `Bibliography`. No dispatch change
  here either, for the identical reason.
- `architecture.md` -- add decisions 57, 58, 59.
- `README.md` -- document `[@key]` syntax, the `hayagriva` fence, and
  frontmatter `bibliography:`.

## Verification

1. Edit loop per CLAUDE.md, then `dankg fmt --check` and
   `dankg check .` over the whole corpus.
2. `data::bib::parse` unit tests: a minimal single-field entry; a
   scalar vs. list `author:`; one level of `parent:` populating
   `container_title`/`publisher`/`volume`/`issue`; a structured author
   list item warned and dropped, the rest of the list kept; an
   unsupported field (`serial-number`) warned and dropped, the rest of
   the entry kept; a `parent.parent` warned and ignored; a duplicate
   top-level key warned, last wins.
3. `md/inline.md` unit tests: `[@key]`; `[@a; @b]`; a malformed
   bracket (`[@]`, an unclosed `[@key`, an invalid character inside)
   falls back to literal text, matching `bracket_without_destination_is_literal`'s
   own precedent.
4. `weave.rs` bibliography pre-pass tests: a `hayagriva` fence alone;
   a frontmatter `bibliography:` path alone; both together with a
   duplicate key, fence winning; citation order across a document with
   citations inside a list item and inside a heading, not just a
   paragraph; a cited key absent from every entry, warned; an uncited
   entry never appearing in the ordered list.
5. Both backends, matching pair per existing style:
   - A `[@key]` with a resolvable bibliography: Typst emits
     `@key`; HTML emits a link to `#ref-key` with the right ordinal.
   - Multiple keys in one bracket: Typst emits both keys
     space-separated; HTML emits one bracketed group with both links.
   - No bibliography configured at all: both backends render the
     literal bracket text, never speculative citation markup.
   - A `hayagriva` fence with no `weave=hidden` renders as an ordinary
     code block, its raw YAML visible, in both backends -- no behavior
     change from any other fenced block. With `weave=hidden`, it
     disappears the same way any other hidden block already does.
     Either way, the generated references list/`#bibliography()` call
     still renders at the document's end, never at the fence's own
     position.
   - `render_pdf` writes `.dankg/build/weave/bibliography.yml`
     matching the merged raw text, mirroring the existing
     `produced_artifacts`/assets-copy test shape.
6. A manual smoke test against `dankg_weave_example`: a small
   Hayagriva fence with two or three entries, a couple of `[@key]`
   citations in prose, `dankg weave --format html` and `--format pdf`,
   confirming a real `typst compile` produces a PDF with a real
   references page and that the HTML's in-text links actually jump to
   the right `<li>`.
