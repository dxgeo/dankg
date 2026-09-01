//! Freshness checks for `src/` modules that are generated from their own
//! literate `.md` source rather than hand-edited directly.
//!
//! `src/hash.rs` is committed alongside `src/hash.md` (src/hash.md,
//! *Literate source*) so `cargo build` never needs a working `dankg` binary
//! just to compile the crate -- but that only stays honest if drift between
//! the two gets caught, not assumed away. Each test here re-tangles the
//! literate source into a scratch directory and diffs the result against
//! what's actually committed, byte for byte.

use dankg::tangle;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scratch_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("dankg-literate-test-{}-{n}", std::process::id()))
}

/// Re-tangles `source` (a path relative to the crate root, exactly as it
/// would be typed on the command line -- the banner comment quotes it
/// verbatim) and asserts the named file inside the output matches the
/// committed one at `committed` (also crate-root-relative) byte for byte.
fn assert_matches_literate_source(source: &str, generated_name: &str, committed: &str) {
    let root = crate_root();
    let out = scratch_dir();
    let report = tangle::run(&[source.to_string()], "rust", Some(out.to_str().unwrap()), false)
        .unwrap_or_else(|e| panic!("tangling {source} failed: {e}"));
    assert_eq!(report.files, vec![out.join(generated_name)], "{source} did not tangle to the expected single file");

    let fresh = fs::read_to_string(out.join(generated_name)).unwrap();
    let committed_text = fs::read_to_string(root.join(committed))
        .unwrap_or_else(|e| panic!("{committed}: {e}"));
    assert_eq!(
        fresh, committed_text,
        "{committed} is stale: regenerate with `cargo run --bin dankg -- tangle {source} --lang rust -o {}` and re-commit",
        PathBuf::from(committed).parent().unwrap().display(),
    );

    let _ = fs::remove_dir_all(&out);
}

#[test]
fn hash_rs_matches_its_literate_source() {
    assert_matches_literate_source("src/hash.md", "hash.rs", "src/hash.rs");
}
