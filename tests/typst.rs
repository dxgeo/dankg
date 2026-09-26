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

/// A captioned table artifact and a captioned image artifact in one
/// document, each wrapped in a `#figure` carrying its own declared kind.
/// A string assertion cannot judge `kind: image`. A wrong kind name is
/// still a string the emitter is internally consistent about. Typst is
/// the half that refuses to compile it. The PDF is read back with
/// `pdftotext` because the declared kind is only worth declaring if the
/// two counters stay separate. `Table 1` beside `Figure 1` is the only
/// place that shows.
const FIGURE_KINDS_SOURCE: &str = r#"# Figure kinds smoke test

```python name=t produces=file:data.csv caption="A table"
write_csv()
```

<!-- dankg:result name=t hash=0000000000000001 -->

```
wrote data.csv
```

```python name=c produces=file:chart.png caption="A chart"
savefig()
```

<!-- dankg:result name=c hash=0000000000000002 -->

```
wrote chart.png
```
"#;

/// A 4x4 red PNG, built byte by byte rather than checked in. Typst reads
/// the real file `#image(...)` points at. The bytes therefore have to be
/// a PNG a decoder accepts. Zero dependencies (decision 1) rules out a
/// crate for this. `render_pdf` is the half that normally copies an
/// artifact into `assets/`, which this test stands in for.
fn red_png() -> Vec<u8> {
    fn chunk(tag: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(tag);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc32(&[tag, data].concat()).to_be_bytes());
        out
    }
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &b in bytes {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
    // One uncompressed deflate block, since `std` has no deflate encoder:
    // a zlib header, a stored block, and an Adler-32 of the raw scanlines.
    fn zlib_stored(raw: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x01];
        out.push(0x01);
        out.extend_from_slice(&(raw.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(raw.len() as u16)).to_le_bytes());
        out.extend_from_slice(raw);
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in raw {
            a = (a + byte as u32) % 65521;
            b = (b + a) % 65521;
        }
        out.extend_from_slice(&((b << 16) | a).to_be_bytes());
        out
    }
    let (w, h) = (4u32, 4u32);
    let mut raw = Vec::new();
    for _ in 0..h {
        raw.push(0); // filter: none
        for _ in 0..w {
            raw.extend_from_slice(&[0xff, 0x00, 0x00]);
        }
    }
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit truecolor
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&chunk(b"IDAT", &zlib_stored(&raw)));
    png.extend_from_slice(&chunk(b"IEND", &[]));
    png
}

#[test]
fn a_table_figure_and_an_image_figure_compile_and_number_on_separate_counters() {
    let mut parse_diags = Diags::new("t.md");
    let doc = Document::parse(FIGURE_KINDS_SOURCE, &mut parse_diags);
    assert!(parse_diags.is_empty(), "fixture should parse cleanly: {:?}", parse_diags.items());

    let mut tables = HashMap::new();
    tables.insert(1, ("csv".to_string(), "name,amount\nwidgets,3\n".to_string()));
    let mut images = HashMap::new();
    images.insert(4, (Vec::new(), "chart.png".to_string()));

    let mut diags = Diags::new("t.md");
    let typ = typst::render(&doc, "Figure Kinds Smoke Test", false, &tables, &images, false, None, &mut diags);
    assert!(diags.is_empty(), "fixture should render with no warnings: {:?}", diags.items());
    assert!(typ.contains("#figure(kind: table, caption: [A table])["), "{typ}");
    assert!(typ.contains("#figure(kind: image, caption: [A chart])["), "{typ}");

    let dir = std::env::temp_dir().join(format!("dankg-typst-figkinds-smoke-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("assets")).expect("scratch dir");
    fs::write(dir.join("assets/chart.png"), red_png()).expect("write chart.png");
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

    // `pdftotext` is optional the same way `typst` is, and skipped the same
    // way when it is absent.
    let text = match Command::new("pdftotext").arg(&pdf_path).arg("-").output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => {
            eprintln!("skipping the numbering assertion: `pdftotext` not runnable");
            let _ = fs::remove_dir_all(&dir);
            return;
        }
    };
    assert!(text.contains("Table 1: A table"), "the table takes Typst's own table counter: {text}");
    assert!(text.contains("Figure 1: A chart"), "the image takes Typst's own figure counter: {text}");

    let _ = fs::remove_dir_all(&dir);
}
