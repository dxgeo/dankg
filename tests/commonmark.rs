//! CommonMark conformance.
//!
//! DanKG implements a subset of CommonMark on purpose (architecture.org,
//! decision 3). This harness measures exactly how much, so the scope claim in
//! the README is a number rather than a guess.
//!
//! The vendored `spec.json` is test *data*, not a dependency.
//!
//! The gate is a regression gate: unimplemented sections never fail the build,
//! but a section that loses ground does. Re-bless after deliberate changes:
//!
//!     DANKG_BLESS=1 cargo test --test commonmark
//!
//! See the full table with:
//!
//!     cargo test --test commonmark -- --nocapture
//!
//! Inspect what is failing in one section:
//!
//!     DANKG_SHOW=Links cargo test --test commonmark -- --nocapture

mod support;

use dankg::diag::Diags;
use dankg::md::Document;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

const SPEC: &str = "tests/data/commonmark/spec.json";
const BASELINE: &str = "tests/data/commonmark/baseline.txt";

struct Score {
    passed: usize,
    total: usize,
}

#[test]
fn commonmark_conformance() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let spec_text = fs::read_to_string(root.join(SPEC)).expect("vendored spec.json is missing");
    let spec = support::json::parse(&spec_text);
    let cases = spec.as_array();

    // Section order follows the spec, not the alphabet.
    let mut order: Vec<String> = Vec::new();
    let mut scores: BTreeMap<String, Score> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let show = std::env::var("DANKG_SHOW").ok();

    for case in cases {
        let section = case.str("section").to_string();
        let markdown = case.str("markdown");
        let expected = case.str("html");
        let example = case.num("example") as u32;

        if !order.contains(&section) {
            order.push(section.clone());
        }
        let entry = scores.entry(section.clone()).or_insert(Score { passed: 0, total: 0 });
        entry.total += 1;

        // A panic here is a genuine parser bug, not a conformance gap, so it is
        // caught and reported separately rather than aborting the run.
        let rendered = std::panic::catch_unwind(|| {
            let mut diags = Diags::new("spec");
            let doc = Document::parse(markdown, &mut diags);
            support::html::render(&doc)
        });

        match rendered {
            Ok(html) if html == expected => entry.passed += 1,
            Ok(html) => {
                if show.as_deref().is_some_and(|s| section.starts_with(s)) {
                    println!("--- example {example} ---");
                    println!("markdown:  {markdown:?}");
                    println!("expected:  {expected:?}");
                    println!("actual:    {html:?}");
                }
            }
            Err(_) => failures.push(format!("example {example} ({section}) panicked")),
        }
    }

    let report = format_table(&order, &scores);
    println!("{report}");
    let _ = fs::write(root.join("target/commonmark-report.txt"), &report);

    assert!(failures.is_empty(), "parser panicked on {} case(s):\n  {}", failures.len(), failures.join("\n  "));

    let baseline_path = root.join(BASELINE);
    if std::env::var("DANKG_BLESS").is_ok() {
        let mut out = String::new();
        for section in &order {
            let s = &scores[section];
            let _ = writeln!(out, "{}\t{}\t{}", section, s.passed, s.total);
        }
        fs::write(&baseline_path, out).expect("could not write baseline");
        println!("baseline blessed: {}", baseline_path.display());
        return;
    }

    let baseline_text = fs::read_to_string(&baseline_path)
        .expect("no baseline; run with DANKG_BLESS=1 to create one");
    let mut regressions = Vec::new();
    for line in baseline_text.lines() {
        let mut parts = line.split('\t');
        let (Some(section), Some(was), Some(_)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let was: usize = was.parse().expect("bad baseline line");
        let now = scores.get(section).map(|s| s.passed).unwrap_or(0);
        if now < was {
            regressions.push(format!("{section}: {was} -> {now}"));
        }
    }

    assert!(
        regressions.is_empty(),
        "conformance regressed in {} section(s):\n  {}\n\nIf intended, re-bless with DANKG_BLESS=1.",
        regressions.len(),
        regressions.join("\n  ")
    );
}

fn format_table(order: &[String], scores: &BTreeMap<String, Score>) -> String {
    let mut out = String::new();
    let total: usize = scores.values().map(|s| s.total).sum();
    let passed: usize = scores.values().map(|s| s.passed).sum();

    let _ = writeln!(out, "\nCommonMark conformance: {passed}/{total} ({}%)\n", pct(passed, total));
    let width = order.iter().map(String::len).max().unwrap_or(0);

    for section in order {
        let s = &scores[section];
        let _ = writeln!(
            out,
            "  {:<width$}  {:>3}/{:<3}  {:>3}%  {}",
            section,
            s.passed,
            s.total,
            pct(s.passed, s.total),
            bar(s.passed, s.total),
            width = width
        );
    }
    out
}

fn pct(n: usize, d: usize) -> usize {
    if d == 0 { 100 } else { n * 100 / d }
}

fn bar(n: usize, d: usize) -> String {
    let filled = if d == 0 { 0 } else { n * 20 / d };
    let mut s = String::new();
    for i in 0..20 {
        s.push(if i < filled { '#' } else { '.' });
    }
    s
}
