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
