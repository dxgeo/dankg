# `dankg weave`: caption= and splitting a pair's own two halves

## Context

Decision 46 pairs a recorded eval result with its source into one
`<figure>`/`#block(...)`, and decisions 50/51 append a `produces=file:`
artifact (table or image) inside that same figure. Every caption in
either backend today is synthesized, never authored: the eval-result
caption is always literally `Output` or `Output (failed)`, and the
artifact caption is just the raw `produces=file:PATH` string echoed
back verbatim -- a path, not a description.

<!-- dankg:depends target=../architecture.md#decision-46-a-recorded-eval-result-renders-paired-with-its-source quote="renders as one visual unit in both weave backends instead of three unrelated blocks" -->

Decision 48's `weave=hidden` is the only lever on a pair's own
visibility today, and it is all-or-nothing: source, output, artifact,
and provenance disappear together, "absent as if it were never in the
document."

<!-- dankg:depends target=../architecture.md#decision-48-weavehidden-drops-a-block-from-woven-output quote="absent as if it were never in the document" -->

This plan closes both gaps: a `caption=` attribute that overrides a
pair's own synthesized caption text, and two new `weave=` values that
split a pair into its two independent halves -- source and result --
so either can be hidden while the other still renders. Confirmed with
the user: `caption=` reuses `cmd::split`'s existing quote-aware
tokenizer rather than a new one, and the two new `weave=` values are
named `source-hidden` and `output-hidden`, one for each half.

## What this reuses

**`cmd::split` (decision 1: zero crates).** `caption=` text is
reader-facing prose and needs to carry spaces
(`caption="Quarterly revenue"`), but `parse_info`'s own
`raw.split_whitespace()` cannot tell a quoted space from a word
boundary. `depends.md`'s own `dankg:depends quote="..."` marker
already solved the identical problem by tokenizing through
`cmd::split` instead of `split_whitespace`, for the identical
reason: a `quote` value almost always has a space in it too, so it
is written quoted.

<!-- dankg:depends target=../src/depends.md#the-marker quote="so it is written quoted" -->

`cmd::split` already strips the surrounding quotes as it tokenizes, so
`parse_info` needs no separate unquoting step: swapping its tokenizer
is the entire parser change. Its own known limitation carries over
unchanged, undocumented a second time nowhere else: "no escape
sequences beyond that" -- a caption containing a literal `"` is not
supported, the same gap every other `cmd::split` caller already lives
with.

<!-- dankg:depends target=../src/cmd.md#split quote="No escape sequences beyond that, and no shell features" -->

**Decision 48's own `weave` attribute and `InfoString::weave_hidden()`
pattern.** `source-hidden`/`output-hidden` are two more recognized
values of the same `weave=` key, read the same loose way
`weave_hidden()` already reads `hidden`: the one recognized value
wins, anything else is silently ignored. No new attribute, no new
`KNOWN_ATTRS` entry beyond `caption` itself.

**`eval_pair` in both `render/weave_html.md` and `render/typst.md`.**
Both already build a pair's figure/block as an ordered sequence:
source, then `Output`/`Output (failed)` caption and text, then an
optional table/image, then provenance. The two new `weave=` values
gate exactly two contiguous slices of that existing sequence; nothing
about the sequence itself changes.

## What's new

### Decision 52: `caption=` overrides a pair's own synthesized caption

`InfoString::caption()` reads a new `caption` key, added to
`KNOWN_ATTRS` right after `weave`. When present on a recognized pair's
source block, it replaces whichever caption is the pair's own
*payload* label -- the artifact's caption when a `produces=file:`
table or image is present, otherwise the eval-result caption. It never
replaces both: a pair with both a captured stdout line and a produced
chart would otherwise show the identical author-written caption
twice, once per figcaption, which reads as a mistake rather than
intent.

Concretely, in `eval_pair` (both backends): if `table`/`image` is
`Some`, the artifact's figcaption becomes `caption` verbatim instead
of the raw `produces=file:PATH` string; the `Output`/`Output (failed)`
label is untouched. If neither artifact is present, the eval-result
caption becomes `caption` instead -- with ` (failed)` appended when
`pair.failed`, so a custom caption never silently drops the one signal
that already exists nowhere else as visible text (the CSS class/stroke
color also carries it, but a reader scanning captions alone should
still see it).

`dankg fmt`'s `info_text` gains one rule: a known attribute's value
containing whitespace is written back quoted
(`caption="Quarterly revenue"`), every other attribute unchanged since
none of them can contain whitespace today. `parse_info` swaps
`raw.split_whitespace()` for `crate::cmd::split(raw)`; `handle_attr`'s
own `word.split_once('=')` needs no change, since `cmd::split` already
hands back `caption=Quarterly revenue` as one token with the quotes
already stripped.

### Decision 53: `weave=source-hidden` and `weave=output-hidden` split a pair's own two halves

A recognized pair's figure is, structurally, two halves: the source
block, and everything after it (the `Output`/`Output (failed)`
caption and text, any `produces=file:` artifact, and the provenance
line). `weave=hidden` (decision 48) already hides both halves
together. Two new values each hide exactly one half, leaving the
other exactly as it renders today:

- `weave=source-hidden`: the source's own `code_or_data_table` is
  skipped. The result half -- caption, output text, artifact,
  provenance -- renders unchanged.
- `weave=output-hidden`: the result half is skipped in full --
  caption, output text, artifact, *and* provenance all disappear
  together, not just the captured stdout. The source renders
  unchanged.

Both are read off the source block's own `weave=` value, the same
attribute `weave_hidden()` already reads -- `InfoString::weave_source_hidden()`
and `weave_output_hidden()`, sibling accessors reading the same loose
way. Since `weave=` holds one value, the two are mutually exclusive by
construction; wanting both halves gone is already `weave=hidden`, not
a new combination to support.

**Unpaired blocks.** A lone named block with no recorded result has no
second half for `output-hidden` to preserve or `source-hidden` to
uncover, so each value's effect on `block()`'s own unpaired dispatch
differs: `source-hidden` hides the lone block entirely, the same
fallback `hidden` already gets, since "hide the source, keep the
result" is meaningless with no result. `output-hidden` does nothing
to an unpaired block -- it renders exactly as if the attribute were
absent, since there is no separate result half for it to have hidden
in the first place.

**The pair's own class/stroke color is unaffected either way.**
`pair.failed` still picks `eval-pair`/`eval-pair failed`
(`red`/`gray` in Typst) regardless of which half is visible. A figure
showing only source, styled as failed, still tells a reader "this
one broke" even with the reason hidden; recomputing the class from
what happens to be visible would make the same source block change
its own rendered color depending on an unrelated attribute, which is
more surprising than leaving it alone.

## What this explicitly does not do

- Escaping a literal `"` inside `caption=`. `cmd::split`'s own
  documented limitation ("no escape sequences beyond that") carries
  over unchanged; a caption needing a literal quote character is not
  supported, matching every other `cmd::split` caller's own existing
  gap rather than fixing it here first.
- A third `weave=` value that hides only the captured stdout text
  while still showing a `produces=file:` artifact and provenance.
  `output-hidden` hides the whole result half at once. Splitting the
  result half further into its own sub-parts is a separate, later
  decision if a real use case shows up.
- Any change to `caption=`'s own scope beyond a recognized pair
  (decision 46). A standalone `csv`/`tsv`/`json` fence (decision 45)
  renders as a bare `<table>` with no `<figure>`/figcaption at all
  today; giving it one is out of scope here.
- Multiple `caption=` values, or a caption that varies by which half
  is hidden. One block, one caption, exactly like `produces=`'s own
  "one block, one artifact" stance (plan-weave-artifacts.md).

<!-- dankg:depends target=../architecture.md#decision-50-a-producesfile-csvtsvjson-artifact-renders-as-a-table quote="warns on stderr and adds nothing to the page" -->

## Critical files

- `src/md/mod.md` / `md/mod.rs` -- `KNOWN_ATTRS` gains `caption`;
  `InfoString` gains `caption()`, `weave_source_hidden()`,
  `weave_output_hidden()`.
- `src/md/block.md` / `md/block.rs` -- `parse_info`'s tokenizer swaps
  `raw.split_whitespace()` for `crate::cmd::split(raw)`.
- `src/md/fmt.md` / `md/fmt.rs` -- `info_text` quotes a value
  containing whitespace on write-back.
- `src/render/weave_html.md` / `weave_html.rs` -- `eval_pair` gates
  the source `code_or_data_table` call on `!weave_source_hidden()`,
  and the caption/output/artifact/provenance block on
  `!weave_output_hidden()`; the artifact figcaption and the
  `Output`/`Output (failed)` figcaption both check `source_info.caption()`
  first. `block()`'s unpaired dispatch adds `weave_source_hidden()`
  alongside `weave_hidden()`.
- `src/render/typst.md` / `typst.rs` -- the identical shape:
  `eval_pair`'s body-building gains the same two gates and the same
  `caption()` check; the standalone `block()` dispatch's
  `if info.weave_hidden()` guard gains `|| info.weave_source_hidden()`.
- `architecture.md` -- add decisions 52 and 53.

## Verification

1. Edit loop per CLAUDE.md, then `dankg fmt --check` and `dankg check .`
   over the whole corpus.
2. Unit tests (existing style, both backends get a matching pair):
   - `caption="two words"` round-trips through `dankg fmt` unchanged
     (parse, then re-serialize, produces the identical quoted form).
   - A pair with a `produces=file:chart.png` and
     `caption=Quarterly revenue` renders the artifact's figcaption as
     `Quarterly revenue`, not `file:chart.png`; the `Output` caption
     is untouched.
   - A pair with no artifact and `caption=Result` renders the
     eval-result figcaption as `Result`, and as `Result (failed)` when
     the recorded result is `failed`.
   - `weave=source-hidden` on a paired block: the source's own text
     never appears; the output text, artifact, and provenance still
     do.
   - `weave=output-hidden` on a paired block: the source's own text
     still appears; the output text, artifact, and provenance all
     disappear together.
   - `weave=source-hidden` on an unpaired named block with no recorded
     result: nothing renders, the same as `weave=hidden` today.
   - `weave=output-hidden` on an unpaired named block: renders exactly
     as if the attribute were absent.
   - The pair's own `failed` class/stroke color is present under both
     new values, matching the unmodified case.
3. A manual smoke test against `dankg_weave_example`: a block tagged
   `caption="..."` and `produces=file:...`, and a second block tagged
   `weave=source-hidden`, `dankg weave --format html` and
   `--format pdf`, confirming both render as expected in an opened
   browser and an opened PDF.
