//! Freshness check for every `src/` module generated from its own literate
//! `.md` source rather than hand-edited directly.
//!
//! Those files are committed alongside their `.md` source (src/lib.md,
//! *Literate source*) so `cargo build` never needs a working `dankg` binary
//! just to compile the crate -- but that only stays honest if drift between
//! the two is a checked failure, not an assumption. This test re-tangles the
//! whole corpus into a scratch directory and diffs every file it produced
//! against the committed one at the same relative path under `src/`.
//!
//! One test, not one per module: every literate source in `src/` uses an
//! explicit `path=` (src/lib.md explains why), so growing the set of
//! converted modules only ever adds more files to `report.files` -- this
//! test's own body does not change shape as that happens.

use dankg::tangle;
use std::fs;
use std::path::PathBuf;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scratch_dir() -> PathBuf {
    std::env::temp_dir().join(format!("dankg-literate-test-{}", std::process::id()))
}

#[test]
fn generated_src_files_match_their_literate_sources() {
    let root = crate_root();
    let out = scratch_dir();
    let _ = fs::remove_dir_all(&out);

    let report = tangle::run(&[".".to_string()], "rust", Some(out.to_str().unwrap()), false)
        .expect("corpus-wide tangle should succeed");
    assert!(!report.files.is_empty(), "no literate `src/` modules were tangled -- expected at least `src/hash.rs`");

    for file in &report.files {
        let rel = file.strip_prefix(&out).expect("tangled file should be under the scratch output dir");
        let fresh = fs::read_to_string(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        let committed_path = root.join("src").join(rel);
        let committed = fs::read_to_string(&committed_path)
            .unwrap_or_else(|e| panic!("{}: {e} (is a new literate module missing from git?)", committed_path.display()));
        assert_eq!(
            fresh, committed,
            "src/{} is stale: regenerate with `cargo run --bin dankg -- tangle . --lang rust -o src` and re-commit",
            rel.display(),
        );
    }

    let _ = fs::remove_dir_all(&out);
}
