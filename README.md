# DanKG

DanKG (Dan's Knowledge Grapher) is a vanilla-markdown knowledge graphing
tool. You write plain markdown. DanKG turns headings into graph nodes and
links between them into edges. It can run code blocks and record their
output in place. It can also assemble a literate document's named code
blocks into a real, buildable source tree.

- **Zero dependencies.** Pure Rust, standard library only, forever.
- **Plaintext-driven.** Every input and output is a file an editor (or an
  LLM) can read and write directly.
- **Nothing runs automatically.** `dankg graph` only ever renders.
  `dankg eval` is the only command that runs code. It prints the plan and
  asks for confirmation before running anything.

<!-- dankg:depends target=architecture.md#decision-9-eval-trigger quote="never automatic." -->

See `architecture.md` for the full design record: numbered decisions,
data model, module layout. See `project.md` for the what and why. Both
files are themselves a dankg corpus. `dankg graph architecture.md`
renders the design record the same way DanKG renders any other graph.

## Install

Requires a recent stable Rust toolchain (edition 2024).

```sh
git clone https://github.com/dxgeo/dankg
cd dankg
cargo install --path .
```

This installs the `dankg` binary to `~/.cargo/bin`. To build without
installing:

```sh
cargo build --release
./target/release/dankg --help
```

## Quick start

```sh
mkdir notes && cd notes
cat > index.md <<'EOF'
# Index

See [[Ideas]] for what's next.
EOF
cat > ideas.md <<'EOF'
# Ideas

Back to [the index](index.md).
EOF

dankg graph index.md --format html -o graph.html   # open graph.html in a browser
dankg tui index.md                                 # or explore it in the terminal
```

A `.dankg/` directory marks the root of a knowledge base. Create one with
`mkdir .dankg` at the top of your notes tree. Without one, DanKG infers
the root from the paths you name. Everything under the root belongs to
the corpus. Nothing outside it does.

`dankg init notes` does the `mkdir`/`.dankg/`/`index.md` steps above
for you -- see [`init`](#init-scaffold-a-new-corpus) below.

## Commands

```
dankg graph <path>... [--format <fmt>] [--depth N | --all] [--live] [-o <file>]
dankg index [<path>]  [--no-cache]
dankg fmt   <path>... [--check]
dankg tui   <path>... [--depth N | --all] [--no-cache]
dankg eval  <path> [--block <name> | --all | --each] [--yes] [--no-write] [--if-stale]
dankg eval  [<path>...] --list [--no-cache]
dankg check [<path>...] [--no-cache]
dankg tangle <path>... --lang <lang> [-o <dir>] [--no-cache]
dankg init  [<path>]
```

Run `dankg --help` for the full option reference. It stays in sync with
the binary. This document doesn't duplicate it.

### `graph` — render a knowledge graph

```sh
dankg graph notes/ --format html -o graph.html   # one self-contained page
dankg graph notes/index.md --format json         # the whole index, for scripting
dankg graph notes/index.md --format dot | dot -Tsvg -o graph.svg
dankg graph notes/index.md --format mermaid
```

`--format json` always emits the whole index, regardless of the entry
point. That's what makes it a stable, scriptable surface. `--depth` and
`--all` shape only the rendered formats: `html`, `dot`, and `mermaid`.

A file that declares a frontmatter `title` gets that title as its own
top-level node, and every heading in the file hangs off it. A leading
heading repeating the title is absorbed rather than graphed twice, so
`title: Hash` over `# Hash` is one node, not two. A file declaring no
`title` is unchanged: its first heading is what `[text](file.md)`
lands on.

```markdown
---
title: Hash
---

# Hash

## The hash
```

Two nodes: `src/hash#hash`, with `src/hash#the-hash` beneath it.

<!-- dankg:depends target=architecture.md#decision-62-a-declared-frontmatter-title-is-the-documents-top-level-node quote="A file that declares no `title` keeps exactly the shape it has today" -->

### `tui` — explore the graph in a terminal

```sh
dankg tui notes/
```

Keys:

- Arrow keys or `hjkl` — move; left/right collapse/expand a node or step to its parent/first child
- `tab` — toggle focus between the tree and the cross-reference panel (previews the hovered link before you commit to it)
- `enter` — open the selected node in your configured `[editor] command` (or jump to a focused panel link)
- `/` — jump to a node by title, anywhere in the corpus; `n`/`N` repeat it forward/backward
- `f` — open the filter menu (all, blocks, eval-chain, file-artifact, plus any `kind=` a `dankg:tag` marker has actually set anywhere in the corpus); `enter` applies it, `esc` cancels
- `t` — tag the selected node: pick from every declared `[kind.*]`, or press `n` to declare a new one (name, then an optional icon); re-tagging overwrites. Refused for a node marked `∅` (see below) or a relation — neither has a real place to attach a marker
- `e` then `enter` — cycle and run a node's named code blocks in place
- `r` — reset
- `b` — toggle the origin breadcrumb (status line, while the panel has focus)
- `q` — quit
- `?` — full keybinding reference, including any `[tui] commands` below; both this and the filter menu draw as a small box over the tree, not a full-screen replacement

A node marked `∅` is a dangling link's own placeholder, not real
content — the same thing `dankg check` counts as unresolved. `t`
refuses to tag one: there's no real line to attach a marker to.

Inside tmux, `enter` does not take over the terminal at all. It opens
the editor in a new pane, side by side, so the tree stays visible the
whole time. This needs no configuration; it activates on its own
whenever `dankg tui` is itself running inside tmux.

Add `reuse` under `[editor]` so repeated `enter` presses retarget one
pane instead of spawning a new one for every node:

```ini
[editor]
command = nvim +{line} {file}
reuse   = :tab drop {file}<CR>:{line}<CR>
```

`reuse` is keystrokes typed into that pane, not a program to spawn —
`<CR>` marks each Enter press. The line above is vim/nvim's own
`:tab drop`: it switches to a file's tab if one is already open and
opens a new one otherwise, so the same file jumps in place and a
different file opens beside it, with nothing for `dankg` itself to
tell apart. Leave `reuse` unset and every `enter` spawns a fresh pane,
same as without it. It is the natural way to keep `dankg tui` open in
one narrow pane, browsing the whole corpus, while an editor pane on
the other side follows wherever `enter` sends it — even across
quitting and reopening `dankg tui`, since the pane it remembers lives
in tmux itself, not in that one process.

Give a block `key=` and name its file in `[tui] commands` (see
Configuration below) to bind a key directly to it, skipping the
`e`-then-`enter` cycle:

````markdown
```sh name=greet key=g
echo "hello from a custom dankg command"
```
````

Pressing `g` while the tree has focus runs it through the same
`eval::run` `e`/`enter` already use, and reports the outcome on the
status line. Help, the filter menu, and panel focus are all modal and
swallow the key like any other, the same way they swallow `e`/`enter`
too. `key=` accepts a single character, `ctrl+<char>`, or one of
`enter`/`tab`/`backspace`/`esc`/`up`/`down`/`left`/`right`. A binding
that collides with a built-in key, or with another command in the
same file, is refused with a warning rather than silently shadowing
one.

Add `protocol=lines` to a `key=` block to give it a richer output
protocol instead of plain text on the status line:

````markdown
```sh name=classify key=g protocol=lines
echo "select: notes/index.md#some-heading"
echo "status: jumped and tagged"
echo "tag: notes/index.md#some-heading kind=task"
```
````

`select: file#heading` moves the tree cursor there, and `status: message` replaces the status line. `tag: file#heading kind=value`
writes that node's own classification into its file, as a
`<!-- dankg:tag -->` comment right above it — a real edit, not
something only this session remembers, so it's still there the next
time you open `dankg tui`, or open the file itself. All three are
opt-in: a `key=` block without `protocol=lines` always shows its raw
output verbatim, exactly as before, so nothing a command already
prints by coincidence (a tool's own `status: ...` line, say) is ever
misread as one of these.

Give `kind=task` an icon, and every node tagged that way shows it —
once, in config, not on every `tag:` line:

```
[kind.task]
icon = ☐
```

`kind=` becomes a selectable entry in the `f` filter menu once
something is actually tagged with it. A `kind=` with no matching
`[kind.*]` section still classifies the node — it just has no icon to
show — but `dankg check` reports it and fails, the same severity as a
dangling link: there's no reading under which a name nothing declares
is fine. `dankg check` also warns, advisory only, about a configured
icon likely to render wider than one terminal column or in color —
the same bug this project's own TUI badges have already hit once by
hand (see `architecture.md`'s *Eval in the TUI* for the story).

The marker itself also records which node it's for
(`target=#some-heading`), not just its position. Insert a new heading
between the marker and the node it was meant for — an ordinary edit —
and pure position would silently reattach it to the wrong one.
`dankg check` catches that too: a `target=` that no longer points
back at the node the marker actually sits on fails the build, the
same way an unknown `kind=` does.

None of this needs a command at all: `t` (see *Keys* above) does the
same thing directly — pick a declared kind, or declare a new one on
the spot, name and optional icon, without writing a single line of
`[tui] commands`.

### `eval` — run and record literate code blocks

Give a fenced code block a `name=`. It becomes runnable:

````markdown
```python name=greet
print("hello")
```
````

```sh
dankg eval notes/index.md --block greet     # prints the plan, asks, runs, writes the result back
dankg eval notes/index.md --block greet --yes --no-write   # skip the prompt, don't write
dankg eval notes/index.md --block greet --yes --if-stale   # skip it if it's already up to date
dankg eval notes/index.md --list            # list every named block without running anything
```

A block's language needs a configured `[lang.<name>] command` (see
Configuration below). Otherwise `dankg eval` refuses to run it. `deps=`
names other blocks that must run first. DanKG concatenates them ahead of
the target into one process:

````markdown
```python name=setup
count = 42
```

```python name=greet deps=setup
print(count)
```
````

`deps=` can also reach into another file. `deps=lib.md#helper` resolves
relative to the file that wrote it, the same way a written link would.
This is enough to write a genuinely multi-file literate program with no
`import`, `use`, or `mod` at all. DanKG concatenates the whole reachable
chain into one file before handing it to the interpreter or compiler.

`deps=` never crosses a language: concatenating a shell block ahead of
a Python target and running the result through one interpreter is far
more likely a mistake than an intentional pipeline, so it is refused.
`xdeps=` is the cross-language counterpart, for tracking staleness
without concatenating:

````markdown
```sh name=fetch
curl -o raw.csv https://example.com/data.csv
```

```python name=summarize xdeps=fetch
print(open("raw.csv").read().strip())
```
````

A target with an `xdeps=` entry always runs alone; `fetch` is never
pulled into `summarize`'s own process. `dankg check` still verifies
`fetch`'s own recorded result is fresh before trusting `summarize`'s,
recursively, exactly the way a same-language `deps=` chain is
verified. `xdeps=` resolves across files the same way `deps=` does
(`xdeps=lib.md#helper`).

A writer block can also declare `produces=file:PATH`, and a reader
`reads=file:PATH`, alongside a `deps=`/`xdeps=` entry naming the other
block directly:

````markdown
```sh name=fetch produces=file:raw.csv
curl -o raw.csv https://example.com/data.csv
```

```python name=clean deps=fetch reads=file:raw.csv
print(open("raw.csv").read().strip())
```
````

Neither attribute resolves anything on its own: the `deps=`/`xdeps=`
edge still says which block this is about. `dankg check` just confirms
the two sides agree on which file that edge is actually about (see
`check` below).

### `check` — the CI gate

```sh
dankg check notes/
```

Exits non-zero if the corpus has an unresolved link. It also exits
non-zero if a written `eval` result's hash no longer matches its current
source, dependencies, or configured command, including a dependency in
another file. `check` is deliberately separate from `graph`. This way, a
half-written note never fails a build.

<!-- dankg:depends target=architecture.md#cli quote="It is deliberately separate from `graph` so that drafting a half-written note never fails." -->

`check` also reports every `<!-- dankg:depends target=other.md#heading quote="..." -->` marker whose quoted claim can no longer be found,
whitespace differences aside, in the section it names. This is advisory
only. It never affects the exit code, because a substring match is a
much weaker signal than a source hash (see architecture.md, *Prose
dependencies*).

<!-- dankg:depends target=architecture.md#prose-dependencies quote="A substring match is a much weaker signal than a content hash" -->

`check` also gates on `produces=file:PATH`/`reads=file:PATH`: a writer
block declares the file it writes, a reader declares the file it reads
and names the writer directly in its own `deps=`/`xdeps=`, and `check`
fails if the two paths do not agree. Unlike `dankg:depends`, this one
*does* fail the build: two declared strings disagreeing is a much
stronger signal than a prose substring match (see architecture.md,
*File dependencies*).

`check` also reports a heading whose title collides with an earlier
heading's in the same file. This is advisory too, like `dankg:depends`.
The file still resolves correctly. But the colliding heading's slug is
order-dependent. A later rename or reorder can silently repoint it.
`check` tells you whether that risk is real. A collision with a
written link or `dankg:depends` marker already pointing at one of its
two slugs prints as referenced. One with neither prints as cosmetic.
It also reports whether the pair is sibling (same immediate parent) or
differently-nested. A reader is far more likely to confuse two
siblings sharing a title than two headings under clearly different
sections (see architecture.md, *Title collisions*).

<!-- dankg:depends target=architecture.md#title-collisions quote="That slug is order-dependent, though." -->
<!-- dankg:depends target=architecture.md#title-collisions quote="A collision with a reference prints as a live risk." -->
<!-- dankg:depends target=architecture.md#title-collisions quote="A sibling pair looks identical to a reader scanning the one section they are both under." -->

`check` also gates on every `<!-- dankg:tag kind=value target=... -->`
marker (see *`tui`* above): a `kind=` naming no configured `[kind.*]`
section fails the build, the same severity as `produces=`/`reads=`
above — there's no fuzzy reading under which a name nothing declares
is fine. So does a `target=` that no longer resolves back to the node
the marker actually sits on — the same drift a heading inserted
between a marker and its intended node would otherwise cause
silently. Separately, and only advisory, it warns about a configured
`[kind.*]` icon likely to render wider than one terminal column or in
color.

### `fmt` — normalize markdown

```sh
dankg fmt notes/*.md           # rewrite in place
dankg fmt --check notes/*.md   # report which files would change; write nothing
```

`fmt` refuses to write any file whose formatted form doesn't re-parse to
the same document. This way, a formatter bug can't quietly corrupt a
note.

<!-- dankg:depends target=architecture.md#decision-15-format-safety quote="Re-parse and compare before writing." -->

### `tangle` — assemble a literate program into a source tree

```sh
dankg tangle notes/ --lang rust -o build/
```

`tangle` groups every named, top-level code block in the given language
by its containing top-level heading (one heading, one file). It
assembles the groups into `build/` (default `.dankg/build/<lang>/`).
Then it runs any configured `[tangle.<lang>] glue` and `command` against
the result.

<!-- dankg:depends target=architecture.md#decision-24-tangle-placement quote="Heading containment + document order; `deps` not consulted." -->

### `weave` — typeset one file as a document

```sh
dankg weave notes/index.md --format html -o out.html   # a self-contained page
dankg weave notes/index.md --format pdf                # compiled through Typst
```

`weave` turns one markdown file into a document a person reads,
never a whole corpus. It walks every block in the file, not just the
named, top-level ones `eval`/`tangle` narrow to, and never runs
anything: it only typesets what is already there, including whatever
`dankg eval` already recorded.

`author`/`date` frontmatter never render as raw text in either
backend. The PDF backend gives the document its own cover page first:
`title` large and centered, `author` beneath it as an unlabeled
byline, `date` beneath that as a real Typst date -- `2026-09-18`
becomes "September 18, 2026" -- then every other frontmatter entry as
its own labeled line: `tags: [rust, typst]` reads as
"Tags: rust, typst". `bibliography` and any `dankg.*` key never
appear there; neither is meant for a reader. HTML has no cover page,
but still gives `author`/`date` the same non-literal treatment right
beneath its own `<h1>` -- an unlabeled byline, a formatted date --
without dumping the rest of a document's frontmatter the way the PDF
cover page does.

Frontmatter's own `cover` key decides where a document's title
appears. The title block is the PDF cover page, or HTML's `<h1>` and
byline. The repeated heading is the document's own leading heading,
when that heading repeats the title. `cover: true` keeps the title
block and drops the repeated heading, so the title is typeset once.
`cover: false` does the reverse: no cover page, no generated `<h1>`,
and the repeated heading carries the title alone. Leave `cover` out
and nothing changes -- both render, exactly as they always have.
`cover` itself never appears as a line on the page it names.

```markdown
---
title: Weave
cover: true
---

# Weave

This page shows "Weave" once, on the cover.
```

A named block immediately followed by its own recorded
`<!-- dankg:result ... -->` marker renders as one paired unit --
source, then a labeled "Output" -- instead of three unrelated blocks.
A failed run gets a visually distinct pairing. `weave=hidden` on a
block drops it from the page entirely, paired result included, while
leaving it tangle-able and eval-able exactly as before:

````markdown
```python name=setup weave=hidden
data = {"Rust": 2010, "Typst": 2019}
```

```python name=show deps=setup
for name, year in sorted(data.items()):
    print(f"{name}: {year}")
```
````

A stale recorded result (its source changed since `dankg eval` last
ran) never changes the rendered page. Weave only ever warns on
`stderr`, the same way a missing `[weave.html] css` does. The same
file always weaves to the same output, regardless of what state
anything else is in.

A block that also declares `produces=file:PATH` (see `eval` above)
may have a real table or a real image sitting on disk, not just
captured stdout. A `csv`/`tsv`/`json` extension renders as a genuine
table; an image extension (`png`/`jpg`/`jpeg`/`gif`/`svg`/`webp`)
renders as a genuine embedded image -- inlined in HTML, copied
alongside the compiled PDF in Typst's case. Both render inside the
same paired unit as the block's captured stdout, since a chart-making
block usually logs a line rather than printing the chart itself:

````markdown
```python name=chart deps=setup produces=file:chart.png
draw_chart(data, "chart.png")
print("wrote chart.png")
```
````

A synthesized caption -- "Output", or a produced artifact's own raw
`produces=file:PATH` -- can be overridden with `caption=`, free text
unlike every other attribute here, so it may carry its own spaces
when quoted:

````markdown
```python name=chart deps=setup produces=file:chart.png caption="Yearly release count"
draw_chart(data, "chart.png")
```
````

A captioned artifact renders as a real figure, not a caption line
beside raw content. A table's own caption sits above it and an image's
own sits below. Both backends number each figure -- "Table 1",
"Figure 1" -- counting tables and images separately. The PDF's numbers
are Typst's own figure counter. The HTML page's are `dankg`'s, since
no browser can put one element's counter value into a link elsewhere
on the page.

A figure can be referenced from prose. Its label is the block's own
`name=`, so `[[#chart]]` points at the figure the block named `chart`
produced. A bare reference reads as the figure's own number; add a
`|` and your own words to say something else:

```markdown
Release counts have climbed every year (see [[#chart]]), which
[[#chart|the yearly chart]] shows at a glance.
```

`label=` renames the target without renaming the block, for when a
block's `name=` has to change for a code reason and the prose should
not have to follow:

````markdown
```python name=chart deps=setup produces=file:chart.png label=releases caption="Yearly release count"
draw_chart(data, "chart.png")
```
````

A heading is referenceable the same way, by its own slug -- the
anchor its table-of-contents link already uses. `[[#the-edit-loop]]`
points at `## The edit loop`. The markdown link form works for either
target too: `[the chart](#chart)` and `[that section](#the-edit-loop)`
resolve exactly as the wikilinks above do.

The two backends deliberately differ on one point. A bare heading
reference reads as a section number in the PDF and as the heading's
own title in HTML, because HTML numbers no heading. Writing your own
label makes the two identical. The PDF needs one line in a
`[weave.pdf] template` before a bare heading reference will compile at
all, since Typst cannot reference a heading it has not numbered:

```typst
#set heading(numbering: "1.")
```

A reference that resolves to nothing fails the weave, on the line you
wrote it. No page or PDF is written. One run reports every bad
reference rather than the first. A figure hidden by `weave=hidden` is
reported as hidden rather than as missing. A wikilink naming another
file is untouched and still renders as plain text. `weave` reads one
file, so there is no corpus to resolve that against.

Whether that figure sits inside the pair's own block, or stands
apart from it as a sibling, is `dankg weave`'s own call:
`--figures-inside`/`--figures-outside` sets the document-wide
default (`inside`, unless one is given), and one block's own
`figure=inside`/`figure=outside` overrides it for that block's own
artifact alone:

````markdown
```python name=chart deps=setup produces=file:chart.png figure=outside
draw_chart(data, "chart.png")
```
````

<!-- dankg:depends target=architecture.md#decision-56---figures-inside--figures-outside-with-a-per-block-figure-override quote="lets one block override that document-wide default for its own artifact alone" -->

An uncaptioned artifact is not a real figure in either backend, so
`figure=` changes nothing about it.

`weave=hidden` drops both halves of a paired block together.
`weave=source-hidden` keeps the block's own `produces=file:` artifact
and nothing else -- no source, no captured output, no provenance
line -- for a walkthrough that shows a chart without the code behind
it or the `wrote chart.png` under it. `weave=output-hidden` is the
mirror image: it keeps the source and drops that whole second half,
artifact included, for a snippet worth showing without spoiling the
answer it produces. A failed run is never hidden by either one:

````markdown
```python name=chart deps=setup produces=file:chart.png weave=source-hidden
draw_chart(data, "chart.png")
```
````

`[weave.html] css` and `[weave.pdf] template`/`command` configure a
stylesheet, a Typst preamble, and the compiler invocation (see
Configuration below).

A woven document can also carry real citations and a real references
list. `[@key]` cites one entry, `[@a; @b]` cites several together; a
bare `@key`, with no brackets, is a narrative citation, read as part
of the sentence rather than set off in parentheses:

```markdown
The original result [@netwok2019] held for small graphs.
@smith2020 later extended it to the general case.
```

Entries come from a `hayagriva`-tagged fence -- Typst's own native
bibliography format -- an external file named by frontmatter
`bibliography:`, or both together:

````markdown
---
bibliography: refs.yml
---

```hayagriva
netwok2019:
  type: article
  title: A Networked Result
  author: Doe, Jane
  date: 2019
```
````

The fence renders exactly like any other fenced block -- its raw YAML
shown by default, hidden only with `weave=hidden` -- since the tag
only matters to weave's own bibliography pre-pass, not to rendering.
Everything Hayagriva's own format supports parses: structured authors
and editors, translators and other credited roles under `affiliated`,
a `parent` chain for an article inside an issue inside a journal, and
every remaining field (`doi`, `url`, `page-range`, and the rest).

The PDF backend hands `@key`/`#cite(...)` straight to Typst, which
resolves and formats the whole bibliography itself. The HTML backend
has no such engine, so it renders its own fixed reference-list entry
instead -- every field shown when present, numbered to match each
in-text link. A citation with no bibliography configured at all, or
whose key resolves to nothing, falls back to its own literal text
rather than broken markup.

<!-- dankg:depends target=architecture.md#recorded-eval-output quote="A named `Code` block immediately followed by its own recorded" -->
<!-- dankg:depends target=architecture.md#staleness quote="The rendered page itself never changes because of this" -->
<!-- dankg:depends target=architecture.md#produced-artifacts quote="A recognized pair's source block may also declare" -->
<!-- dankg:depends target=architecture.md#captions quote="so a caption can carry its own spaces" -->
<!-- dankg:depends target=architecture.md#hiding-one-half-of-a-pair quote="each keep exactly one thing" -->
<!-- dankg:depends target=architecture.md#hiding-one-half-of-a-pair quote="A failed run is the one thing neither value hides" -->
<!-- dankg:depends target=architecture.md#decision-54-a-produced-artifact-renders-as-a-real-captioned-figure quote="gives a reader genuine, automatic" -->
<!-- dankg:depends target=architecture.md#decision-55-a-captioned-figure-renders-outside-the-pairs-own-block-in-typst quote="nothing to unwrap a figure out of a box from the outside" -->
<!-- dankg:depends target=architecture.md#decision-58-a-hayagriva-tagged-fence-or-a-frontmatter-bibliography-path-as-a-documents-bibliography-source----full-hayagriva-schema-fidelity quote="Everything Hayagriva's own format supports parses, not a curated" -->
<!-- dankg:depends target=architecture.md#decision-59-citations-and-a-references-list-render-in-both-weave-backends quote="HTML has no such engine to defer to." -->

### `init` — scaffold a new corpus

```sh
dankg init notes/   # creates notes/ if needed, plus .dankg/config,
                     # .dankgignore, and index.md inside it
```

`init` refuses outright, writing nothing, if `notes/.dankg/` already
exists. A `.dankg/` somewhere *above* the target is not a reason to
refuse -- running `init` inside an existing corpus on purpose creates
a nested one. `index.md` and `.dankgignore` are only ever written
when not already there; nothing else already in the directory is
touched.

<!-- dankg:depends target=architecture.md#dankg-init quote="not a mistake to guard against" -->

## Configuration

`.dankg/config` is a minimal INI file at the root of your knowledge base:

```ini
[graph]
depth = 2

[lang.python]
command = uv run python {file}
ext     = py

[lang.sh]
command = sh {file}

[editor]
command = code -g {file}:{line}

[tangle.rust]
glue    = dankg-glue-rust {dir}
command = cargo build --manifest-path {dir}/Cargo.toml
ext     = rs

[keys]
up   = k
down = j

[tui]
commands = commands.md
```

`{file}` substitutes the temporary file eval writes (or the assembled
tree's `{dir}` for tangle). A `[lang.*]` or `[tangle.*]` section is also
the allowlist. DanKG only reports a block in an unconfigured language. It
never runs that block.

<!-- dankg:depends target=architecture.md#config quote="a fenced block in a language with no configured command is never executed, only reported." -->

## Development

```sh
cargo test              # unit tests + golden-output/conformance suites
cargo test --test commonmark -- --nocapture   # CommonMark conformance table
```

DanKG has no external dependencies. Building and testing need no network
access.

## License

GPL-3.0-or-later. See `LICENSE`.
