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

## Commands

```
dankg graph <path>... [--format <fmt>] [--depth N | --all] [-o <file>]
dankg index [<path>]  [--no-cache]
dankg fmt   <path>... [--check]
dankg tui   <path>... [--depth N | --all] [--no-cache]
dankg eval  <path> [--block <name> | --all | --each] [--yes] [--no-write]
dankg eval  [<path>...] --list [--no-cache]
dankg check [<path>...] [--no-cache]
dankg tangle <path>... --lang <lang> [-o <dir>] [--no-cache]
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
- `/` — jump to a node by title, anywhere in the corpus; `n`/`p` repeat it forward/backward
- `e` then `enter` — cycle and run a node's named code blocks in place
- `r` — reset
- `q` — quit
- `?` — full keybinding reference

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
