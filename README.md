# DanKG

DanKG (Dan's Knowledge Grapher) turns a directory of ordinary markdown into a
knowledge graph. Every heading is a node, every link you write is an edge, and
the graph comes out as JSON, Graphviz, Mermaid, or a single self-contained HTML
page you can commit next to the notes it describes.

It is written in Rust with no dependencies at all — no crates, standard library
only. The markdown parser, the JSON writer, the argument parser, and the graph
layout are all in this repository.

## Why

Three things follow from "plain markdown, no dependencies":

* **Your notes stay yours.** There is no database and no proprietary format.
  Every file is readable, greppable, and diffable without DanKG installed.
* **An agent can drive it.** Any LLM can run DanKG for you, because the input
  and the output are both plaintext.
* **The graph is a build artifact.** Layout is deterministic, so a rendered
  graph can be committed and reviewed in a diff like anything else.

## Install

```sh
git clone <your-clone-url> dankg
cd dankg
cargo build --release
```

The binary lands at `target/release/dankg`. There is nothing else to install.

## Use

```sh
dankg graph notes/project.md                 # JSON graph on stdout
dankg graph notes/project.md -o graph.json
dankg graph notes/project.md --format dot    # graphviz, with DanKG's coordinates
dankg graph notes/project.md --format mermaid
dankg index                                  # what the walk found, and the cache
dankg fmt notes/*.md                         # rewrite to normal form
dankg fmt --check notes/*.md                 # CI gate: names files that differ
```

Diagnostics always go to stderr, so stdout stays pipeable:

```sh
dankg graph notes/project.md | jq '.nodes[] | select(.resolved == false)'
```

### The root

DanKG works on one directory at a time. Marking it is a `mkdir`:

```sh
mkdir .dankg
```

From then on, running DanKG on any file inside it walks up, finds the `.dankg/`,
and indexes the whole directory -- because backlinks are only honest when every
file has been seen. The path you name picks the view, never the index. With no
`.dankg/` anywhere above it, the root is the directory the named paths share.

The root is also a hard boundary: a link that climbs above it is refused rather
than followed, and symlinks are never walked through. The markdown being graphed
is frequently something an agent wrote.

Two optional files live alongside it:

```
.dankg/config      [graph] depth, [lang.*] interpreters -- INI, no nesting
.dankgignore       one pattern per line; `!` un-ignores, `**` crosses directories
.dankg/cache/      derived data; delete it whenever you like
```

Dot-directories are skipped without a pattern, so `.git/` never needs mentioning.
The cache is an optimisation and nothing else -- `--no-cache` produces identical
output, and there is a test that says so.

### Links

Two forms, both read, and they mean the same thing:

```markdown
[Constraints](#constraints)                  same file, by heading
[the ideas file](notes/ideas.md#future)      another file, by heading
[[Ideas]]                                    by heading, anywhere in the root
[[project#Constraints|see here]]             by file stem, with a label
```

### Drawing

`--format json` always gives you the whole index; it is the scriptable surface,
so it never hands back a fragment. The drawn formats show a *view*: the file you
named, plus everything within a couple of hops of it.

```sh
dankg graph notes/project.md --format dot --depth 1
dankg graph notes/project.md --format mermaid --all
dankg graph notes/ --format dot              # a directory means the whole corpus
```

Layout is DanKG's own -- layered, computed in Rust, in whole pixels, and
identical from one run to the next, so a drawn graph can be committed and
diffed. Graphviz honours those coordinates exactly under `neato -n`:

```sh
dankg graph notes/project.md --format dot | neato -n -Tsvg > graph.svg
dankg graph notes/project.md --format dot | dot -Tsvg > graph.svg   # its own layout
```

Mermaid will not take coordinates, so it gets the ordering instead: nodes rank
by rank, left to right, with DanKG's edge directions. Paste it into anything
that renders mermaid.

### Links

A link creates an edge in one direction. If the target links back, DanKG draws
one undirected edge instead of two arrows. A link that resolves to nothing
becomes a placeholder node and a warning on stderr — dangling links are signal,
so nothing is dropped silently.

Links that escape the root directory are refused, which matters because "an LLM
can run it for you" means the markdown being graphed is frequently untrusted.

### Frontmatter

A flat `key: value` subset of YAML. Scalars and inline lists, nothing nested:

```markdown
---
title: DanKG
tags: [rust, graphs]
alias: grapher
---
```

Anything outside that subset warns with its line number and is skipped, never
guessed at. `dankg fmt` writes the block back exactly as you wrote it.

### Formatting

`dankg fmt` rewrites markdown into a normal form so that a knowledge base stays
diffable and a graph never changes because someone indented a list differently.
It normalizes heading style, bullet and delimiter consistency, list
indentation, fence length, blank lines, and trailing whitespace. It does not
reflow paragraphs: where you break a line is your business.

It refuses to write any file whose formatted form does not re-parse to the same
document, so a formatter bug cannot quietly destroy a note.

## Markdown coverage

DanKG implements a documented subset of CommonMark, and measures it rather than
claiming it. Implemented: ATX headings, fenced code with info strings, inline
and reference links, wikilinks, nested ordered and unordered lists, emphasis and
strong, code spans, paragraphs, thematic breaks, hard breaks.

Everything else — setext headings, HTML blocks, tables, block quotes, indented
code, images, autolinks — is preserved verbatim as passthrough text. It carries
no graph meaning, but it is never lost or rewritten.

```sh
cargo test --test commonmark -- --nocapture   # the full per-section table
```

Current conformance is 368/652 (56%), with the implemented sections scoring as
intended: emphasis 94%, ATX headings 94%, code spans 90%, fenced code 89%. Most
of the remainder is deliberately out of scope.

Separately, 648 of the spec's 652 markdown inputs survive a full
format-and-reparse round trip. The four that do not are listed by example
number in `tests/fmt.rs`, and `dankg fmt` refuses to rewrite them rather than
mangling them.

## Status

`dankg graph` in all four formats — JSON, dot, mermaid and HTML — plus
`dankg index` and `dankg fmt` work today, over a discovered root with a cache
and a layered layout. Code evaluation and literate database management over
DuckDB are designed but not yet built; `architecture.org` records the design
and the milestone order, and `project.org` records what DanKG is for.

The HTML page is one file: the stylesheet and the script are inlined, so there
is nothing to fetch and nothing to serve. Rust computes the coordinates; in the
browser you can pan, zoom, open a node's source file, and click a node to
expand neighbours the view left out — the whole index travels in the page, so
expanding needs no second run.

## Testing

```sh
cargo test
```

The suite covers CommonMark conformance against a vendored copy of the spec,
golden JSON graphs over a fixture corpus, format round-tripping, link
resolution including root-escape refusal, golden dot, mermaid and HTML output,
and the real binary at the process boundary.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
