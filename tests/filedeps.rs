//! End-to-end coverage for `produces=`/`reads=file:PATH` (decision 33), over
//! a small dedicated corpus at `tests/data/filedeps-corpus/`. It is its own
//! root, isolated from `tests/data/corpus/`, so these fixtures never perturb
//! the golden dumps built over that one.
//!
//! `eval/plan.rs`'s own unit tests already cover `check_file_deps` in
//! isolation, including path normalization across directories. This file
//! covers the one thing those cannot: that `dankg check`, run against the
//! real binary over a real corpus, wires it in correctly and -- unlike
//! `dankg:depends` -- actually fails the exit code on what it finds.

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
    fn a_fresh_pair_linked_by_deps_is_silent() {
        let (_, stderr, _) = run(&["check", "tests/data/filedeps-corpus"]);
        assert!(!stderr.contains("fresh_deps.md"), "{stderr}");
    }

    #[test]
    fn a_fresh_pair_linked_by_xdeps_across_languages_is_silent() {
        let (_, stderr, _) = run(&["check", "tests/data/filedeps-corpus"]);
        assert!(!stderr.contains("fresh_xdeps.md"), "{stderr}");
    }

    #[test]
    fn a_match_among_several_dependencies_is_silent() {
        let (_, stderr, _) = run(&["check", "tests/data/filedeps-corpus"]);
        assert!(!stderr.contains("multiple_deps.md"), "{stderr}");
    }

    #[test]
    fn different_relative_spellings_across_directories_still_match() {
        let (_, stderr, _) = run(&["check", "tests/data/filedeps-corpus"]);
        assert!(!stderr.contains("cross/deep/read.md"), "{stderr}");
    }

    #[test]
    fn an_unrecognized_artifact_kind_is_never_reported() {
        let (_, stderr, _) = run(&["check", "tests/data/filedeps-corpus"]);
        assert!(!stderr.contains("unrecognized_kind.md"), "{stderr}");
    }

    #[test]
    fn a_mismatched_path_is_reported_by_file_line_and_block() {
        let (_, stderr, _) = run(&["check", "tests/data/filedeps-corpus"]);
        assert!(
            stderr.contains("stale-filedep: mismatch.md:7 `reader` reads=file:different.csv -- no dependency produces it"),
            "{stderr}"
        );
    }

    #[test]
    fn a_dependency_declaring_no_produces_is_reported() {
        let (_, stderr, _) = run(&["check", "tests/data/filedeps-corpus"]);
        assert!(
            stderr.contains(
                "stale-filedep: no_producer_declared.md:7 `reader` reads=file:out.csv -- no dependency produces it"
            ),
            "{stderr}"
        );
    }

    #[test]
    fn reads_with_no_dependency_edge_at_all_is_reported() {
        let (_, stderr, _) = run(&["check", "tests/data/filedeps-corpus"]);
        assert!(
            stderr.contains("stale-filedep: no_dependency.md:3 `reader` reads=file:out.csv -- no dependency produces it"),
            "{stderr}"
        );
    }

    #[test]
    fn a_reads_path_escaping_the_root_is_reported() {
        let (_, stderr, _) = run(&["check", "tests/data/filedeps-corpus"]);
        assert!(
            stderr.contains("stale-filedep: escape.md:3 `reader` reads=file:../../etc/passwd -- escapes the root"),
            "{stderr}"
        );
    }

    #[test]
    fn the_summary_line_counts_every_recognized_declaration_once() {
        let (_, stderr, _) = run(&["check", "tests/data/filedeps-corpus"]);
        assert!(stderr.contains("4 stale of 8 file dependency declaration(s)"), "{stderr}");
    }

    #[test]
    fn a_file_dependency_mismatch_fails_the_exit_code() {
        // Unlike `dankg:depends` (advisory only), decision 33 is a hard
        // failure: two declared strings disagreeing is a much stronger
        // signal than a prose substring match.
        let (_, _, ok) = run(&["check", "tests/data/filedeps-corpus"]);
        assert!(!ok, "a file-dependency mismatch must fail `dankg check`");
    }
}
