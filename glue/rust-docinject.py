#!/usr/bin/env python3
"""Reference `dankg tangle` glue script: writes a real `///` doc comment
onto every named block that does not already carry one, and rewrites any
DanKG cross-reference inside the prose it copies into a rustdoc intra-doc
link (architecture.org, Tangle, and the "binary-only reader" gap the
visibility/composition pilots' own discussion surfaced).

Like `rust.py` and `rust-doclint.py`, this is a plain, standalone, opt-in
program -- not part of dankg's own source. It reads both block *and* prose
content, which decision 27 reserves for glue and refuses to let DanKG's
own code do.

Usage, in `.dankg/config`:

    [tangle.rust]
    glue = python3 /path/to/glue/rust-docinject.py {dir} [source-root]

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
import re
import sys
from pathlib import Path

MANIFEST_NAME = ".dankg-tangle-manifest.json"

# `[text](file.md#name)` or `[text](#name)` -- the two anchored forms
# DanKG's own link syntax uses (architecture.org, Link resolution). Crude
# and independent of dankg's own parser, the same tradeoff every glue
# script here already makes (decision 27: glue has no access to `md/`) --
# in particular this does not know about fences or code spans, so a
# literal `[x](y#z)` shown as an *example* inside a code span would be
# rewritten too. None of this repo's own literate sources do that.
_LINK_RE = re.compile(r"\[([^\]]+)\]\(([\w./-]*)#([\w-]+)\)")


def markdown_paragraphs(md_src: str) -> list[str]:
    """Crude paragraph split: consecutive non-blank lines outside fences,
    skipping headings and fence content. Independent of `rust-doclint.py`'s
    own copy on purpose -- each glue script is a standalone program
    (decision 27), forkable and replaceable without touching its
    siblings."""
    paragraphs = []
    current: list[str] = []
    in_fence = False
    fence_marker = ""
    for line in md_src.splitlines():
        stripped = line.strip()
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


def build_name_table(manifest: dict) -> dict[str, dict[str, str]]:
    """`source .md filename -> {block name -> intra-doc path}`, so a link
    inside one file's prose can be resolved against any file this same
    tangle run produced, not just its own."""
    table: dict[str, dict[str, str]] = {}
    for entry in manifest.get("files", []):
        mod_path = entry["path"]
        if mod_path.endswith(".rs"):
            mod_path = mod_path[: -len(".rs")]
        mod_path = mod_path.replace("/", "::")
        names = table.setdefault(entry["source"], {})
        for b in entry.get("blocks", []):
            names[b["name"]] = f"crate::{mod_path}::{b['name']}"
    return table


def rewrite_links(text: str, this_source: str, name_table: dict[str, dict[str, str]]) -> str:
    def repl(match: re.Match) -> str:
        label, filepart, name = match.group(1), match.group(2), match.group(3)
        source = filepart if filepart else this_source
        target = name_table.get(source, {}).get(name)
        if target is None:
            return match.group(0)  # unresolved -- leave exactly as written
        return f"[{label}]({target})"

    return _LINK_RE.sub(repl, text)


def doc_lines(paragraphs: list[str]) -> list[str]:
    out: list[str] = []
    for i, paragraph in enumerate(paragraphs):
        if i:
            out.append("///")
        out.append(f"/// {paragraph}")
    return out


def plan_injections(
    manifest: dict, source_root: Path, name_table: dict[str, dict[str, str]]
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

            rewritten = [rewrite_links(p, source, name_table) for p in paragraphs]
            comment = "\n".join(doc_lines(rewritten)) + "\n"
            injections.setdefault(out_path, []).append((content, comment))

    return injections


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


def main() -> None:
    if len(sys.argv) not in (2, 3):
        sys.exit(f"usage: {sys.argv[0]} <tangled-directory> [source-root]")
    out_dir = Path(sys.argv[1])
    source_root = Path(sys.argv[2]) if len(sys.argv) == 3 else Path(".")
    manifest_path = out_dir / MANIFEST_NAME
    if not manifest_path.exists():
        return  # no file -> source mapping without it; nothing to do

    manifest = json.loads(manifest_path.read_text())
    name_table = build_name_table(manifest)
    injections = plan_injections(manifest, source_root, name_table)

    for out_path, items in injections.items():
        if apply_injections(out_dir, out_path, items):
            print(f"docinject: added doc comments to {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
