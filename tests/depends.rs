//! End-to-end coverage for `dankg:depends` markers, over a small dedicated
//! corpus at `tests/data/depends-corpus/`. It is its own root, isolated
//! from `tests/data/corpus/`, so these fixtures never perturb the golden
//! dumps built over that one.
//!
//! `depends.rs`'s own unit tests already cover `parse_marker`,
//! `resolve_target`, `section_text` and `verify` in isolation. This file
//! covers the one thing those cannot: that `dankg check`, run against the
//! real binary over a real corpus, wires them together correctly and
//! never fails its exit code on what they find.

mod binary {
    use std::process::Command;

    fn run(args: &[&str]) -> (String, String, bool) {
        let out = Command::new(env!("CARGO_BIN_EXE_dankg"))
            .args(args)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("failed to run dankg");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.success(),
        )
    }

    #[test]
    fn a_fresh_same_file_quote_is_silent() {
        let (_, stderr, _) = run(&["check", "tests/data/depends-corpus"]);
        assert!(!stderr.contains("fresh.md"), "{stderr}");
    }

    #[test]
    fn a_stale_quote_is_reported_by_file_line_and_target() {
        let (_, stderr, _) = run(&["check", "tests/data/depends-corpus"]);
        assert!(
            stderr.contains("stale-prose: stale.md:3 depends on `changed.md#changed` -- quote no longer found"),
            "{stderr}"
        );
    }

    #[test]
    fn a_rewrapped_but_unchanged_quote_stays_fresh() {
        // Same words as `wrapped.md#wrapped`, just wrapped across lines
        // there. Whitespace normalization is what keeps this out of the
        // stale count.
        let (_, stderr, _) = run(&["check", "tests/data/depends-corpus"]);
        assert!(!stderr.contains("rewrap.md"), "{stderr}");
    }

    #[test]
    fn a_target_that_was_never_written_is_reported_as_not_found() {
        let (_, stderr, _) = run(&["check", "tests/data/depends-corpus"]);
        assert!(
            stderr.contains("stale-prose: unresolved.md:3 depends on `missing.md#nowhere` -- target not found"),
            "{stderr}"
        );
    }

    #[test]
    fn a_target_escaping_the_root_is_reported_and_never_followed() {
        let (_, stderr, _) = run(&["check", "tests/data/depends-corpus"]);
        assert!(
            stderr.contains("stale-prose: escape.md:3 depends on `../../etc/passwd.md#x` -- target escapes the root"),
            "{stderr}"
        );
    }

    #[test]
    fn a_cross_file_dependency_resolves_and_stays_fresh() {
        let (_, stderr, _) = run(&["check", "tests/data/depends-corpus"]);
        assert!(!stderr.contains("consumer.md"), "{stderr}");
    }

    #[test]
    fn the_summary_line_counts_every_marker_once() {
        let (_, stderr, _) = run(&["check", "tests/data/depends-corpus"]);
        assert!(stderr.contains("3 of 6 prose dependency marker(s) advisory-stale"), "{stderr}");
    }

    #[test]
    fn advisory_staleness_never_fails_the_exit_code() {
        // Half the corpus's own markers are stale by construction. Exit
        // code 0 is the whole point: see `main.md`'s own check_cmd doc
        // comment for why this never gates the way an unresolved link or
        // a stale eval result does.
        let (_, _, ok) = run(&["check", "tests/data/depends-corpus"]);
        assert!(ok, "advisory-stale prose dependencies must not fail `dankg check`");
    }
}
