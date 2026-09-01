# DanKG

DanKG (Dan's Knowledge Grapher) is a vanilla-markdown knowledge graphing
tool. You write plain markdown; DanKG turns headings into graph nodes and
links between them into edges, can run and record the output of code
blocks in place, and can assemble a literate document's named code blocks
into a real, buildable source tree.

- **Zero dependencies.** Pure Rust, standard library only, forever.
- **Plaintext-driven.** Every input and output is a file an editor (or an
  LLM) can read and write directly.
- **Nothing runs automatically.** `dankg graph` only ever displays; code is
  evaluated only when `dankg eval` is invoked, and only after printing the
  plan and asking for confirmation.

See `architecture.md` for the full design record (numbered decisions,
data model, module layout) and `project.md` for the "what and why." Both
are themselves a dankg corpus -- `dankg graph architecture.md` renders
the design record as the same kind of graph this tool draws for anything
else.

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

A `.dankg/` directory marks the root of a knowledge base (`mkdir .dankg`
at the top of your notes tree); without one, the root is inferred from the
paths you name. Everything under the root is in the corpus; nothing
outside it is.

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

Run `dankg --help` for the full option reference — it's kept in sync with
the binary, not duplicated here.

### `graph` — render a knowledge graph

```sh
dankg graph notes/ --format html -o graph.html   # one self-contained page
dankg graph notes/index.md --format json         # the whole index, for scripting
dankg graph notes/index.md --format dot | dot -Tsvg -o graph.svg
dankg graph notes/index.md --format mermaid
```

`--format json` always emits the whole index regardless of the entry
point — that's what makes it a stable, scriptable surface. `--depth`/
`--all` only shape the drawn formats (`html`/`dot`/`mermaid`).

### `tui` — explore the graph in a terminal

```sh
dankg tui notes/
```

Arrow keys or `hjkl` to move, `tab` to expand a node's hidden neighbours,
`enter` to open the selected node in your configured `[editor] command`,
`e` then `enter` to cycle and run a node's named code blocks in place, `p`
to pan, `r` to reset, `q` to quit, `?` for the full keybinding reference.

### `eval` — run and record literate code blocks

Give a fenced code block a `name=` and it becomes runnable:

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
Configuration below) or `dankg eval` refuses to run it. `deps=` names other
blocks that must run first, concatenated ahead of the target into one
process:

````markdown
```python name=setup
count = 42
```

```python name=greet deps=setup
print(count)
```
````

`deps=` can also reach into another file — `deps=lib.md#helper` resolves
relative to the file that wrote it, the same way a written link would.
This is enough to write a genuinely multi-file literate program with no
`import`/`use`/`mod` at all: the whole reachable chain is concatenated into
one file before it's handed to the interpreter or compiler.

### `check` — the CI gate

```sh
dankg check notes/
```

Exits non-zero if the corpus has an unresolved link, or a written `eval`
result whose hash no longer matches its current source, dependencies, or
configured command (including a dependency in another file). Deliberately
separate from `graph`, so a half-written note never fails a build.

### `fmt` — normalize markdown

```sh
dankg fmt notes/*.md           # rewrite in place
dankg fmt --check notes/*.md   # report which files would change; write nothing
```

Refuses to write any file whose formatted form doesn't re-parse to the
same document, so a formatter bug can't quietly corrupt a note.

### `tangle` — assemble a literate program into a source tree

```sh
dankg tangle notes/ --lang rust -o build/
```

Groups every named, top-level code block in the given language by its
containing top-level heading (one heading, one file), assembles them into
`build/` (default `.dankg/build/<lang>/`), and runs any configured
`[tangle.<lang>] glue`/`command` against the result.

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
tree's `{dir}` for tangle). A `[lang.*]`/`[tangle.*]` section is also the
allowlist: a block in an unconfigured language is only ever reported, never
run.

## Development

```sh
cargo test              # unit tests + golden-output/conformance suites
cargo test --test commonmark -- --nocapture   # CommonMark conformance table
```

No external dependencies means no network access is needed to build or
test.

## License

GPL-3.0-or-later. See `LICENSE`.
