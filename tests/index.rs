//! Root discovery, the corpus walk, and the cache.
//!
//! Milestone 4's whole job is deciding which files are yours, so most of these
//! tests are about exclusion: what the walk leaves out, and what it refuses to
//! follow. The root boundary already had one leak that unit tests could not
//! see -- fixtures at depth zero cannot climb out of anything -- so the checks
//! that matter run against real directories on disk.

use dankg::diag::Diags;
use dankg::graph::index::{self, Corpus};
use dankg::graph::{resolve, NodeId};
use dankg::render::json;
use std::fs;
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn load(paths: &[&str], cache: bool) -> (Corpus, Diags) {
    let owned: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    let mut diags = Diags::new("dankg");
    let corpus = index::load(&owned, cache, &mut diags).expect("corpus loads");
    (corpus, diags)
}

fn indexed(corpus: &Corpus) -> Vec<String> {
    corpus.paths.clone()
}

/// The committed corpus under `tests/data/corpus`, which carries a `.dankg/`
/// and a `.dankgignore` precisely so that discovery has something to find.
mod fixture {
    use super::*;

    const CORPUS: &str = "tests/data/corpus";

    #[test]
    fn the_dankg_directory_declares_the_root_from_any_depth() {
        let (corpus, _) = load(&["tests/data/corpus/notes/ideas.md"], false);
        assert!(corpus.declared, "the root was found, not derived");
        assert_eq!(corpus.display, CORPUS);
        assert_eq!(corpus.root, crate_root().join(CORPUS));
    }

    #[test]
    fn naming_one_file_still_indexes_the_whole_root() {
        // Decision 6: backlinks are only honest when every file has been seen.
        let (corpus, _) = load(&["tests/data/corpus/orphan.md"], false);
        assert_eq!(
            indexed(&corpus),
            vec!["notes/ideas.md", "orphan.md", "project.md"],
            "the index is the root; the entry only sets the view"
        );
        assert_eq!(corpus.entries, vec!["orphan.md".to_string()]);
    }

    #[test]
    fn dankgignore_excludes_a_directory_and_a_pattern() {
        let (corpus, _) = load(&[CORPUS], false);
        let paths = indexed(&corpus);
        assert!(!paths.iter().any(|p| p.starts_with("drafts/")), "`drafts/` is excluded");
        assert!(!paths.contains(&"wip.tmp.md".to_string()), "`*.tmp.md` is excluded");
        assert_eq!(corpus.stats.ignored, 2);

        // An excluded file's links must not reach the graph either, or
        // exclusion would only be cosmetic.
        let mut diags = Diags::new("t");
        let graph = resolve::resolve(&corpus.files, &mut diags);
        assert!(
            !graph.edges.iter().any(|e| e.from.file.starts_with("drafts/")),
            "an ignored file contributes no edges"
        );
    }

    #[test]
    fn naming_an_ignored_file_indexes_it_anyway_and_says_so() {
        let (corpus, diags) = load(&["tests/data/corpus/drafts/scratch.md"], false);
        assert!(indexed(&corpus).contains(&"drafts/scratch.md".to_string()));
        assert_eq!(corpus.entries, vec!["drafts/scratch.md".to_string()]);
        assert!(
            diags.items().iter().any(|d| d.message.contains("indexed because it was named")),
            "an override this surprising has to be reported"
        );
    }

    #[test]
    fn dot_directories_need_no_pattern_to_be_skipped() {
        let (corpus, _) = load(&[CORPUS], false);
        assert!(!indexed(&corpus).iter().any(|p| p.starts_with('.')));
    }

    #[test]
    fn config_and_ignore_are_read_from_the_discovered_root() {
        let (corpus, mut diags) = load(&["tests/data/corpus/notes/ideas.md"], false);
        assert_eq!(corpus.config.source.as_deref(), Some(".dankg/config"));
        assert_eq!(corpus.ignore.source.as_deref(), Some(".dankgignore"));
        assert_eq!(corpus.config.depth(&mut diags), 2);
        assert_eq!(corpus.config.lang("sh").map(|l| l.command), Some("sh {file}".to_string()));
    }

    #[test]
    fn node_ids_stay_root_relative_however_the_path_was_written() {
        let (a, _) = load(&["tests/data/corpus/project.md"], false);
        let (b, _) = load(&["./tests/data/./corpus/notes/../project.md"], false);
        assert_eq!(indexed(&a), indexed(&b));

        let mut diags = Diags::new("t");
        let graph = resolve::resolve(&a.files, &mut diags);
        assert!(graph.node(&NodeId::new("project", "overview")).is_some());
    }

    #[test]
    fn a_path_outside_the_root_is_refused() {
        let paths = vec!["tests/data/corpus/project.md".to_string(), "README.md".to_string()];
        let mut diags = Diags::new("dankg");
        let err = index::load(&paths, false, &mut diags).unwrap_err();
        assert!(err.contains("outside the root"), "{err}");
    }

    #[test]
    fn a_missing_path_is_a_usage_error_not_a_diagnostic() {
        let mut diags = Diags::new("dankg");
        let err = index::load(&["tests/data/corpus/gone.md".to_string()], false, &mut diags)
            .unwrap_err();
        assert!(err.contains("no such file"), "{err}");
    }
}

/// Corpora built on the fly, for the cases that need a file to change.
mod scratch {
    use super::*;

    fn scratch_root(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".dankg")).expect("scratch root");
        dir
    }

    fn write(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    fn run(root: &Path, cache: bool) -> (Corpus, String, Vec<String>) {
        let paths = vec![root.to_string_lossy().into_owned()];
        let mut diags = Diags::new("dankg");
        let corpus = index::load(&paths, cache, &mut diags).expect("corpus loads");
        let graph = resolve::resolve(&corpus.files, &mut diags);
        diags.sort();
        let reported = diags.items().iter().map(|d| d.to_string()).collect();
        (corpus, json::render(&graph), reported)
    }

    fn seed(name: &str) -> PathBuf {
        let root = scratch_root(name);
        write(&root, "a.md", "# One\n\nSee [two](b.md#two) and [gone](nope.md#lost).\n");
        write(&root, "notes/b.md", "# Two\n\nBack to [one](../a.md#one).\n");
        root
    }

    #[test]
    fn a_warm_cache_changes_runtime_and_nothing_else() {
        let root = seed("warm");

        let (cold, cold_json, cold_diags) = run(&root, true);
        assert_eq!(cold.stats.cache.hits, 0);
        assert_eq!(cold.stats.cache.writes, 2);

        let (warm, warm_json, warm_diags) = run(&root, true);
        assert_eq!(warm.stats.cache.hits, 2, "every file was reused");
        assert_eq!(warm.stats.cache.misses, 0);

        assert_eq!(cold_json, warm_json, "the graph is identical");
        assert_eq!(
            cold_diags, warm_diags,
            "a cached run must report exactly what a cold run reported"
        );
        assert!(!cold_diags.is_empty(), "the fixture does produce a diagnostic");
    }

    #[test]
    fn no_cache_matches_a_cached_run_exactly() {
        let root = seed("nocache");
        let (_, warm_first, _) = run(&root, true);
        let (uncached, uncached_json, uncached_diags) = run(&root, false);
        let (_, warm_json, warm_diags) = run(&root, true);

        assert!(uncached.cache_dir.is_none(), "--no-cache opens no cache");
        assert_eq!(uncached_json, warm_json);
        assert_eq!(warm_first, warm_json);
        assert_eq!(uncached_diags, warm_diags);
    }

    #[test]
    fn editing_a_file_invalidates_only_its_own_entry() {
        let root = seed("edit");
        run(&root, true);

        write(&root, "a.md", "# One Renamed\n\nSee [two](b.md#two) and [gone](nope.md#lost).\n");
        let (corpus, _, _) = run(&root, true);

        assert_eq!(corpus.stats.cache.misses, 1, "only the edited file is reparsed");
        assert_eq!(corpus.stats.cache.hits, 1);
        let mut diags = Diags::new("t");
        let graph = resolve::resolve(&corpus.files, &mut diags);
        assert!(graph.node(&NodeId::new("a", "one-renamed")).is_some());
    }

    #[test]
    fn changing_the_config_invalidates_everything() {
        let root = seed("config");
        run(&root, true);
        let (warm, _, _) = run(&root, true);
        assert_eq!(warm.stats.cache.hits, 2);

        fs::write(root.join(".dankg/config"), "[graph]\ndepth = 4\n").unwrap();
        let (after, _, _) = run(&root, true);
        assert_eq!(after.stats.cache.misses, 2, "config is stamped into every entry");
    }

    #[test]
    fn a_corrupt_entry_is_a_miss_rather_than_a_failure() {
        let root = seed("corrupt");
        run(&root, true);

        let cache_dir = root.join(".dankg/cache");
        for entry in fs::read_dir(&cache_dir).unwrap().flatten() {
            fs::write(entry.path(), "not a cache entry at all").unwrap();
        }
        let (corpus, json, _) = run(&root, true);
        assert_eq!(corpus.stats.cache.hits, 0);
        assert_eq!(corpus.stats.cache.misses, 2);

        let (_, expected, _) = run(&root, false);
        assert_eq!(json, expected, "a broken cache cannot change the answer");
    }

    #[test]
    fn a_root_without_a_dankg_directory_gets_no_cache() {
        // Deliberately not `scratch_root` (`CARGO_TARGET_TMPDIR`, under this
        // repo's own `target/`): the repo now has its own root-level
        // `.dankg/` (the design record's own corpus), so an ancestor walk
        // from anywhere inside it would find that one instead of finding
        // none at all, which is exactly the case this test means to cover.
        let root = std::env::temp_dir().join("dankg-test-nodir");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        write(&root, "a.md", "# One\n");

        let (corpus, _, _) = run(&root, true);
        assert!(corpus.cache_dir.is_none(), "DanKG does not create .dankg/ uninvited");
        assert!(!root.join(".dankg").exists());
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_is_reported_and_not_followed() {
        let root = scratch_root("symlink");
        write(&root, "a.md", "# One\n");

        let outside = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("symlink-target");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.md"), "# Secret\n").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();

        let paths = vec![root.to_string_lossy().into_owned()];
        let mut diags = Diags::new("dankg");
        let corpus = index::load(&paths, false, &mut diags).unwrap();

        assert_eq!(indexed(&corpus), vec!["a.md"], "the link is not walked through");
        assert!(diags.items().iter().any(|d| d.message.contains("symlink")));
        assert_eq!(corpus.stats.skipped, 1);
    }

    #[test]
    fn an_empty_root_is_an_empty_graph_not_an_error() {
        let root = scratch_root("empty");
        let (corpus, json, _) = run(&root, true);
        assert!(corpus.files.is_empty());
        assert!(json.contains("\"nodes\": [\n  ]"));
    }
}

/// The lesson from the root-escape leak: path handling needs an end-to-end
/// test, because the binary is where the working directory becomes real.
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
    fn index_reports_the_root_the_config_and_the_walk() {
        let (stdout, _, ok) = run(&["index", "tests/data/corpus/notes/ideas.md"]);
        assert!(ok);
        assert!(stdout.contains("root:    tests/data/corpus (declared by .dankg/)"), "{stdout}");
        assert!(stdout.contains("config:  .dankg/config"), "{stdout}");
        assert!(stdout.contains("ignore:  .dankgignore"), "{stdout}");
        assert!(stdout.contains("files:   3 indexed, 2 ignored, 0 skipped"), "{stdout}");
        assert!(stdout.contains("entry:   notes/ideas.md"), "{stdout}");
    }

    #[test]
    fn index_statistics_are_stdout_and_diagnostics_are_stderr() {
        let (stdout, stderr, _) = run(&["index", "tests/data/corpus"]);
        assert!(!stdout.contains("warn:"));
        assert!(stderr.contains("warn:"));
    }

    #[test]
    fn graph_output_does_not_depend_on_the_cache() {
        let (cached, _, _) = run(&["graph", "tests/data/corpus/project.md"]);
        let (uncached, _, _) = run(&["graph", "tests/data/corpus/project.md", "--no-cache"]);
        assert_eq!(cached, uncached);
        assert!(!cached.is_empty());
    }

    #[test]
    fn a_second_root_on_the_command_line_is_refused() {
        let (_, stderr, ok) = run(&["graph", "tests/data/corpus/project.md", "README.md"]);
        assert!(!ok);
        assert!(stderr.contains("cross-root graphs are refused"), "{stderr}");
    }
}
