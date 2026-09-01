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
docinject = _load("rust_docinject", "rust-docinject.py")


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


class DocinjectBackoff(unittest.TestCase):
    def test_a_doc_comment_anywhere_in_the_block_counts(self):
        self.assertTrue(docinject.has_doc_comment("fn a() {}\n/// trailing, not leading\n"))

    def test_no_doc_comment_at_all(self):
        self.assertFalse(docinject.has_doc_comment("fn a() {}\n"))

    def test_an_inner_doc_comment_also_counts(self):
        self.assertTrue(docinject.has_doc_comment("//! module doc\nfn a() {}\n"))


class DocinjectFormatting(unittest.TestCase):
    def test_one_paragraph_is_one_line(self):
        self.assertEqual(docinject.doc_lines(["hello"]), ["/// hello"])

    def test_two_paragraphs_get_a_blank_slash_line_between_them(self):
        self.assertEqual(
            docinject.doc_lines(["one", "two"]),
            ["/// one", "///", "/// two"],
        )


class DocinjectNameTable(unittest.TestCase):
    def _manifest(self):
        return {
            "files": [
                {
                    "path": "producer/greeting.rs",
                    "source": "producer.md",
                    "blocks": [{"name": "make_greeting", "line": 5, "end_line": 8}],
                },
                {
                    "path": "consumer/caller.rs",
                    "source": "consumer.md",
                    "blocks": [{"name": "call_greeting", "line": 5, "end_line": 8}],
                },
            ]
        }

    def test_builds_a_crate_path_per_block(self):
        table = docinject.build_name_table(self._manifest())
        self.assertEqual(
            table["producer.md"]["make_greeting"],
            "crate::producer::greeting::make_greeting",
        )

    def test_a_cross_file_link_resolves_through_the_table(self):
        table = docinject.build_name_table(self._manifest())
        rewritten = docinject.rewrite_links(
            "See [it](producer.md#make_greeting) for details.", "consumer.md", table
        )
        self.assertEqual(rewritten, "See [it](crate::producer::greeting::make_greeting) for details.")

    def test_a_same_file_link_resolves_against_its_own_source(self):
        table = docinject.build_name_table(self._manifest())
        rewritten = docinject.rewrite_links("See [it](#call_greeting).", "consumer.md", table)
        self.assertEqual(rewritten, "See [it](crate::consumer::caller::call_greeting).")

    def test_an_unresolved_target_is_left_exactly_as_written(self):
        table = docinject.build_name_table(self._manifest())
        original = "See [it](nowhere.md#nothing)."
        self.assertEqual(docinject.rewrite_links(original, "consumer.md", table), original)


class DocinjectWindowing(unittest.TestCase):
    """The regression this pilot's own reasoning caught before ever running
    it: a source `.md` that contributes more than one output file (`path=`
    landing one block outside the heading-derived tree, exactly `hash.md`'s
    own shape) must still window a later block's prose starting from its
    true document-order predecessor, not from the start of whichever
    *output file* it happens to land in -- otherwise an earlier, unrelated
    file's own prose bleeds into a block that never asked for it."""

    def _fixture(self):
        source = (
            "# Root\n\n"
            "Cargo prose here, about the scaffold.\n\n"
            "```rust name=scaffold path=Cargo.toml\n"
            "[package]\n"
            "```\n\n"
            "Real explanation of the actual function.\n\n"
            "```rust name=real\n"
            "fn real() {}\n"
            "```\n"
        )
        return source.splitlines()

    def _manifest(self):
        return {
            "files": [
                {
                    "path": "scaffold.rs",
                    "source": "root.md",
                    "blocks": [{"name": "scaffold", "line": 5, "end_line": 7}],
                },
                {
                    "path": "lib.rs",
                    "source": "root.md",
                    "blocks": [{"name": "real", "line": 11, "end_line": 13}],
                },
            ]
        }

    def test_the_later_files_own_block_does_not_inherit_the_earlier_files_prose(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "root.md").write_text("\n".join(self._fixture()) + "\n")
            manifest = self._manifest()
            injections = docinject.plan_injections(manifest, root, docinject.build_name_table(manifest))
            _, comment = injections["lib.rs"][0]
            self.assertIn("Real explanation of the actual function.", comment)
            self.assertNotIn("Cargo prose", comment)

    def test_the_earlier_files_own_block_still_gets_its_own_window(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "root.md").write_text("\n".join(self._fixture()) + "\n")
            manifest = self._manifest()
            injections = docinject.plan_injections(manifest, root, docinject.build_name_table(manifest))
            _, comment = injections["scaffold.rs"][0]
            self.assertIn("Cargo prose here, about the scaffold.", comment)


class DocinjectPlanningBackoff(unittest.TestCase):
    def test_a_block_with_its_own_doc_comment_is_never_planned(self):
        source = (
            "# Test\n\nSome prose that would otherwise be injected.\n\n"
            "```rust name=one\n//! already documented by hand\nfn one() {}\n```\n"
        )
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "x.md").write_text(source)
            manifest = {
                "files": [
                    {"path": "x.rs", "source": "x.md", "blocks": [{"name": "one", "line": 5, "end_line": 8}]}
                ]
            }
            injections = docinject.plan_injections(manifest, root, {})
            self.assertNotIn("x.rs", injections)

    def test_a_block_with_no_preceding_prose_is_never_planned(self):
        source = "# Test\n\n```rust name=one\nfn one() {}\n```\n"
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "x.md").write_text(source)
            manifest = {
                "files": [
                    {"path": "x.rs", "source": "x.md", "blocks": [{"name": "one", "line": 3, "end_line": 5}]}
                ]
            }
            injections = docinject.plan_injections(manifest, root, {})
            self.assertNotIn("x.rs", injections)


class DocinjectFrontmatter(unittest.TestCase):
    """Regression pin for the second bug the `docinject-pilot` run found:
    a leading frontmatter block is not a paragraph and must never be
    swept into the first block's own injected doc comment."""

    def test_a_leading_frontmatter_block_is_blanked_not_read_as_prose(self):
        lines = ["---", "dankg.tangle.public: true", "---", "", "# Caller", "", "Real prose."]
        stripped = docinject.strip_frontmatter(lines)
        self.assertEqual(len(stripped), len(lines))  # line numbers must still index correctly
        self.assertNotIn("dankg.tangle.public: true", stripped)
        self.assertEqual(stripped[4], "# Caller")

    def test_a_dots_closing_delimiter_is_also_recognised(self):
        lines = ["---", "k: v", "...", "prose"]
        stripped = docinject.strip_frontmatter(lines)
        self.assertEqual(stripped, ["", "", "", "prose"])

    def test_no_opening_delimiter_means_nothing_is_touched(self):
        lines = ["# Heading", "", "Some prose."]
        self.assertEqual(docinject.strip_frontmatter(lines), lines)

    def test_an_unclosed_leading_dashes_line_is_left_alone(self):
        lines = ["---", "# Heading", "", "prose, no closing delimiter anywhere"]
        self.assertEqual(docinject.strip_frontmatter(lines), lines)

    def test_frontmatter_is_actually_excluded_from_the_first_blocks_window(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "x.md").write_text(
                "---\ndankg.tangle.public: true\n---\n\n# Caller\n\nReal prose only.\n\n"
                "```rust name=one\nfn one() {}\n```\n"
            )
            manifest = {
                "files": [{"path": "x.rs", "source": "x.md", "blocks": [{"name": "one", "line": 9, "end_line": 11}]}]
            }
            injections = docinject.plan_injections(manifest, root, {})
            _, comment = injections["x.rs"][0]
            self.assertIn("Real prose only.", comment)
            self.assertNotIn("dankg.tangle.public", comment)


class DocinjectSkipsNonRustOutput(unittest.TestCase):
    """Regression pin for the bug the `hash.md` pilot's own real run found:
    a `path=Cargo.toml` block is selected by *fence* language, matching
    `--lang rust`, but its own output is TOML, not Rust -- `///` there is
    not a comment, it is a parse error. The fix has two halves, both
    covered here: the non-Rust output must never be written to, and its
    own prose must still count as spent so a *later* Rust block's window
    does not silently widen to include it."""

    def _fixture(self):
        source = (
            "# Root\n\n"
            "Cargo prose that must never become a TOML comment.\n\n"
            "```rust name=scaffold path=Cargo.toml\n"
            "[package]\n"
            "```\n\n"
            "Real explanation of the actual function.\n\n"
            "```rust name=real\n"
            "fn real() {}\n"
            "```\n"
        )
        return source.splitlines()

    def _manifest(self):
        return {
            "files": [
                {
                    "path": "Cargo.toml",
                    "source": "root.md",
                    "blocks": [{"name": "scaffold", "line": 5, "end_line": 7}],
                },
                {
                    "path": "lib.rs",
                    "source": "root.md",
                    "blocks": [{"name": "real", "line": 11, "end_line": 13}],
                },
            ]
        }

    def test_the_toml_output_is_never_planned_for_injection(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "root.md").write_text("\n".join(self._fixture()) + "\n")
            manifest = self._manifest()
            injections = docinject.plan_injections(manifest, root, docinject.build_name_table(manifest))
            self.assertNotIn("Cargo.toml", injections)

    def test_the_tomls_own_prose_does_not_bleed_into_the_next_rust_blocks_window(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "root.md").write_text("\n".join(self._fixture()) + "\n")
            manifest = self._manifest()
            injections = docinject.plan_injections(manifest, root, docinject.build_name_table(manifest))
            _, comment = injections["lib.rs"][0]
            self.assertIn("Real explanation of the actual function.", comment)
            self.assertNotIn("Cargo prose", comment)


class DocinjectLeadingSkip(unittest.TestCase):
    """Regression pin for the third bug the `docinject-pilot` run found:
    a block that opens with a `use` (the composition-pilot's own escape
    hatch for a real cross-tangle-file reference) must have its injected
    comment attach to the item *after* the `use`, not the `use` itself."""

    def test_a_leading_use_is_skipped(self):
        content = "use crate::producer::greeting::make_greeting;\n\npub fn shout() {}\n"
        offset = docinject.leading_skip_offset(content)
        self.assertEqual(content[offset:], "pub fn shout() {}\n")

    def test_a_leading_attribute_is_also_skipped(self):
        content = "#[derive(Debug)]\npub struct Thing;\n"
        offset = docinject.leading_skip_offset(content)
        self.assertEqual(content[offset:], "pub struct Thing;\n")

    def test_no_leading_use_or_attribute_means_no_skip_at_all(self):
        content = "pub fn hello() {}\n"
        self.assertEqual(docinject.leading_skip_offset(content), 0)

    def test_the_injected_comment_lands_on_the_function_not_the_use(self):
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            (out / "x.rs").write_text(
                "// generated -- do not edit\n\n"
                "use crate::producer::greeting::make_greeting;\n\n"
                "pub fn shout() {}\n"
            )
            content = "use crate::producer::greeting::make_greeting;\n\npub fn shout() {}\n"
            ok = docinject.apply_injections(out, "x.rs", [(content, "/// docs for shout\n")])
            self.assertTrue(ok)
            text = (out / "x.rs").read_text()
            self.assertIn("/// docs for shout\npub fn shout() {}\n", text)
            self.assertNotIn("/// docs for shout\nuse", text)


class DocinjectApply(unittest.TestCase):
    def test_the_comment_lands_directly_above_the_blocks_own_content(self):
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            (out / "x.rs").write_text("// generated -- do not edit\n\nfn one() {}\n")
            ok = docinject.apply_injections(out, "x.rs", [("fn one() {}", "/// one thing\n")])
            self.assertTrue(ok)
            self.assertEqual(
                (out / "x.rs").read_text(),
                "// generated -- do not edit\n\n/// one thing\nfn one() {}\n",
            )

    def test_content_that_cannot_be_found_writes_nothing(self):
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            original = "// generated -- do not edit\n\nfn one() {}\n"
            (out / "x.rs").write_text(original)
            ok = docinject.apply_injections(out, "x.rs", [("fn missing() {}", "/// nope\n")])
            self.assertFalse(ok)
            self.assertEqual((out / "x.rs").read_text(), original)


class DocinjectMain(unittest.TestCase):
    def test_no_manifest_means_silent_no_op(self):
        with tempfile.TemporaryDirectory() as d:
            sys.argv = ["docinject", d]
            docinject.main()  # must not raise

    def test_end_to_end_over_a_real_manifest_and_source(self):
        import contextlib
        import io

        with tempfile.TemporaryDirectory() as base:
            base = Path(base)
            root = base / "corpus"
            root.mkdir()
            (root / "a.md").write_text(
                "# Greeting\n\nA short hello, nothing fancy.\n\n"
                "```rust name=hello\npub fn hello() -> &'static str { \"hi\" }\n```\n"
            )

            out = base / "out"
            out.mkdir()
            (out / "a.rs").write_text('pub fn hello() -> &\'static str { "hi" }\n')
            manifest = {
                "version": 2,
                "files": [
                    {
                        "path": "a.rs",
                        "source": "a.md",
                        "public": False,
                        "blocks": [{"name": "hello", "line": 5, "end_line": 7}],
                    }
                ],
            }
            (out / docinject.MANIFEST_NAME).write_text(json.dumps(manifest))

            sys.argv = ["docinject", str(out), str(root)]
            captured = io.StringIO()
            with contextlib.redirect_stderr(captured):
                docinject.main()
            self.assertIn("added doc comments to a.rs", captured.getvalue())
            self.assertIn("/// A short hello, nothing fancy.", (out / "a.rs").read_text())


if __name__ == "__main__":
    unittest.main()
