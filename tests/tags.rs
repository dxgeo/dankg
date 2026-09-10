//! End-to-end coverage for `[kind.*]` config and `<!-- dankg:tag -->`
//! markers, over two small dedicated corpora: `tests/data/tags-corpus/`
//! (every marker names a declared kind) and
//! `tests/data/tags-bad-corpus/` (one does not). Kept apart from each
//! other, and from `tests/data/corpus/`, for the same reason
//! `tests/data/filedeps-corpus/` already is: an unknown `kind=` fails
//! `dankg check`'s exit code, and that failure must never bleed into an
//! assertion about a corpus that is supposed to be entirely clean.
//!
//! `tag.rs`'s own unit tests already cover marker parsing, placement,
//! and the icon-width heuristic in isolation. This file covers the one
//! thing those cannot: that `dankg check`, run against the real binary
//! over a real corpus and a real `[kind.*]` config, wires both checks
//! in at the right severity.

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
    fn a_marker_naming_a_declared_kind_is_silent() {
        let (_, stderr, _) = run(&["check", "tests/data/tags-corpus"]);
        assert!(!stderr.contains("bad-tag"), "{stderr}");
    }

    #[test]
    fn a_target_that_still_matches_the_node_it_sits_on_is_silent() {
        // "Two"'s own marker carries `target=#two`, matching where it
        // actually sits; "Three"'s carries no `target=` at all, a
        // hand-written marker's own style. Neither should ever be
        // reported.
        let (_, stderr, _) = run(&["check", "tests/data/tags-corpus"]);
        assert!(!stderr.contains("target=#two"), "{stderr}");
    }

    #[test]
    fn a_kind_section_with_no_icon_still_counts_as_declared() {
        // `[kind.done]` has no `icon =` line at all -- `known.md`'s own
        // `kind=done` marker must still be recognized, not flagged.
        let (_, stderr, _) = run(&["check", "tests/data/tags-corpus"]);
        assert!(!stderr.contains("kind=done"), "{stderr}");
    }

    #[test]
    fn every_marker_in_a_clean_corpus_still_lets_check_succeed() {
        let (_, _, ok) = run(&["check", "tests/data/tags-corpus"]);
        assert!(ok, "every kind= here is declared; nothing should fail the exit code");
    }

    #[test]
    fn a_wide_icon_is_reported_but_never_fails_the_exit_code() {
        // `[kind.wide] icon = ⌛` -- the exact glyph this codebase's own
        // history already got bitten by (eval-custom-plan.md).
        let (_, stderr, ok) = run(&["check", "tests/data/tags-corpus"]);
        assert!(
            stderr.contains("wide-icon: [kind.wide] icon=⌛ -- may render wider than one column, or in color"),
            "{stderr}"
        );
        assert!(ok, "an advisory icon warning must never fail the exit code");
    }

    #[test]
    fn a_narrow_icon_is_never_reported_as_wide() {
        let (_, stderr, _) = run(&["check", "tests/data/tags-corpus"]);
        assert!(!stderr.contains("[kind.task]"), "{stderr}");
    }

    #[test]
    fn the_summary_lines_count_markers_and_icons_separately() {
        let (_, stderr, _) = run(&["check", "tests/data/tags-corpus"]);
        assert!(stderr.contains("0 bad of 2 dankg:tag marker(s)"), "{stderr}");
        assert!(stderr.contains("1 of 2 configured icon(s) advisory-wide"), "{stderr}");
    }

    #[test]
    fn a_marker_naming_an_undeclared_kind_is_reported_by_file_and_line() {
        let (_, stderr, _) = run(&["check", "tests/data/tags-bad-corpus"]);
        assert!(
            stderr.contains("bad-tag: unknown.md:5 kind=bogus -- not declared in any [kind.*] section"),
            "{stderr}"
        );
    }

    #[test]
    fn an_undeclared_kind_fails_the_exit_code() {
        // Unlike `dankg:depends` (advisory only), an unrecognized `kind=`
        // is a hard failure: there is no fuzzy interpretation under which
        // a name nothing declares is fine, the same severity a
        // `produces=`/`reads=file:PATH` mismatch already gets.
        let (_, _, ok) = run(&["check", "tests/data/tags-bad-corpus"]);
        assert!(!ok, "an unrecognized kind= must fail `dankg check`");
    }

    #[test]
    fn a_target_pointing_at_a_different_node_than_the_marker_sits_on_is_reported() {
        // mismatch.md: the marker sits directly before `## Two` but
        // declares `target=#three` -- exactly the drift an edit that
        // inserted or reordered a heading would produce.
        let (_, stderr, _) = run(&["check", "tests/data/tags-bad-corpus"]);
        assert!(
            stderr.contains("bad-tag: mismatch.md:5 target=#three -- no longer points back at this marker's own node"),
            "{stderr}"
        );
    }

    #[test]
    fn a_target_mismatch_fails_the_exit_code_too() {
        // Same severity as an unrecognized kind=: there is no fuzzy
        // reading under which a marker pointing at the wrong node is
        // fine.
        let (_, _, ok) = run(&["check", "tests/data/tags-bad-corpus"]);
        assert!(!ok, "a target= mismatch must fail `dankg check`");
    }

    #[test]
    fn the_bad_corpus_summary_line_counts_both_bad_markers() {
        let (_, stderr, _) = run(&["check", "tests/data/tags-bad-corpus"]);
        assert!(stderr.contains("2 bad of 2 dankg:tag marker(s)"), "{stderr}");
    }
}
