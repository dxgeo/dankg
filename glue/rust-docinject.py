#!/usr/bin/env python3
"""Reference `dankg tangle` glue script: writes a real `///` doc comment
onto every named block that does not already carry one, and rewrites any
DanKG cross-reference inside the prose it copies into a rustdoc intra-doc
link (architecture.md, Tangle, and the "binary-only reader" gap the
visibility/composition pilots' own discussion surfaced).

Like `rust.py` and `rust-doclint.py`, this is a plain, standalone, opt-in
program -- not part of dankg's own source. It reads both block *and* prose
content, which decision 27 reserves for glue and refuses to let DanKG's
own code do.

Usage, in `.dankg/config`:

    [tangle.rust]
    glue = python3 /path/to/glue/rust-docinject.py {dir} [source-root] \
               [--doc-base <url>]

`--doc-base` is what a link into the design record itself resolves
against -- a corpus `.md` this tangle run never produced has no module
path to point at. Given one, such a link becomes a reference-style link
to `<url>/<root-relative path>#<fragment>`; omitted, the link is dropped
and its label kept as plain text. See `rewrite_links`.

`source-root` is the same escape hatch `rust-doclint.py` already needed,
for the same reason: a corpus-wide tangle's manifest `source` field is
root-relative, and neither `{dir}` nor anything else this script is
handed carries that root on its own.

**What it writes, and the backoff.** A block whose own tangled content
already starts with `//!`/`///` is left untouched entirely -- the same
"does a file already exist at this exact spot" rule `rust.py` already
applies to `mod.rs`, one level deeper. Everything else gets a `///`
synthesized from its *owned prose*: the paragraphs between the previous
block's own closing fence (or, for a file's first block, the nearest
heading above it) and this block's own opening fence -- manifest v2's
`line`/`end_line` are exactly what makes that span computable without
reading the whole file, or anything DanKG did not already hand over.

Deliberately `///` only, never a synthesized `//!`. A file-level module
doc characterizes the *whole file*, and this script has no way to know
whether a given block's own owned prose was ever meant to carry that
weight -- `hash.md`'s own `module_doc` block writes one by hand for
exactly this reason, and this script's backoff leaves it alone rather
than guessing.

**Owned prose spans the whole *source* file, not one output file.** A
single `.md` can contribute more than one tangled file (`hash.md` itself
does: `Cargo.toml` via `path=`, `hash.rs` from everything else), and a
block's own predecessor in *document order* may sit in a different output
file entirely. Windowing per output file instead of per source file would
let an earlier, unrelated output file's own prose bleed into a later
file's first block -- this script gathers every block sharing one
`source`, sorts them by their true document order, and walks that,
writing each block's synthesized comment into whichever output file it
actually belongs to.

**Cross-reference rewriting.** A DanKG link inside the copied prose,
`[text](other.md#name)` or `[text](#name)`, is rewritten to
`[text](crate::path::to::name)` when `name` names a block the *same*
tangle run also produced -- an intra-doc link, which `cargo doc` resolves
and verifies, unlike a relative path to a `.md` file a binary-only reader
cannot open at all. This assumes a block's DanKG `name=` matches the Rust
identifier it actually defines; nothing here parses Rust to confirm that
(decision 1), so an author who names a block differently from what it
defines gets a link that resolves to the wrong place, or nowhere -- an
unresolved target is left exactly as written rather than guessed at.
Nothing about the *code* reference (a `use`, if the target lives in
another tangled file) is touched -- that stays the author's own, and stays
subject to the composition-pilot's own finding about what does and does
not survive a cross-tangle-file boundary unmodified.
"""

import json
import posixpath
import re
import sys
from pathlib import Path

MANIFEST_NAME = ".dankg-tangle-manifest.json"

# `[text](file.md#name)` or `[text](#name)` -- the two anchored forms
# DanKG's own link syntax uses (architecture.md, Link resolution). Crude
# and independent of dankg's own parser, the same tradeoff every glue
# script here already makes (decision 27: glue has no access to `md/`) --
# in particular this does not know about fences or code spans, so a
# literal `[x](y#z)` shown as an *example* inside a code span would be
# rewritten too. None of this repo's own literate sources do that.
_LINK_RE = re.compile(r"\[([^\]]+)\]\(([\w./-]*)(?:#([\w-]+))?\)")


def markdown_paragraphs(md_src: str) -> list[str]:
    """Crude paragraph split: consecutive non-blank lines outside fences,
    skipping headings and fence content. Independent of `rust-doclint.py`'s
    own copy on purpose -- each glue script is a standalone program
    (decision 27), forkable and replaceable without touching its
    siblings."""
    paragraphs = []
    current: list[str] = []
    in_fence = False
    in_comment = False
    fence_marker = ""
    for line in md_src.splitlines():
        stripped = line.strip()
        if in_comment:
            # A `dankg:depends` marker is corpus metadata, not prose, and
            # spans as many lines as its `quote=` needs. Copying one into a
            # `///` put raw marker syntax in rustdoc output -- found by
            # reading the first wrapped run's own over-long lines, every one
            # of which turned out to be a leaked `target=`.
            if "-->" in stripped:
                in_comment = False
            continue
        if stripped.startswith("<!--"):
            if current:
                paragraphs.append(" ".join(current))
                current = []
            if "-->" not in stripped:
                in_comment = True
            continue
        if stripped.startswith("```") or stripped.startswith("~~~"):
            if not in_fence:
                in_fence = True
                fence_marker = stripped[:3]
            elif stripped.startswith(fence_marker):
                in_fence = False
            continue
        if in_fence:
            continue
        if not stripped or stripped.startswith("#"):
            if current:
                paragraphs.append(" ".join(current))
                current = []
            continue
        current.append(stripped)
    if current:
        paragraphs.append(" ".join(current))
    return paragraphs


def strip_frontmatter(lines: list[str]) -> list[str]:
    """Blanks out a leading `---`/`...`-delimited frontmatter block in
    place of its own lines, preserving every line number so a block's
    `line`/`end_line` (measured against the real file, frontmatter
    included) still index correctly. Independent of `md/frontmatter.rs`'s
    own delimiter rule on purpose (decision 27: glue has no access to it),
    matched by hand instead: opens on line one with exactly `---`, closes
    on the next line that is exactly `---` or `...`. Found the same way as
    the TOML-corruption bug -- a real run, not read-through -- on a
    frontmatter line otherwise read as an ordinary paragraph and swept
    into the very first block's own injected doc comment."""
    if not lines or lines[0].strip() != "---":
        return lines
    for i in range(1, len(lines)):
        if lines[i].strip() in ("---", "..."):
            out = lines.copy()
            for j in range(i + 1):
                out[j] = ""
            return out
    return lines  # no closing delimiter -- not frontmatter after all, leave it


def paragraphs_in_window(lines: list[str], start_line: int, end_line_exclusive: int) -> list[str]:
    """`markdown_paragraphs`, restricted to the 1-indexed line range
    `(start_line, end_line_exclusive)`."""
    window = lines[start_line : end_line_exclusive - 1]
    return markdown_paragraphs("\n".join(window))


def block_content(lines: list[str], line: int, end_line: int) -> str:
    """A block's own fenced content, excluding the fence delimiters --
    `line`/`end_line` are the fence lines themselves (manifest version 2)."""
    return "\n".join(lines[line : end_line - 1])


def has_doc_comment(content: str) -> bool:
    """Whether a block's own source already opens a `//!`/`///` run
    anywhere in it -- the backoff check. Anywhere, not just the first
    line, since a block may lead with a plain `//` comment or an
    attribute before its own doc comment."""
    return any(s.startswith("//!") or s.startswith("///") for s in (l.strip() for l in content.splitlines()))


def module_path_for(out_path: str) -> str | None:
    """`crate::...` module path for one tangled output file, or `None` when
    the file is not addressable from the library crate's own docs at all.

    `mod.rs` and `lib.rs` do not contribute a path segment: `layout/mod.rs`
    is `crate::layout`, not `crate::layout::mod`. `mod` is a *keyword*, so
    the naive spelling is not merely ugly, it is an invalid path, and
    `architecture.md`'s planned
    `#![deny(rustdoc::broken_intra_doc_links)]` would make one a build
    failure rather than a warning. `main.rs` is a separate binary crate,
    which a library doc comment cannot link into on any spelling, so it
    resolves to nothing and its links are left exactly as written."""
    if not out_path.endswith(".rs"):
        return None  # a `path=` block can land on Cargo.toml -- no module
    segments = out_path[: -len(".rs")].split("/")
    if segments[-1] == "main":
        return None
    if segments[-1] in ("mod", "lib"):
        segments.pop()
    return "::".join(["crate", *segments])


def resolve_source(this_source: str, filepart: str) -> str:
    """A link's own target, as the manifest's `source` field spells it.

    Prose writes a path relative to the file it sits in (`mermaid.md`,
    `../layout/mod.md`); the manifest keys `source` relative to the tangle
    root (`src/render/mermaid.md`). Looking the written form up directly
    missed every cross-file link silently -- `mermaid.md` is simply not a
    key. An empty `filepart` is a same-file link and already *is* the
    key."""
    if not filepart:
        return this_source
    joined = posixpath.join(posixpath.dirname(this_source), filepart)
    return posixpath.normpath(joined)


def external_url(doc_base: str, path: str, fragment: str | None) -> str:
    """A corpus file's own URL under `doc_base`, for a link this tangle run
    cannot answer with a module path."""
    url = f"{doc_base.rstrip('/')}/{path}"
    return f"{url}#{fragment}" if fragment else url


def build_name_table(manifest: dict) -> dict[str, dict[str, str]]:
    """`source .md filename -> {block name -> intra-doc path}`, so a link
    inside one file's prose can be resolved against any file this same
    tangle run produced, not just its own."""
    table: dict[str, dict[str, str]] = {}
    for entry in manifest.get("files", []):
        mod_path = module_path_for(entry["path"])
        if mod_path is None:
            continue
        names = table.setdefault(entry["source"], {})
        for b in entry.get("blocks", []):
            names[b["name"]] = f"{mod_path}::{b['name']}"
    return table


def build_module_table(manifest: dict) -> dict[str, str]:
    """`source .md filename -> that file's own module path`, for a link
    that names a whole module rather than one block. The dominant style in
    this corpus's own prose is exactly that: `[`render::mermaid`](mermaid.md)`
    with no `#fragment` at all, which the block table cannot answer.

    First output wins. One source can tangle to several files (`hash.md`
    writes `Cargo.toml` as well as `hash.rs`), and only one of them can be
    what a bare file link means."""
    table: dict[str, str] = {}
    for entry in manifest.get("files", []):
        mod_path = module_path_for(entry["path"])
        if mod_path is not None:
            table.setdefault(entry["source"], mod_path)
    return table


def rewrite_links(
    text: str,
    this_source: str,
    name_table: dict[str, dict[str, str]],
    module_table: dict[str, str] | None = None,
    doc_base: str | None = None,
    source_root: Path | None = None,
    refs: dict[str, str] | None = None,
) -> str:
    """Rewrites a DanKG link for a reader who has only the built docs.

    A `#fragment` names one block and resolves through `name_table`; a bare
    file link names a whole module and resolves through `module_table`.
    Either becomes a rustdoc intra-doc link, which `cargo doc` resolves
    through the compiler's own name resolution.

    A link into a corpus file this tangle run never produced -- the design
    record itself, `architecture.md` -- has no module to point at. Left
    alone it renders as a relative `href` that 404s in any built docs,
    which is the reader this whole script exists for. So:

    * with `doc_base`, it becomes a *reference-style* link: `[label]` here,
      and `label -> url` recorded in `refs` for the caller to emit as a
      trailing definition. This is std's own idiom for an external design
      document (`[RFC]: https://github.com/rust-lang/rfcs/blob/...`), and
      it keeps a long URL out of the middle of a sentence.
    * without one, the link is dropped and its label kept as plain text. A
      reader loses the pointer, which beats following a dead one.

    Both paths require the target to be a `.md` file that actually exists
    under `source_root`. Prose here *discusses* link syntax, so a doc
    comment can contain `[*a*](x)` or a `file.md#name` illustration, and
    neither names a real file. Those are left exactly as written."""

    def repl(match: re.Match) -> str:
        label, filepart, name = match.group(1), match.group(2), match.group(3)
        if not filepart and not name:
            return match.group(0)  # `[text]()` names nothing at all
        source = resolve_source(this_source, filepart)
        if name:
            target = name_table.get(source, {}).get(name)
        else:
            target = (module_table or {}).get(source)
        if target is not None:
            return f"[{label}]({target})"

        # Not something this run tangled. Only a real corpus `.md` qualifies.
        if not source.endswith(".md"):
            return match.group(0)
        if source_root is None or not (source_root / source).exists():
            return match.group(0)
        if doc_base is None:
            return label  # de-linked, label kept
        url = external_url(doc_base, source, name)
        if refs is None:
            return f"[{label}]({url})"
        if refs.setdefault(label, url) != url:
            return f"[{label}]({url})"  # same label, two targets -- inline it
        return f"[{label}]"

    return _LINK_RE.sub(repl, text)


_ATOM_RE = re.compile(r"(?:``[^`]*``|`[^`]*`|\[[^\]]*\]\([^)]*\)|[^\s])+")

DOC_WIDTH = 76


def wrap_doc(text: str, width: int = DOC_WIDTH) -> list[str]:
    """Greedy wrap that treats a `code span`, or a `[text](link)` whose
    target `rewrite_links` just rewrote, as one atomic token and never
    breaks inside one. A naive word wrap splits a span that itself
    contains a space, the same hazard `dankg fmt` hits on prose and the
    reason CLAUDE.md forbids it there. An atom longer than `width` gets
    its own line and overflows, since the alternative is corrupting it.
    `width` budgets the text alone, so a `/// ` prefix lands the line at
    80 columns."""
    lines: list[str] = []
    current = ""
    for atom in _ATOM_RE.findall(text):
        if not current:
            current = atom
        elif len(current) + 1 + len(atom) <= width:
            current += " " + atom
        else:
            lines.append(current)
            current = atom
    if current:
        lines.append(current)
    return lines or [""]


def doc_lines(paragraphs: list[str], refs: dict[str, str] | None = None) -> list[str]:
    """Doc-comment lines for one block. `refs` becomes a trailing paragraph
    of link reference definitions, one per line and never wrapped -- a
    definition's own destination has to stay on its line to read as one."""
    out: list[str] = []
    for i, paragraph in enumerate(paragraphs):
        if i:
            out.append("///")
        out.extend(f"/// {line}" for line in wrap_doc(paragraph))
    if refs:
        out.append("///")
        out.extend(f"/// [{label}]: {url}" for label, url in refs.items())
    return out


def plan_injections(
    manifest: dict,
    source_root: Path,
    name_table: dict[str, dict[str, str]],
    doc_base: str | None = None,
) -> dict[str, list[tuple[str, str]]]:
    """`out_path -> [(block's own raw content, doc comment text), ...]`, in
    the document order each output file's own blocks will need them
    inserted -- see the module doc for why this groups by *source* file
    first rather than walking each output file's own (possibly
    out-of-order-relative-to-the-source) block list independently."""
    by_source: dict[str, list[tuple[dict, str]]] = {}
    for entry in manifest.get("files", []):
        for b in entry.get("blocks", []):
            by_source.setdefault(entry["source"], []).append((b, entry["path"]))

    module_table = build_module_table(manifest)

    injections: dict[str, list[tuple[str, str]]] = {}
    for source, blocks in by_source.items():
        source_path = source_root / source
        if not source_path.exists():
            continue
        source_lines = strip_frontmatter(source_path.read_text().splitlines())
        blocks.sort(key=lambda pair: pair[0]["line"])

        window_start = 0
        for b, out_path in blocks:
            paragraphs = paragraphs_in_window(source_lines, window_start, b["line"])
            window_start = b["end_line"]  # advances regardless of `out_path` --
            # a `path=` block's own prose is still spent once it is read,
            # even when that block's output is not Rust at all (below)

            if not out_path.endswith(".rs"):
                continue  # a `path=` block can land anywhere -- e.g.
                # Cargo.toml, selected by *fence* language, not by what its
                # own output is (hash.md's own note on `tangle.rs`'s
                # `comment_prefix`); `///` is Rust syntax and would corrupt
                # anything else -- found by actually running this against
                # `hash.md`, which has exactly this shape

            content = block_content(source_lines, b["line"], b["end_line"])
            if has_doc_comment(content) or not paragraphs:
                continue
            if targets_mod_declaration(content):
                continue  # see `targets_mod_declaration`

            refs: dict[str, str] = {}
            rewritten = [
                rewrite_links(
                    p, source, name_table, module_table, doc_base, source_root, refs
                )
                for p in paragraphs
            ]
            comment = "\n".join(doc_lines(rewritten, refs)) + "\n"
            injections.setdefault(out_path, []).append((content, comment))

    return injections


_MOD_RE = re.compile(r"^(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*;")


def targets_mod_declaration(content: str) -> bool:
    """Whether the item an injected comment would attach to is a `mod`
    declaration. It must then write nothing at all.

    Rust *merges* a `///` on `mod foo;` with the inner `//!` docs inside
    `foo.rs` itself, and rustdoc then resolves that merged doc's intra-doc
    links in the *parent* module's scope. `graph/build.md`'s own
    `[`super::resolve`]` resolves correctly as `graph::resolve` while it
    stays attached to `graph::build`. Injecting a `///` onto
    `graph/mod.md`'s `pub mod build;` re-scoped it to `dankg::resolve`,
    which does not exist, and `cargo doc` reported exactly that: "no item
    named `resolve` in module `dankg`". Nothing in the injected comment
    mentions `resolve` at all, which is what made it hard to place --
    reverting one file at a time is what found it.

    Not cosmetic. `architecture.md`'s own plan is a
    `#![deny(rustdoc::broken_intra_doc_links)]` crate attribute, which
    turns a merged-scope break into a hard build failure rather than a
    warning. This is the same class as `leading_skip_offset`'s `use`: an
    item an injected comment must not land on."""
    for line in content.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("//") or stripped.startswith("#["):
            continue
        return bool(_MOD_RE.match(stripped))
    return False


def leading_skip_offset(content: str) -> int:
    """Byte offset within a block's own content, just past any leading
    blank lines, `use` statements, and attributes -- a Rust doc comment
    attaches to whatever item comes *directly* after it, and a `use`
    (needed by a real cross-tangle-file reference -- the composition
    pilot's own escape hatch) is itself an item. Without this, an injected
    comment silently documents the `use` instead of the function it was
    actually written about, and rustdoc renders nothing wrong -- it just
    renders nothing at all where a reader expected the doc to be. Found
    exactly that way, on `docinject-pilot`'s own `call_greeting` block,
    not read through in advance."""
    offset = 0
    for line in content.splitlines(keepends=True):
        stripped = line.strip()
        if stripped == "" or stripped.startswith("use ") or stripped.startswith("#"):
            offset += len(line)
            continue
        break
    return offset


def apply_injections(out_dir: Path, out_path: str, items: list[tuple[str, str]]) -> bool:
    """Locates each block's own already-tangled content by literal
    substring search -- tangle copies a block's source verbatim (join
    blank lines aside), so this needs no output-file line numbers of its
    own, only the block's raw text and a search cursor that only ever
    moves forward, which is what keeps a repeated block body from being
    found twice out of order."""
    rust_path = out_dir / out_path
    if not rust_path.exists():
        return False
    text = rust_path.read_text()

    cursor = 0
    pieces: list[tuple[int, str]] = []
    for content, comment in items:
        idx = text.find(content, cursor)
        if idx == -1:
            return False  # tangle's own output did not match what we expected -- write nothing
        cursor = idx + len(content)
        pieces.append((idx + leading_skip_offset(content), comment))

    for idx, comment in reversed(pieces):
        text = text[:idx] + comment + text[idx:]
    rust_path.write_text(text)
    return True


def parse_args(argv: list[str]) -> tuple[Path, Path, str | None]:
    """`(out_dir, source_root, doc_base)`. `--doc-base` is a flag rather
    than a third positional, so it stays unambiguous next to the optional
    `source-root` that already reads as one."""
    doc_base = None
    rest: list[str] = []
    i = 0
    while i < len(argv):
        if argv[i] == "--doc-base":
            if i + 1 >= len(argv):
                raise ValueError("--doc-base needs a URL")
            doc_base = argv[i + 1]
            i += 2
            continue
        if argv[i].startswith("--doc-base="):
            doc_base = argv[i].split("=", 1)[1]
            i += 1
            continue
        rest.append(argv[i])
        i += 1
    if len(rest) not in (1, 2):
        raise ValueError("wrong number of paths")
    return Path(rest[0]), Path(rest[1]) if len(rest) == 2 else Path("."), doc_base


def main() -> None:
    try:
        out_dir, source_root, doc_base = parse_args(sys.argv[1:])
    except ValueError:
        sys.exit(
            f"usage: {sys.argv[0]} <tangled-directory> [source-root] "
            "[--doc-base <url>]"
        )
    manifest_path = out_dir / MANIFEST_NAME
    if not manifest_path.exists():
        return  # no file -> source mapping without it; nothing to do

    manifest = json.loads(manifest_path.read_text())
    name_table = build_name_table(manifest)
    injections = plan_injections(manifest, source_root, name_table, doc_base)

    for out_path, items in injections.items():
        if apply_injections(out_dir, out_path, items):
            print(f"docinject: added doc comments to {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
