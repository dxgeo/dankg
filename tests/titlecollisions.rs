//! End-to-end coverage for `dankg check`'s duplicate-heading-title
//! advisory (decision 34), over a small dedicated corpus at
//! `tests/data/titlecollisions-corpus/`. It is its own root, isolated
//! from `tests/data/corpus/`, so these fixtures never perturb the golden
//! dumps built over that one.
//!
//! `graph::build::title_collisions`'s own unit tests already cover which
//! nodes count as a collision, including that a block sharing its own
//! heading's title does not. This file covers the one thing those
//! cannot: that `dankg check`, run against the real binary, correctly
//! tells a *live* collision (something already links to one of the two
//! slugs) apart from a *cosmetic* one (nothing does) -- and that neither
//! shape ever fails the exit code.

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
    fn a_cosmetic_collision_is_reported_with_no_references() {
        let (_, stderr, success) = run(&["check", "tests/data/titlecollisions-corpus"]);
        assert!(
            stderr.contains("dup-title: cosmetic.md:5 `Bar` shares its title with cosmetic.md:1 -- resolved as #bar-1 instead of #bar (no incoming references; cosmetic)"),
            "{stderr}"
        );
        assert!(success, "a cosmetic collision must never fail the exit code");
    }

    #[test]
    fn a_written_link_to_the_suffixed_slug_makes_the_collision_live() {
        let (_, stderr, success) = run(&["check", "tests/data/titlecollisions-corpus"]);
        assert!(
            stderr.contains("dup-title: referenced.md:5 `Foo` shares its title with referenced.md:1 -- resolved as #foo-1 instead of #foo (1 incoming reference(s)"),
            "{stderr}"
        );
        assert!(success, "a live collision is still advisory, never a build gate");
    }

    #[test]
    fn a_dankg_depends_marker_targeting_the_suffixed_slug_also_counts_as_a_reference() {
        let (_, stderr, _) = run(&["check", "tests/data/titlecollisions-corpus"]);
        assert!(
            stderr.contains("dup-title: via_depends.md:5 `Baz` shares its title with via_depends.md:1 -- resolved as #baz-1 instead of #baz (1 incoming reference(s)"),
            "{stderr}"
        );
    }

    #[test]
    fn the_summary_line_counts_referenced_collisions_separately() {
        let (_, stderr, _) = run(&["check", "tests/data/titlecollisions-corpus"]);
        assert!(
            stderr.contains("3 duplicate-title node(s), advisory (2 referenced)"),
            "{stderr}"
        );
    }
}
