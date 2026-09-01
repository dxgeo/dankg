#!/usr/bin/env python3
"""Tests for the reference glue scripts (rust.py, rust-doclint.py).

Both scripts are opt-in, external, and outside dankg's own dependency
policy (decision 1 is a Rust-crate rule) -- but there is no reason their
own correctness should rest only on the ad-hoc runs that first proved
them out. Standard library only, matching the instinct even where nothing
requires it.

Run: python3 glue/test_glue.py
"""

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

GLUE_DIR = Path(__file__).parent


def _load(name: str, filename: str):
    spec = importlib.util.spec_from_file_location(name, GLUE_DIR / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


rust = _load("rust_glue", "rust.py")
doclint = _load("rust_doclint", "rust-doclint.py")


class ModGeneration(unittest.TestCase):
    """`rust.py`'s `write_mod_files`. `root.rglob("*")` never yields `root`
    itself, which is *how* "never touches the crate root" actually holds --
    every case here that expects a generated `mod.rs` uses a subdirectory,
    not the passed-in root directly, to stay honest about that."""

    def test_writes_mod_rs_listing_files_and_subdirs(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "layout").mkdir()
            (root / "layout" / "acyclic.rs").write_text("")
            (root / "layout" / "rank.rs").write_text("")
            (root / "layout" / "sub").mkdir()
            (root / "layout" / "sub" / "x.rs").write_text("")
            rust.write_mod_files(root, {})
            content = (root / "layout" / "mod.rs").read_text()
            self.assertEqual(content, "mod acyclic;\nmod rank;\npub mod sub;\n")

    def test_never_overwrites_an_existing_mod_rs(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "sub").mkdir()
            (root / "sub" / "a.rs").write_text("")
            (root / "sub" / "mod.rs").write_text("// hand-written\n")
            rust.write_mod_files(root, {})
            self.assertEqual((root / "sub" / "mod.rs").read_text(), "// hand-written\n")

    def test_the_output_root_itself_never_gets_a_generated_mod_rs(self):
        """Pins the documented "never touches the crate root" behaviour --
        it holds because `rglob` never yields the root, not because of the
        already-exists check, and that is worth a test of its own."""
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "lib.rs").write_text("")
            rust.write_mod_files(root, {})
            self.assertFalse((root / "mod.rs").exists())

    def test_public_flag_makes_a_file_pub_mod(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "sub").mkdir()
            (root / "sub" / "a.rs").write_text("")
            rust.write_mod_files(root, {"sub/a.rs": True})
            self.assertIn("pub mod a;", (root / "sub" / "mod.rs").read_text())

    def test_no_public_entry_defaults_to_private(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "sub").mkdir()
            (root / "sub" / "a.rs").write_text("")
            rust.write_mod_files(root, {})
            self.assertIn("mod a;", (root / "sub" / "mod.rs").read_text())
            self.assertNotIn("pub mod a;", (root / "sub" / "mod.rs").read_text())

    def test_a_subdirectory_declaration_is_always_pub(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "sub").mkdir()
            (root / "sub" / "a.rs").write_text("")
            (root / "sub" / "inner").mkdir()
            (root / "sub" / "inner" / "b.rs").write_text("")
            rust.write_mod_files(root, {})
            self.assertIn("pub mod inner;", (root / "sub" / "mod.rs").read_text())

    def test_an_empty_directory_gets_no_mod_rs(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "empty").mkdir()
            rust.write_mod_files(root, {})
            self.assertFalse((root / "empty" / "mod.rs").exists())

    def test_loads_public_flags_from_the_real_manifest_shape(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            manifest = {
                "version": 2,
                "files": [{"path": "a.rs", "source": "x.md", "public": True, "blocks": []}],
            }
            (root / rust.MANIFEST_NAME).write_text(json.dumps(manifest))
            self.assertEqual(rust.load_public_flags(root), {"a.rs": True})

    def test_no_manifest_means_no_public_flags(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(rust.load_public_flags(Path(d)), {})


class DoclintClauses(unittest.TestCase):
    def test_short_fragments_are_never_flagged(self):
        self.assertEqual(doclint.clauses("too short"), [])

    def test_splits_on_sentence_and_dash_boundaries(self):
        text = "One long enough clause sits right here -- and another long enough one too."
        self.assertEqual(len(doclint.clauses(text)), 2)


class DoclintWindowing(unittest.TestCase):
    """The point of manifest v2: a shared clause is flagged only for the
    block whose own window (the prose between it and the previous block)
    actually contains it -- proven with two blocks sharing the exact same
    borrowed clause, only one of which sits next to the paragraph it came
    from. This is the fixture from the layout-pilot session, now pinned."""

    def _fixture(self):
        source = (
            "# Test\n\n"
            "Paragraph A explains widgets in detail: a very particular phrase that must\n"
            "not leak forward into another block's own comment window belongs here.\n\n"
            "```rust name=one\n"
            "//! a very particular phrase that must not leak forward into another\n"
            "//! block's own comment window belongs here\n"
            "fn one() {}\n"
            "```\n\n"
            "Paragraph B is totally different text about gadgets, nothing shared here.\n\n"
            "```rust name=two\n"
            "//! a very particular phrase that must not leak forward into another\n"
            "//! block's own comment window belongs here\n"
            "fn two() {}\n"
            "```\n"
        )
        return source.splitlines()

    def _blocks(self):
        return [{"name": "one", "line": 6, "end_line": 10}, {"name": "two", "line": 14, "end_line": 18}]

    def test_the_block_next_to_the_source_paragraph_is_flagged(self):
        findings = doclint.check_file_v2(self._fixture(), self._blocks())
        self.assertIn("one", {name for name, _, _ in findings})

    def test_the_distant_block_sharing_the_same_clause_is_not_flagged(self):
        findings = doclint.check_file_v2(self._fixture(), self._blocks())
        self.assertNotIn("two", {name for name, _, _ in findings})

    def test_a_clean_block_produces_no_findings_at_all(self):
        source = (
            "# Test\n\nSome prose explaining nothing in particular here at all.\n\n"
            "```rust name=one\n//! a short unrelated pointer\nfn one() {}\n```\n"
        )
        lines = source.splitlines()
        findings = doclint.check_file_v2(lines, [{"name": "one", "line": 4, "end_line": 8}])
        self.assertEqual(findings, [])


class DoclintV1Fallback(unittest.TestCase):
    """A manifest with no `blocks` field (an older `dankg`) falls back to
    whole-file comparison rather than erroring."""

    def test_whole_file_comparison_still_catches_a_duplicate(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "source.md").write_text("# Hash\n\nFNV-1a: eight lines, no tables, no dependency.\n")
            (root / "hash.rs").write_text("//! FNV-1a: eight lines, no tables, no dependency.\n")
            findings = doclint.check_file_v1(root / "hash.rs", root / "source.md")
            self.assertEqual(len(findings), 1)

    def test_no_overlap_means_no_findings(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "source.md").write_text("# Hash\n\nSomething about widgets entirely.\n")
            (root / "hash.rs").write_text("//! A totally unrelated one-line pointer comment.\n")
            findings = doclint.check_file_v1(root / "hash.rs", root / "source.md")
            self.assertEqual(findings, [])


class DoclintMain(unittest.TestCase):
    def test_no_manifest_means_silent_no_op(self):
        with tempfile.TemporaryDirectory() as d:
            sys.argv = ["doclint", d]
            doclint.main()  # must not raise, must not print anything requiring a manifest

    def test_source_root_resolves_a_corpus_relative_source_path(self):
        """Regression pin for the bug the layout-pilot chaining test found:
        a corpus-wide manifest's `source` is root-relative, not resolvable
        from wherever this script happens to run, unless told where that
        root is. Without the optional third argument this silently found
        nothing to check on every entry -- see the script's own docstring."""
        import contextlib
        import io

        with tempfile.TemporaryDirectory() as base:
            base = Path(base)
            root = base / "corpus"
            root.mkdir()
            (root / "a.md").write_text(
                "# Hash\n\n"
                "FNV-1a: eight lines, no tables, no dependency.\n\n"
                "```rust name=x\n"
                "//! FNV-1a: eight lines, no tables, no dependency.\n"
                "```\n"
            )

            out = base / "out"
            out.mkdir()
            manifest = {
                "version": 2,
                "files": [{
                    "path": "a.rs",
                    "source": "a.md",  # root-relative, not resolvable from `out`
                    "public": False,
                    "blocks": [{"name": "x", "line": 5, "end_line": 7}],
                }],
            }
            (out / doclint.MANIFEST_NAME).write_text(json.dumps(manifest))

            sys.argv = ["doclint", str(out), str(root)]
            captured = io.StringIO()
            with contextlib.redirect_stderr(captured):
                doclint.main()
            self.assertIn("borrows a clause", captured.getvalue())


if __name__ == "__main__":
    unittest.main()
