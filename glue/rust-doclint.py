#!/usr/bin/env python3
"""Reference `dankg tangle` glue script: flags a tangled block's rustdoc
comment (`//!`/`///`) that duplicates the literate source's own prose,
rather than pointing back to it or adding something new (architecture.org,
Tangle, and the doc-comment-duplication finding from the hash.rs pilot).

Like `glue/rust.py`, this is a plain, standalone, opt-in program -- not
part of dankg's own source, and not something dankg ever imports or links.
It reads both block *and* prose content, which is exactly what decision 27
reserves for glue and refuses to let DanKG's own code do: DanKG must not
infer what a block's content means, but an extension the reader configures
and runs is free to.

Usage, in `.dankg/config`:

    [tangle.rust]
    glue = python3 /path/to/glue/rust-doclint.py {dir} [source-root]

Chain it after the structural glue (mod.rs generation) with `&&` in a
shell wrapper if both are needed; this script does not generate anything
itself and never touches the tangled tree.

`source-root` exists because the manifest's own `source` field is not
consistently resolvable from wherever this script happens to run. Tangle
a single named file and it is exactly the path given on the command line
(cwd-relative, so no root is needed); tangle a corpus (several paths, or a
directory) and it becomes *root*-relative instead -- and the tangle root
is neither `{dir}` (the output tree) nor anything else this script is
handed, so it has no way to reconstruct it on its own. Found the hard way:
version 2's own multi-file pilot (`literate/layout-pilot/`) tangled clean
and silent, which looked like a passing result until every one of its
`source` lookups turned out to be silently missing the file entirely --
see that pilot's own notes. Passing `source-root` (wherever the reader's
own `dankg tangle` was invoked from, relative to this file's `source`
values) is the fix; omitted, it defaults to `.`, which is exactly right
for the single-file case and was the only case this script had actually
been proven against before that pilot.

What it does (manifest version 2): the sidecar manifest DanKG writes when
`glue` is configured (`.dankg-tangle-manifest.json`) now carries each
tangled file's own contributing blocks, in document order, each with the
`line`/`end_line` of its fence in the *source* `.md` -- the same fields
`result.rs` already uses for write-back positioning, just exposed. That is
enough to read a block's own doc comment straight out of the source (the
lines between its fence markers) and compare it only against the prose in
the *window* immediately before it -- from the previous block's closing
fence to this one's opening fence -- rather than every paragraph in the
whole file. DanKG hands over line numbers, nothing more: which paragraph
"belongs" to which block is still entirely this script's own judgment
(decision 27), just a narrower and more accurate one than version 1's
whole-file comparison could make.

A version-1 manifest (no `blocks` field, an older `dankg`) falls back to
the coarser whole-file comparison version 1 of this script used -- additive
manifest fields mean an old manifest still produces a correct, just
less-precise, answer rather than an error.

A match is reported on stderr; it is never a build failure, since "does
this comment restate the prose" is a style judgment with fuzzy edges,
unlike the missing `mod.rs` `glue = rust.py` exists to prevent.
"""

import json
import re
import sys
from pathlib import Path

MANIFEST_NAME = ".dankg-tangle-manifest.json"
MIN_CLAUSE_LEN = 30  # shorter fragments are too generic to flag reliably

# Splits a comment into clauses, not whole sentences: a paraphrase that
# keeps one clause word-for-word (found on the hash.rs pilot itself --
# "a collision costs a stale parse, which the next edit corrects" survived
# a full rewrite of the sentence around it) still needs to be caught, and
# whole-comment-vs-whole-paragraph containment misses it the moment either
# side adds one clause of its own framing. `--` splits alongside sentence
# punctuation because that is where this codebase's own prose habitually
# breaks a clause (see architecture.org throughout).
_CLAUSE_SPLIT = re.compile(r"[.!?]|--")


def normalize(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip().lower()


def clauses(text: str) -> list[str]:
    return [c.strip() for c in _CLAUSE_SPLIT.split(text) if len(c.strip()) >= MIN_CLAUSE_LEN]


def doc_comment_groups(rust_src: str) -> list[str]:
    """Consecutive `//!`/`///` lines, one joined string per run -- a doc
    comment's own paragraph breaks inside the source prose do not survive
    tangling (they become blank lines mid-block, not a fresh block), so
    treating a whole run as one unit is what actually matches how a reader
    -- and `cargo doc` -- would read it. Works equally on a whole tangled
    file (version 1's fallback) or one block's own content (version 2).
    """
    groups = []
    current: list[str] = []
    for line in rust_src.splitlines():
        stripped = line.strip()
        if stripped.startswith("//!") or stripped.startswith("///"):
            current.append(stripped[3:].strip())
        else:
            if current:
                groups.append(" ".join(current))
                current = []
    if current:
        groups.append(" ".join(current))
    return groups


def markdown_paragraphs(md_src: str) -> list[str]:
    """Crude paragraph split: consecutive non-blank lines outside fences,
    skipping headings and fence content. Deliberately not shared with
    dankg's own `md/` parser -- glue has no access to it (it is a plain
    external program, decision 27), so this is its own, independent, and
    necessarily cruder reading of the same CommonMark-subset rules."""
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


def likely_duplicate(comment: str, paragraphs: list[str]) -> tuple[str, str] | None:
    """The first (comment clause, paragraph) pair where the clause turns up
    verbatim in a paragraph's full text -- not clause-against-clause on
    both sides, since the same words can sit inside a differently-shaped
    sentence on the paragraph's side without ceasing to be the same
    borrowed clause."""
    norm_paragraphs = [(p, normalize(p)) for p in paragraphs]
    for clause in clauses(comment):
        norm_clause = normalize(clause)
        for original, norm_p in norm_paragraphs:
            if norm_clause in norm_p:
                return clause, original
    return None


def paragraphs_in_window(lines: list[str], start_line: int, end_line_exclusive: int) -> list[str]:
    """`markdown_paragraphs`, restricted to the 1-indexed line range
    `(start_line, end_line_exclusive)` -- the prose between two blocks."""
    window = lines[start_line : end_line_exclusive - 1]
    return markdown_paragraphs("\n".join(window))


def block_content(lines: list[str], line: int, end_line: int) -> str:
    """A block's own fenced content, excluding the fence delimiters --
    `line`/`end_line` are the *fence* lines themselves (manifest version 2,
    straight off the `BlockRef` fields `result.rs` already uses)."""
    return "\n".join(lines[line : end_line - 1])


def check_file_v2(source_lines: list[str], blocks: list[dict]) -> list[tuple[str, str, str]]:
    """(block name, matched clause, source paragraph) for every block whose
    own doc comment borrowed a clause from the prose in the window right
    before it -- not the whole file, just what sits between it and the
    previous block."""
    findings = []
    window_start = 0
    for b in sorted(blocks, key=lambda b: b["line"]):
        paragraphs = paragraphs_in_window(source_lines, window_start, b["line"])
        content = block_content(source_lines, b["line"], b["end_line"])
        for comment in doc_comment_groups(content):
            match = likely_duplicate(comment, paragraphs)
            if match:
                clause, paragraph = match
                findings.append((b["name"], clause, paragraph))
        window_start = b["end_line"]
    return findings


def check_file_v1(rust_path: Path, source_md: Path) -> list[tuple[str, str, str]]:
    """Whole-file comparison, for a manifest with no `blocks` field (an
    older `dankg`). Coarser than `check_file_v2`, but still a correct
    answer on a single-section file -- just not one that scales to a
    many-section corpus without false positives."""
    rust_src = rust_path.read_text()
    md_src = source_md.read_text()
    paragraphs = markdown_paragraphs(md_src)
    return [
        ("(file)", *match)
        for comment in doc_comment_groups(rust_src)
        if (match := likely_duplicate(comment, paragraphs)) is not None
    ]


def main() -> None:
    if len(sys.argv) not in (2, 3):
        sys.exit(f"usage: {sys.argv[0]} <tangled-directory> [source-root]")
    out_dir = Path(sys.argv[1])
    source_root = Path(sys.argv[2]) if len(sys.argv) == 3 else Path(".")
    manifest_path = out_dir / MANIFEST_NAME
    if not manifest_path.exists():
        return  # no file -> source mapping without it; nothing checkable

    manifest = json.loads(manifest_path.read_text())
    any_found = False
    for entry in manifest.get("files", []):
        source_path = source_root / entry["source"]
        if not source_path.exists():
            continue

        blocks = entry.get("blocks")
        if blocks:
            findings = check_file_v2(source_path.read_text().splitlines(), blocks)
        else:
            rust_path = out_dir / entry["path"]
            if not rust_path.exists():
                continue
            findings = check_file_v1(rust_path, source_path)

        for name, clause, paragraph in findings:
            any_found = True
            print(f"doclint: {entry['path']} ({name}): doc comment borrows a clause from {entry['source']}:", file=sys.stderr)
            print(f"  shared clause:  {clause[:100]}", file=sys.stderr)
            print(f"  from paragraph: {paragraph[:100]}", file=sys.stderr)

    if any_found:
        print("doclint: duplication is a style warning, not a build failure -- exiting 0", file=sys.stderr)


if __name__ == "__main__":
    main()
