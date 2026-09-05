#!/usr/bin/env python3
"""Reference `dankg tangle` glue script for Rust (architecture.md, Tangle,
decision 27).

This is a plain, standalone program -- not part of dankg's own source, and
not something dankg ever imports or links. It is an example of what a
`[tangle.<lang>] glue` command is: something a reader configures and dankg
spawns, never something dankg writes or understands itself. Fork it,
replace it, or write an equivalent for another language entirely; dankg
does not care what `glue` points at, only that it is a command.

Usage, in `.dankg/config`:

    [tangle.rust]
    glue    = python3 /path/to/glue/rust.py {dir}
    command = cargo build --manifest-path {dir}/Cargo.toml
    ext     = rs

What it does: Rust will not compile a directory tree unless every file and
subdirectory is declared with `mod name;` somewhere. That declaration is
purely structural -- it names siblings, it never needs to read what a
block's code means -- so this script walks the directory dankg just wrote,
and for every directory containing `.rs` files or subdirectories that need
declaring, writes a `mod.rs` listing them.

Two things it deliberately does not do:

- It never touches the crate root (`lib.rs`/`main.rs`). That file usually
  carries real content -- `fn main()`, crate attributes -- not just a
  module list, so it stays the author's own `path=`-placed block, the same
  as `Cargo.toml` already is. This script only manages the structural
  layer underneath whatever the root declares.
- It never overwrites a `mod.rs` that already exists. A directory that
  already has one -- because the author placed it there with `path=` --
  is left alone entirely; the backoff is "does a file already exist at
  this exact spot", the same rule `path=` already gets everywhere else in
  tangle.

Visibility: dankg's sidecar manifest (`.dankg-tangle-manifest.json`, only
written when `glue` is configured) carries a `public` flag per tangled
file, from that file's own source `.md`'s `dankg.tangle.public`
frontmatter. A declared *file* gets `pub mod` when its manifest entry says
so, `mod` otherwise (including when there is no manifest at all, or no
entry for it -- private is always the safe default). A declared
*subdirectory* is always `pub mod`: visibility is a leaf-level, per-file
authorial choice, and a directory is just the path to reach one, not
something narrower than its most permissive child would need anyway.
"""

import json
import sys
from pathlib import Path

MANIFEST_NAME = ".dankg-tangle-manifest.json"


def load_public_flags(root: Path) -> dict[str, bool]:
    manifest = root / MANIFEST_NAME
    if not manifest.exists():
        return {}
    data = json.loads(manifest.read_text())
    return {entry["path"]: entry.get("public", False) for entry in data.get("files", [])}


def write_mod_files(root: Path, public: dict[str, bool]) -> None:
    for directory in sorted(root.rglob("*"), key=lambda p: -len(p.parts)):
        if not directory.is_dir():
            continue
        mod_rs = directory / "mod.rs"
        if mod_rs.exists():
            continue  # an author already placed one here with `path=`; leave it alone

        files = sorted(p.stem for p in directory.glob("*.rs"))
        subdirs = sorted(p.name for p in directory.iterdir() if p.is_dir())
        if not files and not subdirs:
            continue

        lines = []
        for name in files:
            rel = str((directory / f"{name}.rs").relative_to(root))
            visibility = "pub mod" if public.get(rel) else "mod"
            lines.append(f"{visibility} {name};")
        for name in subdirs:
            lines.append(f"pub mod {name};")

        mod_rs.write_text("\n".join(lines) + "\n")


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(f"usage: {sys.argv[0]} <tangled-directory>")
    root = Path(sys.argv[1])
    if not root.is_dir():
        sys.exit(f"{root}: not a directory")
    write_mod_files(root, load_public_flags(root))


if __name__ == "__main__":
    main()
