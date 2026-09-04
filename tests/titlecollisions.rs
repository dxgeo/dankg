//! End-to-end coverage for `dankg check`'s duplicate-heading-title
//! advisory (decision 34), over a small dedicated corpus at
//! `tests/data/titlecollisions-corpus/`. It is its own root, isolated
//! from `tests/data/corpus/`, so these fixtures never perturb the golden
//! dumps built over that one.
//!
//! `graph::build::title_collisions`'s own unit tests already cover which
//! nodes count as a collision, including that a block sharing its own
//! heading's title does not, and which pairs count as `sibling`. This
//! file covers what those cannot: that `dankg check`, run against the
//! real binary, correctly reports both axes at once -- *live* versus
//! *cosmetic* (something already links to one of the two slugs, or not)
//! and *sibling* versus *differently-nested* (same immediate parent, or
//! not) -- and that no combination of the two ever fails the exit code.

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
    fn a_cosmetic_sibling_collision_is_reported_as_such() {
        let (_, stderr, success) = run(&["check", "tests/data/titlecollisions-corpus"]);
        assert!(
            stderr.contains("dup-title: cosmetic.md:5 `Bar` shares its title with sibling cosmetic.md:1 -- resolved as #bar-1 instead of #bar (no incoming references; cosmetic)"),
            "{stderr}"
        );
        assert!(success, "a cosmetic collision must never fail the exit code");
    }

    #[test]
    fn a_written_link_to_the_suffixed_slug_makes_a_sibling_collision_live() {
        let (_, stderr, success) = run(&["check", "tests/data/titlecollisions-corpus"]);
        assert!(
            stderr.contains("dup-title: referenced.md:5 `Foo` shares its title with sibling referenced.md:1 -- resolved as #foo-1 instead of #foo (1 incoming reference(s)"),
            "{stderr}"
        );
        assert!(success, "a live collision is still advisory, never a build gate");
    }

    #[test]
    fn a_dankg_depends_marker_targeting_the_suffixed_slug_also_counts_as_a_reference() {
        let (_, stderr, _) = run(&["check", "tests/data/titlecollisions-corpus"]);
        assert!(
            stderr.contains("dup-title: via_depends.md:5 `Baz` shares its title with sibling via_depends.md:1 -- resolved as #baz-1 instead of #baz (1 incoming reference(s)"),
            "{stderr}"
        );
    }

    #[test]
    fn a_cosmetic_collision_under_different_parents_is_differently_nested() {
        let (_, stderr, success) = run(&["check", "tests/data/titlecollisions-corpus"]);
        assert!(
            stderr.contains("dup-title: nested_cosmetic.md:9 `Notes` shares its title with differently-nested nested_cosmetic.md:3 -- resolved as #notes-1 instead of #notes (no incoming references; cosmetic)"),
            "{stderr}"
        );
        assert!(success);
    }

    #[test]
    fn a_referenced_collision_under_different_parents_is_also_differently_nested() {
        // The two axes are independent: a differently-nested pair can
        // still be live if something links to it, same as a sibling pair.
        let (_, stderr, success) = run(&["check", "tests/data/titlecollisions-corpus"]);
        assert!(
            stderr.contains("dup-title: nested_referenced.md:9 `Steps` shares its title with differently-nested nested_referenced.md:3 -- resolved as #steps-1 instead of #steps (1 incoming reference(s)"),
            "{stderr}"
        );
        assert!(success);
    }

    #[test]
    fn the_summary_line_counts_referenced_and_sibling_collisions_separately() {
        let (_, stderr, _) = run(&["check", "tests/data/titlecollisions-corpus"]);
        assert!(
            stderr.contains("5 duplicate-title node(s), advisory (3 referenced, 3 sibling)"),
            "{stderr}"
        );
    }
}
