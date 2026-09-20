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
*file*'s own parser is a new module pair (`data::yaml` and
`data::bib`, decision 58), not an extension of `md/frontmatter.rs`:
Hayagriva's real shape needs unbounded nested mappings (`parent:`,
which frontmatter's parser explicitly refuses at any depth) and list
items that are themselves mappings, so this is a sibling parser under
the same discipline, not a loosening of that one's own documented
limit.

<!-- dankg:depends target=../src/md/frontmatter.md#markdown-frontmatter quote="Unsupported input warns with its line number and is skipped, never guessed at." -->

**`data/table.md`'s hand-rolled-reader precedent (decision 1: zero
crates).** `data::yaml`/`data::bib` are a third and fourth hand-rolled
reader alongside `from_delimited`/`from_json`, not a YAML crate. They
are deliberately not a *general* YAML parser -- a bounded grammar over
exactly the constructs Hayagriva's own file format actually uses,
detailed under Decision 58 below, the same discipline `md/frontmatter.rs`
holds at a shallower depth.

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
(`bracket_without_destination_is_literal`).

A new `Inline::Citation { keys: Vec<String>, narrative: bool }`
variant joins `Inline` in `md/mod.md`, parsed in `md/inline.md` by a
`citation()` function sitting alongside `wiki_link()`, triggered from
`open_bracket` the same way, with `narrative: false`.
`Inline::write_plain` renders it as `[key, key]`, joined by `, ` --
heading titles and slugs have no reason to contain a citation, but the
fallback has to produce *something* rather than panic if one ever
does.

**Rationale:** `@key` is already Typst's own native citation shorthand
\-- adopting `[@key]` (Pandoc's own bracketed form, one character
different) costs a reader nothing new to learn who already knows
either tool. Multiple keys separated by `;` matches both Pandoc's own
existing convention and Typst's own adjacent-citation merging
(`@a @b` renders as one bracketed group), so both backends read the
identical author intent off one syntax.

<!-- dankg:depends target=../src/md/mod.md#markdown quote="`[[target]]` or `[[target|label]]`. Not standard markdown." -->

### Decision 57b: A bare, unbracketed `@key` as a narrative citation

Pandoc's own narrative-citation form -- `@key` with no surrounding
brackets, conventionally read aloud with the author's own name in the
sentence rather than set off in parentheses -- is also recognized, a
second trigger in `citation()` alongside the bracketed one, guarded by
one rule: the `@` is a citation only when it is *not* immediately
preceded by `[A-Za-z0-9_]`. `user@example.com` stays ordinary text,
because its `@` immediately follows the alphanumeric local part of an
email address. `(see @smith2020)`, `@smith2020 argues`, or a `@key` at
the very start of a line are all citations, because their `@` is
preceded by punctuation, whitespace, or nothing at all. This is a
single character of lookbehind, not content-sniffing -- the same
"one deterministic rule, no guessing" bar `data::bib::parse` (decision
58\) and `md/frontmatter.rs` both already hold.

A bare key is matched with the identical `[A-Za-z0-9_:.-]+` charset as
the bracketed form, greedily, then trimmed of any trailing `.`, `,`,
`;`, or `:` -- sentence punctuation immediately after a bare citation
(`Cited by @smith2020.`) would otherwise be swallowed into the key
itself, since `.` is already a legal key character and nothing bounds
a bare key the way `]` bounds a bracketed one. A bare citation always
holds exactly one key: there is no delimiter to bundle a second one
the way `[@a; @b]` does. Two adjacent bare citations (`@a @b`) parse
as two separate single-key `Inline::Citation` nodes, each with
`narrative: true`, not one two-key node -- citing two works together
in one narrative mention still needs the bracketed form.

**Rationale:** the ambiguity between a bare citation and an email
address turns entirely on what sits immediately to the left of the
`@`, never on anything inside the key itself -- an email's `@` always
has a local part directly beside it; a narrative citation's `@` never
does. That makes the lookbehind exact rather than a heuristic that
could misfire, and keeps `citation()` a grammar rule instead of a
sniff over surrounding prose.

### Decision 58: A `hayagriva`-tagged fence, or a frontmatter `bibliography:` path, as a document's bibliography source -- full Hayagriva schema fidelity

A fenced code block tagged `hayagriva` is parsed by a new
`data::bib::parse(text, diags) -> Vec<BibEntry>`, read for its data
only. Rendering is untouched: like any lang tag `code_or_data_table`
does not specifically recognize, it falls through to `code_block`'s
own existing literal rendering in both backends, shown by default,
hidden only if the author writes `weave=hidden` themselves (see *What
this reuses*, above) -- no behavior change from how every other fenced
block already works. A document's frontmatter may additionally (or
instead) name `bibliography: PATH`, a root-relative or file-relative
path resolved through `plan::resolve_artifact` and read off disk the
same way a `produces=file:` table already is (decision 50) -- an
external, shareable bibliography with no in-document fence at all.
`Frontmatter` gains `bibliography() -> Option<&str>`, reading the new
key through the existing `scalar()` accessor.

Both sources converge on one `Vec<BibEntry>` before either renderer
sees it, merged by `weave.rs`'s new bibliography pre-pass: the
external file's entries load first, the inline fence's entries load
second and win on a duplicate key -- the same "last wins" convention
`md/frontmatter.rs`'s own duplicate-key handling already established
\-- each duplicate warned about by line. At most one `hayagriva` fence
is recognized per document; a second one warns and is ignored, since
nothing here has a use case yet for more than one anchor point.

Confirmed with the user: dankg supports everything Hayagriva's own
format supports, not a curated subset -- every field Hayagriva
documents parses, and (decision 59) every field HTML's reference-list
formatter can meaningfully show, renders. This is a materially
different grammar from a flat, two-level field matcher, because
Hayagriva's real schema needs recursion in two independent places: a
structured person can appear as a list item that is itself a mapping
(`author:` `- name: ... given-name: ...`), and `parent` nests without
a depth limit -- an article's `parent` is an issue, whose own `parent`
is a journal, whose own `parent` can be a publisher series, and
`parent` itself can hold either one entry or a list of entries. Both
are ordinary YAML block constructs at increasing indentation; nothing
here bounds how deep either goes in a real file.

**Two layers, not one.** `data::yaml::parse(text, diags) -> YamlNode`
(new module, sibling to `data::bib`) is a generic, indentation-driven
tree parser with no notion of Hayagriva's own field names at all --
block mappings and sequences at any depth, `key: [a, b]`/`key: {a: b}`
inline flow forms, single- and double-quoted scalars with backslash
escapes, and `#` comments stripped outside quotes:

```
enum YamlNode {
    Scalar(String),
    Seq(Vec<YamlNode>),
    Map(Vec<(String, YamlNode)>),
}
```

`data::bib::from_yaml(YamlNode, diags) -> Vec<BibEntry>` is the schema
layer on top, walking that tree against Hayagriva's own documented
field names. This mirrors the split `md/mod.rs` already has over
`md/inline.rs` -- a general structural parser underneath a
domain-specific reader -- rather than one function doing both jobs.

**What "bounded" still means, even at full fidelity.** `data::yaml` is
still not a general YAML implementation (decision 1) -- it supports
exactly the constructs Hayagriva's own file format actually uses:
block and flow collections, quoted and bare scalars, comments. It does
not support anchors/aliases (`&x`/`*x`), tags (`!!str`), multi-document
files (`---`/`...`), or merge keys (`<<:`), none of which Hayagriva's
own file-format spec uses, so none of which "everything Hayagriva
supports" requires. An author who hand-writes one of these gets a
warning by line and that node dropped, the same "warn and drop, never
guess" stance every other parser in this codebase already holds. A
consequence worth naming: because nothing here supports aliases, a
`parent` chain built by this parser can never contain a cycle -- every
nested entry is a literal, freshly-parsed node, not a reference back
to an earlier one, so no cycle guard is needed anywhere in `data::bib`.

**`BibEntry`, restructured for the full field set** (field names and
the structured-person/serial-number shapes below read off Hayagriva's
own published file-format documentation):

```
struct Person {
    name: String,              // family name -- the one required part
    given_name: Option<String>,
    prefix: Option<String>,
    suffix: Option<String>,
    alias: Option<String>,
}

struct Affiliated {
    person: Person,
    role: Option<String>,      // "translator", "director", "illustrator", ...
}

enum SerialNumber {
    Plain(String),
    Structured {
        doi: Option<String>, isbn: Option<String>, issn: Option<String>,
        pmid: Option<String>, pmcid: Option<String>, arxiv: Option<String>,
        serial: Option<String>,
    },
}

struct BibEntry {
    key: String,
    kind: Option<String>,          // `type:` -- free string, not a closed enum
    title: Option<String>,
    authors: Vec<Person>,
    editors: Vec<Person>,
    affiliated: Vec<Affiliated>,   // covers translator/foreword/illustrator/etc
    date: Option<String>,
    publisher: Option<String>,
    location: Option<String>,
    organization: Option<String>,
    edition: Option<String>,
    genre: Option<String>,
    volume: Option<String>,
    volume_total: Option<String>,
    issue: Option<String>,
    chapter: Option<String>,
    page_range: Option<String>,
    page_total: Option<String>,
    time_range: Option<String>,
    runtime: Option<String>,
    serial_number: Option<SerialNumber>,
    url: Option<String>,
    url_date: Option<String>,
    archive: Option<String>,
    archive_location: Option<String>,
    call_number: Option<String>,
    language: Option<String>,
    note: Option<String>,
    abstract_: Option<String>,     // `abstract` -- reserved word, field renamed
    parent: Vec<BibEntry>,         // Hayagriva's own `parent`: one entry or a
                                    // list, unlimited nesting depth either way
}
```

`author`/`editor` each accept a bare scalar, a list of scalars, or a
list where any item is itself a structured mapping
(`name`/`given-name`/`prefix`/`suffix`/`alias`) -- every item in one
list may independently be either shape. A scalar person string is
`Family, Suffix, Given` or `Family, Given` (the middle segment
optional): split on comma, a single remaining segment is
`given_name`, two remaining segments are `suffix` then `given_name` in
that order. `affiliated` is always a list of `{person, role}` --
Hayagriva has no separate top-level `translator` field; a translator
is an affiliated person with `role: translator`. `serial-number` is
either a bare scalar (`Plain`) or a mapping with any subset of
`doi`/`isbn`/`issn`/`pmid`/`pmcid`/`arxiv`/`serial` (`Structured`).
`parent` is either one mapping or a list of mappings, each recursively
a full `BibEntry` by the identical grammar -- an issue's own `parent`
names its journal, and so on with no depth limit.

**Rationale:** the recursive two-layer split keeps `data::yaml`
genuinely reusable and genuinely bounded -- exactly as general as
Hayagriva's real file format needs and no further, the same discipline
`md/frontmatter.rs` already holds at a shallower depth. Splitting
structural parsing from schema mapping also means a future second
schema on top of the same generic tree (nothing currently planned, but
the shape falls out for free) would not need a second hand-rolled
indentation parser.

<!-- dankg:depends target=../src/data/table.md#data-table quote="Two hand-rolled readers, no parsing crate for either" -->
<!-- dankg:depends target=../src/md/frontmatter.md#markdown-frontmatter quote="Unsupported input warns with its line number and is skipped, never guessed at." -->

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

**Typst.** Unchanged in shape from this plan's original design:
`render::typst::render` gains a `bibliography: Option<&BibliographySummary>`
parameter carrying only what it needs -- the set of valid keys (to
warn early, redundantly with but ahead of Typst's own compile error)
and the resolved asset path. `inline_text` gains an `Inline::Citation`
arm: a non-narrative citation emits the keys space-separated,
`@netwok2019 @smith2020` -- Typst's own citation shorthand, relying on
its own adjacent-citation merging for the multi-key case, no
`#cite(...)` call needed. A narrative citation (`narrative: true`,
decision 57b) emits `#cite(<key>, form: "prose")` instead -- Typst's
`@key` shorthand only ever produces its own default (non-prose)
citation form, so a narrative citation needs the explicit `#cite(...)`
call to get Typst's own prose rendering (author name inline, no
enclosing parenthetical) rather than dankg reimplementing author-year
formatting itself. `render::typst::render` appends one
`#bibliography("bibliography.yml")` call after every body block, the
document's own last line -- unconditionally at the end, never at the
`hayagriva` fence's own position, and identically whether the
bibliography came from that fence, from a frontmatter path, or both.
`render_pdf` (`weave.rs`) writes the merged raw Hayagriva text to
`.dankg/build/weave/bibliography.yml` before compiling, the identical
copy-before-compile shape `images` already gets (decision 51). No
bibliography configured at all: a `[@key]` or bare `@key` in the
source renders as its own literal escaped text -- `\[@key\]` or `@key`
respectively -- never speculative citation markup handed to a
compiler with nothing to resolve it against.

Decision 58's widened parser barely touches this paragraph, and that
is the point: Typst reads the real Hayagriva file itself and does its
own full-schema formatting -- `render::typst` never touches an
entry's fields beyond its `key`. Full fidelity for the PDF path was
already close to free before this plan reached for it explicitly; the
only place the wider grammar matters here is that "the set of valid
keys" is now extracted by walking the full recursive tree, so a key is
never lost to a construct the old flat grammar would have choked on.

**HTML.** `weave_html::render` gains the full `Bibliography`. Citation
rendering is unchanged from decisions 57/57b: a non-narrative
`Inline::Citation` renders `<span class="citation">[<a href="#ref-KEY">N</a>, ...]</span>`, one link per key, `N` its resolved position
in the cited-order list, `[?]` for a key absent from every parsed
entry; a narrative citation renders dankg's own fixed author-year link
(`<a href="#ref-KEY">Smith (2020)</a>`, built from the resolved
entry's first author -- see below -- with the unresolved-key case
falling back to the literal `@key` text, unlinked, warned). What
changes is the entry's own rendered text in
`<ol class="reference-list">`: still one fixed, hand-built,
non-configurable, non-CSL format -- not an attempt at APA/MLA/Chicago
accuracy, and not varying by entry `kind` -- but now covering every
field `BibEntry` can carry, each shown only when present, in one fixed
order: authors, then `(year)` (from `date`'s leading four digits, the
same extraction the narrative-citation formatter uses), title,
editors (`ed. Family, Family`) and any `affiliated` persons
(`role: Family`, one per entry) if present, the container chain --
walking `parent` outward one step per level (`In *Journal Name*, vol. N, no. M` for an issue-then-journal chain, continuing outward for any
further `parent` on that journal entry itself; an entry with more than
one `parent` -- Hayagriva allows a list -- shows the first as the
primary container and any further ones appended as `also in: ...`) --
edition, location, organization, publisher, page-range/page-total,
volume-total, chapter, time-range/runtime, language, genre, a
`serial-number`'s `doi` rendered as a link
(`https://doi.org/<doi>`) alongside any bare
`isbn`/`issn`/`pmid`/`pmcid`/`arxiv`/`serial` shown as labeled plain
text, `url` as a trailing link with `url_date` beside it as `(accessed <date>)`, then archive/archive-location/call-number, and finally
note/annotation/`abstract` as trailing free text. `weave_html::render`
appends the `<ol class="reference-list">` after every body block, the
document's own last element -- the same unconditional end-of-document
placement Typst gets, one `<li id="ref-KEY">` per cited key in
citation order (narrative and bracketed citations of the same key
share one entry and one ordinal). The browser's own list numbering
*is* the citation numbering, matching every in-text `N` with no number
kept or recomputed by hand anywhere in `weave_html.rs`. The
author-year formatter driving both the narrative-citation link and an
entry's own author list uses the identical family-name extraction
(text before the first comma in a `Person`'s scalar form, or its
structured `name` field directly): two authors join as
`Family & Family`, three or more as `Family et al.`. No bibliography
configured at all: the identical literal-text fallback Typst gets,
`[@key]` or `@key` shown verbatim, warned.

**Rationale:** Typst already owns real citation resolution and
formatting once it has a file to read; dankg's own job there is
narrow -- pass `@key` through untouched, and get a real file onto
disk at the path referenced. HTML has no such engine to defer to
(decision 1 rules out linking a CSL/citeproc crate, and there is no
natural external-command escape hatch here the way `typst compile`/`duckdb` have, since this has to run inline during HTML
rendering rather than as a spawnable post-process) -- so HTML gets
dankg's own complete field-by-field formatter instead of feature
parity with Typst's. "Complete" here means every field shows when
present, not that the format adapts per `kind` the way a real citation
style would -- most fields are simply absent on most entries, so a
uniform fixed order already produces a reasonable-looking entry per
`kind` without dankg ever branching on what kind of source it is.
Numeric, `<ol>`-numbered, one fixed entry format is the smallest thing
that is still genuinely useful: a reader can find "\[3\]" in the
references list from the in-text link, which is the one property an
un-styled reference list absolutely has to have -- full fidelity means
that entry can now show everything the bibliography actually recorded
about it, not that dankg is reimplementing a citation-style engine.

<!-- dankg:depends target=../architecture.md#decision-44-weave-pdf-via-typst quote="let a real incompatibility surface as the compiler's own error." -->
<!-- dankg:depends target=../architecture.md#decision-50-a-producesfile-csvtsvjson-artifact-renders-as-a-table quote="misconfigured is reported, not fatal" -->

## What this explicitly does not do

- BibTeX. Confirmed with the user: Hayagriva YAML first, a `.bib`
  reader is a later decision if it comes up, not bundled here.
- Bare-form multi-key citations. A bare `@key` (decision 57b) always
  holds exactly one key -- there is no delimiter to bundle a second
  one the way `[@a; @b]` does. Citing two or more works together in
  one narrative mention still needs the bracketed form.
- Real CSL styling for either backend: locale-aware name ordering, a
  journal-specific author-year format, "and" vs. "&" as a configurable
  choice, or any per-`kind` template. HTML's fixed `Family (year)` /
  `Family & Family (year)` / `Family et al. (year)` narrative shape
  and its fixed, uniform field order for a full reference-list entry
  (decision 59) are the whole of it -- every field Hayagriva supports
  now parses and can render, but the order and wording those fields
  render in never varies by entry `kind`, source language, or any
  other configurable rule.
- General YAML: anchors/aliases (`&x`/`*x`), tags (`!!str`),
  multi-document files (`---`/`...`), or merge keys (`<<:`). None of
  these are part of Hayagriva's own file-format spec, so "everything
  Hayagriva supports" (decision 58) does not require them -- an author
  who writes one gets a warning by line and that node dropped, the
  same as any other construct outside `data::yaml`'s documented
  subset.
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

- `src/md/mod.md` / `md/mod.rs` -- `Inline` gains
  `Citation { keys: Vec<String>, narrative: bool }`; `Inline::write_plain`
  gains its fallback arm.
- `src/md/inline.md` / `md/inline.rs` -- a new `citation()` function
  beside `wiki_link()`: the bracketed trigger from `open_bracket`
  (`narrative: false`), plus a second bare-`@` trigger anywhere in a
  text run, guarded by the not-preceded-by-`[A-Za-z0-9_]` lookbehind
  and the trailing-punctuation trim (decision 57b, `narrative: true`).
- `src/md/frontmatter.md` / `md/frontmatter.rs` -- `Frontmatter` gains
  `bibliography() -> Option<&str>` reading the new `bibliography` key
  through the existing `scalar()`.
- `src/data/yaml.md` / `data/yaml.rs` -- new module: `YamlNode`, the
  generic bounded YAML-subset tree parser (`parse`), registered in
  `src/data/mod.md`.
- `src/data/bib.md` / `data/bib.rs` -- new module: `BibEntry`,
  `Person`, `Affiliated`, `SerialNumber`, and the Hayagriva-schema
  mapper (`from_yaml`) consuming a `YamlNode` from `data::yaml`.
- `src/weave.md` / `weave.rs` -- the new `bibliography()` pre-pass
  (locates/merges the two sources, walks every inline for citation
  order); `render_pdf` writes the merged raw text to
  `.dankg/build/weave/bibliography.yml` before compiling.
- `src/render/typst.md` / `typst.rs` -- `render` gains the
  `bibliography` parameter; `inline_text` gains the `Citation` arm,
  branching on `narrative` between the `@key` shorthand and
  `#cite(<key>, form: "prose")`; `#bibliography(...)` appended once,
  unconditionally, after all body blocks. No change to `block`'s own
  dispatch -- a `hayagriva` fence already falls through to
  `code_block` today, same as any other unrecognized lang tag.
- `src/render/weave_html.md` / `weave_html.rs` -- the identical
  `Citation`-arm shape (bracketed `[N]` link vs. the fixed
  author-year link), plus the new full-field `<li id="ref-KEY">`
  formatter (author/editor/affiliated lists, the walked `parent`
  chain, `serial-number` rendering, and every remaining scalar field)
  and the `<ol class="reference-list">` appended once after all body
  blocks, built from `Bibliography`. No dispatch change here either,
  for the identical reason.
- `architecture.md` -- add decisions 57, 57b, 58, 59.
- `README.md` -- document `[@key]` and bare `@key` syntax, the
  `hayagriva` fence, and frontmatter `bibliography:`.

## Verification

1. Edit loop per CLAUDE.md, then `dankg fmt --check` and
   `dankg check .` over the whole corpus.

2. `data::yaml::parse` unit tests: nested block mappings three or more
   levels deep; a block sequence; inline flow `[a, b]` and `{a: b}`;
   single- and double-quoted scalars with escaped quotes; a `#`
   comment stripped outside quotes but preserved inside one; an
   anchor/alias/tag/multi-document marker warned by line and the node
   dropped, the rest of the document kept.

   `data::bib::from_yaml` unit tests: a minimal single-field entry; a
   scalar vs. list `author:`; a structured author list item
   (`name`/`given-name`/`prefix`/`suffix`/`alias`) alongside a plain
   scalar item in the same list; a scalar person string with the
   optional suffix segment (`Family, Suffix, Given`); an `editor` list;
   an `affiliated` list with two different roles (e.g. `translator`,
   `director`); `serial-number` as a plain string and, separately, as a
   structured map with several of `doi`/`isbn`/`issn`/`pmid`/`pmcid`/`arxiv`/`serial`
   present; `parent` as a single mapping two levels deep (article ->
   issue -> journal); `parent` as a list of two entries; every
   remaining scalar field (`edition`, `genre`, `location`,
   `organization`, `page-total`, `volume-total`, `chapter`,
   `time-range`, `runtime`, `archive`, `archive-location`,
   `call-number`, `language`, `note`, `abstract`) round-tripping into
   its `BibEntry` field; a duplicate top-level key warned, last wins.

3. `md/inline.md` unit tests: `[@key]`; `[@a; @b]`; a malformed
   bracket (`[@]`, an unclosed `[@key`, an invalid character inside)
   falls back to literal text, matching `bracket_without_destination_is_literal`'s
   own precedent. Bare-form tests: `@key` at start of line, after
   whitespace, and after `(` all parse as `narrative: true`;
   `user@example.com` and `foo_bar@x` do not parse as citations at
   all (the `@` is preceded by an alphanumeric/`_` character);
   `Cited by @smith2020.` trims the trailing period out of the key;
   `@a @b` parses as two single-key narrative citations, not one
   two-key node.

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
   - A bare narrative `@key` with a resolvable bibliography: Typst
     emits `#cite(<key>, form: "prose")`; HTML emits
     `<a href="#ref-key">Family (year)</a>`. A two-author entry and a
     three-plus-author entry each get their own HTML test
     (`Family & Family (year)` / `Family et al. (year)`).
   - A bare narrative `@key` whose key resolves to no entry: both
     backends fall back to the literal `@key` text, warned, distinct
     from the bracketed form's `[?]`/`\[@key\]` fallback.
   - No bibliography configured at all: both backends render the
     literal source text verbatim -- bracket text for `[@key]`, bare
     `@key` for the narrative form -- never speculative citation
     markup.
   - A `hayagriva` fence with no `weave=hidden` renders as an ordinary
     code block, its raw YAML visible, in both backends -- no behavior
     change from any other fenced block. With `weave=hidden`, it
     disappears the same way any other hidden block already does.
     Either way, the generated references list/`#bibliography()` call
     still renders at the document's end, never at the fence's own
     position.
   - `render_pdf` writes `.dankg/build/weave/bibliography.yml`
     matching the merged raw text byte-for-byte, mirroring the
     existing `produced_artifacts`/assets-copy test shape -- this is
     what makes Typst's own full-fidelity formatting possible
     regardless of how deep or structured the source entry is,
     independent of anything `data::bib` itself parsed.
   - HTML full-field rendering: an entry with an `editor` and an
     `affiliated` translator renders both credit lines; a two-level
     `parent` chain (article/issue/journal) renders its container
     hierarchy in one `In *Journal*, vol. N, no. M` line; an entry with
     two `parent` entries renders the second as `also in: ...`; a
     structured `serial-number` renders a `doi` link plus plain
     `isbn`/`issn` text; a `url` with `url_date` renders `(accessed <date>)` beside the link.

6. A manual smoke test against `dankg_weave_example`: a small
   Hayagriva fence with two or three entries, a couple of `[@key]`
   citations in prose, `dankg weave --format html` and `--format pdf`,
   confirming a real `typst compile` produces a PDF with a real
   references page and that the HTML's in-text links actually jump to
   the right `<li>`.
