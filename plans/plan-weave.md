# `dankg weave`: HTML and PDF output

## Context

`dankg tangle` extracts a corpus's named code blocks into a compilable
source tree. Architecture.md's own *Tangle* section already names the
literate-programming counterpart DanKG has never built: assembling
blocks into a program is "tangling," as opposed to "weaving" them into
typeset documentation.

<!-- dankg:depends target=../architecture.md#tangle quote="extracting and reassembling code chunks into compilable source, as opposed to" -->

This plan is that counterpart: `dankg weave` turns one markdown file
into a readable document, either HTML (with a reader-toggleable table
of contents) or PDF (compiled through Typst, with the table of contents
controlled by a CLI flag instead, since a PDF has no runtime to toggle
anything in). Confirmed with the user: single file only for a first
cut, no corpus-wide weave yet.

**Priority for this pass: the PDF/Typst backend, with real table
support, not the HTML backend.** Two things drive that: Typst tables
are the whole reason a reader would pick PDF over HTML in the first
place (a typeset table is where PDF actually earns the compile step),
and DanKG's own corpus already produces exactly the kind of content a
table renders best -- an eval block's captured `duckdb -csv` output.
Implementation proceeds table-subset first, then the Typst backend and
its table mapping, then CSV/JSON data tables, and only then the HTML
backend and the orchestration/CLI wiring that ties both formats
together. Decision numbers below are assigned in final narrative order
for architecture.md, not implementation order -- they do not match the
sequence just described.

## What this reuses

**The single-file path `tangle::run` already takes.** Naming one file
skips `index::load`'s whole-root walk and graph build entirely --
`tangle.rs` already does this for exactly the reason weave needs it
too.

<!-- dankg:depends target=../src/tangle.md#tangle quote="nothing here needs to know about any file but the one asked for" -->

**The external-command escape hatch.** Zero crates stays intact the
same way it does for `tangle`: DanKG never links a PDF library, only
emits Typst source and spawns a configured external `typst` command
against it, via the same `cmd::build` substitution-then-split every
other configured command already goes through. The same zero-crate
discipline is why the two new data formats below (CSV, JSON) get
hand-rolled readers instead of a parsing crate -- no different from
`eval/sql.rs`'s own hand-rolled scanner refusing a SQL-parser crate for
the same reason.

<!-- dankg:depends target=../architecture.md#decision-1-dependency-policy quote="Zero crates, std only, forever." -->
<!-- dankg:depends target=../src/cmd.md#build quote="there is no program to run" -->

**Graceful degradation when unconfigured.** `[tangle.<lang>] command`
being absent doesn't fail tangle -- it just stops at materializing the
tree, which is the whole operation for a language with no build step.
`[weave.pdf] command` being absent works the same way: weave still
writes the `.typ` source and says so, rather than refusing to run. The
same shape covers a fenced block tagged `json` that turns out not to
parse as a table (see decision 45): weave falls back to an ordinary
code block and says so, rather than refusing to run.

<!-- dankg:depends target=../architecture.md#tangle-config quote="`command` is optional, spawned once against the whole assembled tree" -->

**Slug generation.** Heading anchors in the woven HTML page reuse
`graph::slug::Slugger`, the same slugger `graph/build.rs` and
`tangle.rs` already run per file, so a woven page's `#anchor`s agree
with the graph's own node slugs instead of inventing a second numbering
scheme for the same headings.

## What's new

### Weave scope: the whole document, not named/top-level blocks

`tangle` and `eval` both narrow to `plan::top_level_blocks` -- named,
top-level, not nested in a list (decision 23). Weave can't: the point
is a document someone reads, so it walks every `Block` in
`Document.blocks`, in document order -- headings, paragraphs, lists,
thematic breaks, passthrough, tables, and code blocks alike. Code
blocks are rendered read-only. Weave never executes anything and never
consults `deps=`/`name=`.

This becomes `## Decision 41: Weave scope` in architecture.md, right
after decision 40.

### GFM tables enter the markdown subset

Today a pipe table is `Block::Passthrough`, kept verbatim, the same as
a block quote or an indented code block:

<!-- dankg:depends target=../architecture.md#markdown-subset quote="Passed through as literal text: setext headings, HTML blocks, tables, block" -->

That is fine for `dankg graph` -- a table carries no heading, no
executable block, nothing the graph needs. It stops being fine the
moment weave has to typeset the same file: a raw `| a | b |` line
surviving into a PDF as literal pipes is not a table, it is a
formatting bug. Tables have to become real structure before either
backend can render one, so this is a `md/block.rs` change, not a
`render/` one, and it benefits `dankg fmt` too -- a table gets
normalized the same way a list or a heading already does.

A new `Block::Table { aligns: Vec<Align>, header: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>>, line: u32 }`, `Align` being
`None`/`Left`/`Center`/`Right` per column, read off the delimiter row's
own `:---`/`:---:`/`---:` markers. Detection: a line containing an
unescaped `|`, immediately followed by a line that is nothing but
`-`/`:`/`|`/whitespace with the same cell count -- the same two-line
lookahead GFM itself uses. The row splitter is the one new hand-rolled
scanner this needs: split on `|` not preceded by a backslash, trim each
cell, and hand the raw cell text to `inline::parse` exactly as a
paragraph already does. `\|` needs no new escape handling on the way
in -- `inline::parse` already turns any backslash-escaped ASCII
punctuation into literal text, `|` included. `dankg fmt`'s own
`Writer` needs the one new thing here: a table-cell rendering mode that
escapes a bare `|` in cell content on the way *out*, so a literal pipe
in a cell round-trips instead of silently becoming a new column on the
next parse.

Two deliberate narrowings, in the same spirit as decision 23's
list-vs-paragraph scope cut elsewhere in this codebase:

- A `|` inside an inline code span still splits a cell. Wrap it as
  `\|` to keep it out of the boundary. GFM special-cases pipes
  inside backticks; this hand-rolled scanner does not.
- A table never interrupts an in-progress paragraph the way GFM allows.
  A blank line before one is required here, matching how thematic
  breaks and fences already behave in `interrupts_paragraph`... except
  a table *is* added to that function's own checks, so a table **does**
  correctly end a paragraph that precedes it without a blank line in
  between -- it just cannot begin output mid-paragraph the one line a
  table's header and delimiter rows require lookahead for.
- A short or long data row is kept exactly as parsed, never padded or
  truncated to the header's column count, by the parser or by `fmt`.
  Padding at format time would make `fmt::verify`'s round-trip check
  disagree with the original document -- the AST has to keep what was
  actually there. Rectangling a ragged table into a grid is a
  rendering-time concern, and belongs to decision 44's Typst mapping,
  not to the subset itself.

This becomes `## Decision 42: GFM tables enter the markdown subset`.

### PDF backend, via Typst (`src/render/typst.md` -> `typst.rs`)

`render::typst::render(doc, title, toc: bool) -> String` emits Typst
markup only -- a markup emitter alongside `dot.rs`/`mermaid.rs`, never
a PDF generator:

- Headings become `=`, `==`, `===` by level.
- Paragraph text gets Typst's own special characters escaped (`#`,
  `*`, `_`, `` ` ``, `<`, `@`, `$`, `\`).
- Emphasis and strong become `_..._` and `*...*`; links become
  `#link("url")[text]`.
- Code fences become Typst raw blocks. Typst highlights these itself,
  a free win that costs no dependency.
- Lists become `-`/`+` items; a thematic break becomes a `#line()`
  rule.
- **`Block::Table` becomes `#table()`.** Column count comes from the
  header; a ragged data row (decision 42 keeps these ragged in the
  AST) is padded with empty cells or truncated to that count right
  here, at the one place padding has no round-trip obligation to
  satisfy. Column alignment maps `Align::Left/Center/Right/None` to
  Typst's own `align: (left, center, right, auto)` argument. The
  header row becomes `table.header(...)` so it repeats across a page
  break, something a fenced-in `<table>` in the HTML backend gets for
  free from the browser and Typst has to be told about explicitly. A
  cell's own inline content (emphasis, code spans, links) recurses
  through the same inline emitter paragraph text already uses --
  a table cell is not a second markup dialect.
- A fenced block tagged `csv`, `tsv`, or `json` renders as a `#table()`
  too, built from `data::table` (decision 45) instead of from
  `Block::Table`. Both paths converge on one internal
  `emit_table(columns, header, rows, aligns)` helper, so the Typst
  output for a GFM table and a CSV-tagged code block share one code
  path and one set of tests.
- Passthrough content -- anything outside DanKG's markdown subset --
  is emitted as escaped literal text, never as raw Typst. An unparsed
  construct must never become unvalidated Typst markup.
- `#outline()` is inserted right after the title only when `toc` is
  true.

`weave::run` always writes the `.typ` to
`.dankg/build/weave/<name>.typ`, then spawns a configured
`[weave.pdf] command` against it -- mirroring
`tangle::spawn_against_dir`'s `cmd::build` plus `std::process::Command`
call exactly. New config family in `src/config.md`: `FAMILIES` gains
`("weave.", &["command"])`, a `Weave { name, command: Option<String> }`
struct, and a `Config::weave(name)` lookup, so a corpus opts in with:

```
[weave.pdf]
command = typst compile {typ} {pdf}
```

Unconfigured, weave still writes the `.typ` and reports that no PDF
was produced, the same graceful degradation tangle already gives an
unconfigured build step. No confirm prompt either way -- weave doesn't
run the reader's program, only compiles a document.

This becomes `## Decision 44: Weave PDF via Typst`.

### Data tables from CSV/JSON/TSV fenced blocks

The second table source, and the one that motivated prioritizing PDF
in the first place: a fenced block's own captured output, rendered as
a typeset table instead of a wall of raw text.

**New module, `src/data/table.md` -> `src/data/table.rs`.** One shared
shape, `TableData { header: Vec<String>, rows: Vec<Vec<String>> }`,
built by two hand-rolled readers -- no parsing crate, the same
zero-dependency discipline `eval/sql.rs`'s own scanner already
follows:

- `from_delimited(text, delim: char) -> TableData` -- CSV (`,`) and TSV
  (`\t`) share one reader. Quoted fields, `""`-escaped quotes, first
  row is the header. Every byte sequence is *some* valid delimited
  text, so this never fails -- there is no malformed-CSV case to
  report.
- `from_json(text) -> Option<TableData>` -- a small hand-rolled
  recursive-descent JSON parser (null/bool/number/string/array/object;
  numbers and strings only, no crate), restricted to exactly the two
  shapes a table can come from: a top-level array of objects (the
  first element's own keys, in declared order, become the header; a
  later element's missing key renders empty, an extra key is ignored),
  or a top-level array of arrays (the first inner array is the header
  row, matching the delimited reader's own convention). Anything else
  \-- a bare object, a scalar, a nesting the table shape can't flatten
  \-- returns `None`.

**Weave picks the reader from the fence's own language tag** --
`csv`, `tsv`, `json` -- never by sniffing content. The info string's
`lang` slot already answers "what shape is this text," which is
exactly what deciding whether to parse it as a table needs to know
before it looks at a single byte. A block tagged anything else (or
tagged `csv` but not actually valid CSV -- there is no such thing, see
above -- or tagged `json` but not one of the two recognized shapes)
renders exactly as it does today, unchanged, with a `Diags` warning in
the `json` case. Guessing table-ness from content instead would make
weave's output depend on a heuristic nobody asked for, the same
reasoning that already keeps `dankg eval` from inferring a block's
language from its output.

**Eval interop is explicitly deferred, not solved here.** An eval
result fence is written back with no language tag at all -- bare
triple backtick, whatever the reader's own configured command printed.
Tagging a result fence with its true shape (`duckdb -csv` implies
`csv`) would mean `eval::result::write_back` has to learn what format
a command's output is in, and no config key records that today. That
is a real, separate feature -- teaching `[db.*]`/`[lang.*]` sections an
explicit `format=` key, or inferring one from a `-csv` flag already
sitting in `command` -- and it is out of scope for this plan. For now,
a corpus author who wants a captured SQL result to render as a table
adds `csv` to the result fence's info string by hand once `dankg eval`
has written it; `dankg eval`'s own next run does not touch that tag
either way, since it rewrites only the fence's *content*, not its
info string. Both HTML and Typst renderers read the same `TableData`,
whether it came from a hand-written CSV block or a hand-tagged eval
result, so nothing above is eval-specific.

This becomes `## Decision 45: Data tables from CSV/JSON/TSV fenced blocks`.

### CLI (`src/cli.md`)

```
dankg weave <path> --format html|pdf [-o <file>] [--toc | --no-toc]
```

- Exactly one path, the same single-target shape `init` already uses.
  Corpus-wide weave is out of scope for now, so there's no
  file-or-directory branch to build, unlike `tangle`/`check`.
- `--format` is required, the same "needs `--lang`" refusal `tangle`'s
  own parser already gives for a missing required value.
- A new `WeaveFormat { Html, Pdf }` enum, kept separate from
  `cli::Format` (graph's json/html/dot/mermaid set) -- folding `pdf`
  into that shared enum would let `graph --format pdf` parse too.
- `-o` stays optional for both formats, following `graph`'s own
  optional `-o`/stdout default: `html` defaults to stdout, `pdf`
  defaults to `.dankg/build/weave/<name>.pdf` (`<name>` from
  `graph::build::strip_extension`, already public and already reused
  by tangle for the same purpose).
- `--toc`/`--no-toc` only apply to `--format pdf`. HTML's table of
  contents always ships with its own in-page toggle (see the HTML
  section below), so combining `--toc`/`--no-toc` with `--format html`
  is a parse error -- the same "ask for different things" refusal
  `--all`/`--depth` already give each other in `graph`'s own parser.
  Default when omitted: `--toc` (shown). Tables need no flag of their
  own in either format -- a table renders as a table whenever the
  source has one, the same as any other block.

### HTML backend (`src/render/weave_html.md` -> `weave_html.rs`)

A new renderer, distinct from `render/html.rs` (the graph page).
`tests/support/html.rs` already proves out a CommonMark block/inline ->
HTML walk, but it lives in the test crate on purpose, as a conformance
oracle scored against the markdown spec, not a product renderer -- and
`src/` can't depend on `tests/` in the first place. So `weave_html.rs`
is a fresh implementation with the same shape, extended with what a
woven page actually needs:

- Every heading gets an `id` from `graph::slug::Slugger`.
- A `<nav id="toc">` built from `doc.headings()`, nested by level.
- The toggle is pure CSS: a hidden checkbox plus a `<label>` collapses
  and expands the nav. No JavaScript at all -- unlike the graph page
  (pan, zoom, expand-on-click), a woven page has nothing to compute
  client-side.
- `Block::Table` becomes `<table><thead>...` with a per-cell
  `text-align` inline style carrying `Align`, and a fenced block tagged
  `csv`/`tsv`/`json` renders through the exact same `TableData` ->
  `<table>` path (decision 45), with no `Align` to apply -- browser
  default alignment, nothing invented.
- One self-contained file, the same convention `render/html.rs`
  already follows: inline `<style>`, no network requests. A new
  `WEAVE_CSS` constant in `render/assets.md`, kept separate from the
  graph page's own `CSS` -- prose typography (and now table styling)
  has nothing in common with a pan/zoom SVG canvas's rules.

This becomes `## Decision 43: Weave HTML rendering`. It also means one
existing sentence in architecture.md stops being true and needs fixing
in the same commit: the *Markdown subset* section currently says DanKG
never renders markdown to HTML at runtime.

<!-- dankg:depends target=../architecture.md#markdown-subset quote="DanKG itself never renders markdown to HTML." -->

The fix is a one-line correction, not a reversal: that was true before
weave existed, and the woven HTML page is the one deliberate exception,
kept in its own renderer rather than folded into the graph's. The
`tests/support/html.rs` doc comment gets the matching one-line update.
Neither gets any functional change.

### Orchestration (`src/weave.md` -> `weave.rs`)

Much smaller than `tangle.rs`: only the single-file path, no
corpus-walk branch. Reads and parses the one file, discovers root and
config the same way `tangle::run`'s single-file branch already does,
picks the HTML or PDF renderer, writes the result, and returns a
`Report` for `main.rs` to print -- mirroring `tangle::Report` and
`tangle_cmd`'s own shape.

### Wiring

- `src/render/mod.md`: add `pub mod typst;`, `pub mod weave_html;`.
- `src/lib.md`: add `pub mod weave;`, `pub mod data;` (housing
  `data::table`).
- `src/main.md`: import `dankg::weave`, add a `Command::Weave { .. }`
  arm, and a `weave_cmd` function mirroring `tangle_cmd`'s
  report-printing shape.
- architecture.md: a new `# Weave` prose section near *Tangle* (CLI
  shape, scope, the two backends, config, the table story end to end),
  decisions 41-45 added to the decision list, the *Markdown subset*
  section updated for both the HTML-rendering correction and the new
  table entry, and the *Pipeline*/*Module layout* diagrams updated to
  mention `weave.rs`, `render/weave_html.rs`, `render/typst.rs`, and
  `data/table.rs`.

## What this explicitly does not do

- Corpus-wide weave -- `paths: Vec<String>`, directory walking,
  per-file output nesting the way tangle's decision 26 does. Confirmed
  single-file only for now.
- Syntax highlighting in the HTML backend, tables included. Typst gets
  code-fence highlighting for free in the PDF backend; HTML code
  blocks and table cells stay plain markup, the same as the graph
  page's own conformance oracle.
- Tagging an eval result fence with its output format automatically.
  Deferred, see decision 45's own "Eval interop" note above.
- Pipe characters inside an inline code span, within a GFM table cell,
  getting GFM's own special-cased treatment. Wrap one as `\|`.
- Sniffing a fenced block's content to decide it "looks like" a table.
  The language tag is the only signal; content is never inspected
  before the tag says to.
- Any functional change to `tests/support/html.rs` -- doc comment
  only.

## Critical files

- `src/tangle.md` / `src/tangle.rs` -- the precedent this plan follows
  almost file-for-file.
- `src/md/mod.md`, `src/md/block.md`, `src/md/fmt.md` -- the `Block`
  enum, table detection/parsing, and round-trip-safe table formatting
  (decision 42). Touching all three is unavoidable: a new `Block`
  variant is exhaustively matched in `fmt.rs` twice (normalize and
  render) by design, so the compiler is what catches a forgotten arm.
- `src/eval/sql.md` -- not modified, but the hand-rolled-scanner
  precedent `data::table`'s CSV and JSON readers both follow.
- `src/cli.md`, `src/config.md`, `src/main.md` -- CLI, config family,
  and dispatch wiring, each getting the same shape `tangle` already
  has in each.
- `src/cmd.md` -- unchanged, but `cmd::build` is exactly what spawns
  `[weave.pdf] command`.
- `src/render/mod.md`, `src/render/assets.md` -- module list and CSS
  constants gain new entries.
- `tests/support/html.rs` -- read for its block/inline walk shape, not
  depended on; doc comment only.

## Verification

1. Edit loop per CLAUDE.md: `cargo run --release --bin dankg -- tangle . --lang rust -o src`, `cargo build --release`, `cargo test`.
2. `dankg fmt --check` over every changed `.md` file, and `dankg check .` over the whole corpus (this file's own `dankg:depends` markers
   included).
3. Unit tests, following `tangle.rs`/`config.rs`'s own test style --
   scratch directories, `assert_eq!` on `Report`/`Command`:
   - `md/block.rs`: table detection (header + delimiter lookahead),
     alignment parsing, escaped-pipe cells, a ragged data row kept
     exactly as parsed.
   - `md/fmt.rs`: table round-trips through `verify()` unchanged,
     including a ragged row and a cell containing a literal `|`.
   - `data/table.rs`: CSV/TSV quoting and escaping; JSON array-of-
     objects and array-of-arrays; the documented non-table JSON shapes
     each return `None`.
   - `weave.md`, `weave_html.md`, `typst.md`, `config.md`, `cli.md`:
     the same `Report`/`Command` assertions `tangle.rs`/`config.rs`
     already establish the pattern for.
   - `typst.rs`: a GFM table with mixed alignment, a ragged row padded
     at render time, and a `csv`-tagged block, each rendering the
     expected `#table()` call.
4. A manual smoke test against this repo: a scratch file combining a
   GFM table and a fenced `csv` block, woven with `dankg weave scratch.md --format html -o /tmp/scratch.html`, opened by hand; and, with
   `typst` installed locally (`/opt/homebrew/bin/typst`, confirmed
   0\.15.1) and a scratch `.dankg/config` setting `[weave.pdf] command = typst compile {typ} {pdf}`, `dankg weave scratch.md --format pdf -o /tmp/scratch.pdf` run end to end -- confirming the compiled PDF
   actually opens, that both tables typeset correctly (including the
   aligned columns and the repeated header), and that `--no-toc`
   actually omits the outline.
