//! End-to-end parsing of the constructs DanKG relies on.
//!
//! CommonMark's suite cannot cover any of this: frontmatter, wikilinks, and
//! info-string attributes are all outside the spec.

use dankg::diag::Diags;
use dankg::md::{Block, Document, Inline};

fn parse(src: &str) -> (Document, Diags) {
    let mut d = Diags::new("notes.md");
    let doc = Document::parse(src, &mut d);
    (doc, d)
}

const SAMPLE: &str = "\
---
title: DanKG Design
tags: [rust, graphs]
alias: dankg-arch
---
# Overview

Links to [Constraints](project.md#constraints) and [[Code eval]].

## Key Features

- Lightweight
- Plaintext-driven

```python name=setup
DATA = \"root.json\"
```

```python name=index deps=setup timeout=60
print(load(DATA))
```
";

#[test]
fn parses_a_realistic_document_without_warnings() {
    let (doc, diags) = parse(SAMPLE);
    assert!(diags.is_empty(), "unexpected: {:?}", diags.items());
    assert_eq!(doc.frontmatter.title(), Some("DanKG Design"));
    assert_eq!(doc.frontmatter.tags(), vec!["rust", "graphs"]);
    assert_eq!(doc.frontmatter.aliases(), vec!["dankg-arch"]);
}

#[test]
fn headings_carry_level_and_true_source_line() {
    let (doc, _) = parse(SAMPLE);
    let headings: Vec<_> = doc
        .headings()
        .into_iter()
        .map(|(level, inlines, line)| (level, Inline::plain(inlines), line))
        .collect();

    // Line numbers must be document lines, counted through the frontmatter.
    assert_eq!(
        headings,
        vec![(1, "Overview".to_string(), 6), (2, "Key Features".to_string(), 10)]
    );
}

#[test]
fn both_link_forms_are_recognised() {
    let (doc, _) = parse(SAMPLE);
    let Block::Paragraph { inlines, .. } = &doc.blocks[1] else {
        panic!("expected a paragraph, got {:?}", doc.blocks[1]);
    };

    let dests: Vec<&str> = inlines
        .iter()
        .filter_map(|i| match i {
            Inline::Link { dest, .. } => Some(dest.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(dests, vec!["project.md#constraints"]);

    let wikis: Vec<&str> = inlines
        .iter()
        .filter_map(|i| match i {
            Inline::WikiLink { target, .. } => Some(target.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(wikis, vec!["Code eval"]);
}

#[test]
fn named_blocks_expose_their_dependency_metadata() {
    let (doc, _) = parse(SAMPLE);
    let blocks = doc.named_blocks();
    assert_eq!(blocks.len(), 2);

    let (setup, setup_src, _) = &blocks[0];
    assert_eq!(setup.name(), Some("setup"));
    assert!(setup.deps().is_empty());
    assert_eq!(*setup_src, "DATA = \"root.json\"\n");

    let (index, _, line) = &blocks[1];
    assert_eq!(index.name(), Some("index"));
    assert_eq!(index.deps(), vec!["setup"]);
    assert_eq!(index.timeout(), Some(60));
    assert_eq!(*line, 19, "the fence opens on line 19 of the sample");
}

#[test]
fn unnamed_blocks_are_excluded_from_named_blocks() {
    let (doc, _) = parse("```sh\necho hi\n```\n");
    assert!(doc.named_blocks().is_empty());
    assert_eq!(doc.blocks.len(), 1);
}

#[test]
fn headings_inside_list_items_are_still_found() {
    let (doc, _) = parse("- item\n\n  # Nested\n");
    let titles: Vec<String> =
        doc.headings().into_iter().map(|(_, i, _)| Inline::plain(i)).collect();
    assert_eq!(titles, vec!["Nested"]);
}

#[test]
fn crlf_input_yields_the_same_tree_as_lf() {
    let (lf, _) = parse("# A\n\ntext\n");
    let (crlf, _) = parse("# A\r\n\r\ntext\r\n");
    assert_eq!(lf, crlf);
}

#[test]
fn frontmatter_warnings_carry_document_line_numbers() {
    let (_, diags) = parse("---\ntitle: ok\nbody: |\n---\n# H\n");
    assert_eq!(diags.items().len(), 1);
    assert_eq!(diags.items()[0].line, 3);
    assert!(diags.items()[0].message.contains("multiline scalar"));
}

#[test]
fn unsupported_constructs_are_kept_not_dropped() {
    let (doc, diags) = parse("> quoted\n\n    indented\n");
    let kept: Vec<&str> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Passthrough { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(kept, vec!["> quoted", "    indented"]);
    assert!(diags.is_empty(), "passthrough is out of scope, not an error");
}
