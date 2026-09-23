//! `render::typst`'s output against a real `typst compile`, not just string
//! assertions on the emitted markup -- a syntax mistake in the emitter (an
//! unbalanced bracket, a bad escape) would still produce *some* string that
//! satisfies a `.contains(...)` check, but Typst itself is the only real
//! judge of whether the result is valid markup.
//!
//! Unlike `tests/db.rs`'s own real-`duckdb` precedent, this test *skips*
//! rather than fails when `typst` is not on `PATH`, and CI does not install
//! one: `duckdb` is core to what `dankg eval` does and every dev/CI
//! environment is expected to have it, but `typst` is an opt-in external
//! command a corpus owner configures (`[weave.pdf] command`, plan-
//! weave.md's own decision 44) -- weave's whole design already treats it as
//! optional, writing the `.typ` and saying so when it is absent. This test
//! follows that same shape: strong verification wherever `typst` happens to
//! be installed (this repo's own dev machine included, confirmed 0.15.1),
//! no CI dependency on it.

use dankg::diag::Diags;
use dankg::md::Document;
use dankg::render::typst;
use std::collections::HashMap;
use std::fs;
use std::process::Command;

/// A document exercising every construct `render::typst` handles: headings,
/// emphasis/strong/code spans, a link, a list, a thematic break, a GFM
/// table with alignment and a ragged row, a `csv`-tagged block, an
/// ordinary code block, and frontmatter's own author byline and real
/// Typst date on the cover page.
const SOURCE: &str = r#"---
title: Weave Smoke Test
author: Jane Doe
date: 2026-09-18
---
# Weave smoke test

Some *emphasis*, **strong**, and `a code span`, plus a [link](https://example.com).

- one
- two

---

| Name | Amount |
|:--|--:|
| widgets | 3 |
| gadgets |

```csv
name,amount
widgets,3
gadgets,7
```

```rust
fn f() {}
```
"#;

fn typst_compile(typ_path: &std::path::Path, pdf_path: &std::path::Path) -> std::io::Result<std::process::Output> {
    Command::new("typst").arg("compile").arg(typ_path).arg(pdf_path).output()
}

#[test]
fn rendered_typst_actually_compiles() {
    let mut parse_diags = Diags::new("t.md");
    let doc = Document::parse(SOURCE, &mut parse_diags);
    assert!(parse_diags.is_empty(), "fixture should parse cleanly: {:?}", parse_diags.items());

    let mut diags = Diags::new("t.md");
    let typ = typst::render(&doc, "Weave Smoke Test", true, &HashMap::new(), &HashMap::new(), false, None, &mut diags);
    assert!(diags.is_empty(), "fixture should render with no warnings: {:?}", diags.items());

    let dir = std::env::temp_dir().join(format!("dankg-typst-smoke-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    let typ_path = dir.join("doc.typ");
    let pdf_path = dir.join("doc.pdf");
    fs::write(&typ_path, &typ).expect("write .typ");

    let output = match typst_compile(&typ_path, &pdf_path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("skipping: `typst` not runnable ({e})");
            let _ = fs::remove_dir_all(&dir);
            return;
        }
    };

    assert!(
        output.status.success(),
        "typst compile failed:\n-- .typ --\n{typ}\n-- stderr --\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(pdf_path.exists(), "typst reported success but wrote no PDF");
    assert!(fs::metadata(&pdf_path).unwrap().len() > 0, "PDF is empty");

    let _ = fs::remove_dir_all(&dir);
}

/// A bracketed single citation, a bracketed multi-key one, and a narrative
/// citation, against a real Hayagriva file on disk -- decision 59
/// (`plan-weave-citations.md`). Confirms `#cite(<key>, form: "prose")` and
/// `#bibliography(...)` are real, compilable Typst syntax, not just strings
/// this crate's own emitter happens to be internally consistent about.
const BIB_SOURCE: &str = r#"# Citations smoke test

A bracketed citation [@netwok2019] and a multi-key one [@netwok2019; @smith2020].

@smith2020 argues this works.
"#;

const BIB_YAML: &str = r#"netwok2019:
  type: article
  title: A Paper
  author: Smith, John
  date: 2019
smith2020:
  type: article
  title: Another Paper
  author: Smith, Jane
  date: 2020
"#;

#[test]
fn rendered_citations_and_bibliography_actually_compile() {
    let mut parse_diags = Diags::new("t.md");
    let doc = Document::parse(BIB_SOURCE, &mut parse_diags);
    assert!(parse_diags.is_empty(), "fixture should parse cleanly: {:?}", parse_diags.items());

    let dir = std::env::temp_dir().join(format!("dankg-typst-bib-smoke-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    fs::write(dir.join("bibliography.yml"), BIB_YAML).expect("write bibliography.yml");

    let summary = typst::BibliographySummary {
        valid_keys: ["netwok2019", "smith2020"].iter().map(|s| s.to_string()).collect(),
        asset_path: "bibliography.yml".to_string(),
    };

    let mut diags = Diags::new("t.md");
    let typ = typst::render(
        &doc,
        "Citations Smoke Test",
        false,
        &HashMap::new(),
        &HashMap::new(),
        false,
        Some(&summary),
        &mut diags,
    );
    assert!(diags.is_empty(), "fixture should render with no warnings: {:?}", diags.items());
    assert!(typ.contains("#bibliography(\"bibliography.yml\")"), "{typ}");

    let typ_path = dir.join("doc.typ");
    let pdf_path = dir.join("doc.pdf");
    fs::write(&typ_path, &typ).expect("write .typ");

    let output = match typst_compile(&typ_path, &pdf_path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("skipping: `typst` not runnable ({e})");
            let _ = fs::remove_dir_all(&dir);
            return;
        }
    };

    assert!(
        output.status.success(),
        "typst compile failed:\n-- .typ --\n{typ}\n-- stderr --\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(pdf_path.exists(), "typst reported success but wrote no PDF");
    assert!(fs::metadata(&pdf_path).unwrap().len() > 0, "PDF is empty");

    let _ = fs::remove_dir_all(&dir);
}

/// A `cover: false` document (decision 61) starts with `#outline()` or a
/// heading, never the `#align(center)[...]` block every other emitted
/// document opens with. Dropping the cover page changes the shape of the
/// markup Typst is handed, not just a line inside it, so a real compile is
/// the only thing that confirms the remaining document still stands on its
/// own.
const NO_COVER_SOURCE: &str = r#"---
title: No Cover Smoke Test
author: Jane Doe
date: 2026-09-18
cover: false
---
# No cover smoke test

Body text, with *emphasis* and a [link](https://example.com).

## Later
"#;

#[test]
fn a_document_with_its_cover_page_dropped_actually_compiles() {
    let mut parse_diags = Diags::new("t.md");
    let doc = Document::parse(NO_COVER_SOURCE, &mut parse_diags);
    assert!(parse_diags.is_empty(), "fixture should parse cleanly: {:?}", parse_diags.items());

    let mut diags = Diags::new("t.md");
    let typ =
        typst::render(&doc, "No Cover Smoke Test", true, &HashMap::new(), &HashMap::new(), false, None, &mut diags);
    assert!(diags.is_empty(), "fixture should render with no warnings: {:?}", diags.items());
    assert!(typ.starts_with("#outline()"), "the cover page and its pagebreak are gone: {typ}");
    assert!(!typ.contains("Jane Doe"), "the byline goes with the page: {typ}");

    let dir = std::env::temp_dir().join(format!("dankg-typst-nocover-smoke-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    let typ_path = dir.join("doc.typ");
    let pdf_path = dir.join("doc.pdf");
    fs::write(&typ_path, &typ).expect("write .typ");

    let output = match typst_compile(&typ_path, &pdf_path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("skipping: `typst` not runnable ({e})");
            let _ = fs::remove_dir_all(&dir);
            return;
        }
    };

    assert!(
        output.status.success(),
        "typst compile failed:\n-- .typ --\n{typ}\n-- stderr --\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(pdf_path.exists(), "typst reported success but wrote no PDF");
    assert!(fs::metadata(&pdf_path).unwrap().len() > 0, "PDF is empty");

    let _ = fs::remove_dir_all(&dir);
}
